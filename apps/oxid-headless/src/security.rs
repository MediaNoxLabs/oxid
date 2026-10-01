// SPDX-License-Identifier: Apache-2.0

use oxid_wallet_application::{GenerateWalletKeyCommand, WalletProfileSecurityCommand};
use serde_json::json;

use crate::{
    HeadlessWallet,
    errors::{invalid_empty_params, key_error, security_error},
    parameters::{GenerateKeyParams, key_algorithm, key_purpose},
    projections::{key_value, security_status_value},
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
        // Headless has no trusted approval surface. Neither legacy confirmation
        // prose nor an asserted JSON capability can cross this boundary.
        Dispatch::continue_with(Response::error(
            request.id,
            "approval_unavailable",
            "approval_unavailable",
        ))
    }

    pub(super) fn delete_key(&self, request: Request) -> Dispatch {
        Dispatch::continue_with(Response::error(
            request.id,
            "approval_unavailable",
            "approval_unavailable",
        ))
    }
}
