// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap as StdHashMap,
    io::Cursor,
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use bech32::{Bech32m, primitives::decode::CheckedHrpstring};
use midnight_base_crypto::{
    hash::HashOutput,
    schnorr::{Signature, VerifyingKey},
    time::Timestamp,
};
use midnight_coin_structure::coin::{NIGHT, UserAddress};
use midnight_ledger::structure::{
    Intent, IntentHash, ProofPreimageMarker, StandardTransaction, Transaction, UnshieldedOffer,
    UtxoOutput, UtxoSpend,
};
use midnight_serialize::Deserializable;
use midnight_storage::{DefaultDB, arena::Sp, storage::HashMap as LedgerHashMap};
use midnight_transient_crypto::commitment::PedersenRandomness;
use oxid_wallet_application::{
    AuthorizeWalletTransferRequest, PrepareWalletTransferRequest, SubmitWalletTransferRequest,
    SubmittedWalletTransfer, WalletDerivedSecretUsePort, WalletHdPath, WalletHdPathComponent,
    WalletKeyOperationPort, WalletSecurityPortError, WalletTransactionPort,
    WalletTransactionPortError, WalletTransactionPortFuture,
};
use oxid_wallet_domain::{
    AssetBalance, ChainAddress, ChainBlockId, ChainNetwork, ChainTransactionId,
    DerivedChainAccount, MAX_WALLET_TRANSFER_INPUTS, PublicKeyEncoding, WalletKeyAlgorithm,
    WalletProfileId, WalletSignature, WalletTransactionAuthorizationChallenge,
    WalletTransactionDraftId, WalletTransactionDraftState, WalletTransactionFeeState,
    WalletTransferPreview, WalletTransferSubmission, WalletTransferSubmissionMode,
};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    BIP44_PURPOSE, MIDNIGHT_COIN_TYPE, MidnightWalletAdapter, ProtectedMidnightAccountDeriver,
    SPECKS_PER_DUST, STARS_PER_NIGHT, SimulatedMidnightAccountSource,
    UnavailableMidnightAccountDeriver, UnavailableMidnightAccountSource, midnight_asset,
    network_by_id,
};

const SEND_UNSHIELDED_SEGMENT: u16 = 0xCAFE;

type LedgerIntent = Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;
type LedgerTransaction = Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

const DUST_ROLE: u32 = 2;
const DUST_INDEX: u32 = 0;

#[derive(Clone)]
pub(crate) struct MidnightCompletionRequest {
    pub(crate) transaction: LedgerTransaction,
    pub(crate) expires_at_seconds: u64,
    cancellation: Arc<AtomicBool>,
}

impl MidnightCompletionRequest {
    pub(crate) fn cancellation_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancellation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightCompletionOutcome {
    pub(crate) fee_specks: u128,
    pub(crate) transaction_hash: [u8; 32],
    pub(crate) block_hash: [u8; 32],
    pub(crate) mode: WalletTransferSubmissionMode,
}

pub(crate) trait MidnightTransactionCompleter: Send + Sync {
    fn complete(
        &self,
        request: MidnightCompletionRequest,
        dust_seed: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnavailableMidnightTransactionCompleter;

impl MidnightTransactionCompleter for UnavailableMidnightTransactionCompleter {
    fn complete(
        &self,
        _: MidnightCompletionRequest,
        _: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SimulatedMidnightTransactionCompleter;

impl MidnightTransactionCompleter for SimulatedMidnightTransactionCompleter {
    fn complete(
        &self,
        request: MidnightCompletionRequest,
        _: &[u8; 32],
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        let mut encoded = Vec::new();
        midnight_serialize::tagged_serialize(&request.transaction, &mut encoded)
            .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let transaction_hash: [u8; 32] = Sha256::digest(&encoded).into();
        let mut block_digest = Sha256::new();
        block_digest.update(b"oxid:simulated-midnight-block:v1\0");
        block_digest.update(transaction_hash);
        Ok(MidnightCompletionOutcome {
            fee_specks: 1_000_000,
            transaction_hash,
            block_hash: block_digest.finalize().into(),
            mode: WalletTransferSubmissionMode::Simulated,
        })
    }
}

/// Exact native UTXO material retained behind the Midnight adapter boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightSpendableUtxo {
    pub(crate) value: u128,
    pub(crate) intent_hash: [u8; 32],
    pub(crate) output_index: u32,
}

/// A derived account and its latest synchronized native UTXO set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MidnightSpendableAccount {
    pub(crate) account: DerivedChainAccount,
    pub(crate) utxos: Vec<MidnightSpendableUtxo>,
}

/// Internal source capability required for canonical transfer planning.
pub(crate) trait MidnightTransactionSource: Send + Sync {
    fn spendable_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError>;
}

trait MidnightTransactionAuthorizer: Send + Sync {
    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        account: &DerivedChainAccount,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError>;

    fn use_dust_seed(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        operation: &mut dyn FnMut(
            &[u8; 32],
        )
            -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>;
}

/// Chain-specific draft state. Neither its signing payload nor transaction is
/// available through application or incoming-adapter views.
pub(crate) struct RetainedMidnightDraft {
    planning_fingerprint: [u8; 32],
    preview: WalletTransferPreview,
    account: DerivedChainAccount,
    signing_payload: Zeroizing<Vec<u8>>,
    unsigned_intent: LedgerIntent,
    signed_transaction: Option<LedgerTransaction>,
    submission: Option<WalletTransferSubmission>,
}

pub(crate) type RetainedMidnightDrafts =
    Mutex<StdHashMap<(WalletProfileId, WalletTransactionDraftId), RetainedMidnightDraft>>;

impl MidnightTransactionSource for UnavailableMidnightAccountSource {
    fn spendable_account(
        &self,
        _: &WalletProfileId,
        _: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

impl<C> MidnightTransactionSource for SimulatedMidnightAccountSource<C>
where
    C: oxid_platform_ports::ClockPort + 'static,
{
    fn spendable_account(
        &self,
        profile_id: &WalletProfileId,
        network: &ChainNetwork,
    ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
        let key = (profile_id.clone(), network.id().clone());
        let synchronized = self
            .synchronized
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .contains(&key);
        if !synchronized {
            return Err(WalletTransactionPortError::AccountNotSynchronized);
        }
        let account = self
            .derived_accounts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?
            .get(&key)
            .cloned()
            .ok_or(WalletTransactionPortError::AccountNotDerived)?;
        Ok(MidnightSpendableAccount {
            account,
            utxos: vec![
                simulated_utxo(STARS_PER_NIGHT, 1, 0),
                simulated_utxo(2 * STARS_PER_NIGHT, 2, 0),
                simulated_utxo(2 * STARS_PER_NIGHT, 3, 0),
            ],
        })
    }
}

fn simulated_utxo(value: u128, hash_byte: u8, output_index: u32) -> MidnightSpendableUtxo {
    MidnightSpendableUtxo {
        value,
        intent_hash: [hash_byte; 32],
        output_index,
    }
}

impl MidnightTransactionAuthorizer for UnavailableMidnightAccountDeriver {
    fn authorize(
        &self,
        _: &WalletProfileId,
        _: &DerivedChainAccount,
        _: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }

    fn use_dust_seed(
        &self,
        _: &WalletProfileId,
        _: u32,
        _: &mut dyn FnMut(
            &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        Err(WalletTransactionPortError::Unavailable)
    }
}

impl<K> MidnightTransactionAuthorizer for ProtectedMidnightAccountDeriver<K>
where
    K: WalletDerivedSecretUsePort + WalletKeyOperationPort + 'static,
{
    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        account: &DerivedChainAccount,
        payload: &[u8],
    ) -> Result<WalletSignature, WalletTransactionPortError> {
        self.keys
            .sign(profile_id, account.transaction_key(), payload)
            .map_err(map_security_error)
    }

    fn use_dust_seed(
        &self,
        profile_id: &WalletProfileId,
        account_index: u32,
        operation: &mut dyn FnMut(
            &[u8; 32],
        )
            -> Result<MidnightCompletionOutcome, WalletTransactionPortError>,
    ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
        let path = dust_path(account_index)?;
        let mut outcome = None;
        self.keys
            .use_derived_secret(profile_id, &path, &mut |secret| {
                outcome = Some(operation(secret));
                Ok(())
            })
            .map_err(map_security_error)?;
        outcome.ok_or(WalletTransactionPortError::InvalidData)?
    }
}

fn dust_path(account_index: u32) -> Result<WalletHdPath, WalletTransactionPortError> {
    let component = |index, hardened| {
        WalletHdPathComponent::new(index, hardened)
            .map_err(|_| WalletTransactionPortError::InvalidData)
    };
    WalletHdPath::new(vec![
        component(BIP44_PURPOSE, true)?,
        component(MIDNIGHT_COIN_TYPE, true)?,
        component(account_index, true)?,
        component(DUST_ROLE, false)?,
        component(DUST_INDEX, false)?,
    ])
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

impl<S, D> WalletTransactionPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer + Clone + 'static,
{
    fn prepare(
        &self,
        profile_id: &WalletProfileId,
        request: PrepareWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let selected = self.selected(profile_id).map_err(map_account_error)?;
        let network = network_by_id(&selected)
            .map_err(map_account_error)?
            .ok_or(WalletTransactionPortError::UnsupportedNetwork)?;
        let recipient = decode_recipient(&request.recipient, &selected)?;
        let spendable = self.source.spendable_account(profile_id, &network)?;
        validate_account(&spendable.account, &selected)?;

        let (selected_utxos, total) = select_utxos(spendable.utxos, request.amount_atomic_units)?;
        let change = total
            .checked_sub(request.amount_atomic_units)
            .ok_or(WalletTransactionPortError::InvalidData)?;
        let owner = decode_verifying_key(&spendable.account)?;
        let mut inputs = selected_utxos
            .iter()
            .map(|utxo| UtxoSpend {
                value: utxo.value,
                owner: owner.clone(),
                type_: NIGHT,
                intent_hash: IntentHash(HashOutput(utxo.intent_hash)),
                output_no: utxo.output_index,
            })
            .collect::<Vec<_>>();
        let mut outputs = vec![UtxoOutput {
            value: request.amount_atomic_units,
            owner: recipient,
            type_: NIGHT,
        }];
        if change > 0 {
            outputs.push(UtxoOutput {
                value: change,
                owner: UserAddress::from(owner),
                type_: NIGHT,
            });
        }
        inputs.sort();
        outputs.sort();

        let planning_fingerprint = planning_fingerprint(
            profile_id,
            &selected,
            &request,
            &spendable.account,
            &selected_utxos,
        );
        {
            let drafts = self
                .drafts
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            if let Some(existing) = drafts.iter().find_map(|((stored_profile, _), retained)| {
                (stored_profile == profile_id
                    && retained.planning_fingerprint == planning_fingerprint)
                    .then(|| retained.preview.clone())
            }) {
                return Ok(existing);
            }
        }

        let mut rng = OsRng;
        let offer: UnshieldedOffer<Signature, DefaultDB> = UnshieldedOffer {
            inputs: inputs.into(),
            outputs: outputs.into(),
            signatures: Vec::new().into(),
        };
        let mut intent = LedgerIntent::empty(
            &mut rng,
            Timestamp::from_secs(request.expires_at.value() / 1_000),
        );
        intent.guaranteed_unshielded_offer = Some(Sp::new(offer));
        let signing_payload = intent
            .erase_proofs()
            .erase_signatures()
            .data_to_sign(SEND_UNSHIELDED_SEGMENT);
        let draft_id = digest_id("txdraft", &signing_payload)?;
        let challenge = authorization_challenge(&draft_id, &signing_payload)?;
        let night = midnight_asset("midnight:night", "NIGHT", STARS_PER_NIGHT)
            .map_err(map_account_error)?;
        let preview = WalletTransferPreview::new(
            draft_id.clone(),
            challenge,
            selected,
            spendable.account.account_id().clone(),
            request.recipient,
            AssetBalance::new(night.clone(), request.amount_atomic_units),
            AssetBalance::new(night, change),
            None,
            WalletTransactionFeeState::RequiresBalancing,
            u16::try_from(selected_utxos.len())
                .map_err(|_| WalletTransactionPortError::InvalidData)?,
            request.expires_at,
            WalletTransactionDraftState::Prepared,
        )
        .map_err(|_| WalletTransactionPortError::InvalidData)?;
        let retained = RetainedMidnightDraft {
            planning_fingerprint,
            preview: preview.clone(),
            account: spendable.account,
            signing_payload: Zeroizing::new(signing_payload),
            unsigned_intent: intent,
            signed_transaction: None,
            submission: None,
        };
        let key = (profile_id.clone(), draft_id);
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        if let Some(existing) = drafts.iter().find_map(|((stored_profile, _), retained)| {
            (stored_profile == profile_id && retained.planning_fingerprint == planning_fingerprint)
                .then(|| retained.preview.clone())
        }) {
            return Ok(existing);
        }
        if let Some(existing) = drafts.get(&key) {
            return if existing.preview == preview {
                Ok(existing.preview.clone())
            } else {
                Err(WalletTransactionPortError::DraftConflict)
            };
        }
        drafts.insert(key, retained);
        Ok(preview)
    }

    fn authorize(
        &self,
        profile_id: &WalletProfileId,
        request: AuthorizeWalletTransferRequest,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let key = (profile_id.clone(), request.draft_id.clone());
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&key)
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        if retained.preview.authorization_challenge() != &request.authorization_challenge {
            return Err(WalletTransactionPortError::AuthorizationChallengeMismatch);
        }
        if request.now.value() >= retained.preview.expires_at().value() {
            retained.preview = retained
                .preview
                .with_state(WalletTransactionDraftState::Expired);
            retained.signing_payload = Zeroizing::new(Vec::new());
            retained.signed_transaction = None;
            return Err(WalletTransactionPortError::DraftExpired);
        }
        if matches!(
            retained.preview.state(),
            WalletTransactionDraftState::Authorized
                | WalletTransactionDraftState::Submitting
                | WalletTransactionDraftState::Submitted
        ) {
            return Ok(retained.preview.clone());
        }

        let signature = self.deriver.authorize(
            profile_id,
            &retained.account,
            retained.signing_payload.as_slice(),
        )?;
        if signature.algorithm() != WalletKeyAlgorithm::Secp256k1Schnorr {
            return Err(WalletTransactionPortError::InvalidData);
        }
        let ledger_signature = decode_signature(&signature)?;
        let verifying_key = decode_verifying_key(&retained.account)?;
        if !verifying_key.verify(retained.signing_payload.as_slice(), &ledger_signature) {
            return Err(WalletTransactionPortError::InvalidData);
        }

        let mut signed = retained.unsigned_intent.clone();
        let offer = signed
            .guaranteed_unshielded_offer
            .as_ref()
            .ok_or(WalletTransactionPortError::InvalidData)?;
        let input_count = offer.inputs.len();
        let mut signed_offer = offer.deref().clone();
        signed_offer.add_signatures(vec![ledger_signature; input_count]);
        signed.guaranteed_unshielded_offer = Some(Sp::new(signed_offer));
        let mut intents = LedgerHashMap::new();
        intents = intents.insert(SEND_UNSHIELDED_SEGMENT, signed);
        let transaction = StandardTransaction::new(
            retained.preview.network_id().as_str(),
            intents,
            None,
            LedgerHashMap::new(),
        );
        retained.signed_transaction = Some(Transaction::Standard(transaction));
        retained.signing_payload = Zeroizing::new(Vec::new());
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
        Ok(retained.preview.clone())
    }

    fn submit<'a>(
        &'a self,
        profile_id: &'a WalletProfileId,
        request: SubmitWalletTransferRequest,
    ) -> WalletTransactionPortFuture<'a> {
        Box::pin(async move {
            let key = (profile_id.clone(), request.draft_id.clone());
            let (transaction, account_index, expires_at_seconds) = {
                let mut drafts = self
                    .drafts
                    .lock()
                    .map_err(|_| WalletTransactionPortError::Unavailable)?;
                let retained = drafts
                    .get_mut(&key)
                    .ok_or(WalletTransactionPortError::DraftNotFound)?;
                match retained.preview.state() {
                    WalletTransactionDraftState::Submitted => {
                        let submission = retained
                            .submission
                            .clone()
                            .ok_or(WalletTransactionPortError::InvalidData)?;
                        return Ok(SubmittedWalletTransfer {
                            preview: retained.preview.clone(),
                            submission,
                        });
                    }
                    WalletTransactionDraftState::Submitting => {
                        return Err(WalletTransactionPortError::SubmissionInProgress);
                    }
                    WalletTransactionDraftState::Prepared
                    | WalletTransactionDraftState::Authorized => {}
                    WalletTransactionDraftState::Expired => {
                        return Err(WalletTransactionPortError::DraftExpired);
                    }
                }
                if request.now.value() >= retained.preview.expires_at().value() {
                    retained.preview = retained
                        .preview
                        .with_state(WalletTransactionDraftState::Expired);
                    retained.signing_payload = Zeroizing::new(Vec::new());
                    retained.signed_transaction = None;
                    return Err(WalletTransactionPortError::DraftExpired);
                }
                if retained.preview.state() == WalletTransactionDraftState::Prepared {
                    return Err(WalletTransactionPortError::DraftConflict);
                }
                let transaction = retained
                    .signed_transaction
                    .clone()
                    .ok_or(WalletTransactionPortError::InvalidData)?;
                retained.preview = retained
                    .preview
                    .with_state(WalletTransactionDraftState::Submitting);
                (
                    transaction,
                    retained.account.account_index(),
                    retained.preview.expires_at().value() / 1_000,
                )
            };

            let profile = profile_id.clone();
            let deriver = self.deriver.clone();
            let completer = Arc::clone(&self.completer);
            let drafts = Arc::clone(&self.drafts);
            let worker_key = key.clone();
            let draft_id = request.draft_id;
            let cancellation = Arc::new(AtomicBool::new(false));
            let mut cancel_on_drop = CancelSubmissionOnDrop::new(Arc::clone(&cancellation));
            let (sender, receiver) = futures::channel::oneshot::channel();
            let spawn = thread::Builder::new()
                .name("oxid-midnight-submit".to_owned())
                .spawn(move || {
                    let mut operation = |dust_seed: &[u8; 32]| {
                        completer.complete(
                            MidnightCompletionRequest {
                                transaction: transaction.clone(),
                                expires_at_seconds,
                                cancellation: Arc::clone(&cancellation),
                            },
                            dust_seed,
                        )
                    };
                    let completion = deriver.use_dust_seed(&profile, account_index, &mut operation);
                    let result =
                        finish_submission(drafts.as_ref(), &worker_key, draft_id, completion);
                    let _ = sender.send(result);
                });
            if spawn.is_err() {
                cancel_on_drop.disarm();
                restore_authorized(self.drafts.as_ref(), &key)?;
                return Err(WalletTransactionPortError::Unavailable);
            }

            let result = receiver
                .await
                .unwrap_or(Err(WalletTransactionPortError::SubmissionOutcomeUnknown));
            cancel_on_drop.disarm();
            result
        })
    }

    fn get(
        &self,
        profile_id: &WalletProfileId,
        draft_id: &WalletTransactionDraftId,
        now: oxid_foundation::UnixTimestampMillis,
    ) -> Result<WalletTransferPreview, WalletTransactionPortError> {
        let key = (profile_id.clone(), draft_id.clone());
        let mut drafts = self
            .drafts
            .lock()
            .map_err(|_| WalletTransactionPortError::Unavailable)?;
        let retained = drafts
            .get_mut(&key)
            .ok_or(WalletTransactionPortError::DraftNotFound)?;
        if now.value() >= retained.preview.expires_at().value()
            && matches!(
                retained.preview.state(),
                WalletTransactionDraftState::Prepared | WalletTransactionDraftState::Authorized
            )
        {
            retained.preview = retained
                .preview
                .with_state(WalletTransactionDraftState::Expired);
            retained.signing_payload = Zeroizing::new(Vec::new());
            retained.signed_transaction = None;
        }
        Ok(retained.preview.clone())
    }
}

struct CancelSubmissionOnDrop {
    cancellation: Arc<AtomicBool>,
    armed: bool,
}

impl CancelSubmissionOnDrop {
    fn new(cancellation: Arc<AtomicBool>) -> Self {
        Self {
            cancellation,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelSubmissionOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.cancellation.store(true, Ordering::Release);
        }
    }
}

fn finish_submission(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
    draft_id: WalletTransactionDraftId,
    completion: Result<MidnightCompletionOutcome, WalletTransactionPortError>,
) -> Result<SubmittedWalletTransfer, WalletTransactionPortError> {
    let outcome = match completion {
        Ok(outcome) => outcome,
        Err(WalletTransactionPortError::DraftExpired) => {
            expire_submission(drafts, key)?;
            return Err(WalletTransactionPortError::DraftExpired);
        }
        Err(WalletTransactionPortError::SubmissionOutcomeUnknown) => {
            return Err(WalletTransactionPortError::SubmissionOutcomeUnknown);
        }
        Err(error) => {
            restore_authorized(drafts, key)?;
            return Err(error);
        }
    };
    let fee_asset =
        midnight_asset("midnight:dust", "DUST", SPECKS_PER_DUST).map_err(map_account_error)?;
    let fee = AssetBalance::new(fee_asset, outcome.fee_specks);
    let submission = WalletTransferSubmission::new(
        draft_id,
        ChainTransactionId::parse(hex::encode(outcome.transaction_hash))
            .map_err(|_| WalletTransactionPortError::InvalidData)?,
        ChainBlockId::parse(hex::encode(outcome.block_hash))
            .map_err(|_| WalletTransactionPortError::InvalidData)?,
        fee.clone(),
        outcome.mode,
    );
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() != WalletTransactionDraftState::Submitting {
        return Err(WalletTransactionPortError::DraftConflict);
    }
    retained.preview = retained
        .preview
        .with_final_fee(fee)
        .with_state(WalletTransactionDraftState::Submitted);
    retained.submission = Some(submission.clone());
    retained.signed_transaction = None;
    Ok(SubmittedWalletTransfer {
        preview: retained.preview.clone(),
        submission,
    })
}

fn restore_authorized(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Authorized);
    }
    Ok(())
}

fn expire_submission(
    drafts: &RetainedMidnightDrafts,
    key: &(WalletProfileId, WalletTransactionDraftId),
) -> Result<(), WalletTransactionPortError> {
    let mut drafts = drafts
        .lock()
        .map_err(|_| WalletTransactionPortError::Unavailable)?;
    let retained = drafts
        .get_mut(key)
        .ok_or(WalletTransactionPortError::DraftNotFound)?;
    if retained.preview.state() == WalletTransactionDraftState::Submitting {
        retained.preview = retained
            .preview
            .with_state(WalletTransactionDraftState::Expired);
        retained.signing_payload = Zeroizing::new(Vec::new());
        retained.signed_transaction = None;
    }
    Ok(())
}

fn validate_account(
    account: &DerivedChainAccount,
    network_id: &oxid_wallet_domain::ChainNetworkId,
) -> Result<(), WalletTransactionPortError> {
    if account.network_id() != network_id {
        return Err(WalletTransactionPortError::DraftConflict);
    }
    if account.transaction_public_key().encoding() != PublicKeyEncoding::Secp256k1XOnly
        || account.transaction_public_key().bytes().len() != 32
    {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Ok(())
}

fn select_utxos(
    mut utxos: Vec<MidnightSpendableUtxo>,
    amount: u128,
) -> Result<(Vec<MidnightSpendableUtxo>, u128), WalletTransactionPortError> {
    // Match the prototype's greedy picker: largest native UTXOs first, with
    // stable identity tie-breakers so the retained intent is reproducible.
    utxos.sort_by(|left, right| {
        right
            .value
            .cmp(&left.value)
            .then_with(|| left.intent_hash.cmp(&right.intent_hash))
            .then_with(|| left.output_index.cmp(&right.output_index))
    });
    let mut selected = Vec::new();
    let mut total = 0_u128;
    for utxo in utxos {
        total = total
            .checked_add(utxo.value)
            .ok_or(WalletTransactionPortError::InvalidData)?;
        selected.push(utxo);
        if selected.len() > usize::from(MAX_WALLET_TRANSFER_INPUTS) {
            return Err(WalletTransactionPortError::InvalidData);
        }
        if total >= amount {
            return Ok((selected, total));
        }
    }
    Err(WalletTransactionPortError::InsufficientFunds)
}

fn decode_verifying_key(
    account: &DerivedChainAccount,
) -> Result<VerifyingKey, WalletTransactionPortError> {
    validate_account(account, account.network_id())?;
    VerifyingKey::deserialize(
        &mut Cursor::new(account.transaction_public_key().bytes()),
        0,
    )
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn decode_signature(signature: &WalletSignature) -> Result<Signature, WalletTransactionPortError> {
    if signature.bytes().len() != 64 {
        return Err(WalletTransactionPortError::InvalidData);
    }
    Signature::deserialize(&mut Cursor::new(signature.bytes()), 0)
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn decode_recipient(
    address: &ChainAddress,
    network_id: &oxid_wallet_domain::ChainNetworkId,
) -> Result<UserAddress, WalletTransactionPortError> {
    let decoded = CheckedHrpstring::new::<Bech32m>(address.value())
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    let expected = if network_id.as_str() == "mainnet" {
        "mn_addr".to_owned()
    } else {
        format!("mn_addr_{}", network_id.as_str())
    };
    if decoded.hrp().as_str() != expected {
        return Err(WalletTransactionPortError::RecipientNetworkMismatch);
    }
    let payload = decoded.byte_iter().collect::<Vec<_>>();
    let bytes: [u8; 32] = payload
        .try_into()
        .map_err(|_| WalletTransactionPortError::InvalidRecipient)?;
    Ok(UserAddress(HashOutput(bytes)))
}

fn planning_fingerprint(
    profile_id: &WalletProfileId,
    network_id: &oxid_wallet_domain::ChainNetworkId,
    request: &PrepareWalletTransferRequest,
    account: &DerivedChainAccount,
    utxos: &[MidnightSpendableUtxo],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:transfer-plan:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(network_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(account.account_id().as_str().as_bytes());
    digest.update([0]);
    digest.update(request.recipient.value().as_bytes());
    digest.update(request.amount_atomic_units.to_be_bytes());
    digest.update(request.expires_at.value().to_be_bytes());
    for utxo in utxos {
        digest.update(utxo.value.to_be_bytes());
        digest.update(utxo.intent_hash);
        digest.update(utxo.output_index.to_be_bytes());
    }
    digest.finalize().into()
}

fn digest_id(
    prefix: &str,
    payload: &[u8],
) -> Result<WalletTransactionDraftId, WalletTransactionPortError> {
    let digest = Sha256::digest(payload);
    WalletTransactionDraftId::parse(format!("{prefix}_{}", hex::encode(digest)))
        .map_err(|_| WalletTransactionPortError::InvalidData)
}

fn authorization_challenge(
    draft_id: &WalletTransactionDraftId,
    payload: &[u8],
) -> Result<WalletTransactionAuthorizationChallenge, WalletTransactionPortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:midnight:transfer-authorization:v1\0");
    digest.update(draft_id.as_str().as_bytes());
    digest.update(payload);
    WalletTransactionAuthorizationChallenge::parse(format!(
        "txauth_{}",
        hex::encode(digest.finalize())
    ))
    .map_err(|_| WalletTransactionPortError::InvalidData)
}

const fn map_security_error(error: WalletSecurityPortError) -> WalletTransactionPortError {
    match error {
        WalletSecurityPortError::NotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        WalletSecurityPortError::Locked => WalletTransactionPortError::ProtectionLocked,
        WalletSecurityPortError::Unavailable => WalletTransactionPortError::Unavailable,
        WalletSecurityPortError::NotFound => WalletTransactionPortError::DraftConflict,
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::Conflict
        | WalletSecurityPortError::UnsupportedAlgorithm
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => WalletTransactionPortError::InvalidData,
    }
}

const fn map_account_error(
    error: oxid_wallet_application::WalletAccountPortError,
) -> WalletTransactionPortError {
    match error {
        oxid_wallet_application::WalletAccountPortError::Unavailable => {
            WalletTransactionPortError::Unavailable
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionNotInitialized => {
            WalletTransactionPortError::ProtectionNotInitialized
        }
        oxid_wallet_application::WalletAccountPortError::ProtectionLocked => {
            WalletTransactionPortError::ProtectionLocked
        }
        oxid_wallet_application::WalletAccountPortError::UnsupportedNetwork => {
            WalletTransactionPortError::UnsupportedNetwork
        }
        oxid_wallet_application::WalletAccountPortError::NotFound => {
            WalletTransactionPortError::AccountNotDerived
        }
        oxid_wallet_application::WalletAccountPortError::InvalidData => {
            WalletTransactionPortError::InvalidData
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Condvar, Mutex, mpsc},
        task::{Context, Poll, Waker},
        time::{Duration, Instant},
    };

    use midnight_base_crypto::schnorr::SigningKey;
    use midnight_serialize::Serializable;
    use oxid_foundation::UnixTimestampMillis;
    use oxid_wallet_application::WalletTransactionPort;
    use oxid_wallet_domain::{
        ChainAccountId, ChainAddressKind, PublicKeyEncoding, WalletKeyReference, WalletPublicKey,
    };

    use super::*;
    use crate::{UnavailableMidnightAccountDeriver, fixture_addresses, network_id};

    struct FixedSpendableSource {
        account: DerivedChainAccount,
    }

    #[derive(Clone)]
    struct FixedAuthorizer {
        signing_key: SigningKey,
    }

    impl MidnightTransactionAuthorizer for FixedAuthorizer {
        fn authorize(
            &self,
            _: &WalletProfileId,
            _: &DerivedChainAccount,
            payload: &[u8],
        ) -> Result<WalletSignature, WalletTransactionPortError> {
            let signature = self.signing_key.sign(&mut OsRng, payload);
            let mut bytes = Vec::new();
            signature
                .serialize(&mut bytes)
                .map_err(|_| WalletTransactionPortError::InvalidData)?;
            Ok(WalletSignature::new(
                WalletKeyAlgorithm::Secp256k1Schnorr,
                bytes,
            ))
        }

        fn use_dust_seed(
            &self,
            _: &WalletProfileId,
            _: u32,
            operation: &mut dyn FnMut(
                &[u8; 32],
            ) -> Result<
                MidnightCompletionOutcome,
                WalletTransactionPortError,
            >,
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            operation(&[0x5a; 32])
        }
    }

    struct FailingCompleter;

    impl MidnightTransactionCompleter for FailingCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            Err(WalletTransactionPortError::ProvingFailed)
        }
    }

    struct UnknownOutcomeCompleter;

    impl MidnightTransactionCompleter for UnknownOutcomeCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            Err(WalletTransactionPortError::SubmissionOutcomeUnknown)
        }
    }

    struct PanickingCompleter;

    impl MidnightTransactionCompleter for PanickingCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            panic!("test-only unexpected worker termination")
        }
    }

    struct BlockingCompleter {
        started: mpsc::SyncSender<()>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }

    impl MidnightTransactionCompleter for BlockingCompleter {
        fn complete(
            &self,
            _: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            self.started
                .send(())
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let (lock, condition) = self.release.as_ref();
            let mut released = lock
                .lock()
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            while !*released {
                released = condition
                    .wait(released)
                    .map_err(|_| WalletTransactionPortError::Unavailable)?;
            }
            Ok(MidnightCompletionOutcome {
                fee_specks: 1,
                transaction_hash: [1; 32],
                block_hash: [2; 32],
                mode: WalletTransferSubmissionMode::Simulated,
            })
        }
    }

    struct CancellationAwareCompleter {
        started: mpsc::SyncSender<()>,
    }

    impl MidnightTransactionCompleter for CancellationAwareCompleter {
        fn complete(
            &self,
            request: MidnightCompletionRequest,
            _: &[u8; 32],
        ) -> Result<MidnightCompletionOutcome, WalletTransactionPortError> {
            self.started
                .send(())
                .map_err(|_| WalletTransactionPortError::Unavailable)?;
            let cancellation = request.cancellation_token();
            let deadline = Instant::now() + Duration::from_secs(1);
            while !cancellation.load(Ordering::Acquire) {
                if Instant::now() >= deadline {
                    return Err(WalletTransactionPortError::Unavailable);
                }
                std::thread::yield_now();
            }
            Err(WalletTransactionPortError::SubmissionCancelled)
        }
    }

    impl MidnightTransactionSource for FixedSpendableSource {
        fn spendable_account(
            &self,
            _: &WalletProfileId,
            _: &ChainNetwork,
        ) -> Result<MidnightSpendableAccount, WalletTransactionPortError> {
            Ok(MidnightSpendableAccount {
                account: self.account.clone(),
                utxos: vec![
                    simulated_utxo(STARS_PER_NIGHT, 1, 0),
                    simulated_utxo(2 * STARS_PER_NIGHT, 2, 0),
                    simulated_utxo(2 * STARS_PER_NIGHT, 3, 0),
                ],
            })
        }
    }

    fn adapter() -> MidnightWalletAdapter<FixedSpendableSource, UnavailableMidnightAccountDeriver> {
        let network = network_id("undeployed").expect("network is valid");
        let address = fixture_addresses(&network)
            .expect("fixture addresses encode")
            .remove(0);
        let public_key =
            hex::decode("b193e54524dc796402870a883fbdcd83869c9c307dda8c0d99c5f769169fc883")
                .expect("public key vector is valid");
        let account = DerivedChainAccount::new(
            network,
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            0,
            0,
            address,
            WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, public_key),
            WalletKeyReference::parse("key_test").expect("key reference is valid"),
        )
        .expect("derived account is valid");
        MidnightWalletAdapter::with_deriver(
            FixedSpendableSource { account },
            UnavailableMidnightAccountDeriver,
        )
    }

    fn submittable_adapter(
        completer: Arc<dyn MidnightTransactionCompleter>,
    ) -> MidnightWalletAdapter<FixedSpendableSource, FixedAuthorizer> {
        let network = network_id("undeployed").expect("network is valid");
        let address = fixture_addresses(&network)
            .expect("fixture addresses encode")
            .remove(0);
        let signing_key = SigningKey::from_bytes(&[3; 32]).expect("test scalar is valid");
        let mut public_key = Vec::new();
        signing_key
            .verifying_key()
            .serialize(&mut public_key)
            .expect("verifying key serializes");
        let account = DerivedChainAccount::new(
            network,
            ChainAccountId::parse("midnight_account_0_0").expect("account id is valid"),
            0,
            0,
            address,
            WalletPublicKey::new(PublicKeyEncoding::Secp256k1XOnly, public_key),
            WalletKeyReference::parse("key_test").expect("key reference is valid"),
        )
        .expect("derived account is valid");
        MidnightWalletAdapter::with_deriver_and_completer(
            FixedSpendableSource { account },
            FixedAuthorizer { signing_key },
            completer,
        )
    }

    fn authorize_transfer(
        adapter: &MidnightWalletAdapter<FixedSpendableSource, FixedAuthorizer>,
    ) -> WalletTransferPreview {
        let prepared = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        adapter
            .authorize(
                &profile(),
                AuthorizeWalletTransferRequest {
                    draft_id: prepared.draft_id().clone(),
                    authorization_challenge: prepared.authorization_challenge().clone(),
                    now: UnixTimestampMillis::new(1_000),
                },
            )
            .expect("transfer authorizes")
    }

    fn profile() -> WalletProfileId {
        WalletProfileId::parse("profile_test").expect("profile is valid")
    }

    fn request(expires_at: u64) -> PrepareWalletTransferRequest {
        let recipient = fixture_addresses(&network_id("undeployed").expect("network is valid"))
            .expect("fixture addresses encode")
            .remove(0);
        assert_eq!(recipient.kind(), ChainAddressKind::Unshielded);
        PrepareWalletTransferRequest {
            recipient,
            amount_atomic_units: 1_500_000,
            expires_at: UnixTimestampMillis::new(expires_at),
        }
    }

    #[test]
    fn planning_matches_prototype_greedy_selection_and_is_idempotent() {
        let adapter = adapter();
        let first = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        let repeated = adapter
            .prepare(&profile(), request(2_000))
            .expect("same transfer is idempotent");

        assert_eq!(first.input_count(), 1);
        assert_eq!(first.change().atomic_units(), 500_000);
        assert_eq!(first.draft_id(), repeated.draft_id());
        assert_eq!(
            first.authorization_challenge(),
            repeated.authorization_challenge()
        );
    }

    #[test]
    fn retained_material_expires_without_becoming_submission_ready() {
        let adapter = adapter();
        let prepared = adapter
            .prepare(&profile(), request(2_000))
            .expect("transfer prepares");
        let expired = adapter
            .get(
                &profile(),
                prepared.draft_id(),
                UnixTimestampMillis::new(2_000),
            )
            .expect("safe expired state is readable");

        assert_eq!(expired.state(), WalletTransactionDraftState::Expired);
        assert_eq!(
            expired.fee_state(),
            WalletTransactionFeeState::RequiresBalancing
        );
    }

    #[test]
    fn selection_rejects_insufficient_oversized_and_overflowing_inputs() {
        assert_eq!(
            select_utxos(vec![simulated_utxo(1, 1, 0)], 2),
            Err(WalletTransactionPortError::InsufficientFunds)
        );

        let oversized = (0..=MAX_WALLET_TRANSFER_INPUTS)
            .map(|index| simulated_utxo(1, 1, u32::from(index)))
            .collect();
        assert_eq!(
            select_utxos(oversized, u128::from(MAX_WALLET_TRANSFER_INPUTS) + 2),
            Err(WalletTransactionPortError::InvalidData)
        );

        assert_eq!(
            select_utxos(
                vec![simulated_utxo(u128::MAX - 1, 1, 0), simulated_utxo(2, 2, 0),],
                u128::MAX,
            ),
            Err(WalletTransactionPortError::InvalidData)
        );
    }

    #[test]
    fn dust_witness_uses_the_wallet_sdk_role_two_child() {
        let path = dust_path(7).expect("DUST path is valid");
        let components = path
            .components()
            .iter()
            .map(|component| (component.index(), component.hardened()))
            .collect::<Vec<_>>();

        assert_eq!(
            components,
            vec![(44, true), (2400, true), (7, true), (2, false), (0, false)]
        );
    }

    #[test]
    fn simulated_submission_is_final_and_idempotent() {
        let adapter = submittable_adapter(Arc::new(SimulatedMidnightTransactionCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let first = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect("transfer submits");
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect("submitted transfer is idempotent");
        let repeated_after_draft_ttl = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(3_000),
            },
        ))
        .expect("completed outcome remains idempotent after the draft TTL");

        assert_eq!(
            first.preview.state(),
            WalletTransactionDraftState::Submitted
        );
        assert_eq!(first.preview.fee_state(), WalletTransactionFeeState::Final);
        assert_eq!(
            first.submission.mode(),
            WalletTransferSubmissionMode::Simulated
        );
        assert_eq!(first.submission, repeated.submission);
        assert_eq!(first.submission, repeated_after_draft_ttl.submission);
    }

    #[test]
    fn completion_failure_restores_authorized_state_for_retry() {
        let adapter = submittable_adapter(Arc::new(FailingCompleter));
        let authorized = authorize_transfer(&adapter);
        let error = futures::executor::block_on(adapter.submit(
            &profile(),
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        ))
        .expect_err("proving failure is returned");

        assert_eq!(error, WalletTransactionPortError::ProvingFailed);
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000)
                )
                .expect("draft remains readable")
                .state(),
            WalletTransactionDraftState::Authorized
        );
    }

    #[test]
    fn unknown_node_outcome_cannot_be_retried_as_a_second_send() {
        let adapter = submittable_adapter(Arc::new(UnknownOutcomeCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let error = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect_err("unknown node outcome is returned");
        assert_eq!(error, WalletTransactionPortError::SubmissionOutcomeUnknown);
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("ambiguous draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect_err("ambiguous submission cannot be sent again");
        assert_eq!(repeated, WalletTransactionPortError::SubmissionInProgress);
    }

    #[test]
    fn unexpected_worker_termination_is_an_unknown_non_retryable_outcome() {
        let adapter = submittable_adapter(Arc::new(PanickingCompleter));
        let authorized = authorize_transfer(&adapter);
        let request = SubmitWalletTransferRequest {
            draft_id: authorized.draft_id().clone(),
            now: UnixTimestampMillis::new(1_000),
        };
        let error = futures::executor::block_on(adapter.submit(&profile(), request.clone()))
            .expect_err("worker termination is returned as an unknown outcome");
        assert_eq!(error, WalletTransactionPortError::SubmissionOutcomeUnknown);
        assert_eq!(
            adapter
                .get(
                    &profile(),
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("worker-owned draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let repeated = futures::executor::block_on(adapter.submit(&profile(), request))
            .expect_err("unknown worker outcome cannot be sent again");
        assert_eq!(repeated, WalletTransactionPortError::SubmissionInProgress);
    }

    #[test]
    fn cancelling_submission_future_leaves_the_worker_owning_the_final_transition() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let adapter = submittable_adapter(Arc::new(BlockingCompleter {
            started: started_sender,
            release: Arc::clone(&release),
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion worker starts");
        drop(future);

        assert_eq!(
            adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000)
                )
                .expect("cancelled draft remains readable")
                .state(),
            WalletTransactionDraftState::Submitting
        );
        let (lock, condition) = release.as_ref();
        *lock.lock().expect("release lock is available") = true;
        condition.notify_one();

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let state = adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("worker-owned draft remains readable")
                .state();
            if state == WalletTransactionDraftState::Submitted {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "submission worker did not publish its final state"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn cancellation_aware_completion_restores_the_authorized_draft() {
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let adapter = submittable_adapter(Arc::new(CancellationAwareCompleter {
            started: started_sender,
        }));
        let authorized = authorize_transfer(&adapter);
        let profile = profile();
        let mut future = adapter.submit(
            &profile,
            SubmitWalletTransferRequest {
                draft_id: authorized.draft_id().clone(),
                now: UnixTimestampMillis::new(1_000),
            },
        );
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("completion worker starts");
        drop(future);

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let state = adapter
                .get(
                    &profile,
                    authorized.draft_id(),
                    UnixTimestampMillis::new(1_000),
                )
                .expect("cancelled draft remains readable")
                .state();
            if state == WalletTransactionDraftState::Authorized {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cancelled worker did not restore the authorized state"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn custody_errors_preserve_actionable_transaction_state() {
        assert_eq!(
            map_security_error(WalletSecurityPortError::NotInitialized),
            WalletTransactionPortError::ProtectionNotInitialized
        );
        assert_eq!(
            map_security_error(WalletSecurityPortError::Locked),
            WalletTransactionPortError::ProtectionLocked
        );
        assert_eq!(
            map_security_error(WalletSecurityPortError::AuthorizationDenied),
            WalletTransactionPortError::InvalidData
        );
    }
}
