// SPDX-License-Identifier: Apache-2.0

use oxid_identity_application::{
    CreateDidCommand, DeactivateDidCommand, DidRecordQuery, ListDidRecordsQuery, ResolveDidCommand,
    SignDidPayloadCommand, UpdateDidCommand,
};
use serde_json::json;

use crate::{
    HeadlessWallet,
    errors::{did_error, invalid_empty_params},
    parameters::{
        CreateDidParams, DeactivateDidParams, DidParams, DidUpdateParams, SignDidParams,
        decode_hex, did_update,
    },
    projections::{did_record_value, encode_hex},
    protocol::{Dispatch, Request, Response, params_are_empty},
};

impl HeadlessWallet {
    pub(super) fn resolve_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.resolve requires only a string did field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.resolve_did().execute(
            ResolveDidCommand {
                profile_id,
                did: params.did,
            },
        )) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn create_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CreateDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.create accepts only an optional network string",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.create_did().execute(CreateDidCommand {
            profile_id,
            network: params.network,
        }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn update_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DidUpdateParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.update requires a supported operation and its exact fields",
                ));
            }
        };
        let (did, operation, confirmation) = match did_update(params) {
            Some(value) => value,
            None => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "DID update algorithm or relationship is unsupported",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.update_did().execute(UpdateDidCommand {
            profile_id,
            did,
            operation,
            confirmation,
        }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn sign_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SignDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.sign requires did, methodId, payloadHex, and confirmation",
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
            .sign_did_payload()
            .execute(SignDidPayloadCommand {
                profile_id,
                did: params.did,
                method_id: params.method_id,
                payload,
                confirmation: params.confirmation.into(),
            }) {
            Ok(signature) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "methodId": signature.method_id,
                    "algorithm": signature.algorithm,
                    "signatureHex": encode_hex(&signature.signature_bytes),
                }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn deactivate_did(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeactivateDidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "did.deactivate requires did and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .deactivate_did()
            .execute(DeactivateDidCommand {
                profile_id,
                did: params.did,
                confirmation: params.confirmation.into(),
            }) {
            Ok(record) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecord": did_record_value(&record) }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn list_dids(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "did.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_did_records()
            .execute(ListDidRecordsQuery { profile_id })
        {
            Ok(records) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "didRecords": records.iter().map(did_record_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(did_error(request.id, error)),
        }
    }

    pub(super) fn get_did(&self, request: Request) -> Dispatch {
        self.did_record_operation(request, false)
    }

    pub(super) fn forget_did(&self, request: Request) -> Dispatch {
        self.did_record_operation(request, true)
    }

    pub(super) fn did_record_operation(&self, request: Request, remove: bool) -> Dispatch {
        let params = match serde_json::from_value::<DidParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    if remove {
                        "did.forget requires only a string did field"
                    } else {
                        "did.get requires only a string did field"
                    },
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let query = DidRecordQuery {
            profile_id,
            did: params.did,
        };
        if remove {
            match self.application.forget_did().execute(query) {
                Ok(()) => Dispatch::continue_with(Response::success(
                    request.id,
                    json!({ "forgotten": true }),
                )),
                Err(error) => Dispatch::continue_with(did_error(request.id, error)),
            }
        } else {
            match self.application.get_did_record().execute(query) {
                Ok(record) => Dispatch::continue_with(Response::success(
                    request.id,
                    json!({ "didRecord": did_record_value(&record) }),
                )),
                Err(error) => Dispatch::continue_with(did_error(request.id, error)),
            }
        }
    }
}
