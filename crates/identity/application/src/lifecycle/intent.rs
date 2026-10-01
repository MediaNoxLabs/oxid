// SPDX-License-Identifier: Apache-2.0

use crate::{
    CanonicalDidApprovalDigest, DidApprovalRequest, DidUpdate, SignDidApproval, UpdateDidApproval,
};
use oxid_identity_domain::{IdentityProfileId, MidnightDid};
use oxid_platform_ports::Sha256Port;

// Fixed domain/version and positional, length-prefixed fields. Never Debug/JSON
// serialization: every variant and field has an explicit stable encoding.
fn digest(hash: &dyn Sha256Port, operation: &str, fields: &[&[u8]]) -> CanonicalDidApprovalDigest {
    let mut frame = Vec::new();
    for field in [b"oxid.did.lifecycle".as_slice(), b"1", operation.as_bytes()]
        .into_iter()
        .chain(fields.iter().copied())
    {
        frame.extend_from_slice(&(field.len() as u64).to_be_bytes());
        frame.extend_from_slice(field);
    }
    CanonicalDidApprovalDigest::from_sha256(hash.sha256(&frame))
}

pub(super) fn normalize_update(mut operation: DidUpdate) -> DidUpdate {
    let trim = |value: &mut String| *value = value.trim().to_owned();
    match &mut operation {
        DidUpdate::AddAlsoKnownAs { value } | DidUpdate::RemoveAlsoKnownAs { value } => trim(value),
        DidUpdate::AddVerificationMethod { fragment, .. } => trim(fragment),
        DidUpdate::UpdateVerificationMethod { method_id, .. }
        | DidUpdate::RemoveVerificationMethod { method_id }
        | DidUpdate::AddVerificationRelationship { method_id, .. }
        | DidUpdate::RemoveVerificationRelationship { method_id, .. } => trim(method_id),
        DidUpdate::AddService {
            id,
            service_type,
            endpoint,
        }
        | DidUpdate::UpdateService {
            id,
            service_type,
            endpoint,
        } => {
            trim(id);
            trim(service_type);
            trim(endpoint);
        }
        DidUpdate::RemoveService { id } => trim(id),
    }
    operation
}

pub(super) fn update_request(
    hash: &dyn Sha256Port,
    profile: &IdentityProfileId,
    did: &MidnightDid,
    operation: &DidUpdate,
) -> DidApprovalRequest<UpdateDidApproval> {
    let (variant, fields): (&str, Vec<&str>) = match operation {
        DidUpdate::AddAlsoKnownAs { value } => ("add-also-known-as", vec![value]),
        DidUpdate::RemoveAlsoKnownAs { value } => ("remove-also-known-as", vec![value]),
        DidUpdate::AddVerificationMethod {
            fragment,
            algorithm,
        } => ("add-method", vec![fragment, algorithm.as_str()]),
        DidUpdate::UpdateVerificationMethod {
            method_id,
            algorithm,
        } => ("update-method", vec![method_id, algorithm.as_str()]),
        DidUpdate::RemoveVerificationMethod { method_id } => ("remove-method", vec![method_id]),
        DidUpdate::AddVerificationRelationship {
            relationship,
            method_id,
        } => ("add-relationship", vec![relationship.as_str(), method_id]),
        DidUpdate::RemoveVerificationRelationship {
            relationship,
            method_id,
        } => (
            "remove-relationship",
            vec![relationship.as_str(), method_id],
        ),
        DidUpdate::AddService {
            id,
            service_type,
            endpoint,
        } => ("add-service", vec![id, service_type, endpoint]),
        DidUpdate::UpdateService {
            id,
            service_type,
            endpoint,
        } => ("update-service", vec![id, service_type, endpoint]),
        DidUpdate::RemoveService { id } => ("remove-service", vec![id]),
    };
    let fields: Vec<&[u8]> = [profile.as_str(), did.as_str(), variant]
        .into_iter()
        .chain(fields)
        .map(str::as_bytes)
        .collect();
    DidApprovalRequest::update(
        profile.clone(),
        did.clone(),
        digest(hash, "update", &fields),
    )
}

pub(super) fn sign_request(
    hash: &dyn Sha256Port,
    profile: &IdentityProfileId,
    did: &MidnightDid,
    method: &str,
    payload: &[u8],
) -> DidApprovalRequest<SignDidApproval> {
    DidApprovalRequest::sign(
        profile.clone(),
        did.clone(),
        method,
        digest(
            hash,
            "sign",
            &[
                profile.as_str().as_bytes(),
                did.as_str().as_bytes(),
                method.as_bytes(),
                payload,
            ],
        ),
    )
}

pub(super) fn deactivate_request(
    hash: &dyn Sha256Port,
    profile: &IdentityProfileId,
    did: &MidnightDid,
) -> DidApprovalRequest<crate::DeactivateDidApproval> {
    DidApprovalRequest::deactivate(
        profile.clone(),
        did.clone(),
        digest(
            hash,
            "deactivate",
            &[profile.as_str().as_bytes(), did.as_str().as_bytes()],
        ),
    )
}
