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
