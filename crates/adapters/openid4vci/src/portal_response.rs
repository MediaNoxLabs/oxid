// SPDX-License-Identifier: Apache-2.0

//! Strict Portal authorization/token/nonce/credential response parsing and
//! bounded HTTP response handling. Parent `portal.rs` owns session sequencing;
//! this module owns only response/endpoint validation.

use super::*;

pub(super) fn parse_portal_authorization_metadata(
    bytes: &[u8],
    expected_origin: &str,
) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    exact_keys(
        object,
        &[
            "grant_types_supported",
            "issuer",
            "pre-authorized_grant_anonymous_access_supported",
            "token_endpoint",
        ],
        IssuanceProtocolError::InvalidMetadata,
    )?;
    if required_string(object, "issuer", 2_048)? != expected_origin
        || object
            .get("pre-authorized_grant_anonymous_access_supported")
            .and_then(Value::as_bool)
            != Some(true)
        || required_unique_strings(object, "grant_types_supported", 1, 256)?
            != [PRE_AUTHORIZED_GRANT]
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let token = required_string(object, "token_endpoint", 2_048)?;
    validate_portal_endpoint(&token, expected_origin, "/api/issuer/token")?;
    Ok(token)
}

pub(super) fn parse_token_response(bytes: &[u8]) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    exact_keys(
        object,
        &["access_token", "expires_in", "token_type"],
        IssuanceProtocolError::IssuerRejected,
    )?;
    let token = object
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_SECRET_BYTES)
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    if object.get("token_type").and_then(Value::as_str) != Some("Bearer")
        || object.get("expires_in").and_then(Value::as_u64).is_none()
    {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    Ok(token.to_owned())
}

pub(super) fn parse_nonce_response(bytes: &[u8]) -> Result<String, IssuanceProtocolError> {
    let value = parse_strict_json(bytes)?;
    let object = value
        .as_object()
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    exact_keys(
        object,
        &["c_nonce", "c_nonce_expires_in"],
        IssuanceProtocolError::IssuerRejected,
    )?;
    if object
        .get("c_nonce_expires_in")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    object
        .get("c_nonce")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= MAX_SECRET_BYTES)
        .map(str::to_owned)
        .ok_or(IssuanceProtocolError::IssuerRejected)
}

pub(super) fn parse_portal_credential_response(
    bytes: &[u8],
    expected_holder_did: &str,
    expected_binding_method: &str,
    expected_nonce: &str,
    decoder: &dyn PortalCredentialMaterialDecoder,
) -> Result<IssuedCredentialBytes, IssuanceProtocolError> {
    let value =
        parse_strict_json(bytes).map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?;
    let root = value
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    exact_keys(
        root,
        &["credentials"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    let credentials = root
        .get("credentials")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    let item = credentials[0]
        .as_object()
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    exact_keys(
        item,
        &["credential", "midnight"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    let signed = item
        .get("credential")
        .and_then(Value::as_str)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
        .and_then(decode_payload)?;
    let midnight = required_response_object(item, "midnight")?;
    exact_keys(
        midnight,
        &[
            "credentialFamily",
            "credentialPrivateParts",
            "credentialProof",
            "encoding",
            "expiresAt",
            "hasExpiration",
            "holderBinding",
            "schemaId",
            "schemaVersion",
        ],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(midnight, "credentialFamily")? != PORTAL_FAMILY
        || response_string(midnight, "encoding")? != PORTAL_ENCODING
        || response_string(midnight, "schemaId")? != PORTAL_SCHEMA_ID
        || response_string(midnight, "schemaVersion")? != PORTAL_SCHEMA_VERSION
        || midnight
            .get("hasExpiration")
            .and_then(Value::as_bool)
            .is_none()
        || response_string(midnight, "expiresAt")?.len() > 64
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let proof = required_response_object(midnight, "credentialProof")?;
    exact_keys(
        proof,
        &["encoding", "payload"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(proof, "encoding")? != PORTAL_ENCODING {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let detached_proof = decode_payload(response_string(proof, "payload")?)?;
    let holder = required_response_object(midnight, "holderBinding")?;
    exact_keys(
        holder,
        &["challenge", "holderDidMethod", "method"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(holder, "challenge")? != expected_nonce
        || response_string(holder, "method")? != "explicit_did_method"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let method = required_response_object(holder, "holderDidMethod")?;
    exact_keys(
        method,
        &["did", "keyType", "methodId"],
        IssuanceProtocolError::InvalidCredentialResponse,
    )?;
    if response_string(method, "did")? != expected_holder_did
        || response_string(method, "methodId")? != expected_binding_method
        || response_string(method, "keyType")? != "jubjub"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let private_value = midnight
        .get("credentialPrivateParts")
        .filter(|value| value.is_object())
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
    let private_json = Zeroizing::new(
        serde_json::to_vec(private_value)
            .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?,
    );
    let private_material = decoder
        .decode(&signed, private_json.as_slice())
        .map_err(|error| match error {
            PortalCredentialMaterialError::Invalid => {
                IssuanceProtocolError::InvalidCredentialResponse
            }
            PortalCredentialMaterialError::Unavailable => {
                IssuanceProtocolError::ProtectionUnavailable
            }
        })?;
    if signed.is_empty() || detached_proof.is_empty() || private_material.is_empty() {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    Ok(IssuedCredentialBytes {
        signed_bytes: signed,
        detached_proof: Some(detached_proof),
        private_material: Some(private_material),
    })
}

pub(super) fn decode_payload(value: &str) -> Result<Vec<u8>, IssuanceProtocolError> {
    if value.is_empty() || value.len() > MAX_CREDENTIAL_BYTES * 2 {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let bytes = general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?;
    if general_purpose::URL_SAFE_NO_PAD.encode(&bytes) != value {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    Ok(bytes)
}

pub(super) fn required_response_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, IssuanceProtocolError> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
}

pub(super) fn response_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, IssuanceProtocolError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 2_048 && !value.chars().any(char::is_control)
        })
        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)
}

pub(super) fn exact_keys(
    object: &Map<String, Value>,
    expected: &[&str],
    error: IssuanceProtocolError,
) -> Result<(), IssuanceProtocolError> {
    if object.len() != expected.len() || !expected.iter().all(|key| object.contains_key(*key)) {
        return Err(error);
    }
    Ok(())
}

pub(super) fn validate_portal_endpoint(
    value: &str,
    expected_origin: &str,
    expected_path: &str,
) -> Result<Url, IssuanceProtocolError> {
    let endpoint = validate_endpoint(value, EndpointPolicy::StandaloneLoopback)?;
    if endpoint.origin().ascii_serialization() != expected_origin
        || endpoint.path() != expected_path
        || endpoint.query().is_some()
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(endpoint)
}

pub(super) async fn get_json(
    client: &Client,
    url: Url,
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, IssuanceProtocolError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| IssuanceProtocolError::Unavailable)?;
    read_json_response(response, limit).await
}

pub(super) async fn read_json_response(
    response: Response,
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, IssuanceProtocolError> {
    if response.status() != StatusCode::OK {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    if response.headers().contains_key(CONTENT_ENCODING) {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(IssuanceProtocolError::InvalidMetadata)?;
    validate_json_content_type(content_type)?;
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > limit as u64)
    {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| IssuanceProtocolError::Unavailable)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(IssuanceProtocolError::InvalidMetadata);
        }
        bytes.extend_from_slice(&chunk);
    }
    std::str::from_utf8(&bytes).map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    Ok(bytes)
}

pub(super) fn validate_json_content_type(value: &str) -> Result<(), IssuanceProtocolError> {
    let mut parts = value.split(';');
    if parts.next().map(str::trim) != Some("application/json") {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    match parts.next().map(str::trim) {
        None | Some("") => {}
        Some(parameter) if parameter.eq_ignore_ascii_case("charset=utf-8") => {}
        _ => return Err(IssuanceProtocolError::InvalidMetadata),
    }
    if parts.any(|part| !part.trim().is_empty()) {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(())
}
