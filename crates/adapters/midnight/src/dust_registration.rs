// SPDX-License-Identifier: Apache-2.0

//! Canonical protected NIGHT-to-DUST registration behind a typed wallet port.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
};

use midnight_base_crypto::{hash::HashOutput, schnorr::Signature, time::Timestamp};
use midnight_coin_structure::coin::{NIGHT, UserAddress};
use midnight_ledger::{
    dust::{DustActions, DustPublicKey, DustRegistration},
    structure::{
        IntentHash, StandardTransaction, Transaction, UnshieldedOffer, UtxoOutput, UtxoSpend,
    },
};
use midnight_storage::{
    DefaultDB,
    arena::Sp,
    storage::{Array, HashMap as LedgerHashMap},
};
use oxid_wallet_application::{
    AuthorizeWalletDustRegistrationRequest, PrepareWalletDustRegistrationRequest,
    SubmitWalletDustRegistrationRequest, SubmittedWalletDustRegistration, WalletAccountPortError,
    WalletDustRegistrationPort, WalletDustRegistrationPortError, WalletDustRegistrationPortFuture,
    WalletDustRegistrationStatusPortFuture, WalletTransactionPortError,
};
use oxid_wallet_domain::{
    AssetBalance, ChainBlockId, ChainTransactionId, WalletDustRegistrationPreview,
    WalletDustRegistrationSubmission, WalletDustRegistrationSubmissionStatus, WalletProfileId,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransactionSubmissionState,
};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    MidnightWalletAdapter, SPECKS_PER_DUST, STARS_PER_NIGHT, midnight_asset, network_by_id,
    submission_journal::{StoredSubmissionJournalEntry, StoredSubmissionState},
    transaction::{
        LedgerIntent, LedgerTransaction, MidnightCompletionOutcome, MidnightCompletionRequest,
        MidnightRegistrationContext, MidnightSpendableUtxo, MidnightSubmissionAttempt,
        MidnightSubmissionControl, MidnightSubmissionReconciliation, MidnightTransactionAuthorizer,
        MidnightTransactionSource, decode_signature, decode_verifying_key,
    },
};

const DUST_REGISTRATION_SEGMENT: u16 = 1;

pub(crate) struct RetainedMidnightDustRegistration {
    planning_fingerprint: [u8; 32],
    preview: WalletDustRegistrationPreview,
    account_index: u32,
    signing_payload: Zeroizing<Vec<u8>>,
    unsigned_intent: LedgerIntent,
    signed_transaction: Option<LedgerTransaction>,
    submission: Option<WalletDustRegistrationSubmission>,
    submission_state: WalletTransactionSubmissionState,
    submission_control: Option<Arc<MidnightSubmissionControl>>,
}

pub(crate) type RetainedMidnightDustRegistrations = Arc<
    Mutex<HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>>,
>;

struct PlannedRegistration {
    intent: LedgerIntent,
    signing_payload: Vec<u8>,
    eligibility_fingerprint: [u8; 32],
    registered_night: u128,
    input_count: u16,
    maximum_fee_allowance: u128,
}

impl<S, D> WalletDustRegistrationPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer + Clone + 'static,
{
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        let selected = self.selected(profile_id).map_err(map_account_error)?;
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletDustRegistrationPortError::InvalidChainState)?;
        let spendable = self
            .source
            .spendable_account(profile_id, &network)
            .map_err(map_transaction_error)?;
        if spendable.account.network_id() != &selected {
            return Err(WalletDustRegistrationPortError::DraftConflict);
        }
        let night_key = decode_verifying_key(&spendable.account).map_err(map_transaction_error)?;
        let context = self
            .completer
            .registration_context()
            .map_err(map_transaction_error)?;
        let chain_time = u64::try_from(context.timestamp.to_secs())
            .map_err(|_| WalletDustRegistrationPortError::InvalidChainState)?;
        if request.expires_at.value() / 1_000 <= chain_time {
            return Err(WalletDustRegistrationPortError::DraftExpired);
        }
        let dust_public_key = self
            .deriver
            .dust_public_key(profile_id, spendable.account.account_index())
            .map_err(map_transaction_error)?;
        let plan = plan_registration(
            &selected,
            night_key,
            dust_public_key,
            spendable.utxos,
            &context,
            request.expires_at.value() / 1_000,
        )?;
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(DUST_REGISTRATION_SEGMENT, plan.intent.clone());
        let transaction = Transaction::Standard(StandardTransaction::new(
            selected.as_str(),
            intents,
            None,
            LedgerHashMap::new(),
        ));
        let fee = transaction
            .fees(&context.parameters, false)
            .map_err(|_| WalletDustRegistrationPortError::InvalidChainState)?;
        if fee > plan.maximum_fee_allowance {
            return Err(WalletDustRegistrationPortError::InsufficientRegistrationAllowance);
        }

        let draft_id = registration_id(&plan.signing_payload)?;
        let authorization_challenge = registration_challenge(&draft_id, &plan.signing_payload)?;
        let planning_fingerprint = registration_fingerprint(
            profile_id,
            &selected,
            spendable.account.account_id().as_str(),
            &plan.eligibility_fingerprint,
        );
        if let Some(stored) = self
            .submission_journal
            .find_planning_fingerprint(profile_id, &planning_fingerprint)
            .map_err(map_store_error)?
        {
            return Err(match stored.state {
                StoredSubmissionState::Broadcasting | StoredSubmissionState::OutcomeUnknown => {
                    WalletDustRegistrationPortError::SubmissionOutcomeUnknown
                }
                StoredSubmissionState::Included => {
                    WalletDustRegistrationPortError::RegistrationAlreadyCurrent
                }
                StoredSubmissionState::Rejected | StoredSubmissionState::Expired => {
                    WalletDustRegistrationPortError::DraftConflict
                }
            });
        }
        let night = midnight_asset("midnight:night", "NIGHT", STARS_PER_NIGHT)
            .map_err(map_account_error)?;
        let dust =
            midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
        let preview = WalletDustRegistrationPreview::new(
            draft_id.clone(),
            authorization_challenge,
            selected,
            spendable.account.account_id().clone(),
            AssetBalance::new(night, plan.registered_night),
            plan.input_count,
            AssetBalance::new(dust, plan.maximum_fee_allowance),
            WalletTransactionFeeState::RequiresBalancing,
            request.expires_at,
            WalletTransactionDraftState::Prepared,
        )
        .map_err(|_| WalletDustRegistrationPortError::InvalidData)?;
        let retained = RetainedMidnightDustRegistration {
            planning_fingerprint,
            preview: preview.clone(),
            account_index: spendable.account.account_index(),
            signing_payload: Zeroizing::new(plan.signing_payload),
            unsigned_intent: plan.intent,
            signed_transaction: None,
            submission: None,
            submission_state: WalletTransactionSubmissionState::NotStarted,
            submission_control: None,
        };
        let key = (profile_id.clone(), draft_id);
        let mut drafts = self
            .dust_registration_drafts
            .lock()
            .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
        if let Some(existing) = drafts.iter().find_map(|((stored_profile, _), retained)| {
            (stored_profile == profile_id && retained.planning_fingerprint == planning_fingerprint)
                .then(|| retained.preview.clone())
        }) {
            return Ok(existing);
        }
        if drafts.iter().any(|((stored_profile, _), retained)| {
            stored_profile == profile_id
                && matches!(
                    retained.preview.state(),
                    WalletTransactionDraftState::Prepared
                        | WalletTransactionDraftState::Authorized
                        | WalletTransactionDraftState::Submitting
                )
        }) {
            return Err(WalletDustRegistrationPortError::DraftConflict);
        }
        drafts.insert(key, retained);
        Ok(preview)
    }

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletDustRegistrationRequest,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        let key = (profile_id.clone(), request.draft_id.clone());
        let mut drafts = self
            .dust_registration_drafts
            .lock()
            .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&key)
            .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
        if request.now.value() >= retained.preview.expires_at().value() {
            expire_retained(retained);
            return Err(WalletDustRegistrationPortError::DraftExpired);
        }
        if retained.preview.authorization_challenge() != &request.authorization_challenge {
            return Err(WalletDustRegistrationPortError::AuthorizationChallengeMismatch);
        }
        match retained.preview.state() {
            WalletTransactionDraftState::Prepared => {}
            WalletTransactionDraftState::Authorized => return Ok(retained.preview.clone()),
            WalletTransactionDraftState::Submitting => {
                return Err(WalletDustRegistrationPortError::SubmissionInProgress);
            }
            WalletTransactionDraftState::Submitted | WalletTransactionDraftState::Expired => {
                return Err(WalletDustRegistrationPortError::DraftConflict);
            }
        }
        let selected = self.selected(profile_id).map_err(map_account_error)?;
        if retained.preview.network_id() != &selected {
            return Err(WalletDustRegistrationPortError::DraftConflict);
        }
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletDustRegistrationPortError::InvalidChainState)?;
        let spendable = self
            .source
            .spendable_account(profile_id, &network)
            .map_err(map_transaction_error)?;
        if spendable.account.account_id() != retained.preview.account_id()
            || spendable.account.account_index() != retained.account_index
        {
            return Err(WalletDustRegistrationPortError::DraftConflict);
        }
        let wallet_signature = self
            .deriver
            .authorize(
                profile_id,
                &spendable.account,
                retained.signing_payload.as_slice(),
            )
            .map_err(map_transaction_error)?;
        let signature = decode_signature(&wallet_signature).map_err(map_transaction_error)?;
        let verifying_key =
            decode_verifying_key(&spendable.account).map_err(map_transaction_error)?;
        if !verifying_key.verify(retained.signing_payload.as_slice(), &signature) {
            return Err(WalletDustRegistrationPortError::InvalidData);
        }
        let mut intent = retained.unsigned_intent.clone();
        sign_offer(&mut intent.guaranteed_unshielded_offer, &signature)?;
        sign_offer(&mut intent.fallible_unshielded_offer, &signature)?;
        let dust_actions = intent
            .dust_actions
            .as_ref()
            .ok_or(WalletDustRegistrationPortError::InvalidData)?;
        if dust_actions.registrations.len() != 1 {
            return Err(WalletDustRegistrationPortError::InvalidData);
        }
        let registrations = dust_actions
            .registrations
            .iter_deref()
            .map(|registration| {
                let mut registration = registration.clone();
                if registration.night_key != verifying_key {
                    return Err(WalletDustRegistrationPortError::DraftConflict);
                }
                registration.signature = Some(Sp::new(signature.clone()));
                Ok(registration)
            })
            .collect::<Result<Array<_, DefaultDB>, _>>()?;
        intent.dust_actions = Some(Sp::new(DustActions {
            spends: dust_actions.spends.clone(),
            registrations,
            ctime: dust_actions.ctime,
        }));
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(DUST_REGISTRATION_SEGMENT, intent);
        retained.signed_transaction = Some(Transaction::Standard(StandardTransaction::new(
            selected.as_str(),
            intents,
            None,
            LedgerHashMap::new(),
        )));
        retained.signing_payload = Zeroizing::new(Vec::new());
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
        Ok(retained.preview.clone())
    }

    fn submit<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        request: SubmitWalletDustRegistrationRequest,
    ) -> WalletDustRegistrationPortFuture<'a> {
        Box::pin(async move {
            let key = (profile_id.clone(), request.draft_id.clone());
            let (transaction, account_index, expires_at_seconds, control) = {
                let mut drafts = self
                    .dust_registration_drafts
                    .lock()
                    .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
                let retained = drafts
                    .get_mut(&key)
                    .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
                match retained.preview.state() {
                    WalletTransactionDraftState::Submitted => {
                        return Ok(SubmittedWalletDustRegistration {
                            preview: retained.preview.clone(),
                            submission: retained
                                .submission
                                .clone()
                                .ok_or(WalletDustRegistrationPortError::InvalidData)?,
                        });
                    }
                    WalletTransactionDraftState::Submitting => {
                        return Err(WalletDustRegistrationPortError::SubmissionInProgress);
                    }
                    WalletTransactionDraftState::Prepared => {
                        return Err(WalletDustRegistrationPortError::DraftConflict);
                    }
                    WalletTransactionDraftState::Expired => {
                        return Err(WalletDustRegistrationPortError::DraftExpired);
                    }
                    WalletTransactionDraftState::Authorized => {}
                }
                if request.now.value() >= retained.preview.expires_at().value() {
                    expire_retained(retained);
                    return Err(WalletDustRegistrationPortError::DraftExpired);
                }
                let transaction = retained
                    .signed_transaction
                    .clone()
                    .ok_or(WalletDustRegistrationPortError::InvalidData)?;
                let control = Arc::new(MidnightSubmissionControl::new(
                    MidnightSubmissionAttempt {
                        profile_id: profile_id.clone(),
                        network_id: retained.preview.network_id().clone(),
                        draft_id: request.draft_id.clone(),
                        planning_fingerprint: retained.planning_fingerprint,
                        expires_at: retained.preview.expires_at(),
                        updated_at: request.now,
                    },
                    Arc::clone(&self.submission_journal),
                ));
                retained.preview = retained
                    .preview
                    .with_state(WalletTransactionDraftState::Submitting);
                retained.submission_state = WalletTransactionSubmissionState::Running;
                retained.submission_control = Some(Arc::clone(&control));
                (
                    transaction,
                    retained.account_index,
                    retained.preview.expires_at().value() / 1_000,
                    control,
                )
            };

            let profile = profile_id.clone();
            let deriver = self.deriver.clone();
            let completer = Arc::clone(&self.completer);
            let drafts = Arc::clone(&self.dust_registration_drafts);
            let worker_key = key.clone();
            let draft_id = request.draft_id;
            let worker_control = Arc::clone(&control);
            let (sender, receiver) = futures::channel::oneshot::channel();
            thread::Builder::new()
                .name("oxid-midnight-dust-register".to_owned())
                .spawn(move || {
                    let mut operation = |dust_seed: &[u8; 32]| {
                        completer.complete(
                            MidnightCompletionRequest::new(
                                transaction.clone(),
                                expires_at_seconds,
                                Arc::clone(&worker_control),
                            ),
                            dust_seed,
                        )
                    };
                    let completion = deriver.use_dust_seed(&profile, account_index, &mut operation);
                    let result = finish_registration(
                        drafts.as_ref(),
                        &worker_key,
                        draft_id,
                        worker_control.as_ref(),
                        completion,
                    );
                    let _ = sender.send(result);
                })
                .map_err(|_| {
                    let _ = restore_authorized(
                        self.dust_registration_drafts.as_ref(),
                        &key,
                        WalletTransactionSubmissionState::NotStarted,
                    );
                    WalletDustRegistrationPortError::Unavailable
                })?;
            match receiver.await {
                Ok(result) => result,
                Err(_) => {
                    if control.broadcast_started().unwrap_or(true) {
                        let _ = control.mark_terminal(
                            StoredSubmissionState::OutcomeUnknown,
                            None,
                            None,
                        );
                    }
                    mark_outcome_unknown(self.dust_registration_drafts.as_ref(), &key)?;
                    Err(WalletDustRegistrationPortError::SubmissionOutcomeUnknown)
                }
            }
        })
    }

    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: oxid_foundation::UnixTimestampMillis,
    ) -> Result<WalletDustRegistrationPreview, WalletDustRegistrationPortError> {
        let mut drafts = self
            .dust_registration_drafts
            .lock()
            .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&(profile_id.clone(), draft_id.clone()))
            .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
        if now.value() >= retained.preview.expires_at().value()
            && matches!(
                retained.preview.state(),
                WalletTransactionDraftState::Prepared | WalletTransactionDraftState::Authorized
            )
        {
            expire_retained(retained);
        }
        Ok(retained.preview.clone())
    }

    fn status(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
        if !draft_id.as_str().starts_with("dustreg_") {
            return Err(WalletDustRegistrationPortError::DraftNotFound);
        }
        let retained = self
            .dust_registration_drafts
            .lock()
            .map_err(|_| WalletDustRegistrationPortError::Unavailable)?
            .get(&(profile_id.clone(), draft_id.clone()))
            .map(registration_status)
            .transpose()?;
        let stored = self
            .submission_journal
            .load(profile_id, draft_id)
            .map_err(map_store_error)?;
        match (stored.as_ref(), retained) {
            (Some(entry), _) => registration_status_from_stored(entry),
            (None, Some(status)) => Ok(status),
            (None, None) => Err(WalletDustRegistrationPortError::DraftNotFound),
        }
    }

    fn cancel_submission(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
    ) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
        let mut drafts = self
            .dust_registration_drafts
            .lock()
            .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&(profile_id.clone(), draft_id.clone()))
            .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
        match retained.submission_state {
            WalletTransactionSubmissionState::Running => {
                retained
                    .submission_control
                    .as_ref()
                    .ok_or(WalletDustRegistrationPortError::InvalidData)?
                    .request_cancellation()
                    .map_err(map_transaction_error)?;
                retained.submission_state = WalletTransactionSubmissionState::CancellationRequested;
            }
            WalletTransactionSubmissionState::CancellationRequested
            | WalletTransactionSubmissionState::Cancelled => {}
            WalletTransactionSubmissionState::NotStarted
            | WalletTransactionSubmissionState::Rejected
            | WalletTransactionSubmissionState::Expired => {
                return Err(WalletDustRegistrationPortError::SubmissionNotInProgress);
            }
            WalletTransactionSubmissionState::Included
            | WalletTransactionSubmissionState::Broadcasting
            | WalletTransactionSubmissionState::OutcomeUnknown => {
                return Err(WalletDustRegistrationPortError::SubmissionCancellationUnsafe);
            }
        }
        registration_status(retained)
    }

    fn reconcile_submission<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        draft_id: &'a WalletTransactionDraftId,
    ) -> WalletDustRegistrationStatusPortFuture<'a> {
        Box::pin(async move {
            if !draft_id.as_str().starts_with("dustreg_") {
                return Err(WalletDustRegistrationPortError::DraftNotFound);
            }
            let entry = self
                .submission_journal
                .load(profile_id, draft_id)
                .map_err(map_store_error)?
                .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
            let status = registration_status_from_stored(&entry)?;
            if !status.reconciliation_allowed() {
                return Ok(status);
            }
            let reconciler = Arc::clone(&self.submission_reconciler);
            let journal = Arc::clone(&self.submission_journal);
            let drafts = Arc::clone(&self.dust_registration_drafts);
            let (sender, receiver) = futures::channel::oneshot::channel();
            thread::Builder::new()
                .name("oxid-midnight-dust-reconcile".to_owned())
                .spawn(move || {
                    let result = reconciler
                        .reconcile(&entry)
                        .map_err(map_transaction_error)
                        .and_then(|outcome| {
                            persist_registration_reconciliation(
                                journal.as_ref(),
                                drafts.as_ref(),
                                entry,
                                outcome,
                            )
                        });
                    let _ = sender.send(result);
                })
                .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
            receiver
                .await
                .unwrap_or(Err(WalletDustRegistrationPortError::Unavailable))
        })
    }
}

fn plan_registration(
    network_id: &oxid_wallet_domain::ChainNetworkId,
    night_key: midnight_base_crypto::schnorr::VerifyingKey,
    dust_public_key: DustPublicKey,
    utxos: Vec<MidnightSpendableUtxo>,
    context: &MidnightRegistrationContext,
    expires_at_seconds: u64,
) -> Result<PlannedRegistration, WalletDustRegistrationPortError> {
    if utxos.is_empty() {
        return Err(WalletDustRegistrationPortError::NoEligibleNight);
    }
    let mut eligible = utxos
        .into_iter()
        .filter(|utxo| !utxo.registered_for_dust_generation)
        .map(|utxo| {
            let created_at = utxo
                .created_at_seconds
                .ok_or(WalletDustRegistrationPortError::InvalidChainState)?;
            let generated = generated_dust(&utxo, created_at, context)?;
            Ok((utxo, generated))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if eligible.is_empty() {
        return Err(WalletDustRegistrationPortError::RegistrationAlreadyCurrent);
    }
    if eligible.len() > usize::from(oxid_wallet_domain::MAX_WALLET_DUST_REGISTRATION_INPUTS) {
        return Err(WalletDustRegistrationPortError::InvalidData);
    }
    eligible.sort_by(|(left, left_generated), (right, right_generated)| {
        right_generated
            .cmp(left_generated)
            .then_with(|| left.intent_hash.cmp(&right.intent_hash))
            .then_with(|| left.output_index.cmp(&right.output_index))
    });
    let eligibility_fingerprint = registration_eligibility_fingerprint(&eligible);
    let maximum_fee_allowance = eligible[0].1;
    if maximum_fee_allowance == 0 {
        return Err(WalletDustRegistrationPortError::InsufficientRegistrationAllowance);
    }
    let registered_night = eligible.iter().try_fold(0_u128, |total, (utxo, _)| {
        total
            .checked_add(utxo.value)
            .ok_or(WalletDustRegistrationPortError::InvalidData)
    })?;
    let guaranteed = offer(&eligible[..1], &night_key)?;
    let fallible = (!eligible[1..].is_empty())
        .then(|| offer(&eligible[1..], &night_key))
        .transpose()?;
    let registration: DustRegistration<Signature, DefaultDB> = DustRegistration {
        night_key,
        dust_address: Some(Sp::new(dust_public_key)),
        allow_fee_payment: maximum_fee_allowance,
        signature: None,
    };
    let mut intent = LedgerIntent::empty(&mut OsRng, Timestamp::from_secs(expires_at_seconds));
    intent.guaranteed_unshielded_offer = Some(Sp::new(guaranteed));
    intent.fallible_unshielded_offer = fallible.map(Sp::new);
    intent.dust_actions = Some(Sp::new(DustActions {
        spends: Array::new(),
        registrations: Array::new().push(registration),
        ctime: context.timestamp,
    }));
    let signing_payload = intent
        .erase_proofs()
        .erase_signatures()
        .data_to_sign(DUST_REGISTRATION_SEGMENT);
    if network_id.as_str().is_empty() || signing_payload.is_empty() {
        return Err(WalletDustRegistrationPortError::InvalidData);
    }
    Ok(PlannedRegistration {
        intent,
        signing_payload,
        eligibility_fingerprint,
        registered_night,
        input_count: u16::try_from(eligible.len())
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)?,
        maximum_fee_allowance,
    })
}

fn generated_dust(
    utxo: &MidnightSpendableUtxo,
    created_at_seconds: u64,
    context: &MidnightRegistrationContext,
) -> Result<u128, WalletDustRegistrationPortError> {
    let now = u64::try_from(context.timestamp.to_secs())
        .map_err(|_| WalletDustRegistrationPortError::InvalidChainState)?;
    let elapsed = now.saturating_sub(created_at_seconds) as u128;
    let cap = utxo
        .value
        .saturating_mul(context.parameters.dust.night_dust_ratio as u128);
    let rate = utxo
        .value
        .saturating_mul(context.parameters.dust.generation_decay_rate as u128);
    Ok(elapsed.saturating_mul(rate).min(cap))
}

fn offer(
    utxos: &[(MidnightSpendableUtxo, u128)],
    night_key: &midnight_base_crypto::schnorr::VerifyingKey,
) -> Result<UnshieldedOffer<Signature, DefaultDB>, WalletDustRegistrationPortError> {
    if utxos.is_empty() {
        return Err(WalletDustRegistrationPortError::InvalidData);
    }
    let mut total = 0_u128;
    let mut inputs = Vec::with_capacity(utxos.len());
    for (utxo, _) in utxos {
        total = total
            .checked_add(utxo.value)
            .ok_or(WalletDustRegistrationPortError::InvalidData)?;
        inputs.push(UtxoSpend {
            value: utxo.value,
            owner: night_key.clone(),
            type_: NIGHT,
            intent_hash: IntentHash(HashOutput(utxo.intent_hash)),
            output_no: utxo.output_index,
        });
    }
    inputs.sort();
    Ok(UnshieldedOffer {
        inputs: inputs.into(),
        outputs: vec![UtxoOutput {
            value: total,
            owner: UserAddress::from(night_key.clone()),
            type_: NIGHT,
        }]
        .into(),
        signatures: Vec::new().into(),
    })
}

fn sign_offer(
    offer: &mut Option<Sp<UnshieldedOffer<Signature, DefaultDB>, DefaultDB>>,
    signature: &Signature,
) -> Result<(), WalletDustRegistrationPortError> {
    if let Some(offer) = offer {
        if offer.inputs.is_empty() {
            return Err(WalletDustRegistrationPortError::InvalidData);
        }
        let mut signed = (**offer).clone();
        signed.add_signatures(vec![signature.clone(); offer.inputs.len()]);
        *offer = Sp::new(signed);
    }
    Ok(())
}

fn registration_id(
    signing_payload: &[u8],
) -> Result<WalletTransactionDraftId, WalletDustRegistrationPortError> {
    WalletTransactionDraftId::parse(format!(
        "dustreg_{}",
        hex::encode(Sha256::digest(signing_payload))
    ))
    .map_err(|_| WalletDustRegistrationPortError::InvalidData)
}

fn registration_challenge(
    draft_id: &WalletTransactionDraftId,
    signing_payload: &[u8],
) -> Result<WalletTransactionAuthorizationChallenge, WalletDustRegistrationPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:dust-registration-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(signing_payload);
    WalletTransactionAuthorizationChallenge::parse(format!(
        "dustauth_{}",
        hex::encode(digest.finalize())
    ))
    .map_err(|_| WalletDustRegistrationPortError::InvalidData)
}

fn registration_fingerprint(
    profile_id: &WalletProfileId,
    network_id: &oxid_wallet_domain::ChainNetworkId,
    account_id: &str,
    eligibility_fingerprint: &[u8; 32],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:dust-registration-plan:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(network_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(account_id.as_bytes());
    digest.update([0]);
    digest.update(eligibility_fingerprint);
    digest.finalize().into()
}

fn registration_eligibility_fingerprint(eligible: &[(MidnightSpendableUtxo, u128)]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:dust-registration-eligible-inputs:v1\0");
    for (utxo, _) in eligible {
        digest.update(utxo.intent_hash);
        digest.update(utxo.output_index.to_be_bytes());
        digest.update(utxo.value.to_be_bytes());
        digest.update(utxo.created_at_seconds.unwrap_or_default().to_be_bytes());
    }
    digest.finalize().into()
}

fn registration_status(
    retained: &RetainedMidnightDustRegistration,
) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
    let state = if retained.submission_state == WalletTransactionSubmissionState::Running {
        retained
            .submission_control
            .as_ref()
            .ok_or(WalletDustRegistrationPortError::InvalidData)?
            .public_state()
            .map_err(map_transaction_error)?
    } else {
        retained.submission_state
    };
    match retained.submission.as_ref() {
        Some(submission) if state == WalletTransactionSubmissionState::Included => {
            WalletDustRegistrationSubmissionStatus::included(submission.clone())
                .map_err(|_| WalletDustRegistrationPortError::InvalidData)
        }
        None => WalletDustRegistrationSubmissionStatus::pending(
            retained.preview.draft_id().clone(),
            state,
        )
        .map_err(|_| WalletDustRegistrationPortError::InvalidData),
        Some(_) => Err(WalletDustRegistrationPortError::InvalidData),
    }
}

fn registration_status_from_stored(
    entry: &StoredSubmissionJournalEntry,
) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
    let state = match entry.state {
        StoredSubmissionState::Broadcasting => WalletTransactionSubmissionState::Broadcasting,
        StoredSubmissionState::OutcomeUnknown => WalletTransactionSubmissionState::OutcomeUnknown,
        StoredSubmissionState::Included => WalletTransactionSubmissionState::Included,
        StoredSubmissionState::Rejected => WalletTransactionSubmissionState::Rejected,
        StoredSubmissionState::Expired => WalletTransactionSubmissionState::Expired,
    };
    let transaction_id = ChainTransactionId::parse(hex::encode(entry.transaction_hash))
        .map_err(|_| WalletDustRegistrationPortError::InvalidData)?;
    let fee_asset =
        midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
    let fee = AssetBalance::new(fee_asset, entry.fee_specks);
    match (entry.block_hash, entry.state) {
        (Some(block_hash), StoredSubmissionState::Included) => {
            let submission = WalletDustRegistrationSubmission::new(
                entry.draft_id.clone(),
                transaction_id,
                ChainBlockId::parse(hex::encode(block_hash))
                    .map_err(|_| WalletDustRegistrationPortError::InvalidData)?,
                fee,
                entry.mode,
            )
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)?;
            WalletDustRegistrationSubmissionStatus::included(submission)
                .map_err(|_| WalletDustRegistrationPortError::InvalidData)
        }
        (None, _) if entry.state != StoredSubmissionState::Included => {
            WalletDustRegistrationSubmissionStatus::recorded(
                entry.draft_id.clone(),
                state,
                transaction_id,
                fee,
                entry.mode,
            )
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)
        }
        _ => Err(WalletDustRegistrationPortError::InvalidData),
    }
}

fn finish_registration(
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    key: &(WalletProfileId, WalletTransactionDraftId),
    draft_id: WalletTransactionDraftId,
    control: &MidnightSubmissionControl,
    completion: Result<MidnightCompletionOutcome, WalletTransactionPortError>,
) -> Result<SubmittedWalletDustRegistration, WalletDustRegistrationPortError> {
    let outcome = match completion {
        Ok(outcome) => outcome,
        Err(WalletTransactionPortError::DraftExpired) => {
            expire_submission(drafts, key)?;
            return Err(WalletDustRegistrationPortError::DraftExpired);
        }
        Err(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
            let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
            mark_outcome_unknown(drafts, key)?;
            return Err(WalletDustRegistrationPortError::SubmissionOutcomeUnknown);
        }
        Err(WalletTransactionPortError::SubmissionCancelled) => {
            restore_authorized(drafts, key, WalletTransactionSubmissionState::Cancelled)?;
            return Err(WalletDustRegistrationPortError::SubmissionNotInProgress);
        }
        Err(WalletTransactionPortError::SubmissionRejected) => {
            if control.broadcast_started().map_err(map_transaction_error)? {
                control
                    .mark_terminal(StoredSubmissionState::Rejected, None, None)
                    .map_err(map_transaction_error)?;
                remove_retained(drafts, key)?;
            } else {
                restore_authorized(drafts, key, WalletTransactionSubmissionState::NotStarted)?;
            }
            return Err(WalletDustRegistrationPortError::SubmissionRejected);
        }
        Err(WalletTransactionPortError::InsufficientDust) => {
            restore_authorized(drafts, key, WalletTransactionSubmissionState::NotStarted)?;
            return Err(WalletDustRegistrationPortError::InsufficientRegistrationAllowance);
        }
        Err(error) => {
            if control.broadcast_started().map_err(map_transaction_error)? {
                let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
                mark_outcome_unknown(drafts, key)?;
                return Err(WalletDustRegistrationPortError::SubmissionOutcomeUnknown);
            }
            restore_authorized(drafts, key, WalletTransactionSubmissionState::NotStarted)?;
            return Err(map_transaction_error(error));
        }
    };
    if control
        .mark_terminal(
            StoredSubmissionState::Included,
            Some(outcome.block_hash),
            Some(outcome.block_height),
        )
        .is_err()
    {
        let _ = control.mark_terminal(StoredSubmissionState::OutcomeUnknown, None, None);
        mark_outcome_unknown(drafts, key)?;
        return Err(WalletDustRegistrationPortError::SubmissionOutcomeUnknown);
    }
    let fee_asset =
        midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
    let submission = WalletDustRegistrationSubmission::new(
        draft_id,
        ChainTransactionId::parse(hex::encode(outcome.transaction_hash))
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)?,
        ChainBlockId::parse(hex::encode(outcome.block_hash))
            .map_err(|_| WalletDustRegistrationPortError::InvalidData)?,
        AssetBalance::new(fee_asset, outcome.fee_specks),
        outcome.mode,
    )
    .map_err(|_| WalletDustRegistrationPortError::InvalidData)?;
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
    if retained.preview.state() != WalletTransactionDraftState::Submitting {
        return Err(WalletDustRegistrationPortError::DraftConflict);
    }
    retained.preview = retained
        .preview
        .with_fee_state(WalletTransactionFeeState::Final)
        .with_state(WalletTransactionDraftState::Submitted);
    retained.submission = Some(submission.clone());
    retained.submission_state = WalletTransactionSubmissionState::Included;
    retained.submission_control = None;
    retained.signed_transaction = None;
    Ok(SubmittedWalletDustRegistration {
        preview: retained.preview.clone(),
        submission,
    })
}

fn persist_registration_reconciliation(
    journal: &dyn crate::submission_journal::MidnightSubmissionJournalStore,
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    mut entry: StoredSubmissionJournalEntry,
    outcome: MidnightSubmissionReconciliation,
) -> Result<WalletDustRegistrationSubmissionStatus, WalletDustRegistrationPortError> {
    match outcome {
        MidnightSubmissionReconciliation::Included {
            block_hash,
            block_height,
        } => {
            entry.state = StoredSubmissionState::Included;
            entry.block_hash = Some(block_hash);
            entry.block_height = Some(block_height);
        }
        MidnightSubmissionReconciliation::Rejected => {
            entry.state = StoredSubmissionState::Rejected;
            entry.block_hash = None;
            entry.block_height = None;
        }
        MidnightSubmissionReconciliation::Expired => {
            entry.state = StoredSubmissionState::Expired;
            entry.block_hash = None;
            entry.block_height = None;
        }
        MidnightSubmissionReconciliation::Unresolved => {
            entry.state = StoredSubmissionState::OutcomeUnknown;
            entry.block_hash = None;
            entry.block_height = None;
        }
    }
    journal.save(&entry).map_err(map_store_error)?;
    let status = registration_status_from_stored(&entry)?;
    if let Ok(mut retained) = drafts.lock() {
        let key = (entry.profile_id.clone(), entry.draft_id.clone());
        match status.state() {
            WalletTransactionSubmissionState::Included => {
                if let (Some(draft), Some(submission)) =
                    (retained.get_mut(&key), status.submission())
                {
                    draft.submission_state = WalletTransactionSubmissionState::Included;
                    draft.submission = Some(submission.clone());
                    draft.submission_control = None;
                    draft.preview = draft
                        .preview
                        .with_fee_state(WalletTransactionFeeState::Final)
                        .with_state(WalletTransactionDraftState::Submitted);
                }
            }
            WalletTransactionSubmissionState::Rejected
            | WalletTransactionSubmissionState::Expired => {
                retained.remove(&key);
            }
            _ => {
                if let Some(draft) = retained.get_mut(&key) {
                    draft.submission_state = status.state();
                    draft.submission = status.submission().cloned();
                    draft.submission_control = None;
                }
            }
        }
    }
    Ok(status)
}

fn expire_retained(retained: &mut RetainedMidnightDustRegistration) {
    retained.preview = retained
        .preview
        .with_state(WalletTransactionDraftState::Expired);
    retained.signing_payload = Zeroizing::new(Vec::new());
    retained.signed_transaction = None;
    retained.submission_state = WalletTransactionSubmissionState::Expired;
    retained.submission_control = None;
}

fn expire_submission(
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletDustRegistrationPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
    expire_retained(retained);
    Ok(())
}

fn restore_authorized(
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    key: &(WalletProfileId, WalletTransactionDraftId),
    submission_state: WalletTransactionSubmissionState,
) -> Result<(), WalletDustRegistrationPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
        retained.submission_state = submission_state;
        retained.submission_control = None;
    }
    Ok(())
}

fn mark_outcome_unknown(
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletDustRegistrationPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletDustRegistrationPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletDustRegistrationPortError::DraftNotFound)?;
    retained.submission_state = WalletTransactionSubmissionState::OutcomeUnknown;
    retained.submission_control = None;
    Ok(())
}

fn remove_retained(
    drafts: &Mutex<
        HashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDustRegistration>,
    >,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletDustRegistrationPortError> {
    drafts
        .lock()
        .map_err(|_| WalletDustRegistrationPortError::Unavailable)?
        .remove(key)
        .map(|_| ())
        .ok_or(WalletDustRegistrationPortError::DraftNotFound)
}

const fn map_account_error(error: WalletAccountPortError) -> WalletDustRegistrationPortError {
    match error {
        WalletAccountPortError::ProtectionNotInitialized => {
            WalletDustRegistrationPortError::ProtectionNotInitialized
        }
        WalletAccountPortError::ProtectionLocked => {
            WalletDustRegistrationPortError::ProtectionLocked
        }
        WalletAccountPortError::NotFound => WalletDustRegistrationPortError::AccountNotDerived,
        WalletAccountPortError::UnsupportedNetwork => {
            WalletDustRegistrationPortError::InvalidChainState
        }
        WalletAccountPortError::Unavailable => WalletDustRegistrationPortError::Unavailable,
        WalletAccountPortError::InvalidData => WalletDustRegistrationPortError::InvalidData,
    }
}

const fn map_transaction_error(
    error: WalletTransactionPortError,
) -> WalletDustRegistrationPortError {
    match error {
        WalletTransactionPortError::Unavailable => WalletDustRegistrationPortError::Unavailable,
        WalletTransactionPortError::ProtectionNotInitialized => {
            WalletDustRegistrationPortError::ProtectionNotInitialized
        }
        WalletTransactionPortError::ProtectionLocked => {
            WalletDustRegistrationPortError::ProtectionLocked
        }
        WalletTransactionPortError::AccountNotDerived => {
            WalletDustRegistrationPortError::AccountNotDerived
        }
        WalletTransactionPortError::AccountNotSynchronized => {
            WalletDustRegistrationPortError::AccountNotSynchronized
        }
        WalletTransactionPortError::DraftNotFound => WalletDustRegistrationPortError::DraftNotFound,
        WalletTransactionPortError::DraftExpired => WalletDustRegistrationPortError::DraftExpired,
        WalletTransactionPortError::DraftConflict
        | WalletTransactionPortError::ShieldedStateNotCurrent
        | WalletTransactionPortError::UnsupportedNetwork
        | WalletTransactionPortError::InvalidRecipient
        | WalletTransactionPortError::RecipientNetworkMismatch
        | WalletTransactionPortError::InsufficientFunds => {
            WalletDustRegistrationPortError::DraftConflict
        }
        WalletTransactionPortError::SubmissionInProgress => {
            WalletDustRegistrationPortError::SubmissionInProgress
        }
        WalletTransactionPortError::SubmissionNotInProgress
        | WalletTransactionPortError::SubmissionCancelled => {
            WalletDustRegistrationPortError::SubmissionNotInProgress
        }
        WalletTransactionPortError::SubmissionCancellationUnsafe => {
            WalletDustRegistrationPortError::SubmissionCancellationUnsafe
        }
        WalletTransactionPortError::AuthorizationChallengeMismatch => {
            WalletDustRegistrationPortError::AuthorizationChallengeMismatch
        }
        WalletTransactionPortError::InsufficientDust => {
            WalletDustRegistrationPortError::InsufficientRegistrationAllowance
        }
        WalletTransactionPortError::InvalidChainState => {
            WalletDustRegistrationPortError::InvalidChainState
        }
        WalletTransactionPortError::ProvingFailed => WalletDustRegistrationPortError::ProvingFailed,
        WalletTransactionPortError::SubmissionRejected => {
            WalletDustRegistrationPortError::SubmissionRejected
        }
        WalletTransactionPortError::SubmissionOutcomeUnknown => {
            WalletDustRegistrationPortError::SubmissionOutcomeUnknown
        }
        WalletTransactionPortError::Timeout => WalletDustRegistrationPortError::Timeout,
        WalletTransactionPortError::InvalidData => WalletDustRegistrationPortError::InvalidData,
    }
}

const fn map_store_error(
    error: crate::submission_journal::SubmissionJournalStoreError,
) -> WalletDustRegistrationPortError {
    match error {
        crate::submission_journal::SubmissionJournalStoreError::Unavailable => {
            WalletDustRegistrationPortError::Unavailable
        }
        crate::submission_journal::SubmissionJournalStoreError::InvalidData => {
            WalletDustRegistrationPortError::InvalidData
        }
    }
}

#[cfg(test)]
mod tests {
    use midnight_ledger::structure::INITIAL_PARAMETERS;

    use super::*;

    fn utxo(
        value: u128,
        hash: u8,
        created: Option<u64>,
        registered: bool,
    ) -> MidnightSpendableUtxo {
        MidnightSpendableUtxo {
            value,
            intent_hash: [hash; 32],
            output_index: 0,
            created_at_seconds: created,
            registered_for_dust_generation: registered,
        }
    }

    #[test]
    fn generationless_allowance_is_capped_and_requires_authoritative_ctime() {
        let context = MidnightRegistrationContext {
            timestamp: Timestamp::from_secs(1_000_000),
            parameters: INITIAL_PARAMETERS,
        };
        let candidate = utxo(2, 1, Some(0), false);
        let generated = generated_dust(&candidate, 0, &context).expect("generation is valid");
        assert_eq!(
            generated,
            2_u128 * u128::from(INITIAL_PARAMETERS.dust.night_dust_ratio)
        );
        assert_eq!(
            generated_dust(&utxo(2, 1, Some(1_000_001), false), 1_000_001, &context),
            Ok(0)
        );
    }

    #[test]
    fn planner_rejects_only_registered_or_timestamp_unknown_night() {
        let context = MidnightRegistrationContext {
            timestamp: Timestamp::from_secs(1_700_000_100),
            parameters: INITIAL_PARAMETERS,
        };
        let night_key = midnight_base_crypto::schnorr::SigningKey::from_bytes(&[3; 32])
            .expect("test scalar is valid")
            .verifying_key();
        let dust = midnight_ledger::dust::DustSecretKey::derive_secret_key(&[4; 32]);
        let network =
            oxid_wallet_domain::ChainNetworkId::parse("undeployed").expect("network is valid");
        assert_eq!(
            plan_registration(
                &network,
                night_key.clone(),
                DustPublicKey::from(dust.clone()),
                vec![utxo(1_000_000, 1, Some(1_700_000_000), true)],
                &context,
                1_700_003_600,
            )
            .map(|_| ()),
            Err(WalletDustRegistrationPortError::RegistrationAlreadyCurrent)
        );
        assert_eq!(
            plan_registration(
                &network,
                night_key,
                DustPublicKey::from(dust),
                vec![utxo(1_000_000, 1, None, false)],
                &context,
                1_700_003_600,
            )
            .map(|_| ()),
            Err(WalletDustRegistrationPortError::InvalidChainState)
        );
    }
}
