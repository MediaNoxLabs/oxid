// SPDX-License-Identifier: Apache-2.0

use super::*;
use sha2::{Digest as _, Sha256};

const PORTAL_SHA: &str = "804de0a9e58cf48ece3cc6c24b2245bb70bc80f1";
const CREDENTIAL_OFFER: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/804de0a9e58cf48ece3cc6c24b2245bb70bc80f1/credential-offer.json"
);
const ISSUER_METADATA: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/804de0a9e58cf48ece3cc6c24b2245bb70bc80f1/issuer-metadata.json"
);
const CREDENTIAL_REQUEST: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/804de0a9e58cf48ece3cc6c24b2245bb70bc80f1/credential-request.json"
);
const CREDENTIAL_RESPONSE: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/804de0a9e58cf48ece3cc6c24b2245bb70bc80f1/credential-response.json"
);
const PROVENANCE: &[u8] = include_bytes!(
    "../../../../fixtures/laceid-portal/804de0a9e58cf48ece3cc6c24b2245bb70bc80f1/provenance.json"
);

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn fixture_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("checked-in LaceID Portal fixture must be valid JSON")
}

#[test]
fn pinned_provenance_and_fixture_digests_are_intact() {
    let provenance = fixture_json(PROVENANCE);
    assert_eq!(provenance["upstream"]["commit"], PORTAL_SHA);
    assert_eq!(
        provenance["upstream"]["repository"],
        "https://github.com/input-output-hk/lace-id-portal"
    );
    assert_eq!(provenance["upstream"]["license"], "Apache-2.0");

    let expected_sources = [
        (
            "crates/credential-digital-passport/src/metadata.rs",
            "e7e5667fcc307d165928c01f9da290650caf0c65a60f6757378f3ea47d5d98cf",
        ),
        (
            "crates/issuer-http/src/routes_issuer.rs",
            "573004e23f015a7592dbab71675bef4ee0bf9c5eb70f0f31a44d600082ca4690",
        ),
        (
            "crates/issuer-http/src/well_known.rs",
            "4dff93d21e02b221598c325a8afdc9ff7311b3c5041727652cea9ecac81419b1",
        ),
        (
            "crates/issuer-integration/tests/http_integration.rs",
            "c931f99da1880a18e01e1149b6f54f2d2698ba8df21a8e4f3175ed18a194aa40",
        ),
        (
            "crates/issuer-services/src/credential.rs",
            "3d397eba61ef76fd3b978bac175bc8399f52fce8466e7336f32ae43ccbdf003c",
        ),
        (
            "crates/issuer-services/src/credential_offer.rs",
            "35629c3462af6d83210f046c9adabda6c00d75cc247df111f17387d8e62f5b60",
        ),
    ];
    let sources = provenance["sources"]
        .as_array()
        .expect("provenance sources must be an array");
    assert_eq!(sources.len(), expected_sources.len());
    for (path, digest) in expected_sources {
        let source = sources
            .iter()
            .find(|source| source["path"] == path)
            .unwrap_or_else(|| panic!("missing provenance for upstream source {path}"));
        assert_eq!(source["sha256"], digest, "source digest drift for {path}");
    }

    let fixtures = [
        ("credential-offer.json", CREDENTIAL_OFFER),
        ("issuer-metadata.json", ISSUER_METADATA),
        ("credential-request.json", CREDENTIAL_REQUEST),
        ("credential-response.json", CREDENTIAL_RESPONSE),
    ];
    let recorded = provenance["fixtures"]
        .as_array()
        .expect("provenance fixtures must be an array");
    assert_eq!(recorded.len(), fixtures.len());
    for (name, bytes) in fixtures {
        let fixture = recorded
            .iter()
            .find(|fixture| fixture["path"] == name)
            .unwrap_or_else(|| panic!("missing provenance for fixture {name}"));
        assert_eq!(fixture["sha256"], sha256(bytes), "fixture drift for {name}");
        assert!(
            fixture["source_paths"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty()),
            "fixture {name} must name its exact upstream sources"
        );
    }
    assert_eq!(
        provenance["notes"]["issuer_origin"],
        "Extra source-produced credential-offer query parameter outside the embedded offer."
    );
}

#[test]
fn portal_offer_rejects_extra_issuer_origin_and_null_transaction_code() {
    let fixture = fixture_json(CREDENTIAL_OFFER);
    let offer_uri = fixture
        .as_str()
        .expect("offer fixture must contain its source-produced URI");
    assert_eq!(
        parse_offer(offer_uri).err(),
        Some(IssuanceProtocolError::InvalidOffer)
    );

    let source_url = Url::parse(offer_uri).expect("fixture offer URI must parse");
    let embedded_offer = source_url
        .query_pairs()
        .find_map(|(name, value)| (name == "credential_offer").then(|| value.into_owned()))
        .expect("fixture URI must contain an embedded offer");
    let mut embedded_only = Url::parse("openid-credential-offer://").expect("valid offer scheme");
    embedded_only
        .query_pairs_mut()
        .append_pair("credential_offer", &embedded_offer);
    assert_eq!(
        parse_offer(embedded_only.as_str()).err(),
        Some(IssuanceProtocolError::TransactionCodeRequired)
    );

    parse_offer(&standalone_credential_offer()).expect("valid Oxid offer control must be accepted");
}

#[test]
fn portal_metadata_is_rejected_at_the_exact_oxid_boundary() {
    assert_eq!(
        parse_issuer_metadata(ISSUER_METADATA, EndpointPolicy::HttpsOnly).err(),
        Some(IssuanceProtocolError::InvalidMetadata)
    );
    standalone_issuer_metadata().expect("valid Oxid metadata control must be accepted");
}

#[test]
fn portal_singular_proof_request_is_invalid_proof() {
    let expected_method = "did:midnight:fixture:holder#auth-1";
    let expected_nonce = "FIXTURE_NONCE";
    let now = 1_700_000_000;
    assert_eq!(
        validate_credential_request(
            CREDENTIAL_REQUEST,
            "digital_passport_v1",
            expected_method,
            expected_nonce,
            now,
        ),
        Err(IssuanceProtocolError::InvalidProof)
    );

    let header = general_purpose::URL_SAFE_NO_PAD.encode(
        json!({"alg":"EdDSA","kid":expected_method,"typ":"openid4vci-proof+jwt"}).to_string(),
    );
    let payload = general_purpose::URL_SAFE_NO_PAD.encode(
        json!({"aud":STANDALONE_CREDENTIAL_ISSUER,"iat":now,"nonce":expected_nonce}).to_string(),
    );
    let signature = general_purpose::URL_SAFE_NO_PAD.encode([0_u8; 64]);
    let control = json!({
        "credential_configuration_id": STANDALONE_CONFIGURATION_ID,
        "proofs": {"jwt": [format!("{header}.{payload}.{signature}")]}
    });
    validate_credential_request(
        control.to_string().as_bytes(),
        STANDALONE_CONFIGURATION_ID,
        expected_method,
        expected_nonce,
        now,
    )
    .expect("valid Oxid request control must be accepted");
}

#[test]
fn portal_singular_custom_credential_response_is_invalid() {
    assert_eq!(
        parse_credential_response(CREDENTIAL_RESPONSE),
        Err(IssuanceProtocolError::InvalidCredentialResponse)
    );

    let control = br#"{"credentials":[{"credential":"RklYVFVSRV9DUkVERU5USUFM"}]}"#;
    assert_eq!(
        parse_credential_response(control).expect("valid Oxid response control must be accepted"),
        b"FIXTURE_CREDENTIAL"
    );
}
