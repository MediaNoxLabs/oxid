// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error as _,
    io::{self, BufRead, Read, Write},
};

use oxid_headless::{HeadlessIoError, HeadlessWallet, PROTOCOL_VERSION};
use serde_json::{Value, json};

const VOCABULARY: &str = include_str!("fixtures/protocol-vocabulary.json");
const WIRE_INPUT: &[u8] = include_bytes!("fixtures/protocol-wire.ndjson");
const WIRE_EXPECTED: &[u8] = include_bytes!("fixtures/protocol-wire.expected.ndjson");

fn execute(input: &[u8]) -> Result<Vec<u8>, HeadlessIoError> {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let mut output = Vec::new();
    wallet.run(input, &mut output)?;
    Ok(output)
}

fn execute_json(request: Value) -> Value {
    let mut input = serde_json::to_vec(&request).expect("sanitized request should serialize");
    input.push(b'\n');
    let output = execute(&input).expect("protocol exchange should succeed");
    serde_json::from_slice(&output).expect("protocol response should be JSON")
}

fn assert_wire(input: &[u8], expected: &[u8]) {
    let output = execute(input).expect("wire exchange should execute");
    assert_eq!(output.as_slice(), expected);
}

#[test]
fn preserves_the_exact_sanitized_wire_corpus_and_stream_recovery() {
    let output = execute(WIRE_INPUT).expect("wire corpus should execute");
    assert_eq!(output.as_slice(), WIRE_EXPECTED);
    assert!(
        !String::from_utf8(output)
            .expect("wire output should be UTF-8")
            .contains("must-not-echo")
    );
}

#[test]
fn preserves_literal_alias_bytes_and_structured_shutdown_bytes() {
    assert_wire(
        b"quit\nignored\n",
        b"{\"protocol\":\"oxid.headless.v1\",\"id\":null,\"ok\":true,\"result\":{\"alias\":\"quit\",\"shuttingDown\":true}}\n",
    );
    assert_wire(
        b"exit\nignored\n",
        b"{\"protocol\":\"oxid.headless.v1\",\"id\":null,\"ok\":true,\"result\":{\"alias\":\"exit\",\"shuttingDown\":true}}\n",
    );
    assert_wire(
        br#"{"protocol":"oxid.headless.v1","id":"stop","method":"system.quit"}
ignored
"#,
        b"{\"protocol\":\"oxid.headless.v1\",\"id\":\"stop\",\"ok\":true,\"result\":{\"shuttingDown\":true}}\n",
    );
}

#[test]
fn preserves_request_id_scalar_bounds_and_structural_validation() {
    let maximum_id = "🦀".repeat(128);
    let accepted = execute_json(json!({
        "protocol": PROTOCOL_VERSION,
        "id": maximum_id,
        "method": "not.a.method",
        "params": {}
    }));
    assert_eq!(accepted["id"], maximum_id);
    assert_eq!(accepted["error"]["code"], "method_not_found");

    for invalid_id in [String::new(), "🦀".repeat(129)] {
        let rejected = execute_json(json!({
            "protocol": PROTOCOL_VERSION,
            "id": invalid_id,
            "method": "system.capabilities",
            "params": {}
        }));
        assert!(rejected["id"].is_null());
        assert_eq!(rejected["error"]["code"], "invalid_request");
        assert_eq!(
            rejected["error"]["message"],
            "id must contain between 1 and 128 characters"
        );
    }

    for request in [
        json!({"protocol": 1, "id": "numeric-protocol", "method": "system.capabilities"}),
        json!({"protocol": PROTOCOL_VERSION, "id": "missing-method"}),
        json!({"protocol": PROTOCOL_VERSION, "id": "numeric-method", "method": 1}),
    ] {
        let rejected = execute_json(request);
        assert_eq!(rejected["error"]["code"], "invalid_request");
        assert_eq!(
            rejected["error"]["message"],
            "request must include string protocol and method fields"
        );
    }
}

#[test]
fn preserves_request_defaults_and_top_level_unknown_field_compatibility() {
    let response = execute_json(json!({
        "protocol": PROTOCOL_VERSION,
        "method": "system.quit",
        "futureEnvelopeField": "accepted"
    }));
    assert!(response["id"].is_null());
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["shuttingDown"], true);
}

#[test]
fn every_checked_in_dispatch_name_routes_and_manifest_vocabulary_is_exact() {
    let vocabulary: Value = serde_json::from_str(VOCABULARY).expect("vocabulary should be JSON");
    let dispatch_methods = vocabulary["dispatchMethods"]
        .as_array()
        .expect("dispatchMethods should be an array");
    assert_eq!(dispatch_methods.len(), 107);

    let expected_dispatch = dispatch_methods
        .iter()
        .map(|method| method.as_str().expect("method should be a string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(expected_dispatch.len(), dispatch_methods.len());

    for method in &expected_dispatch {
        let response = execute_json(json!({
            "protocol": PROTOCOL_VERSION,
            "id": "vocabulary",
            "method": method,
            "params": {}
        }));
        assert_ne!(
            response["error"]["code"], "method_not_found",
            "checked-in method did not route: {method}"
        );
    }

    let capabilities = execute_json(json!({
        "protocol": PROTOCOL_VERSION,
        "id": "capabilities",
        "method": "system.capabilities",
        "params": {}
    }));
    let manifest = capabilities["result"]["methods"]
        .as_array()
        .expect("capability methods should be an array");
    let actual_statuses = manifest
        .iter()
        .map(|capability| {
            (
                capability["method"]
                    .as_str()
                    .expect("manifest method should be a string"),
                capability["status"]
                    .as_str()
                    .expect("manifest status should be a string"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_statuses.len(), manifest.len());

    let mut expected_manifest = expected_dispatch;
    for method in vocabulary["manifestOnlyMethods"]
        .as_array()
        .expect("manifestOnlyMethods should be an array")
    {
        expected_manifest.insert(method.as_str().expect("method should be a string"));
    }
    assert_eq!(
        actual_statuses.keys().copied().collect::<BTreeSet<_>>(),
        expected_manifest
    );

    let overrides = vocabulary["manifestStatusOverrides"]
        .as_object()
        .expect("manifestStatusOverrides should be an object");
    for (method, status) in &actual_statuses {
        let expected = overrides
            .get(*method)
            .and_then(Value::as_str)
            .unwrap_or("ready");
        assert_eq!(*status, expected, "unexpected status for {method}");
    }

    let actual_aliases = manifest
        .iter()
        .filter_map(|capability| {
            capability.get("aliasFor").map(|target| {
                (
                    capability["method"]
                        .as_str()
                        .expect("alias method should be a string"),
                    target.as_str().expect("alias target should be a string"),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    let expected_aliases = vocabulary["manifestAliases"]
        .as_object()
        .expect("manifestAliases should be an object")
        .iter()
        .map(|(alias, target)| {
            (
                alias.as_str(),
                target.as_str().expect("alias target should be a string"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(actual_aliases, expected_aliases);
    assert_eq!(
        capabilities["result"]["compatibilityAliases"],
        json!(["quit", "exit"])
    );
}

#[test]
fn manifest_declared_aliases_remain_wire_equivalent_to_their_exact_targets() {
    let vocabulary: Value = serde_json::from_str(VOCABULARY).expect("vocabulary should be JSON");
    for (alias, target) in vocabulary["manifestAliases"]
        .as_object()
        .expect("manifestAliases should be an object")
    {
        let alias_response = execute_json(json!({
            "protocol": PROTOCOL_VERSION,
            "id": "alias-equivalence",
            "method": alias,
            "params": {}
        }));
        let target_response = execute_json(json!({
            "protocol": PROTOCOL_VERSION,
            "id": "alias-equivalence",
            "method": target,
            "params": {}
        }));
        assert_eq!(alias_response, target_response, "alias drifted: {alias}");
    }
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private read detail",
        ))
    }
}

impl BufRead for FailingReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private read detail",
        ))
    }

    fn consume(&mut self, _amount: usize) {}
}

#[derive(Default)]
struct NewlineRejectingWriter {
    bytes: Vec<u8>,
}

impl Write for NewlineRejectingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.contains(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "private write detail",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "private serialization detail",
        ))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn public_io_errors_preserve_variants_safe_display_and_sources() {
    let wallet = HeadlessWallet::new(oxid_composition::compose_in_memory());
    let read_error = wallet
        .run(FailingReader, Vec::new())
        .expect_err("reader should fail");
    assert!(matches!(&read_error, HeadlessIoError::Read(_)));
    assert_eq!(
        read_error.to_string(),
        "failed to read a headless wallet request"
    );
    assert_eq!(
        read_error
            .source()
            .and_then(|source| source.downcast_ref::<io::Error>())
            .map(io::Error::kind),
        Some(io::ErrorKind::InvalidData)
    );

    let write_error = wallet
        .run(
            &br#"{"protocol":"oxid.headless.v1","method":"system.quit"}
"#[..],
            NewlineRejectingWriter::default(),
        )
        .expect_err("newline write should fail");
    assert!(matches!(&write_error, HeadlessIoError::Write(_)));
    assert_eq!(
        write_error.to_string(),
        "failed to write a headless wallet response"
    );
    assert_eq!(
        write_error
            .source()
            .and_then(|source| source.downcast_ref::<io::Error>())
            .map(io::Error::kind),
        Some(io::ErrorKind::BrokenPipe)
    );

    let serialize_error = wallet
        .run(
            &br#"{"protocol":"oxid.headless.v1","method":"system.quit"}
"#[..],
            FailingWriter,
        )
        .expect_err("response serialization should fail");
    assert!(matches!(&serialize_error, HeadlessIoError::Serialize(_)));
    assert_eq!(
        serialize_error.to_string(),
        "failed to serialize a headless wallet response"
    );
    assert!(
        serialize_error
            .source()
            .and_then(|source| source.downcast_ref::<serde_json::Error>())
            .is_some()
    );
}
