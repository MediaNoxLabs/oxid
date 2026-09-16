// SPDX-License-Identifier: Apache-2.0

//! Narrow development-only incoming adapter for funding standalone wallets.

use std::{
    collections::VecDeque,
    io::{self, BufRead, Read as _, Write},
};

use oxid_composition::ApplicationServices;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::faucet_application::ApplicationNightGrant;
use crate::faucet_errors::{FaucetIoError, FaucetStartupError};

const PROTOCOL_VERSION: &str = "oxid.standalone-faucet.v1";
pub(super) const NETWORK_ID: &str = "undeployed";
pub(super) const FIXED_GRANT_ATOMIC_UNITS: u128 = 50_000_000_000;
const MAX_REQUEST_BYTES: usize = 4 * 1024;
const MAX_REQUEST_ID_CHARACTERS: usize = 128;
const MAX_RECIPIENT_CHARACTERS: usize = 256;
const MAX_RECEIPTS: usize = 256;

/// A development faucet that accepts only health, fixed NIGHT grants, and shutdown.
pub struct StandaloneFaucet {
    grant: Box<dyn NightGrantPort>,
    receipts: VecDeque<FundingReceipt>,
}

impl StandaloneFaucet {
    pub fn new(application: ApplicationServices) -> Result<Self, FaucetStartupError> {
        Ok(Self {
            grant: Box::new(ApplicationNightGrant::new(application)?),
            receipts: VecDeque::new(),
        })
    }

    /// Processes bounded line-delimited JSON requests until EOF or shutdown.
    pub fn run<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<(), FaucetIoError> {
        loop {
            let dispatch = match read_frame(&mut reader).map_err(FaucetIoError::Read)? {
                None => return Ok(()),
                Some(RequestFrame::Empty) => continue,
                Some(RequestFrame::TooLarge) => Dispatch::continue_with(Response::error(
                    None,
                    "request_too_large",
                    "request exceeds 4096 bytes",
                )),
                Some(RequestFrame::InvalidUtf8) => Dispatch::continue_with(Response::error(
                    None,
                    "invalid_request",
                    "request must be valid UTF-8",
                )),
                Some(RequestFrame::Line(line)) => self.dispatch(&line),
            };
            serde_json::to_writer(&mut writer, &dispatch.response)
                .map_err(FaucetIoError::Serialize)?;
            writer.write_all(b"\n").map_err(FaucetIoError::Write)?;
            writer.flush().map_err(FaucetIoError::Write)?;
            if dispatch.should_exit {
                return Ok(());
            }
        }
    }

    fn dispatch(&mut self, line: &str) -> Dispatch {
        let request = match serde_json::from_str::<Request>(line) {
            Ok(request) => request,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    None,
                    "invalid_request",
                    "request must be a closed faucet protocol object",
                ));
            }
        };
        if request.protocol != PROTOCOL_VERSION {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unsupported_protocol",
                "request protocol is not supported",
            ));
        }
        if !valid_request_id(request.id.as_deref()) {
            return Dispatch::continue_with(Response::error(
                None,
                "invalid_request",
                "id must be a bounded public identifier",
            ));
        }

        match request.method.as_str() {
            "faucet.health" if empty_object(&request.params) => {
                Dispatch::continue_with(Response::success(request.id, health_value()))
            }
            "faucet.fund" => self.fund(request),
            "faucet.shutdown" if empty_object(&request.params) => Dispatch::exit(
                Response::success(request.id, json!({ "shuttingDown": true })),
            ),
            "faucet.health" | "faucet.shutdown" => Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "method does not accept parameters",
            )),
            _ => Dispatch::continue_with(Response::error(
                request.id,
                "method_not_found",
                "method is not supported",
            )),
        }
    }

    fn fund(&mut self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<FundParams>(request.params) {
            Ok(params)
                if valid_token(&params.request_id, MAX_REQUEST_ID_CHARACTERS)
                    && valid_recipient_shape(&params.recipient_address) =>
            {
                params
            }
            _ => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "funding requires bounded requestId and an undeployed NIGHT address",
                ));
            }
        };

        if let Some(receipt) = self
            .receipts
            .iter()
            .find(|receipt| receipt.request_id == params.request_id)
            .cloned()
        {
            return if receipt.recipient_address == params.recipient_address {
                retained_response(request.id, &receipt, true)
            } else {
                Dispatch::continue_with(Response::error(
                    request.id,
                    "idempotency_conflict",
                    "requestId was already used for different public input",
                ))
            };
        }
        if let Some(receipt) = self
            .receipts
            .iter()
            .find(|receipt| receipt.recipient_address == params.recipient_address)
            .cloned()
        {
            let alias = FundingReceipt {
                request_id: params.request_id,
                recipient_address: params.recipient_address,
                outcome: receipt.outcome,
            };
            self.retain(alias.clone());
            return retained_response(request.id, &alias, true);
        }

        match self.grant.grant(&params.recipient_address) {
            Ok(outcome) => {
                let receipt = FundingReceipt {
                    request_id: params.request_id,
                    recipient_address: params.recipient_address,
                    outcome: RetainedGrantOutcome::Included {
                        transaction_id: outcome.transaction_id,
                        block_id: outcome.block_id,
                    },
                };
                self.retain(receipt.clone());
                retained_response(request.id, &receipt, false)
            }
            Err(GrantError::OutcomeUnknown) => {
                self.retain(FundingReceipt {
                    request_id: params.request_id,
                    recipient_address: params.recipient_address,
                    outcome: RetainedGrantOutcome::OutcomeUnknown,
                });
                Dispatch::continue_with(Response::error(
                    request.id,
                    GrantError::OutcomeUnknown.code(),
                    GrantError::OutcomeUnknown.message(),
                ))
            }
            Err(error) => {
                Dispatch::continue_with(Response::error(request.id, error.code(), error.message()))
            }
        }
    }

    fn retain(&mut self, receipt: FundingReceipt) {
        if self.receipts.len() == MAX_RECEIPTS {
            self.receipts.pop_front();
        }
        self.receipts.push_back(receipt);
    }
}

enum RequestFrame {
    Empty,
    Line(String),
    TooLarge,
    InvalidUtf8,
}

fn read_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<RequestFrame>> {
    let mut bytes = Vec::with_capacity(MAX_REQUEST_BYTES + 1);
    let read = reader
        .by_ref()
        .take((MAX_REQUEST_BYTES + 1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > MAX_REQUEST_BYTES {
        if bytes.last() != Some(&b'\n') {
            reader.skip_until(b'\n')?;
        }
        return Ok(Some(RequestFrame::TooLarge));
    }
    match String::from_utf8(bytes) {
        Ok(line) if line.trim().is_empty() => Ok(Some(RequestFrame::Empty)),
        Ok(line) => Ok(Some(RequestFrame::Line(line))),
        Err(_) => Ok(Some(RequestFrame::InvalidUtf8)),
    }
}

pub(super) trait NightGrantPort: Send + Sync {
    fn grant(&self, recipient_address: &str) -> Result<GrantOutcome, GrantError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GrantOutcome {
    pub(super) transaction_id: String,
    pub(super) block_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GrantError {
    InvalidRecipient,
    NetworkMismatch,
    AuthorityNotReady,
    Rejected,
    OutcomeUnknown,
    Unavailable,
}

impl GrantError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::InvalidRecipient => "invalid_recipient",
            Self::NetworkMismatch => "network_mismatch",
            Self::AuthorityNotReady => "authority_not_ready",
            Self::Rejected => "submission_rejected",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unavailable => "unavailable",
        }
    }

    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::InvalidRecipient => "recipient address is invalid",
            Self::NetworkMismatch => "recipient address belongs to another network",
            Self::AuthorityNotReady => "standalone funding authority is not ready",
            Self::Rejected => "Midnight rejected the funding transaction",
            Self::OutcomeUnknown => "funding transaction outcome is not yet known",
            Self::Unavailable => "standalone funding is unavailable",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol: String,
    #[serde(default)]
    id: Option<String>,
    method: String,
    #[serde(default = "empty_params")]
    params: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FundParams {
    request_id: String,
    recipient_address: String,
}

#[derive(Clone)]
struct FundingReceipt {
    request_id: String,
    recipient_address: String,
    outcome: RetainedGrantOutcome,
}

#[derive(Clone)]
enum RetainedGrantOutcome {
    Included {
        transaction_id: String,
        block_id: String,
    },
    OutcomeUnknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    protocol: &'static str,
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
}

impl Response {
    fn success(id: Option<String>, result: Value) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<String>, code: &'static str, message: &'static str) -> Self {
        Self {
            protocol: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(ErrorBody { code, message }),
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

struct Dispatch {
    response: Response,
    should_exit: bool,
}

impl Dispatch {
    const fn continue_with(response: Response) -> Self {
        Self {
            response,
            should_exit: false,
        }
    }

    const fn exit(response: Response) -> Self {
        Self {
            response,
            should_exit: true,
        }
    }
}

fn health_value() -> Value {
    json!({
        "ready": true,
        "protocol": PROTOCOL_VERSION,
        "networkId": NETWORK_ID,
        "routeProfile": "localhost",
        "authorityClass": "public_standalone_genesis",
        "maxInFlight": 1,
        "grant": {
            "assetId": "midnight:native:night",
            "symbol": "NIGHT",
            "decimals": 6,
            "atomicUnits": FIXED_GRANT_ATOMIC_UNITS.to_string()
        },
        "retainedReceiptCapacity": MAX_RECEIPTS
    })
}

fn retained_response(id: Option<String>, receipt: &FundingReceipt, deduplicated: bool) -> Dispatch {
    match &receipt.outcome {
        RetainedGrantOutcome::Included {
            transaction_id,
            block_id,
        } => Dispatch::continue_with(Response::success(
            id,
            receipt_value(receipt, transaction_id, block_id, deduplicated),
        )),
        RetainedGrantOutcome::OutcomeUnknown => Dispatch::continue_with(Response::error(
            id,
            GrantError::OutcomeUnknown.code(),
            GrantError::OutcomeUnknown.message(),
        )),
    }
}

fn receipt_value(
    receipt: &FundingReceipt,
    transaction_id: &str,
    block_id: &str,
    deduplicated: bool,
) -> Value {
    json!({
        "receipt": {
            "requestId": receipt.request_id,
            "networkId": NETWORK_ID,
            "recipientAddress": receipt.recipient_address,
            "amount": {
                "assetId": "midnight:native:night",
                "symbol": "NIGHT",
                "decimals": 6,
                "atomicUnits": FIXED_GRANT_ATOMIC_UNITS.to_string()
            },
            "state": "included",
            "transactionId": transaction_id,
            "blockId": block_id,
            "deduplicated": deduplicated
        }
    })
}

fn valid_request_id(value: Option<&str>) -> bool {
    value.is_none_or(|value| valid_token(value, MAX_REQUEST_ID_CHARACTERS))
}

fn valid_token(value: &str, maximum: usize) -> bool {
    let count = value.chars().count();
    count > 0
        && count <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_recipient_shape(value: &str) -> bool {
    let count = value.chars().count();
    count > "mn_addr_undeployed1".len()
        && count <= MAX_RECIPIENT_CHARACTERS
        && value.starts_with("mn_addr_undeployed1")
        && value.bytes().all(|byte| byte.is_ascii_graphic())
        && !value.contains('@')
        && !value.contains("://")
}

fn empty_params() -> Value {
    json!({})
}

fn empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(serde_json::Map::is_empty)
}

#[cfg(test)]
#[path = "faucet_tests.rs"]
mod tests;
