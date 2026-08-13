// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, MutexGuard},
};

use midnight_serialize::Deserializable as _;
use midnight_transient_crypto::curve::EmbeddedGroupAffine;
use oxid_identity_application::{
    DidJubjubChallengeDeriver, DidJubjubChallengeSignature, DidJubjubChallengeSigningPort,
    DidKeyAlgorithm, DidLifecyclePort, DidLifecyclePortError, DidLifecycleSignature, DidUpdate,
};
use oxid_identity_domain::{
    DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
    DidResolutionMetadata, DidResolutionSource, IdentityProfileId, JWK_CONTEXT, JwkCurve,
    JwkKeyType, MidnightDid, MidnightNetwork, PublicJwk, Service, ServiceEndpointValue,
    VerificationMethod, VerificationRelationshipEntry,
};
use oxid_wallet_application::{
    GenerateProtectedKeyRequest, WalletJubjubChallengeSigningPort, WalletKeyOperationPort,
    WalletSecurityPortError,
};
use oxid_wallet_domain::{
    PublicKeyEncoding, WalletKeyAlgorithm, WalletKeyDescriptor, WalletKeyLabel, WalletKeyPurpose,
    WalletKeyReference, WalletProfileId,
};
use p256::elliptic_curve::sec1::{FromSec1Point, ToSec1Point};
use p256::{AffinePoint, Sec1Point};
use sha2::{Digest, Sha256};

/// Development-only, process-local implementation of the Midnight DID
/// lifecycle. It owns no secret bytes: all generation and signing is delegated
/// through opaque wallet-custody handles.
pub struct StandaloneDidLifecycle {
    keys: Arc<dyn WalletKeyOperationPort>,
    jubjub_challenge_signing: Option<Arc<dyn WalletJubjubChallengeSigningPort>>,
    managed: Mutex<ManagedDids>,
    next_label: AtomicU64,
}

struct ManagedDid {
    methods: BTreeMap<String, KeyBinding>,
}

type ManagedDidKey = (String, String);
type ManagedDids = BTreeMap<ManagedDidKey, ManagedDid>;

#[derive(Clone)]
struct KeyBinding {
    reference: WalletKeyReference,
    algorithm: DidKeyAlgorithm,
}

impl StandaloneDidLifecycle {
    #[must_use]
    pub fn new(keys: Arc<dyn WalletKeyOperationPort>) -> Self {
        Self {
            keys,
            jubjub_challenge_signing: None,
            managed: Mutex::new(BTreeMap::new()),
            next_label: AtomicU64::new(1),
        }
    }

    /// Enables the protected two-step Jubjub operation used by exact Compact
    /// credential proofs. Generic DID signing remains available independently.
    #[must_use]
    pub fn with_jubjub_challenge_signing(
        keys: Arc<dyn WalletKeyOperationPort>,
        jubjub_challenge_signing: Arc<dyn WalletJubjubChallengeSigningPort>,
    ) -> Self {
        Self {
            keys,
            jubjub_challenge_signing: Some(jubjub_challenge_signing),
            managed: Mutex::new(BTreeMap::new()),
            next_label: AtomicU64::new(1),
        }
    }

    fn managed(&self) -> Result<MutexGuard<'_, ManagedDids>, DidLifecyclePortError> {
        self.managed
            .lock()
            .map_err(|_| DidLifecyclePortError::Unavailable)
    }

    fn profile(profile_id: &IdentityProfileId) -> Result<WalletProfileId, DidLifecyclePortError> {
        WalletProfileId::parse(profile_id.as_str().to_owned())
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    }

    fn key_label(&self, role: &str) -> Result<WalletKeyLabel, DidLifecyclePortError> {
        let sequence = self.next_label.fetch_add(1, Ordering::Relaxed);
        WalletKeyLabel::parse(format!("DID {role} {sequence}"))
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
    }

    fn generate(
        &self,
        profile_id: &IdentityProfileId,
        algorithm: DidKeyAlgorithm,
        role: &str,
        purpose: WalletKeyPurpose,
    ) -> Result<WalletKeyDescriptor, DidLifecyclePortError> {
        self.keys
            .generate(
                &Self::profile(profile_id)?,
                GenerateProtectedKeyRequest {
                    label: self.key_label(role)?,
                    algorithm: wallet_algorithm(algorithm),
                    purpose,
                },
            )
            .map_err(map_security_error)
    }
}

impl DidLifecyclePort for StandaloneDidLifecycle {
    fn managed_method_ids(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
    ) -> Result<Vec<String>, DidLifecyclePortError> {
        Ok(self
            .managed()?
            .get(&(
                profile_id.as_str().to_owned(),
                current.document().id().as_str().to_owned(),
            ))
            .map(|managed| managed.methods.keys().cloned().collect())
            .unwrap_or_default())
    }

    fn create(
        &self,
        profile_id: &IdentityProfileId,
        network: MidnightNetwork,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        if network != MidnightNetwork::Undeployed {
            return Err(DidLifecyclePortError::UnsupportedNetwork);
        }

        let authentication = self.generate(
            profile_id,
            DidKeyAlgorithm::Ed25519,
            "authentication",
            WalletKeyPurpose::Authentication,
        )?;
        let assertion = match self.generate(
            profile_id,
            DidKeyAlgorithm::P256,
            "assertion",
            WalletKeyPurpose::Assertion,
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = self
                    .keys
                    .delete(&Self::profile(profile_id)?, authentication.reference());
                return Err(error);
            }
        };
        let presentation = match self.generate(
            profile_id,
            DidKeyAlgorithm::Jubjub,
            "holder presentation",
            WalletKeyPurpose::Assertion,
        ) {
            Ok(value) => value,
            Err(error) => {
                let profile = Self::profile(profile_id)?;
                let _ = self.keys.delete(&profile, authentication.reference());
                let _ = self.keys.delete(&profile, assertion.reference());
                return Err(error);
            }
        };

        let did = did_from_public_keys(profile_id, &authentication, &assertion)?;
        let authentication_method = verification_method(&did, "auth-1", &authentication)?;
        let assertion_method = verification_method(&did, "assertion-1", &assertion)?;
        let presentation_method = verification_method(&did, "holder-jubjub-1", &presentation)?;
        let document = DidDocument::new(DidDocumentParts {
            contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
            id: did.clone(),
            controllers: vec![did.clone()],
            also_known_as: Vec::new(),
            verification_methods: vec![
                authentication_method.clone(),
                assertion_method.clone(),
                presentation_method.clone(),
            ],
            relationships: vec![
                VerificationRelationshipEntry::new(
                    oxid_identity_domain::VerificationRelationship::Authentication,
                    vec![authentication_method.id().to_owned()],
                ),
                VerificationRelationshipEntry::new(
                    oxid_identity_domain::VerificationRelationship::AssertionMethod,
                    vec![
                        assertion_method.id().to_owned(),
                        presentation_method.id().to_owned(),
                    ],
                ),
                VerificationRelationshipEntry::new(
                    oxid_identity_domain::VerificationRelationship::CapabilityInvocation,
                    vec![authentication_method.id().to_owned()],
                ),
            ],
            services: Vec::new(),
        })
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;

        let key = (profile_id.as_str().to_owned(), did.as_str().to_owned());
        let mut methods = BTreeMap::new();
        methods.insert(
            authentication_method.id().to_owned(),
            KeyBinding {
                reference: authentication.reference().clone(),
                algorithm: DidKeyAlgorithm::Ed25519,
            },
        );
        methods.insert(
            assertion_method.id().to_owned(),
            KeyBinding {
                reference: assertion.reference().clone(),
                algorithm: DidKeyAlgorithm::P256,
            },
        );
        methods.insert(
            presentation_method.id().to_owned(),
            KeyBinding {
                reference: presentation.reference().clone(),
                algorithm: DidKeyAlgorithm::Jubjub,
            },
        );
        if self
            .managed()?
            .insert(key, ManagedDid { methods })
            .is_some()
        {
            return Err(DidLifecyclePortError::Conflict);
        }

        Ok(DidResolution::new(
            document,
            DidDocumentMetadata {
                deactivated: Some(false),
                version_id: Some("standalone-1".to_owned()),
                ..DidDocumentMetadata::default()
            },
            DidResolutionMetadata {
                content_type: Some("application/did+ld+json".to_owned()),
            },
            DidResolutionSource::Standalone,
        ))
    }

    fn update(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
        operation: DidUpdate,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        ensure_active(current)?;
        let did = current.document().id();
        let managed_key = (profile_id.as_str().to_owned(), did.as_str().to_owned());
        let mut managed = self.managed()?;
        let managed_did = managed
            .get_mut(&managed_key)
            .ok_or(DidLifecyclePortError::NotManaged)?;

        let mut contexts = current.document().contexts().to_vec();
        let mut aliases = current.document().also_known_as().to_vec();
        let mut methods = current.document().verification_methods().to_vec();
        let mut relationships = current.document().relationships().to_vec();
        let mut services = current.document().services().to_vec();
        let mut new_binding = None;
        let mut removed_binding = None;

        match operation {
            DidUpdate::AddAlsoKnownAs { value } => {
                if aliases.contains(&value) {
                    return Err(DidLifecyclePortError::Conflict);
                }
                aliases.push(value);
            }
            DidUpdate::RemoveAlsoKnownAs { value } => {
                let before = aliases.len();
                aliases.retain(|alias| alias != &value);
                if aliases.len() == before {
                    return Err(DidLifecyclePortError::NotFound);
                }
            }
            DidUpdate::AddVerificationMethod {
                fragment,
                algorithm,
            } => {
                let descriptor = self.generate(
                    profile_id,
                    algorithm,
                    "verification",
                    WalletKeyPurpose::Assertion,
                )?;
                let method = verification_method(did, &fragment, &descriptor)?;
                if methods.iter().any(|existing| existing.id() == method.id()) {
                    let _ = self
                        .keys
                        .delete(&Self::profile(profile_id)?, descriptor.reference());
                    return Err(DidLifecyclePortError::Conflict);
                }
                new_binding = Some((
                    method.id().to_owned(),
                    KeyBinding {
                        reference: descriptor.reference().clone(),
                        algorithm,
                    },
                ));
                methods.push(method);
            }
            DidUpdate::UpdateVerificationMethod {
                method_id,
                algorithm,
            } => {
                let method_id = canonical_component_id(did, &method_id)?;
                if !managed_did.methods.contains_key(&method_id) {
                    return Err(DidLifecyclePortError::NotManaged);
                }
                let Some(index) = methods.iter().position(|method| method.id() == method_id) else {
                    return Err(DidLifecyclePortError::NotFound);
                };
                let descriptor = self.generate(
                    profile_id,
                    algorithm,
                    "verification",
                    WalletKeyPurpose::Assertion,
                )?;
                let method = verification_method(did, &method_id, &descriptor)?;
                methods[index] = method;
                new_binding = Some((
                    method_id,
                    KeyBinding {
                        reference: descriptor.reference().clone(),
                        algorithm,
                    },
                ));
            }
            DidUpdate::RemoveVerificationMethod { method_id } => {
                let method_id = canonical_component_id(did, &method_id)?;
                if relationships
                    .iter()
                    .any(|entry| entry.method_ids().iter().any(|value| value == &method_id))
                {
                    return Err(DidLifecyclePortError::Conflict);
                }
                if !managed_did.methods.contains_key(&method_id) {
                    return Err(DidLifecyclePortError::NotManaged);
                }
                let before = methods.len();
                methods.retain(|method| method.id() != method_id);
                if methods.len() == before {
                    return Err(DidLifecyclePortError::NotFound);
                }
                removed_binding = Some(method_id);
            }
            DidUpdate::AddVerificationRelationship {
                relationship,
                method_id,
            } => {
                let method_id = canonical_component_id(did, &method_id)?;
                if !methods.iter().any(|method| method.id() == method_id) {
                    return Err(DidLifecyclePortError::NotFound);
                }
                if let Some(entry) = relationships
                    .iter_mut()
                    .find(|entry| entry.relationship() == relationship)
                {
                    if entry.method_ids().contains(&method_id) {
                        return Err(DidLifecyclePortError::Conflict);
                    }
                    let mut ids = entry.method_ids().to_vec();
                    ids.push(method_id);
                    *entry = VerificationRelationshipEntry::new(relationship, ids);
                } else {
                    relationships.push(VerificationRelationshipEntry::new(
                        relationship,
                        vec![method_id],
                    ));
                }
            }
            DidUpdate::RemoveVerificationRelationship {
                relationship,
                method_id,
            } => {
                let method_id = canonical_component_id(did, &method_id)?;
                let Some(index) = relationships
                    .iter()
                    .position(|entry| entry.relationship() == relationship)
                else {
                    return Err(DidLifecyclePortError::NotFound);
                };
                let mut ids = relationships[index].method_ids().to_vec();
                let before = ids.len();
                ids.retain(|value| value != &method_id);
                if ids.len() == before {
                    return Err(DidLifecyclePortError::NotFound);
                }
                if ids.is_empty() {
                    relationships.remove(index);
                } else {
                    relationships[index] = VerificationRelationshipEntry::new(relationship, ids);
                }
            }
            DidUpdate::AddService {
                id,
                service_type,
                endpoint,
            } => {
                let id = canonical_component_id(did, &id)?;
                if services.iter().any(|current| current.id() == id) {
                    return Err(DidLifecyclePortError::Conflict);
                }
                let service = Service::new(
                    id.clone(),
                    vec![service_type],
                    vec![
                        ServiceEndpointValue::uri(endpoint)
                            .map_err(|_| DidLifecyclePortError::InvalidOperation)?,
                    ],
                    false,
                )
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
                services.push(service);
            }
            DidUpdate::UpdateService {
                id,
                service_type,
                endpoint,
            } => {
                let id = canonical_component_id(did, &id)?;
                let Some(index) = services.iter().position(|current| current.id() == id) else {
                    return Err(DidLifecyclePortError::NotFound);
                };
                services[index] = Service::new(
                    id,
                    vec![service_type],
                    vec![
                        ServiceEndpointValue::uri(endpoint)
                            .map_err(|_| DidLifecyclePortError::InvalidOperation)?,
                    ],
                    false,
                )
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            }
            DidUpdate::RemoveService { id } => {
                let id = canonical_component_id(did, &id)?;
                let before = services.len();
                services.retain(|service| service.id() != id);
                if services.len() == before {
                    return Err(DidLifecyclePortError::NotFound);
                }
            }
        }

        if !contexts.iter().any(|value| value == JWK_CONTEXT) {
            contexts.insert(1, JWK_CONTEXT.to_owned());
        }
        let document = DidDocument::new(DidDocumentParts {
            contexts,
            id: did.clone(),
            controllers: vec![did.clone()],
            also_known_as: aliases,
            verification_methods: methods,
            relationships,
            services,
        })
        .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
        if let Some((method_id, binding)) = new_binding {
            managed_did.methods.insert(method_id, binding);
        }
        if let Some(method_id) = removed_binding {
            // The protected key is deliberately retained if public persistence
            // later fails; its opaque handle is no longer reachable via DID APIs.
            managed_did.methods.remove(&method_id);
        }
        Ok(next_resolution(current, document, false))
    }

    fn deactivate(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
    ) -> Result<DidResolution, DidLifecyclePortError> {
        ensure_active(current)?;
        let key = (
            profile_id.as_str().to_owned(),
            current.document().id().as_str().to_owned(),
        );
        if !self.managed()?.contains_key(&key) {
            return Err(DidLifecyclePortError::NotManaged);
        }
        Ok(next_resolution(current, current.document().clone(), true))
    }

    fn sign(
        &self,
        profile_id: &IdentityProfileId,
        current: &DidResolution,
        method_id: &str,
        payload: &[u8],
    ) -> Result<DidLifecycleSignature, DidLifecyclePortError> {
        ensure_active(current)?;
        let method_id = canonical_component_id(current.document().id(), method_id)?;
        if !current
            .document()
            .verification_methods()
            .iter()
            .any(|method| method.id() == method_id)
        {
            return Err(DidLifecyclePortError::NotFound);
        }
        let key = (
            profile_id.as_str().to_owned(),
            current.document().id().as_str().to_owned(),
        );
        let binding = self
            .managed()?
            .get(&key)
            .and_then(|managed| managed.methods.get(&method_id))
            .cloned()
            .ok_or(DidLifecyclePortError::NotManaged)?;
        let signature = self
            .keys
            .sign(&Self::profile(profile_id)?, &binding.reference, payload)
            .map_err(map_security_error)?;
        if signature.algorithm() != wallet_algorithm(binding.algorithm) {
            return Err(DidLifecyclePortError::InvalidOperation);
        }
        Ok(DidLifecycleSignature {
            method_id,
            algorithm: binding.algorithm,
            signature_bytes: signature.bytes().to_vec(),
        })
    }
}

impl DidJubjubChallengeSigningPort for StandaloneDidLifecycle {
    fn sign_jubjub_challenge(
        &self,
        profile_id: &IdentityProfileId,
        did: &MidnightDid,
        method_id: &str,
        derive_challenge: &mut DidJubjubChallengeDeriver<'_>,
    ) -> Result<DidJubjubChallengeSignature, DidLifecyclePortError> {
        let method_id = canonical_component_id(did, method_id)?;
        let key = (profile_id.as_str().to_owned(), did.as_str().to_owned());
        let binding = self
            .managed()?
            .get(&key)
            .and_then(|managed| managed.methods.get(&method_id))
            .cloned()
            .ok_or(DidLifecyclePortError::NotManaged)?;
        if binding.algorithm != DidKeyAlgorithm::Jubjub {
            return Err(DidLifecyclePortError::UnsupportedAlgorithm);
        }
        let signer = self
            .jubjub_challenge_signing
            .as_ref()
            .ok_or(DidLifecyclePortError::ProtectionUnavailable)?;
        let mut callback_error = None;
        let mut bridge = |public_key: &[u8; 32], announcement: &[u8; 32]| {
            derive_challenge(public_key, announcement).map_err(|error| {
                callback_error = Some(error);
                WalletSecurityPortError::InvalidOperation
            })
        };
        let signature = signer
            .sign_jubjub_challenge(&Self::profile(profile_id)?, &binding.reference, &mut bridge)
            .map_err(map_security_error);
        if let Some(error) = callback_error {
            return Err(error);
        }
        let signature = signature?;
        Ok(DidJubjubChallengeSignature {
            method_id,
            public_key: signature.public_key,
            announcement: signature.announcement,
            response: signature.response,
        })
    }
}

fn ensure_active(resolution: &DidResolution) -> Result<(), DidLifecyclePortError> {
    if resolution.document_metadata().deactivated == Some(true) {
        Err(DidLifecyclePortError::Deactivated)
    } else {
        Ok(())
    }
}

fn next_resolution(
    current: &DidResolution,
    document: DidDocument,
    deactivated: bool,
) -> DidResolution {
    let mut metadata = current.document_metadata().clone();
    let version = metadata
        .version_id
        .as_deref()
        .and_then(|value| value.strip_prefix("standalone-"))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_add(1);
    metadata.version_id = Some(format!("standalone-{version}"));
    metadata.deactivated = Some(deactivated);
    DidResolution::new(
        document,
        metadata,
        current.resolution_metadata().clone(),
        DidResolutionSource::Standalone,
    )
}

fn canonical_component_id(did: &MidnightDid, value: &str) -> Result<String, DidLifecyclePortError> {
    let fragment = value
        .strip_prefix(did.as_str())
        .unwrap_or(value)
        .strip_prefix('#')
        .unwrap_or(value);
    if fragment.is_empty()
        || !fragment.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'%')
        })
    {
        return Err(DidLifecyclePortError::InvalidOperation);
    }
    Ok(format!("{}#{fragment}", did.as_str()))
}

fn wallet_algorithm(value: DidKeyAlgorithm) -> WalletKeyAlgorithm {
    match value {
        DidKeyAlgorithm::Ed25519 => WalletKeyAlgorithm::Ed25519,
        DidKeyAlgorithm::Jubjub => WalletKeyAlgorithm::Jubjub,
        DidKeyAlgorithm::P256 => WalletKeyAlgorithm::P256,
    }
}

fn map_security_error(error: WalletSecurityPortError) -> DidLifecyclePortError {
    match error {
        WalletSecurityPortError::Locked => DidLifecyclePortError::Locked,
        WalletSecurityPortError::UnsupportedAlgorithm => {
            DidLifecyclePortError::UnsupportedAlgorithm
        }
        WalletSecurityPortError::NotFound => DidLifecyclePortError::NotFound,
        WalletSecurityPortError::Conflict => DidLifecyclePortError::Conflict,
        WalletSecurityPortError::Unavailable | WalletSecurityPortError::NotInitialized => {
            DidLifecyclePortError::ProtectionUnavailable
        }
        WalletSecurityPortError::AlreadyInitialized
        | WalletSecurityPortError::AuthorizationDenied
        | WalletSecurityPortError::InvalidOperation => DidLifecyclePortError::InvalidOperation,
    }
}

fn did_from_public_keys(
    profile_id: &IdentityProfileId,
    first: &WalletKeyDescriptor,
    second: &WalletKeyDescriptor,
) -> Result<MidnightDid, DidLifecyclePortError> {
    let mut digest = Sha256::new();
    digest.update(b"oxid:standalone:did:midnight:v1\0");
    digest.update(profile_id.as_str().as_bytes());
    digest.update([0]);
    digest.update(first.public_key().bytes());
    digest.update([0]);
    digest.update(second.public_key().bytes());
    let mut identifier = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut identifier, "{byte:02x}")
            .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
    }
    MidnightDid::parse(format!("did:midnight:undeployed:{identifier}"))
        .map_err(|_| DidLifecyclePortError::InvalidOperation)
}

fn verification_method(
    did: &MidnightDid,
    fragment: &str,
    descriptor: &WalletKeyDescriptor,
) -> Result<VerificationMethod, DidLifecyclePortError> {
    let jwk = public_jwk(descriptor)?;
    VerificationMethod::new(
        did,
        canonical_component_id(did, fragment)?,
        did.clone(),
        jwk,
    )
    .map_err(|_| DidLifecyclePortError::InvalidOperation)
}

fn public_jwk(descriptor: &WalletKeyDescriptor) -> Result<PublicJwk, DidLifecyclePortError> {
    match (descriptor.algorithm(), descriptor.public_key().encoding()) {
        (WalletKeyAlgorithm::Ed25519, PublicKeyEncoding::Ed25519Compressed) => PublicJwk::new(
            JwkKeyType::Okp,
            JwkCurve::Ed25519,
            base64url(descriptor.public_key().bytes()),
            None,
        )
        .map_err(|_| DidLifecyclePortError::InvalidOperation),
        (WalletKeyAlgorithm::P256, PublicKeyEncoding::Sec1Compressed) => {
            let encoded = Sec1Point::from_bytes(descriptor.public_key().bytes())
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            let affine = Option::<AffinePoint>::from(AffinePoint::from_sec1_point(&encoded))
                .ok_or(DidLifecyclePortError::InvalidOperation)?;
            let uncompressed = affine.to_sec1_point(false);
            let x = uncompressed
                .x()
                .ok_or(DidLifecyclePortError::InvalidOperation)?;
            let y = uncompressed
                .y()
                .ok_or(DidLifecyclePortError::InvalidOperation)?;
            PublicJwk::new(
                JwkKeyType::Ec,
                JwkCurve::P256,
                base64url(x),
                Some(base64url(y)),
            )
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
        }
        (WalletKeyAlgorithm::Jubjub, PublicKeyEncoding::JubjubCompressed) => {
            let mut reader = descriptor.public_key().bytes();
            let point = EmbeddedGroupAffine::deserialize(&mut reader, 0)
                .map_err(|_| DidLifecyclePortError::InvalidOperation)?;
            if !reader.is_empty() || point.is_identity() {
                return Err(DidLifecyclePortError::InvalidOperation);
            }
            let x = point.x().ok_or(DidLifecyclePortError::InvalidOperation)?;
            let y = point.y().ok_or(DidLifecyclePortError::InvalidOperation)?;
            PublicJwk::new(
                JwkKeyType::Ec,
                JwkCurve::Jubjub,
                base64url(&x.as_le_bytes()),
                Some(base64url(&y.as_le_bytes())),
            )
            .map_err(|_| DidLifecyclePortError::InvalidOperation)
        }
        _ => Err(DidLifecyclePortError::UnsupportedAlgorithm),
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().saturating_mul(4).div_ceil(3));
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        output.push(ALPHABET[usize::from(chunk[0] >> 2)] as char);
        output.push(ALPHABET[usize::from(((chunk[0] & 0x03) << 4) | (chunk[1] >> 4))] as char);
        output.push(ALPHABET[usize::from(((chunk[1] & 0x0f) << 2) | (chunk[2] >> 6))] as char);
        output.push(ALPHABET[usize::from(chunk[2] & 0x3f)] as char);
    }
    match chunks.remainder() {
        [first] => {
            output.push(ALPHABET[usize::from(first >> 2)] as char);
            output.push(ALPHABET[usize::from((first & 0x03) << 4)] as char);
        }
        [first, second] => {
            output.push(ALPHABET[usize::from(first >> 2)] as char);
            output.push(ALPHABET[usize::from(((first & 0x03) << 4) | (second >> 4))] as char);
            output.push(ALPHABET[usize::from((second & 0x0f) << 2)] as char);
        }
        _ => {}
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use midnight_transient_crypto::curve::{EmbeddedFr, Fr};
    use oxid_adapter_platform_system::{OsRandom, SystemClock};
    use oxid_adapter_storage_dev::DevelopmentWalletSecurity;
    use oxid_identity_domain::VerificationRelationship;
    use oxid_wallet_application::WalletProtectionPort;

    type Security = DevelopmentWalletSecurity<SystemClock, OsRandom>;

    fn setup() -> (Arc<Security>, StandaloneDidLifecycle, IdentityProfileId) {
        let security = Arc::new(DevelopmentWalletSecurity::new(
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));
        let profile = IdentityProfileId::parse("profile_did_test").expect("profile");
        security
            .initialize(&WalletProfileId::parse(profile.as_str()).expect("wallet profile"))
            .expect("initialize custody");
        let keys: Arc<dyn WalletKeyOperationPort> = security.clone();
        (security, StandaloneDidLifecycle::new(keys), profile)
    }

    fn setup_challenge() -> (Arc<Security>, StandaloneDidLifecycle, IdentityProfileId) {
        let security = Arc::new(DevelopmentWalletSecurity::new(
            Arc::new(SystemClock),
            Arc::new(OsRandom),
        ));
        let profile = IdentityProfileId::parse("profile_did_challenge").expect("profile");
        security
            .initialize(&WalletProfileId::parse(profile.as_str()).expect("wallet profile"))
            .expect("initialize custody");
        let keys: Arc<dyn WalletKeyOperationPort> = security.clone();
        let challenge_signing: Arc<dyn WalletJubjubChallengeSigningPort> = security.clone();
        (
            security,
            StandaloneDidLifecycle::with_jubjub_challenge_signing(keys, challenge_signing),
            profile,
        )
    }

    #[test]
    fn challenge_signing_keeps_nonce_and_secret_in_custody() {
        let (security, lifecycle, profile) = setup_challenge();
        let resolution = lifecycle
            .create(&profile, MidnightNetwork::Undeployed)
            .expect("create DID");
        let did = resolution.document().id();
        let method_id = format!("{}#holder-jubjub-1", did.as_str());
        let callback_count = std::cell::Cell::new(0);
        let mut derive = |public_key: &[u8; 32], announcement: &[u8; 32]| {
            callback_count.set(callback_count.get() + 1);
            assert_ne!(public_key, &[0; 32]);
            assert_ne!(announcement, &[0; 32]);
            Ok(Fr::from(7_u64)
                .as_le_bytes()
                .try_into()
                .expect("field width"))
        };
        let signature = lifecycle
            .sign_jubjub_challenge(&profile, did, &method_id, &mut derive)
            .expect("protected challenge signature");
        assert_eq!(callback_count.get(), 1);
        assert_eq!(signature.method_id, method_id);

        let decode = |bytes: &[u8; 32]| {
            let mut input = bytes.as_slice();
            let point = EmbeddedGroupAffine::deserialize(&mut input, 0).expect("point");
            assert!(input.is_empty());
            point
        };
        let public_key = decode(&signature.public_key);
        let announcement = decode(&signature.announcement);
        let response =
            EmbeddedFr::try_from(Fr::from_le_bytes(&signature.response).expect("response field"))
                .expect("embedded response");
        assert_eq!(
            EmbeddedGroupAffine::generator() * response,
            announcement + public_key * Fr::from(7_u64)
        );

        security
            .lock(&WalletProfileId::parse(profile.as_str()).expect("wallet profile"))
            .expect("lock custody");
        assert_eq!(
            lifecycle.sign_jubjub_challenge(&profile, did, &method_id, &mut derive),
            Err(DidLifecyclePortError::Locked)
        );
    }

    #[test]
    fn performs_all_standalone_document_and_signature_transitions() {
        let (_, lifecycle, profile) = setup();
        let mut resolution = lifecycle
            .create(&profile, MidnightNetwork::Undeployed)
            .expect("create");
        let did = resolution.document().id().clone();
        assert_eq!(resolution.document().verification_methods().len(), 3);
        assert_eq!(
            lifecycle
                .managed_method_ids(&profile, &resolution)
                .expect("managed methods")
                .len(),
            3
        );
        assert_eq!(
            resolution.document_metadata().version_id.as_deref(),
            Some("standalone-1")
        );

        for operation in [
            DidUpdate::AddAlsoKnownAs {
                value: "https://example.test/alice".to_owned(),
            },
            DidUpdate::AddVerificationMethod {
                fragment: "#recovery-1".to_owned(),
                algorithm: DidKeyAlgorithm::Ed25519,
            },
            DidUpdate::UpdateVerificationMethod {
                method_id: "#recovery-1".to_owned(),
                algorithm: DidKeyAlgorithm::P256,
            },
            DidUpdate::AddVerificationRelationship {
                relationship: VerificationRelationship::CapabilityDelegation,
                method_id: "#recovery-1".to_owned(),
            },
            DidUpdate::AddService {
                id: "#messages".to_owned(),
                service_type: "MessagingService".to_owned(),
                endpoint: "https://example.test/messages".to_owned(),
            },
            DidUpdate::UpdateService {
                id: "#messages".to_owned(),
                service_type: "DIDCommMessaging".to_owned(),
                endpoint: "https://example.test/didcomm".to_owned(),
            },
        ] {
            resolution = lifecycle
                .update(&profile, &resolution, operation)
                .expect("update");
        }
        assert_eq!(resolution.document().also_known_as().len(), 1);
        assert_eq!(resolution.document().verification_methods().len(), 4);
        assert_eq!(resolution.document().services().len(), 1);

        let signature = lifecycle
            .sign(&profile, &resolution, "#auth-1", b"challenge")
            .expect("sign");
        assert_eq!(signature.algorithm, DidKeyAlgorithm::Ed25519);
        assert_eq!(signature.signature_bytes.len(), 64);
        assert_eq!(signature.method_id, format!("{}#auth-1", did.as_str()));
        let holder_signature = lifecycle
            .sign(
                &profile,
                &resolution,
                "#holder-jubjub-1",
                b"holder challenge",
            )
            .expect("sign with protected Jubjub holder method");
        assert_eq!(holder_signature.algorithm, DidKeyAlgorithm::Jubjub);
        assert_eq!(holder_signature.signature_bytes.len(), 96);

        for operation in [
            DidUpdate::RemoveVerificationRelationship {
                relationship: VerificationRelationship::CapabilityDelegation,
                method_id: "#recovery-1".to_owned(),
            },
            DidUpdate::RemoveVerificationMethod {
                method_id: "#recovery-1".to_owned(),
            },
            DidUpdate::RemoveService {
                id: "#messages".to_owned(),
            },
            DidUpdate::RemoveAlsoKnownAs {
                value: "https://example.test/alice".to_owned(),
            },
        ] {
            resolution = lifecycle
                .update(&profile, &resolution, operation)
                .expect("remove");
        }
        assert!(resolution.document().also_known_as().is_empty());
        assert_eq!(resolution.document().verification_methods().len(), 3);
        assert!(resolution.document().services().is_empty());

        resolution = lifecycle
            .deactivate(&profile, &resolution)
            .expect("deactivate");
        assert_eq!(resolution.document_metadata().deactivated, Some(true));
        assert_eq!(
            lifecycle.sign(&profile, &resolution, "#auth-1", b"denied"),
            Err(DidLifecyclePortError::Deactivated)
        );
        assert_eq!(
            lifecycle.update(
                &profile,
                &resolution,
                DidUpdate::AddAlsoKnownAs {
                    value: "https://example.test/late".to_owned()
                }
            ),
            Err(DidLifecyclePortError::Deactivated)
        );
    }

    #[test]
    fn fails_closed_for_live_unmanaged_locked_and_conflicting_operations() {
        let (security, lifecycle, profile) = setup();
        assert_eq!(
            lifecycle.create(&profile, MidnightNetwork::Testnet),
            Err(DidLifecyclePortError::UnsupportedNetwork)
        );
        let resolution = lifecycle
            .create(&profile, MidnightNetwork::Undeployed)
            .expect("create");
        assert_eq!(
            lifecycle.update(
                &profile,
                &resolution,
                DidUpdate::RemoveVerificationMethod {
                    method_id: "#auth-1".to_owned()
                }
            ),
            Err(DidLifecyclePortError::Conflict)
        );
        let other_keys: Arc<dyn WalletKeyOperationPort> = security.clone();
        let other = StandaloneDidLifecycle::new(other_keys);
        assert_eq!(
            other.update(
                &profile,
                &resolution,
                DidUpdate::AddAlsoKnownAs {
                    value: "https://example.test/unmanaged".to_owned()
                }
            ),
            Err(DidLifecyclePortError::NotManaged)
        );
        security
            .lock(&WalletProfileId::parse(profile.as_str()).expect("wallet profile"))
            .expect("lock");
        assert_eq!(
            lifecycle.sign(&profile, &resolution, "#auth-1", b"challenge"),
            Err(DidLifecyclePortError::Locked)
        );
    }

    #[test]
    fn emits_unpadded_canonical_base64url() {
        assert_eq!(base64url(&[]), "");
        assert_eq!(base64url(&[0]), "AA");
        assert_eq!(base64url(&[0, 0]), "AAA");
        assert_eq!(base64url(&[0, 0, 0]), "AAAA");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }
}
