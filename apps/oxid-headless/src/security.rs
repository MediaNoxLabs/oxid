// SPDX-License-Identifier: Apache-2.0

use oxid_wallet_application::{
    DeleteWalletKeyCommand, GenerateWalletKeyCommand, SignWalletDataCommand,
    WalletProfileSecurityCommand,
};
use serde_json::json;

use crate::{
    HeadlessWallet,
    parameters::{DeleteKeyParams, GenerateKeyParams, SignParams},
    projections::{
        algorithm_name, decode_hex, encode_hex, invalid_empty_params, key_algorithm, key_error,
        key_purpose, key_value, security_error, security_status_value, sensitive_error,
    },
    protocol::{Dispatch, Request, Response, params_are_empty},
};

impl HeadlessWallet {
    pub(super) fn security_status(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.status");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_security_status()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    pub(super) fn initialize_security(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.initialize");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .initialize_wallet_security()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    pub(super) fn unlock_wallet(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.unlock");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .unlock_wallet()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    pub(super) fn lock_wallet(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.security.lock");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .lock_wallet()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "security": security_status_value(status) }),
            )),
            Err(error) => Dispatch::continue_with(security_error(request.id, error)),
        }
    }

    pub(super) fn generate_key(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<GenerateKeyParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.generate requires only label, algorithm, and purpose strings",
                ));
            }
        };
        let algorithm = match key_algorithm(&params.algorithm) {
            Some(algorithm) => algorithm,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "algorithm must be ed25519, p256, secp256k1-schnorr, or jubjub",
                ));
            }
        };
        let purpose = match key_purpose(&params.purpose) {
            Some(purpose) => purpose,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "purpose is not supported",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .generate_wallet_key()
            .execute(GenerateWalletKeyCommand {
                profile_id,
                label: params.label,
                algorithm,
                purpose,
            }) {
            Ok(key) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "key": key_value(&key) }),
            )),
            Err(error) => Dispatch::continue_with(key_error(request.id, error)),
        }
    }

    pub(super) fn list_keys(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.key.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_keys()
            .execute(WalletProfileSecurityCommand { profile_id })
        {
            Ok(keys) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "keys": keys.iter().map(key_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(key_error(request.id, error)),
        }
    }

    pub(super) fn sign(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SignParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.sign requires keyRef, payloadHex, and confirmation",
                ));
            }
        };
        let payload = match decode_hex(&params.payload_hex) {
            Some(payload) => payload,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "payloadHex must be bounded even-length hexadecimal",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .sign_wallet_data()
            .execute(SignWalletDataCommand {
                profile_id,
                key_reference: params.key_reference,
                payload,
                confirmation: params.confirmation.into(),
            }) {
            Ok(signature) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "algorithm": algorithm_name(signature.algorithm),
                    "signatureHex": encode_hex(&signature.signature_bytes)
                }),
            )),
            Err(error) => Dispatch::continue_with(sensitive_error(request.id, error)),
        }
    }

    pub(super) fn delete_key(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeleteKeyParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.key.delete requires keyRef and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .delete_wallet_key()
            .execute(DeleteWalletKeyCommand {
                profile_id,
                key_reference: params.key_reference,
                confirmation: params.confirmation.into(),
            }) {
            Ok(()) => {
                Dispatch::continue_with(Response::success(request.id, json!({ "deleted": true })))
            }
            Err(error) => Dispatch::continue_with(sensitive_error(request.id, error)),
        }
    }
}
