// SPDX-License-Identifier: Apache-2.0

use crate::{
    CanonicalApprovalDigest, DeleteKeyApproval, DeleteWalletKeyCommand, DeleteWalletKeyUseCase,
    MAX_SIGNING_PAYLOAD_BYTES, SensitiveWalletOperationError, SignDataApproval,
    SignWalletDataCommand, SignWalletDataUseCase, WalletApprovalCapability, WalletApprovalRequest,
    WalletApprovalService, WalletKeyOperationPort, WalletSignatureView,
};
use oxid_platform_ports::Sha256Port;
use oxid_wallet_domain::{WalletKeyReference, WalletProfileId};
use std::sync::Arc;

/// Direct custody commands share one application approval authority. Hashing is
/// a platform primitive; framing and independently reconstructing intent remain
/// application policy. No caller-supplied digest can authorize a command.
pub struct WalletSensitiveKeyService<K> {
    keys: Arc<K>,
    approvals: Arc<WalletApprovalService>,
    hash: Arc<dyn Sha256Port>,
}
impl<K> WalletSensitiveKeyService<K> {
    #[must_use]
    pub fn new(
        keys: Arc<K>,
        approvals: Arc<WalletApprovalService>,
        hash: Arc<dyn Sha256Port>,
    ) -> Self {
        Self {
            keys,
            approvals,
            hash,
        }
    }

    /// Requests approval, not an approving shortcut. Ordinary composition has
    /// an unavailable trusted port and cannot produce a capability.
    pub fn request_sign_approval(
        &self,
        profile: &str,
        key: &str,
        payload: &[u8],
    ) -> Result<WalletApprovalCapability<SignDataApproval>, SensitiveWalletOperationError> {
        validate_payload(payload)?;
        let (profile, key) = parse_scope(profile, key)?;
        let request = self.sign_request(profile, &key, payload);
        self.approvals
            .request(&request)
            .map_err(SensitiveWalletOperationError::Approval)
    }

    pub fn request_delete_approval(
        &self,
        profile: &str,
        key: &str,
    ) -> Result<WalletApprovalCapability<DeleteKeyApproval>, SensitiveWalletOperationError> {
        let (profile, key) = parse_scope(profile, key)?;
        let request = self.delete_request(profile, &key);
        self.approvals
            .request(&request)
            .map_err(SensitiveWalletOperationError::Approval)
    }

    fn sign_request(
        &self,
        profile: WalletProfileId,
        key: &WalletKeyReference,
        payload: &[u8],
    ) -> WalletApprovalRequest<SignDataApproval> {
        let digest = self.digest(b"oxid.wallet.sign-data.v1", &profile, key, payload);
        WalletApprovalRequest::sign_data(profile, digest)
    }

    fn delete_request(
        &self,
        profile: WalletProfileId,
        key: &WalletKeyReference,
    ) -> WalletApprovalRequest<DeleteKeyApproval> {
        let digest = self.digest(b"oxid.wallet.delete-key.v1", &profile, key, &[]);
        WalletApprovalRequest::delete_key(profile, digest)
    }

    fn digest(
        &self,
        operation: &[u8],
        profile: &WalletProfileId,
        key: &WalletKeyReference,
        payload: &[u8],
    ) -> CanonicalApprovalDigest {
        let mut canonical = Vec::new();
        for field in [
            operation,
            profile.as_str().as_bytes(),
            key.as_str().as_bytes(),
            payload,
        ] {
            canonical.extend_from_slice(&(field.len() as u64).to_be_bytes());
            canonical.extend_from_slice(field);
        }
        CanonicalApprovalDigest::from_sha256(self.hash.sha256(&canonical))
    }
}
impl<K: WalletKeyOperationPort + 'static> SignWalletDataUseCase for WalletSensitiveKeyService<K> {
    fn execute(
        &self,
        command: SignWalletDataCommand<'_>,
    ) -> Result<WalletSignatureView, SensitiveWalletOperationError> {
        validate_payload(&command.payload)?;
        let (profile, key) = parse_scope(&command.profile_id, &command.key_reference)?;
        let expected = self.sign_request(profile.clone(), &key, &command.payload);
        self.approvals
            .consume(command.approval, &expected)
            .map_err(SensitiveWalletOperationError::Approval)?;
        self.keys
            .sign(&profile, &key, &command.payload)
            .map(Into::into)
            .map_err(SensitiveWalletOperationError::Operation)
    }
}
impl<K: WalletKeyOperationPort + 'static> DeleteWalletKeyUseCase for WalletSensitiveKeyService<K> {
    fn execute(
        &self,
        command: DeleteWalletKeyCommand<'_>,
    ) -> Result<(), SensitiveWalletOperationError> {
        let (profile, key) = parse_scope(&command.profile_id, &command.key_reference)?;
        let expected = self.delete_request(profile.clone(), &key);
        self.approvals
            .consume(command.approval, &expected)
            .map_err(SensitiveWalletOperationError::Approval)?;
        self.keys
            .delete(&profile, &key)
            .map_err(SensitiveWalletOperationError::Operation)
    }
}
fn parse_scope(
    profile: &str,
    key: &str,
) -> Result<(WalletProfileId, WalletKeyReference), SensitiveWalletOperationError> {
    Ok((
        WalletProfileId::parse(profile)
            .map_err(SensitiveWalletOperationError::InvalidProfileIdentifier)?,
        WalletKeyReference::parse(key)
            .map_err(SensitiveWalletOperationError::InvalidKeyReference)?,
    ))
}
fn validate_payload(payload: &[u8]) -> Result<(), SensitiveWalletOperationError> {
    if payload.is_empty() {
        return Err(SensitiveWalletOperationError::EmptyPayload);
    }
    if payload.len() > MAX_SIGNING_PAYLOAD_BYTES {
        return Err(SensitiveWalletOperationError::PayloadTooLarge);
    }
    Ok(())
}
