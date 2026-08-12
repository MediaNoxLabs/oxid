// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose};
use ciborium::Value;
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey as Ed25519Key};
use oxid_credential_application::{
    CredentialBytesFuture, CredentialInboxPort, CredentialIngressError, CredentialInspection,
    CredentialInspectionFuture, CredentialVerificationError, CredentialVerificationPort,
};
use oxid_credential_domain::{
    CredentialFormat, CredentialId, CredentialMetadata, VerificationOutcome, VerificationReport,
    VerificationStage, VerificationStageName, VerificationStageStatus,
};
use oxid_foundation::UnixTimestampMillis;
use oxid_identity_application::{DidResolutionPort, DidResolutionPortError};
use oxid_identity_domain::{JwkCurve, MidnightDid, VerificationRelationship};
use p256::ecdsa::{Signature as P256Signature, VerifyingKey as P256Key};
use sha2::{Digest as _, Sha256};

pub const STANDALONE_CREDENTIAL_B64: &str =
    include_str!("../../../../fixtures/credentials/standalone-midnight-phase1.b64");

/// Deterministic, public, non-secret credential ingress used only by the
/// standalone development composition and conformance harness.
#[derive(Clone, Copy, Debug, Default)]
pub struct StandaloneCredentialInbox;

impl CredentialInboxPort for StandaloneCredentialInbox {
    fn receive<'a>(&'a self) -> CredentialBytesFuture<'a> {
        Box::pin(async {
            general_purpose::STANDARD
                .decode(STANDALONE_CREDENTIAL_B64.trim())
                .map_err(|_| CredentialIngressError::Rejected)
        })
    }
}

/// Verifies the exact proof-stripped Midnight phase-1 CBOR bytes against an
/// assertion method from the issuer's resolved DID document.
pub struct MidnightCborCredentialVerifier {
    resolver: Arc<dyn DidResolutionPort>,
}

impl MidnightCborCredentialVerifier {
    #[must_use]
    pub const fn new(resolver: Arc<dyn DidResolutionPort>) -> Self {
        Self { resolver }
    }

    async fn inspect_inner(
        &self,
        signed_bytes: &[u8],
    ) -> Result<CredentialInspection, CredentialVerificationError> {
        let scanned = scan_top_level_map(signed_bytes)?;
        let value: Value = ciborium::from_reader(signed_bytes)
            .map_err(|_| CredentialVerificationError::InvalidCredential)?;
        let map = value
            .as_map()
            .ok_or(CredentialVerificationError::InvalidCredential)?;
        let issuer = required_text(map, "issuer")?;
        let subject = optional_nested_text(map, "credentialSubject", "id")?;
        if let Some(subject) = subject.as_deref() {
            MidnightDid::parse(subject.to_owned())
                .map_err(|_| CredentialVerificationError::InvalidCredential)?;
        }
        let issued_at = optional_u64(map, "issuanceDate")?.map(UnixTimestampMillis::new);
        let credential_type = credential_display_name(map)?;
        let proof = required_map(map, "proof")?;
        let method_id = required_text(proof, "verificationMethod")?;
        let signature = general_purpose::STANDARD
            .decode(required_text(proof, "signature")?)
            .map_err(|_| CredentialVerificationError::InvalidCredential)?;
        let metadata = CredentialMetadata::new(
            credential_type,
            issuer.clone(),
            subject,
            CredentialFormat::MidnightCborPhase1,
            issued_at,
        )
        .map_err(|_| CredentialVerificationError::InvalidCredential)?;
        let id = credential_id(signed_bytes)?;

        let issuer_did = MidnightDid::parse(issuer.clone())
            .map_err(|_| CredentialVerificationError::InvalidCredential)?;
        let resolution = match self.resolver.resolve(&issuer_did).await {
            Ok(resolution) => resolution,
            Err(error) => {
                let reason = match error {
                    DidResolutionPortError::NotFound => "issuer_not_found",
                    DidResolutionPortError::InvalidDid => "invalid_issuer_did",
                    DidResolutionPortError::MethodNotSupported => "issuer_method_unsupported",
                    DidResolutionPortError::Unavailable
                    | DidResolutionPortError::InvalidResponse
                    | DidResolutionPortError::Rejected => "issuer_resolution_error",
                };
                return Ok(CredentialInspection {
                    id,
                    metadata,
                    verification: report(
                        VerificationOutcome::Error,
                        Some((VerificationStageName::Issuer, reason)),
                    )?,
                });
            }
        };
        if resolution.document().id() != &issuer_did {
            return invalid(
                id,
                metadata,
                VerificationStageName::Issuer,
                "issuer_subject_mismatch",
            );
        }
        let canonical_method = if method_id.starts_with('#') {
            format!("{}{method_id}", issuer_did.as_str())
        } else {
            method_id.clone()
        };
        let assertion_authorized =
            resolution
                .document()
                .relationships()
                .iter()
                .any(|relationship| {
                    relationship.relationship() == VerificationRelationship::AssertionMethod
                        && relationship.method_ids().iter().any(|id| {
                            id == &canonical_method
                                || id.strip_prefix('#').is_some_and(|fragment| {
                                    canonical_method
                                        == format!("{}#{fragment}", issuer_did.as_str())
                                })
                        })
                });
        if !assertion_authorized {
            return invalid(
                id,
                metadata,
                VerificationStageName::Proof,
                "method_not_assertion_authorized",
            );
        }
        let Some(method) = resolution
            .document()
            .verification_methods()
            .iter()
            .find(|method| method.id() == canonical_method)
        else {
            return invalid(
                id,
                metadata,
                VerificationStageName::Proof,
                "verification_method_missing",
            );
        };
        if method.controller() != &issuer_did {
            return invalid(
                id,
                metadata,
                VerificationStageName::Proof,
                "method_controller_mismatch",
            );
        }
        let verified = verify_signature(
            method.public_key_jwk().curve(),
            method.public_key_jwk().x(),
            method.public_key_jwk().y(),
            &scanned.proof_stripped,
            &signature,
        );
        if !verified {
            return invalid(
                id,
                metadata,
                VerificationStageName::Proof,
                "invalid_signature",
            );
        }
        Ok(CredentialInspection {
            id,
            metadata,
            verification: report(VerificationOutcome::Valid, None)?,
        })
    }
}

impl CredentialVerificationPort for MidnightCborCredentialVerifier {
    fn inspect<'a>(&'a self, signed_bytes: &'a [u8]) -> CredentialInspectionFuture<'a> {
        Box::pin(async move { self.inspect_inner(signed_bytes).await })
    }
}

fn credential_id(bytes: &[u8]) -> Result<CredentialId, CredentialVerificationError> {
    let digest = Sha256::digest(bytes);
    CredentialId::parse(format!("vc_{}", hex::encode(&digest[..16])))
        .map_err(|_| CredentialVerificationError::InvalidCredential)
}

fn invalid(
    id: CredentialId,
    metadata: CredentialMetadata,
    stage: VerificationStageName,
    reason: &'static str,
) -> Result<CredentialInspection, CredentialVerificationError> {
    Ok(CredentialInspection {
        id,
        metadata,
        verification: report(VerificationOutcome::Invalid, Some((stage, reason)))?,
    })
}

fn report(
    outcome: VerificationOutcome,
    failure: Option<(VerificationStageName, &'static str)>,
) -> Result<VerificationReport, CredentialVerificationError> {
    let stages = VerificationStageName::ALL
        .into_iter()
        .map(|name| {
            let (status, reason) = if let Some((failed, reason)) = failure {
                if name == failed {
                    (VerificationStageStatus::Failed, Some(reason.to_owned()))
                } else if name < failed {
                    (VerificationStageStatus::Passed, None)
                } else {
                    (VerificationStageStatus::NotChecked, None)
                }
            } else if matches!(
                name,
                VerificationStageName::Structural
                    | VerificationStageName::Issuer
                    | VerificationStageName::Proof
            ) {
                (VerificationStageStatus::Passed, None)
            } else {
                (VerificationStageStatus::NotChecked, None)
            };
            VerificationStage::new(name, status, reason)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CredentialVerificationError::InvalidCredential)?;
    VerificationReport::new(outcome, stages)
        .map_err(|_| CredentialVerificationError::InvalidCredential)
}

fn verify_signature(
    curve: JwkCurve,
    x: &str,
    y: Option<&str>,
    message: &[u8],
    signature: &[u8],
) -> bool {
    match curve {
        JwkCurve::Ed25519 => {
            let Ok(bytes) = general_purpose::URL_SAFE_NO_PAD.decode(x) else {
                return false;
            };
            let Ok(bytes) = <[u8; 32]>::try_from(bytes) else {
                return false;
            };
            let Ok(key) = Ed25519Key::from_bytes(&bytes) else {
                return false;
            };
            let Ok(signature) = Ed25519Signature::from_slice(signature) else {
                return false;
            };
            key.verify(message, &signature).is_ok()
        }
        JwkCurve::P256 => {
            let (Ok(x), Some(Ok(y))) = (
                general_purpose::URL_SAFE_NO_PAD.decode(x),
                y.map(|value| general_purpose::URL_SAFE_NO_PAD.decode(value)),
            ) else {
                return false;
            };
            if x.len() != 32 || y.len() != 32 {
                return false;
            }
            let mut point = Vec::with_capacity(65);
            point.push(4);
            point.extend_from_slice(&x);
            point.extend_from_slice(&y);
            let Ok(key) = P256Key::from_sec1_bytes(&point) else {
                return false;
            };
            let signature = P256Signature::from_slice(signature)
                .or_else(|_| P256Signature::from_der(signature));
            signature.is_ok_and(|signature| key.verify(message, &signature).is_ok())
        }
        JwkCurve::X25519
        | JwkCurve::Jubjub
        | JwkCurve::Secp256k1
        | JwkCurve::Bls12381G1
        | JwkCurve::Bls12381G2 => false,
    }
}

fn required_text(map: &[(Value, Value)], key: &str) -> Result<String, CredentialVerificationError> {
    unique_value(map, key)?
        .as_text()
        .map(str::to_owned)
        .ok_or(CredentialVerificationError::InvalidCredential)
}

fn required_map<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<&'a [(Value, Value)], CredentialVerificationError> {
    unique_value(map, key)?
        .as_map()
        .map(|entries| entries.as_slice())
        .ok_or(CredentialVerificationError::InvalidCredential)
}

fn unique_value<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<&'a Value, CredentialVerificationError> {
    let mut found = map
        .iter()
        .filter_map(|(candidate, value)| (candidate.as_text() == Some(key)).then_some(value));
    let value = found
        .next()
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    if found.next().is_some() {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    Ok(value)
}

fn optional_nested_text(
    map: &[(Value, Value)],
    key: &str,
    nested: &str,
) -> Result<Option<String>, CredentialVerificationError> {
    let Some(value) = optional_unique_value(map, key)? else {
        return Ok(None);
    };
    let nested_map = value
        .as_map()
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    optional_unique_value(nested_map, nested)?
        .map(|value| {
            value
                .as_text()
                .map(str::to_owned)
                .ok_or(CredentialVerificationError::InvalidCredential)
        })
        .transpose()
}

fn optional_u64(
    map: &[(Value, Value)],
    key: &str,
) -> Result<Option<u64>, CredentialVerificationError> {
    let Some(value) = optional_unique_value(map, key)? else {
        return Ok(None);
    };
    let integer = value
        .as_integer()
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    u64::try_from(integer)
        .map(Some)
        .map_err(|_| CredentialVerificationError::InvalidCredential)
}

fn optional_unique_value<'a>(
    map: &'a [(Value, Value)],
    key: &str,
) -> Result<Option<&'a Value>, CredentialVerificationError> {
    let mut found = map
        .iter()
        .filter_map(|(candidate, value)| (candidate.as_text() == Some(key)).then_some(value));
    let value = found.next();
    if found.next().is_some() {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    Ok(value)
}

fn credential_display_name(map: &[(Value, Value)]) -> Result<String, CredentialVerificationError> {
    let types = unique_value(map, "type")?
        .as_array()
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    if types.is_empty()
        || types.iter().any(|value| value.as_text().is_none())
        || !types
            .iter()
            .any(|value| value.as_text() == Some("VerifiableCredential"))
    {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let value = types
        .iter()
        .filter_map(Value::as_text)
        .find(|value| *value != "VerifiableCredential")
        .unwrap_or("Verifiable credential");
    if value == "IdentityCredential" {
        Ok("Identity credential".to_owned())
    } else {
        Ok(value.to_owned())
    }
}

struct ScannedCredential {
    proof_stripped: Vec<u8>,
}

fn scan_top_level_map(bytes: &[u8]) -> Result<ScannedCredential, CredentialVerificationError> {
    if bytes.is_empty() || bytes.len() > oxid_credential_domain::MAX_SIGNED_CREDENTIAL_BYTES {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let (major, count, header_len) = read_header(bytes, 0)?;
    if major != 5 || count == 0 {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let count =
        usize::try_from(count).map_err(|_| CredentialVerificationError::InvalidCredential)?;
    let mut cursor = header_len;
    let mut proof_range = None;
    for _ in 0..count {
        let pair_start = cursor;
        let (key_end, key) = read_text_item(bytes, cursor)?;
        cursor = skip_item(bytes, key_end, 0)?;
        if key == "proof" && proof_range.replace(pair_start..cursor).is_some() {
            return Err(CredentialVerificationError::InvalidCredential);
        }
    }
    if cursor != bytes.len() {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let proof_range = proof_range.ok_or(CredentialVerificationError::InvalidCredential)?;
    let mut stripped = encode_map_header(count - 1, header_len)?;
    stripped.extend_from_slice(&bytes[header_len..proof_range.start]);
    stripped.extend_from_slice(&bytes[proof_range.end..]);
    Ok(ScannedCredential {
        proof_stripped: stripped,
    })
}

fn read_text_item(
    bytes: &[u8],
    offset: usize,
) -> Result<(usize, &str), CredentialVerificationError> {
    let (major, length, header) = read_header(bytes, offset)?;
    if major != 3 {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let length =
        usize::try_from(length).map_err(|_| CredentialVerificationError::InvalidCredential)?;
    let start = offset
        .checked_add(header)
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    let end = start
        .checked_add(length)
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    let text = std::str::from_utf8(
        bytes
            .get(start..end)
            .ok_or(CredentialVerificationError::InvalidCredential)?,
    )
    .map_err(|_| CredentialVerificationError::InvalidCredential)?;
    Ok((end, text))
}

fn skip_item(
    bytes: &[u8],
    offset: usize,
    depth: usize,
) -> Result<usize, CredentialVerificationError> {
    if depth > 32 {
        return Err(CredentialVerificationError::InvalidCredential);
    }
    let (major, value, header) = read_header(bytes, offset)?;
    let mut cursor = offset
        .checked_add(header)
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    match major {
        0 | 1 | 7 => Ok(cursor),
        2 | 3 => cursor
            .checked_add(
                usize::try_from(value)
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?,
            )
            .filter(|end| *end <= bytes.len())
            .ok_or(CredentialVerificationError::InvalidCredential),
        4 => {
            for _ in 0..value {
                cursor = skip_item(bytes, cursor, depth + 1)?;
            }
            Ok(cursor)
        }
        5 => {
            for _ in 0..value {
                cursor = skip_item(bytes, cursor, depth + 1)?;
                cursor = skip_item(bytes, cursor, depth + 1)?;
            }
            Ok(cursor)
        }
        6 => skip_item(bytes, cursor, depth + 1),
        _ => Err(CredentialVerificationError::InvalidCredential),
    }
}

fn read_header(
    bytes: &[u8],
    offset: usize,
) -> Result<(u8, u64, usize), CredentialVerificationError> {
    let first = *bytes
        .get(offset)
        .ok_or(CredentialVerificationError::InvalidCredential)?;
    let major = first >> 5;
    let additional = first & 0x1f;
    let (value, header) = match additional {
        value @ 0..=23 => (u64::from(value), 1),
        24 => (
            u64::from(
                *bytes
                    .get(offset + 1)
                    .ok_or(CredentialVerificationError::InvalidCredential)?,
            ),
            2,
        ),
        25 => (
            u64::from(u16::from_be_bytes(
                bytes
                    .get(offset + 1..offset + 3)
                    .ok_or(CredentialVerificationError::InvalidCredential)?
                    .try_into()
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?,
            )),
            3,
        ),
        26 => (
            u64::from(u32::from_be_bytes(
                bytes
                    .get(offset + 1..offset + 5)
                    .ok_or(CredentialVerificationError::InvalidCredential)?
                    .try_into()
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?,
            )),
            5,
        ),
        27 => (
            u64::from_be_bytes(
                bytes
                    .get(offset + 1..offset + 9)
                    .ok_or(CredentialVerificationError::InvalidCredential)?
                    .try_into()
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?,
            ),
            9,
        ),
        _ => return Err(CredentialVerificationError::InvalidCredential),
    };
    Ok((major, value, header))
}

fn encode_map_header(count: usize, width: usize) -> Result<Vec<u8>, CredentialVerificationError> {
    let count = u64::try_from(count).map_err(|_| CredentialVerificationError::InvalidCredential)?;
    match width {
        1 if count < 24 => Ok(vec![
            0xa0 | u8::try_from(count)
                .map_err(|_| CredentialVerificationError::InvalidCredential)?,
        ]),
        2 if count <= u64::from(u8::MAX) => Ok(vec![
            0xb8,
            u8::try_from(count).map_err(|_| CredentialVerificationError::InvalidCredential)?,
        ]),
        3 if count <= u64::from(u16::MAX) => {
            let mut out = vec![0xb9];
            out.extend_from_slice(
                &u16::try_from(count)
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?
                    .to_be_bytes(),
            );
            Ok(out)
        }
        5 if count <= u64::from(u32::MAX) => {
            let mut out = vec![0xba];
            out.extend_from_slice(
                &u32::try_from(count)
                    .map_err(|_| CredentialVerificationError::InvalidCredential)?
                    .to_be_bytes(),
            );
            Ok(out)
        }
        9 => {
            let mut out = vec![0xbb];
            out.extend_from_slice(&count.to_be_bytes());
            Ok(out)
        }
        _ => Err(CredentialVerificationError::InvalidCredential),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxid_identity_application::DidResolutionPortFuture;
    use oxid_identity_domain::{
        DID_CONTEXT, DidDocument, DidDocumentMetadata, DidDocumentParts, DidResolution,
        DidResolutionMetadata, DidResolutionSource, JWK_CONTEXT, JwkKeyType, PublicJwk,
        VerificationMethod, VerificationRelationshipEntry,
    };

    struct FixtureResolver(VerificationRelationship);
    impl DidResolutionPort for FixtureResolver {
        fn resolve<'a>(&'a self, did: &'a MidnightDid) -> DidResolutionPortFuture<'a> {
            let did = did.clone();
            let relationship = self.0;
            Box::pin(async move {
                let method = VerificationMethod::new(
                    &did,
                    "#authentication-1",
                    did.clone(),
                    PublicJwk::new(
                        JwkKeyType::Okp,
                        JwkCurve::Ed25519,
                        "4A3l3ITUWOFUgNTdtN9BS3HEIpnEhewcfd_rEb3iSEo",
                        None,
                    )
                    .expect("JWK"),
                )
                .expect("method");
                let document = DidDocument::new(DidDocumentParts {
                    contexts: vec![DID_CONTEXT.to_owned(), JWK_CONTEXT.to_owned()],
                    id: did.clone(),
                    controllers: vec![did.clone()],
                    also_known_as: Vec::new(),
                    verification_methods: vec![method],
                    relationships: vec![VerificationRelationshipEntry::new(
                        relationship,
                        vec![format!("{}#authentication-1", did.as_str())],
                    )],
                    services: Vec::new(),
                })
                .expect("document");
                Ok(DidResolution::new(
                    document,
                    DidDocumentMetadata::default(),
                    DidResolutionMetadata::default(),
                    DidResolutionSource::Standalone,
                ))
            })
        }
    }

    #[test]
    fn verifies_the_public_standalone_fixture_and_rejects_tampering() {
        let bytes = general_purpose::STANDARD
            .decode(STANDALONE_CREDENTIAL_B64.trim())
            .expect("fixture");
        let verifier = MidnightCborCredentialVerifier::new(Arc::new(FixtureResolver(
            VerificationRelationship::AssertionMethod,
        )));
        let result = poll(verifier.inspect(&bytes)).expect("inspect");
        assert_eq!(result.verification.outcome(), VerificationOutcome::Valid);
        let mut tampered = bytes;
        let position = tampered
            .windows(3)
            .position(|window| window == b"Ada")
            .expect("Ada");
        tampered[position] = b'E';
        let result = poll(verifier.inspect(&tampered)).expect("inspect tampered");
        assert_eq!(result.verification.outcome(), VerificationOutcome::Invalid);
    }

    #[test]
    fn rejects_duplicate_proof_members_before_decoding() {
        let bytes = [
            0xa2, 0x65, b'p', b'r', b'o', b'o', b'f', 0xa0, 0x65, b'p', b'r', b'o', b'o', b'f',
            0xa0,
        ];
        assert_eq!(
            scan_top_level_map(&bytes).err(),
            Some(CredentialVerificationError::InvalidCredential)
        );
    }

    #[test]
    fn rejects_a_key_that_is_not_assertion_authorized() {
        let bytes = general_purpose::STANDARD
            .decode(STANDALONE_CREDENTIAL_B64.trim())
            .expect("fixture");
        let verifier = MidnightCborCredentialVerifier::new(Arc::new(FixtureResolver(
            VerificationRelationship::Authentication,
        )));
        let result = poll(verifier.inspect(&bytes)).expect("inspect");
        assert_eq!(result.verification.outcome(), VerificationOutcome::Invalid);
        let proof = result
            .verification
            .stages()
            .iter()
            .find(|stage| stage.name() == VerificationStageName::Proof)
            .expect("proof stage");
        assert_eq!(proof.status(), VerificationStageStatus::Failed);
        assert_eq!(proof.reason_code(), Some("method_not_assertion_authorized"));
    }

    #[test]
    fn verifies_p256_raw_and_der_signatures() {
        use p256::ecdsa::{Signature, SigningKey, signature::Signer as _};
        let signing = loop {
            let mut candidate = [0_u8; 32];
            getrandom::fill(&mut candidate).expect("OS randomness");
            if let Ok(signing) = SigningKey::from_slice(&candidate) {
                break signing;
            }
        };
        let public = signing.verifying_key().to_sec1_point(false);
        let message = b"bounded P-256 credential proof fixture";
        let signature: Signature = signing.sign(message);
        let x = general_purpose::URL_SAFE_NO_PAD.encode(public.x().expect("x"));
        let y = general_purpose::URL_SAFE_NO_PAD.encode(public.y().expect("y"));
        assert!(verify_signature(
            JwkCurve::P256,
            &x,
            Some(&y),
            message,
            signature.to_bytes().as_ref(),
        ));
        assert!(verify_signature(
            JwkCurve::P256,
            &x,
            Some(&y),
            message,
            signature.to_der().as_bytes(),
        ));
    }

    fn poll<T>(
        mut future: std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + '_>>,
    ) -> T {
        use std::task::{Context, Poll, Waker};
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture future must be ready"),
        }
    }
}
