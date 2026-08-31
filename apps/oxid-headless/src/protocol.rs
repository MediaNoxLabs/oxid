// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, io};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{MAX_REQUEST_ID_CHARACTERS, PROTOCOL_VERSION};

#[derive(Deserialize)]
pub(super) struct Request {
    pub(super) protocol: String,
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) method: String,
    #[serde(default = "empty_params")]
    pub(super) params: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Response {
    protocol: &'static str,
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl Response {
    pub(super) fn success(id: Option<String>, result: Value) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub(super) fn error(
        id: Option<String>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(ErrorBody {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

pub(super) struct Dispatch {
    pub(super) response: Response,
    pub(super) should_exit: bool,
}

impl Dispatch {
    pub(super) const fn continue_with(response: Response) -> Self {
        Self {
            response,
            should_exit: false,
        }
    }

    pub(super) const fn exit(response: Response) -> Self {
        Self {
            response,
            should_exit: true,
        }
    }
}

pub(super) fn request_id(value: &Value) -> Result<Option<String>, &'static str> {
    let Some(id) = value.get("id") else {
        return Ok(None);
    };
    let Some(id) = id.as_str() else {
        return Err("id must be a string when present");
    };
    let character_count = id.chars().count();
    if character_count == 0 || character_count > MAX_REQUEST_ID_CHARACTERS {
        return Err("id must contain between 1 and 128 characters");
    }

    Ok(Some(id.to_owned()))
}

pub(super) fn empty_params() -> Value {
    json!({})
}

pub(super) fn params_are_empty(params: &Value) -> bool {
    params.as_object().is_some_and(serde_json::Map::is_empty)
}

/// Failures while reading or writing the headless protocol stream.
#[derive(Debug)]
pub enum HeadlessIoError {
    Read(io::Error),
    Write(io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for HeadlessIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(_) => formatter.write_str("failed to read a headless wallet request"),
            Self::Write(_) => formatter.write_str("failed to write a headless wallet response"),
            Self::Serialize(_) => {
                formatter.write_str("failed to serialize a headless wallet response")
            }
        }
    }
}

impl Error for HeadlessIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) | Self::Write(error) => Some(error),
            Self::Serialize(error) => Some(error),
        }
    }
}

pub(super) const DISPATCH_METHODS: &[&str] = &[
    "system.capabilities",
    "system.diagnostics.snapshot",
    "system.diagnostics.clear",
    "system.quit",
    "wallet.profile.create",
    "wallet.profile.list",
    "wallet.profile.select",
    "wallet.profile.active",
    "wallet.security.status",
    "wallet.security.initialize",
    "wallet.security.unlock",
    "wallet.security.lock",
    "wallet.key.generate",
    "wallet.key.list",
    "wallet.key.sign",
    "wallet.key.delete",
    "wallet.network.list",
    "wallet.network.select",
    "wallet.account.derive",
    "wallet.account.get",
    "wallet.address.list",
    "wallet.address.unshielded",
    "wallet.address.shielded",
    "wallet.balance.snapshot",
    "wallet.transaction.history",
    "wallet.transaction.prepare_unshielded",
    "wallet.transaction.prepare_shielded",
    "wallet.transaction.authorize_unshielded",
    "wallet.transaction.authorize_shielded",
    "wallet.transaction.submit_unshielded",
    "wallet.transaction.send_unshielded",
    "wallet.transaction.submit_shielded",
    "wallet.transaction.send_shielded",
    "wallet.transaction.start_submission",
    "wallet.transaction.submission_status",
    "wallet.transaction.submission_history",
    "wallet.transaction.reconcile_submission",
    "wallet.transaction.cancel_submission",
    "wallet.transaction.draft",
    "wallet.connect",
    "wallet.sync.force",
    "wallet.dust.sync.status",
    "wallet.dust.sync.start",
    "wallet.dust.sync.cancel",
    "wallet.dust.registration.prepare",
    "wallet.dust.registration.authorize",
    "wallet.dust.registration.submit",
    "wallet.dust.registration.start_submission",
    "wallet.dust.registration.draft",
    "wallet.dust.registration.status",
    "wallet.dust.registration.cancel_submission",
    "wallet.dust.registration.reconcile_submission",
    "wallet.shielded.sync.status",
    "wallet.shielded.sync.start",
    "wallet.shielded.sync.cancel",
    "vault.total_locked",
    "vault.locks.list",
    "vault.contract_state.decode",
    "vault.contract_state.read",
    "vault.contract_call.prepare",
    "vault.contract_call.authorize",
    "vault.contract_call.draft",
    "vault.contract_call.submit",
    "vault.contract_call.start_submission",
    "vault.contract_call.submission_status",
    "vault.contract_call.submission_history",
    "vault.contract_call.cancel_submission",
    "vault.contract_call.reconcile_submission",
    "vault.lock.create",
    "vault.deposit",
    "vault.claim",
    "vault.withdraw",
    "did.create",
    "did.resolve",
    "did.list",
    "did.get",
    "did.update",
    "did.sign",
    "did.deactivate",
    "did.forget",
    "credential.receive",
    "credential.request",
    "credential.list",
    "vault.credentials.list",
    "credential.get",
    "credential.reverify",
    "credential.verify",
    "credential.delete",
    "credential.disclosure.candidates",
    "credential.disclosure.preview",
    "credential.issuance.prepare",
    "credential.issuance.accept",
    "credential.issuance.refuse",
    "credential.issuance.get",
    "credential.issuance.list",
    "credential.presentation.prepare",
    "credential.presentation.accept",
    "credential.presentation.refuse",
    "credential.presentation.get",
    "credential.presentation.list",
    "identity.request.route",
    "identity.login",
    "identity.authentication.prepare",
    "identity.authentication.accept",
    "identity.authentication.refuse",
    "identity.authentication.get",
    "identity.authentication.list",
];

pub(super) fn is_dispatch_method(method: &str) -> bool {
    DISPATCH_METHODS.contains(&method)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::DISPATCH_METHODS;

    #[test]
    fn checked_in_vocabulary_is_the_exact_dispatch_allowlist() {
        let vocabulary: Value =
            serde_json::from_str(include_str!("../tests/fixtures/protocol-vocabulary.json"))
                .expect("protocol vocabulary should be valid JSON");
        let expected = vocabulary["dispatchMethods"]
            .as_array()
            .expect("dispatchMethods should be an array")
            .iter()
            .map(|method| method.as_str().expect("method should be a string"))
            .collect::<BTreeSet<_>>();
        let actual = DISPATCH_METHODS.iter().copied().collect::<BTreeSet<_>>();

        assert_eq!(actual.len(), DISPATCH_METHODS.len());
        assert_eq!(actual, expected);
    }
}
