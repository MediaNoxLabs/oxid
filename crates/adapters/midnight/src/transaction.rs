// SPDX-License-Identifier: Apache-2.0

use std::{io::Cursor, ops::Deref};

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
    AuthorizeWalletTransferRequest, PrepareWalletTransferRequest, WalletKeyOperationPort,
    WalletSecurityPortError, WalletTransactionPort, WalletTransactionPortError,
};
use oxid_wallet_domain::{
    AssetBalance, ChainAddress, ChainNetwork, DerivedChainAccount, MAX_WALLET_TRANSFER_INPUTS,
    PublicKeyEncoding, WalletKeyAlgorithm, WalletProfileId, WalletSignature,
    WalletTransactionAuthorizationChallenge, WalletTransactionDraftId, WalletTransactionDraftState,
    WalletTransactionFeeState, WalletTransferPreview,
};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    MidnightWalletAdapter, ProtectedMidnightAccountDeriver, STARS_PER_NIGHT,
    SimulatedMidnightAccountSource, UnavailableMidnightAccountDeriver,
    UnavailableMidnightAccountSource, midnight_asset, network_by_id,
};

const SEND_UNSHIELDED_SEGMENT: u16 = 0xCAFE;

type LedgerIntent = Intent<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;
type LedgerTransaction = Transaction<Signature, ProofPreimageMarker, PedersenRandomness, DefaultDB>;

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
}

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
}

impl<K> MidnightTransactionAuthorizer for ProtectedMidnightAccountDeriver<K>
where
    K: WalletKeyOperationPort + 'static,
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
}

impl<S, D> WalletTransactionPort for MidnightWalletAdapter<S, D>
where
    S: MidnightTransactionSource,
    D: MidnightTransactionAuthorizer,
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
        if retained.preview.state() == WalletTransactionDraftState::Authorized {
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
            && retained.preview.state() != WalletTransactionDraftState::Expired
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
