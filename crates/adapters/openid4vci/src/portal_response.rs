// SPDX-License-Identifier: Apache-2.0

//! Strict Portal authorization/token/nonce/credential response parsing and
//! bounded HTTP response handling. Parent `portal.rs` owns session sequencing;
//! this module owns only response/endpoint validation.

use std::collections::BTreeSet;

use super::*;
use serde_json::value::RawValue;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenResponse<'a> {
    // Keep attacker-controlled strings borrowed so serde never owns decoded text.
    #[serde(borrow)]
    access_token: &'a RawValue,
    expires_in: u64,
    #[serde(borrow)]
    token_type: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NonceResponse<'a> {
    // Keep the encoded secret borrowed so serde never owns its decoded text.
    #[serde(borrow)]
    c_nonce: &'a RawValue,
    c_nonce_expires_in: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialResponse<'a> {
    #[serde(borrow)]
    credentials: [CredentialResponseItem<'a>; 1],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialResponseItem<'a> {
    credential: &'a str,
    #[serde(borrow)]
    midnight: MidnightCredentialResponse<'a>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct MidnightCredentialResponse<'a> {
    credential_family: &'a str,
    #[serde(borrow)]
    credential_private_parts: &'a RawValue,
    #[serde(borrow)]
    credential_proof: CredentialProof<'a>,
    encoding: &'a str,
    expires_at: &'a str,
    has_expiration: bool,
    #[serde(borrow)]
    holder_binding: HolderBinding<'a>,
    schema_id: &'a str,
    schema_version: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialProof<'a> {
    encoding: &'a str,
    payload: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct HolderBinding<'a> {
    challenge: &'a str,
    #[serde(borrow)]
    holder_did_method: HolderDidMethod<'a>,
    method: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct HolderDidMethod<'a> {
    did: &'a str,
    key_type: &'a str,
    method_id: &'a str,
}

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

pub(super) fn parse_token_response(
    bytes: &[u8],
) -> Result<Zeroizing<String>, IssuanceProtocolError> {
    let raw = parse_secret_response_json(bytes)?;
    let response: TokenResponse<'_> =
        serde_json::from_str(raw.get()).map_err(|_| IssuanceProtocolError::IssuerRejected)?;
    let access_token = decode_bounded_json_string(response.access_token, true)?;
    let token_type = decode_bounded_json_string(response.token_type, true)?;
    if token_type.as_str() != "Bearer" {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    let _ = response.expires_in;
    Ok(access_token)
}

pub(super) fn parse_nonce_response(
    bytes: &[u8],
) -> Result<Zeroizing<String>, IssuanceProtocolError> {
    let raw = parse_secret_response_json(bytes)?;
    let response: NonceResponse<'_> =
        serde_json::from_str(raw.get()).map_err(|_| IssuanceProtocolError::IssuerRejected)?;
    let nonce = decode_bounded_json_string(response.c_nonce, true)?;
    let _ = response.c_nonce_expires_in;
    Ok(nonce)
}

fn parse_secret_response_json(bytes: &[u8]) -> Result<&RawValue, IssuanceProtocolError> {
    if bytes.is_empty() || bytes.len() > super::super::MAX_PROTOCOL_RESPONSE_BYTES {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let raw = <&RawValue>::deserialize(&mut deserializer)
        .map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    deserializer
        .end()
        .map_err(|_| IssuanceProtocolError::InvalidMetadata)?;
    let structure = JsonStructureScanner::new(raw.get().as_bytes())
        .scan()
        .map_err(|()| IssuanceProtocolError::InvalidMetadata)?;
    if structure.has_duplicate || structure.depth > super::super::MAX_JSON_DEPTH {
        return Err(IssuanceProtocolError::InvalidMetadata);
    }
    Ok(raw)
}

#[derive(Clone, Copy)]
struct JsonStructure {
    has_duplicate: bool,
    depth: usize,
}

struct JsonStructureScanner<'a> {
    bytes: &'a [u8],
    index: usize,
}

#[derive(Eq)]
struct JsonKey(Zeroizing<String>);

impl PartialEq for JsonKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl PartialOrd for JsonKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JsonKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_str().cmp(other.0.as_str())
    }
}

impl<'a> JsonStructureScanner<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }

    fn scan(mut self) -> Result<JsonStructure, ()> {
        let structure = self.scan_value(0)?;
        self.skip_whitespace();
        (self.index == self.bytes.len())
            .then_some(structure)
            .ok_or(())
    }

    fn scan_value(&mut self, depth: usize) -> Result<JsonStructure, ()> {
        if depth > super::super::MAX_JSON_DEPTH {
            return Err(());
        }
        self.skip_whitespace();
        match self.bytes.get(self.index) {
            Some(b'{') => self.scan_object(depth),
            Some(b'[') => self.scan_array(depth),
            Some(b'"') => {
                self.scan_string_end()?;
                Ok(JsonStructure {
                    has_duplicate: false,
                    depth,
                })
            }
            Some(_) => {
                let start = self.index;
                while self.bytes.get(self.index).is_some_and(|byte| {
                    !matches!(byte, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
                }) {
                    self.index += 1;
                }
                (self.index > start)
                    .then_some(JsonStructure {
                        has_duplicate: false,
                        depth,
                    })
                    .ok_or(())
            }
            None => Err(()),
        }
    }

    fn scan_object(&mut self, depth: usize) -> Result<JsonStructure, ()> {
        self.index += 1;
        self.skip_whitespace();
        if self.consume(b'}') {
            return Ok(JsonStructure {
                has_duplicate: false,
                depth,
            });
        }

        let mut keys = BTreeSet::new();
        let mut structure = JsonStructure {
            has_duplicate: false,
            depth,
        };
        loop {
            self.skip_whitespace();
            let start = self.index;
            let end = self.scan_string_end()?;
            let key =
                decode_json_string(&self.bytes[start..end], usize::MAX, false).map_err(|_| ())?;
            structure.has_duplicate |= !keys.insert(JsonKey(key));
            self.skip_whitespace();
            if !self.consume(b':') {
                return Err(());
            }
            let child = self.scan_value(depth.saturating_add(1))?;
            structure.has_duplicate |= child.has_duplicate;
            structure.depth = structure.depth.max(child.depth);
            self.skip_whitespace();
            if self.consume(b'}') {
                return Ok(structure);
            }
            if !self.consume(b',') {
                return Err(());
            }
        }
    }

    fn scan_array(&mut self, depth: usize) -> Result<JsonStructure, ()> {
        self.index += 1;
        self.skip_whitespace();
        if self.consume(b']') {
            return Ok(JsonStructure {
                has_duplicate: false,
                depth,
            });
        }

        let mut structure = JsonStructure {
            has_duplicate: false,
            depth,
        };
        loop {
            let child = self.scan_value(depth.saturating_add(1))?;
            structure.has_duplicate |= child.has_duplicate;
            structure.depth = structure.depth.max(child.depth);
            self.skip_whitespace();
            if self.consume(b']') {
                return Ok(structure);
            }
            if !self.consume(b',') {
                return Err(());
            }
        }
    }

    fn scan_string_end(&mut self) -> Result<usize, ()> {
        if !self.consume(b'"') {
            return Err(());
        }
        while let Some(byte) = self.bytes.get(self.index).copied() {
            self.index += 1;
            match byte {
                b'"' => return Ok(self.index),
                b'\\' => {
                    let escape = self.bytes.get(self.index).copied().ok_or(())?;
                    self.index += 1;
                    match escape {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                        b'u' => {
                            let first = self.scan_hex_quad()?;
                            if (0xd800..=0xdbff).contains(&first) {
                                if self.bytes.get(self.index..self.index.saturating_add(2))
                                    != Some(b"\\u")
                                {
                                    return Err(());
                                }
                                self.index += 2;
                                let second = self.scan_hex_quad()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err(());
                                }
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                return Err(());
                            }
                        }
                        _ => return Err(()),
                    }
                }
                0x00..=0x1f => return Err(()),
                _ => {}
            }
        }
        Err(())
    }

    fn scan_hex_quad(&mut self) -> Result<u16, ()> {
        let digits = self
            .bytes
            .get(self.index..self.index.saturating_add(4))
            .filter(|digits| digits.len() == 4)
            .ok_or(())?;
        let mut value = 0_u16;
        for digit in digits {
            value = (value << 4)
                | u16::from(match digit {
                    b'0'..=b'9' => digit - b'0',
                    b'a'..=b'f' => digit - b'a' + 10,
                    b'A'..=b'F' => digit - b'A' + 10,
                    _ => return Err(()),
                });
        }
        self.index += 4;
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.index += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.index) == Some(&expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

fn decode_bounded_json_string(
    value: &RawValue,
    reject_empty: bool,
) -> Result<Zeroizing<String>, IssuanceProtocolError> {
    // Decode directly from the borrowed JSON spelling into the only owned buffer.
    decode_json_string(value.get().as_bytes(), MAX_SECRET_BYTES, reject_empty)
}

fn decode_json_string(
    raw: &[u8],
    max_bytes: usize,
    reject_empty: bool,
) -> Result<Zeroizing<String>, IssuanceProtocolError> {
    if raw.len() < 2 || raw.first() != Some(&b'"') || raw.last() != Some(&b'"') {
        return Err(IssuanceProtocolError::IssuerRejected);
    }

    let end = raw.len() - 1;
    let mut decoded = Zeroizing::new(String::with_capacity(end.saturating_sub(1)));
    let mut index = 1;
    while index < end {
        let byte = raw[index];
        let character = match byte {
            b'"' | 0x00..=0x1f => return Err(IssuanceProtocolError::IssuerRejected),
            b'\\' => {
                index += 1;
                let escape = raw
                    .get(index)
                    .copied()
                    .ok_or(IssuanceProtocolError::IssuerRejected)?;
                index += 1;
                match escape {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{0008}',
                    b'f' => '\u{000c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let first = decode_json_hex_quad(raw, &mut index, end)?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            if raw.get(index..index.saturating_add(2)) != Some(b"\\u") {
                                return Err(IssuanceProtocolError::IssuerRejected);
                            }
                            index += 2;
                            let second = decode_json_hex_quad(raw, &mut index, end)?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(IssuanceProtocolError::IssuerRejected);
                            }
                            0x1_0000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(second) - 0xdc00)
                        } else if (0xdc00..=0xdfff).contains(&first) {
                            return Err(IssuanceProtocolError::IssuerRejected);
                        } else {
                            u32::from(first)
                        };
                        char::from_u32(scalar).ok_or(IssuanceProtocolError::IssuerRejected)?
                    }
                    _ => return Err(IssuanceProtocolError::IssuerRejected),
                }
            }
            0x20..=0x7f => {
                index += 1;
                char::from(byte)
            }
            _ => {
                let width = match byte {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(IssuanceProtocolError::IssuerRejected),
                };
                let encoded = raw
                    .get(index..index.saturating_add(width))
                    .ok_or(IssuanceProtocolError::IssuerRejected)?;
                let character = std::str::from_utf8(encoded)
                    .ok()
                    .and_then(|text| text.chars().next())
                    .ok_or(IssuanceProtocolError::IssuerRejected)?;
                index += width;
                character
            }
        };
        decoded.push(character);
        if decoded.len() > max_bytes {
            return Err(IssuanceProtocolError::IssuerRejected);
        }
    }
    if reject_empty && decoded.is_empty() {
        return Err(IssuanceProtocolError::IssuerRejected);
    }
    Ok(decoded)
}

fn decode_json_hex_quad(
    raw: &[u8],
    index: &mut usize,
    end: usize,
) -> Result<u16, IssuanceProtocolError> {
    let digits = raw
        .get(*index..index.saturating_add(4))
        .filter(|digits| digits.len() == 4 && index.saturating_add(4) <= end)
        .ok_or(IssuanceProtocolError::IssuerRejected)?;
    let mut value = 0_u16;
    for digit in digits {
        value = (value << 4)
            | u16::from(match digit {
                b'0'..=b'9' => digit - b'0',
                b'a'..=b'f' => digit - b'a' + 10,
                b'A'..=b'F' => digit - b'A' + 10,
                _ => return Err(IssuanceProtocolError::IssuerRejected),
            });
    }
    *index += 4;
    Ok(value)
}

pub(super) fn parse_portal_credential_response(
    bytes: &[u8],
    expected_holder_did: &str,
    expected_binding_method: &str,
    expected_nonce: &str,
    decoder: &dyn PortalCredentialMaterialDecoder,
) -> Result<IssuedCredentialBytes, IssuanceProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_CREDENTIAL_BYTES {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    validate_credential_response_depth(bytes)?;
    let response: CredentialResponse<'_> = serde_json::from_slice(bytes)
        .map_err(|_| IssuanceProtocolError::InvalidCredentialResponse)?;
    let item = &response.credentials[0];
    let midnight = &item.midnight;
    let _ = midnight.has_expiration;
    if response_text(midnight.credential_family)? != PORTAL_FAMILY
        || response_text(midnight.encoding)? != PORTAL_ENCODING
        || response_text(midnight.schema_id)? != PORTAL_SCHEMA_ID
        || response_text(midnight.schema_version)? != PORTAL_SCHEMA_VERSION
        || response_text(midnight.expires_at)?.len() > 64
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    if response_text(midnight.credential_proof.encoding)? != PORTAL_ENCODING {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let signed = decode_payload(item.credential)?;
    let detached_proof = decode_payload(response_text(midnight.credential_proof.payload)?)?;
    let holder = &midnight.holder_binding;
    if response_text(holder.challenge)? != expected_nonce
        || response_text(holder.method)? != "explicit_did_method"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let method = &holder.holder_did_method;
    if response_text(method.did)? != expected_holder_did
        || response_text(method.method_id)? != expected_binding_method
        || response_text(method.key_type)? != "jubjub"
    {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let private_json = compact_private_json(midnight.credential_private_parts)?;
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

fn validate_credential_response_depth(bytes: &[u8]) -> Result<(), IssuanceProtocolError> {
    validate_response_depth(bytes, IssuanceProtocolError::InvalidCredentialResponse)
}

fn validate_response_depth(
    bytes: &[u8],
    error: IssuanceProtocolError,
) -> Result<(), IssuanceProtocolError> {
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0_usize;
    for byte in bytes.iter().copied() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.checked_add(1).ok_or(error)?;
                if depth > super::super::MAX_JSON_DEPTH {
                    return Err(error);
                }
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1).ok_or(error)?;
            }
            _ => {}
        }
    }
    if in_string || escaped || depth != 0 {
        return Err(error);
    }
    Ok(())
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

fn compact_private_json(value: &RawValue) -> Result<Zeroizing<Vec<u8>>, IssuanceProtocolError> {
    let source = value.get();
    if !source.trim_ascii_start().starts_with('{') {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    let mut compact = Zeroizing::new(Vec::with_capacity(source.len()));
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0_usize;
    for byte in source.bytes() {
        if in_string {
            compact.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
            compact.push(byte);
        } else if !byte.is_ascii_whitespace() {
            match byte {
                b'{' | b'[' => {
                    depth = depth
                        .checked_add(1)
                        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
                    if depth > super::super::MAX_JSON_DEPTH {
                        return Err(IssuanceProtocolError::InvalidCredentialResponse);
                    }
                }
                b'}' | b']' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or(IssuanceProtocolError::InvalidCredentialResponse)?;
                }
                _ => {}
            }
            compact.push(byte);
        }
    }
    if in_string || depth != 0 {
        return Err(IssuanceProtocolError::InvalidCredentialResponse);
    }
    Ok(compact)
}

fn response_text(value: &str) -> Result<&str, IssuanceProtocolError> {
    (!value.is_empty() && value.len() <= 2_048 && !value.chars().any(char::is_control))
        .then_some(value)
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
    if response.status().is_server_error() {
        return Err(IssuanceProtocolError::Unavailable);
    }
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
