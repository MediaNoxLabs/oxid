// SPDX-License-Identifier: Apache-2.0

use super::*;

fn material() -> CustodyBytes {
    CustodyBytes::receive(4, |bytes| {
        bytes.fill(42);
        Ok(bytes.len())
    })
    .expect("bounded fixture")
}

fn control(operation: Operation, status: Status, protection: Option<Protection>) -> Vec<u8> {
    serde_json::to_vec(&Control {
        version: 1,
        operation,
        status,
        protection,
    })
    .expect("public control")
}

#[test]
fn mutable_native_source_is_wiped_on_success_and_invalid_length() {
    let mut source = [42; 4];
    let bytes = CustodyBytes::take_native(&mut source).expect("valid length");
    assert!(source.iter().all(|byte| *byte == 0));
    assert!(bytes.as_bytes().iter().all(|byte| *byte == 42));
    let mut oversized = vec![42; MAX_CUSTODY_BYTES + 1];
    assert!(CustodyBytes::take_native(&mut oversized).is_err());
    assert!(oversized.iter().all(|byte| *byte == 0));
    assert!(CustodyBytes::take_native(&mut []).is_err());
}

#[test]
fn first_owned_copy_is_bounded_and_fill_failure_never_returns_material() {
    for len in [0, MAX_CUSTODY_BYTES + 1] {
        assert!(CustodyBytes::receive(len, |_| panic!("must not call native fill")).is_err());
    }
    assert!(CustodyBytes::receive(MAX_CUSTODY_BYTES, |bytes| Ok(bytes.len())).is_ok());
    assert!(matches!(
        CustodyBytes::receive(8, |bytes| {
            bytes[..4].fill(42);
            Ok(4)
        }),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        CustodyBytes::receive(8, |bytes| {
            bytes.fill(42);
            Err(Error::Failed)
        }),
        Err(Error::Failed)
    ));
    assert_eq!(format!("{:?}", material()), "CustodyBytes([REDACTED])");
}

#[test]
fn strict_control_rejects_legacy_payloads_and_untrusted_shapes() {
    for raw in [
        r#"{"status":"succeeded","payload":"KioqKg=="}"#,
        r#"{"version":1,"operation":"load","status":"succeeded","protection":"operating_system","payload":null}"#,
        r#"{"version":1,"operation":"load","status":"succeeded","protection":"operating_system","bytes":[42]}"#,
        r#"{"version":1,"operation":"load","status":"succeeded","protection":"operating_system","handle":1}"#,
        r#"{"version":2,"operation":"load","status":"succeeded","protection":"operating_system"}"#,
        r#"{"version":1,"operation":"save","status":"succeeded","protection":"operating_system"}"#,
        r#"{"version":1,"operation":"load","status":"plaintext","protection":"operating_system"}"#,
        r#"{"version":1,"version":1,"operation":"load","status":"succeeded","protection":"operating_system"}"#,
        r#"{"version":1,"operation":"load","status":"succeeded","protection":"made_up"}"#,
        "not json",
    ] {
        assert!(matches!(
            decode_reply(Operation::Load, raw.as_bytes(), Some(material())),
            Err(Error::Invalid)
        ));
    }
    assert!(decode_reply(Operation::Load, &[b' '; MAX_CONTROL_BYTES + 1], None).is_err());
}

#[test]
fn load_and_unlock_require_protected_success_with_separate_bytes() {
    for operation in [Operation::Load, Operation::Unlock] {
        for protection in [Protection::OperatingSystem, Protection::HardwareBacked] {
            let raw = control(operation, Status::Succeeded, Some(protection));
            assert!(decode_reply(operation, &raw, None).is_err());
            let reply = decode_reply(operation, &raw, Some(material())).expect("valid reply");
            assert!(matches!(reply, Reply::Material { .. }));
            assert!(!format!("{reply:?}").contains("42"));
        }
        let raw = control(operation, Status::Succeeded, None);
        assert!(decode_reply(operation, &raw, Some(material())).is_err());
    }
}

#[test]
fn failure_statuses_never_release_material_or_accept_protection() {
    for operation in [
        Operation::Load,
        Operation::Save,
        Operation::Unlock,
        Operation::Initialize,
        Operation::Lock,
        Operation::Inspect,
    ] {
        for (status, error) in [
            (Status::Unavailable, Error::Unavailable),
            (Status::NotInitialized, Error::NotInitialized),
            (Status::AlreadyInitialized, Error::AlreadyInitialized),
            (Status::AuthorizationDenied, Error::AuthorizationDenied),
            (Status::Cancelled, Error::Cancelled),
            (Status::TimedOut, Error::TimedOut),
            (Status::Invalid, Error::Invalid),
            (Status::Failed, Error::Failed),
        ] {
            let raw = control(operation, status, None);
            assert!(matches!(decode_reply(operation, &raw, None), Err(actual) if actual == error));
            assert!(matches!(
                decode_reply(operation, &raw, Some(material())),
                Err(Error::Invalid)
            ));
            let raw = control(operation, status, Some(Protection::HardwareBacked));
            assert!(matches!(
                decode_reply(operation, &raw, None),
                Err(Error::Invalid)
            ));
        }
    }
}

#[test]
fn save_and_initialize_success_are_metadata_only() {
    for operation in [Operation::Save, Operation::Initialize] {
        let raw = control(
            operation,
            Status::Succeeded,
            Some(Protection::OperatingSystem),
        );
        assert!(matches!(
            decode_reply(operation, &raw, None),
            Ok(Reply::Stored(Protection::OperatingSystem))
        ));
        assert!(decode_reply(operation, &raw, Some(material())).is_err());
        let raw = control(operation, Status::Succeeded, None);
        assert!(decode_reply(operation, &raw, None).is_err());
    }
}

#[test]
fn state_and_lock_responses_cannot_smuggle_custody() {
    for status in [Status::Locked, Status::Unlocked] {
        let raw = control(
            Operation::Inspect,
            status,
            Some(Protection::OperatingSystem),
        );
        assert!(decode_reply(Operation::Inspect, &raw, None).is_ok());
        assert!(decode_reply(Operation::Inspect, &raw, Some(material())).is_err());
    }
    let raw = control(Operation::Inspect, Status::Uninitialized, None);
    assert!(matches!(
        decode_reply(Operation::Inspect, &raw, None),
        Ok(Reply::Uninitialized)
    ));
    let raw = control(
        Operation::Lock,
        Status::Locked,
        Some(Protection::OperatingSystem),
    );
    assert!(matches!(
        decode_reply(Operation::Lock, &raw, None),
        Ok(Reply::Locked(_))
    ));
    assert!(decode_reply(Operation::Lock, &raw, Some(material())).is_err());
    for operation in [Operation::Load, Operation::Unlock, Operation::Save] {
        let raw = control(operation, Status::Locked, None);
        assert!(matches!(
            decode_reply(operation, &raw, None),
            Err(Error::Locked)
        ));
    }
}
