// SPDX-License-Identifier: Apache-2.0

use super::*;

impl HeadlessWallet {
    pub(super) fn list_vault_locks(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "vault.locks.list");
        }
        if let Err(response) = self.active_profile_id(request.id.clone()) {
            return Dispatch::continue_with(response);
        }
        match self.application.list_passport_vault_locks().execute() {
            Ok(vault) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "vault": passport_vault_value(&vault) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_error(request.id, error)),
        }
    }

    pub(super) fn decode_vault_contract_state(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DecodeVaultContractStateParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_state.decode requires only contractStateHex",
                ));
            }
        };
        let Some(serialized_contract_state) = decode_hex_bounded(
            &params.contract_state_hex,
            oxid_passport_vault_application::MAX_PASSPORT_VAULT_CONTRACT_STATE_BYTES,
        ) else {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "contractStateHex must be canonical bounded hexadecimal tagged Midnight state",
            ));
        };
        match self
            .application
            .decode_passport_vault_contract_state()
            .execute(DecodePassportVaultContractStateCommand {
                serialized_contract_state,
            }) {
            Ok(vault) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "vault": passport_vault_value(&vault) }),
            )),
            Err(error) => {
                Dispatch::continue_with(passport_vault_contract_state_error(request.id, error))
            }
        }
    }

    pub(super) fn read_vault_contract_state(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<ReadVaultContractStateParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_state.read requires only contractAddressHex",
                ));
            }
        };
        let Some(contract_address) = decode_hex_bounded(&params.contract_address_hex, 32)
            .filter(|address| address.len() == 32)
        else {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "contractAddressHex must be exactly 32 bytes of hexadecimal",
            ));
        };
        let result = futures::executor::block_on(
            self.application
                .read_passport_vault_contract_state()
                .execute(ReadPassportVaultContractStateCommand {
                    contract_address_hex: encode_hex(&contract_address),
                }),
        );
        match result {
            Ok(vault) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "vault": passport_vault_value(&vault) }),
            )),
            Err(error) => {
                Dispatch::continue_with(passport_vault_contract_state_read_error(request.id, error))
            }
        }
    }

    pub(super) fn prepare_vault_contract_call(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareVaultContractCallParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_call.prepare requires contractAddressHex and one typed action",
                ));
            }
        };
        let Some(contract_address) = decode_hex_bounded(&params.contract_address_hex, 32)
            .filter(|address| address.len() == 32)
        else {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "contractAddressHex must be exactly 32 bytes of hexadecimal",
            ));
        };
        let action = match vault_contract_call_action(request.id.clone(), params.action) {
            Ok(action) => action,
            Err(dispatch) => return *dispatch,
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let result =
            futures::executor::block_on(self.application.prepare_passport_vault_call().execute(
                PreparePassportVaultCallCommand {
                    profile_id,
                    contract_address_hex: encode_hex(&contract_address),
                    action,
                },
            ));
        match result {
            Ok(call) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "call": passport_vault_call_preview_value(&call) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_call_error(request.id, error)),
        }
    }

    pub(super) fn authorize_vault_contract_call(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AuthorizeVaultContractCallParams>(
            request.params,
        ) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_call.authorize requires draftId, authorizationChallenge, confirmed, and intent",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.authorize_passport_vault_call().execute(
            AuthorizePassportVaultCallCommand {
                profile_id,
                draft_id: params.draft_id,
                authorization_challenge: params.authorization_challenge,
                confirmed: params.confirmed,
                intent: params.intent,
            },
        ) {
            Ok(call) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "call": passport_vault_call_preview_value(&call) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_call_error(request.id, error)),
        }
    }

    pub(super) fn vault_contract_call_draft(&self, request: Request) -> Dispatch {
        self.vault_contract_call_query_operation(
            request,
            "vault.contract_call.draft",
            |application, query| {
                application
                    .get_passport_vault_call()
                    .execute(query)
                    .map(|call| json!({ "call": passport_vault_call_preview_value(&call) }))
            },
        )
    }

    pub(super) fn submit_vault_contract_call(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitVaultContractCallParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_call.submit requires draftId, confirmed, and intent",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let result =
            futures::executor::block_on(self.application.submit_passport_vault_call().execute(
                SubmitPassportVaultCallCommand {
                    profile_id,
                    draft_id: params.draft_id,
                    confirmed: params.confirmed,
                    intent: params.intent,
                },
            ));
        match result {
            Ok(submission) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submission": passport_vault_call_submission_value(&submission) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_call_error(request.id, error)),
        }
    }

    pub(super) fn start_vault_contract_call_submission(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitVaultContractCallParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.contract_call.start_submission requires draftId, confirmed, and intent",
                ));
            }
        };
        if !params.confirmed {
            return Dispatch::continue_with(passport_vault_call_error(
                request.id,
                PassportVaultCallError::ConfirmationRequired,
            ));
        }
        if params.intent != SUBMIT_PASSPORT_VAULT_CALL_INTENT {
            return Dispatch::continue_with(passport_vault_call_error(
                request.id,
                PassportVaultCallError::InvalidConfirmation,
            ));
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let query = PassportVaultCallQuery {
            profile_id: profile_id.clone(),
            draft_id: params.draft_id.clone(),
        };
        let preview = match self
            .application
            .get_passport_vault_call()
            .execute(query.clone())
        {
            Ok(preview) => preview,
            Err(error) => {
                return Dispatch::continue_with(passport_vault_call_error(request.id, error));
            }
        };
        match preview.state.as_str() {
            "authorized" | "submitting" | "submitted" => {}
            "expired" => {
                return Dispatch::continue_with(passport_vault_call_port_error(
                    request.id,
                    PassportVaultCallPortError::DraftExpired,
                ));
            }
            _ => {
                return Dispatch::continue_with(passport_vault_call_port_error(
                    request.id,
                    PassportVaultCallPortError::DraftConflict,
                ));
            }
        }
        let submit = self.application.submit_passport_vault_call();
        let command = SubmitPassportVaultCallCommand {
            profile_id,
            draft_id: params.draft_id,
            confirmed: true,
            intent: params.intent,
        };
        if thread::Builder::new()
            .name("oxid-headless-vault-submit".to_owned())
            .spawn(move || {
                let _ = futures::executor::block_on(submit.execute(command));
            })
            .is_err()
        {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unavailable",
                "Passport Vault submission worker could not be started",
            ));
        }
        let status = self.application.get_passport_vault_call_submission_status();
        for _ in 0..100 {
            match status.execute(query.clone()) {
                Ok(status) if status.state != "not_started" => {
                    return Dispatch::continue_with(Response::success(
                        request.id,
                        json!({
                            "submissionStatus": passport_vault_call_submission_status_value(&status)
                        }),
                    ));
                }
                Ok(_) => thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Dispatch::continue_with(passport_vault_call_error(request.id, error));
                }
            }
        }
        Dispatch::continue_with(Response::error(
            request.id,
            "unavailable",
            "Passport Vault submission worker did not start",
        ))
    }

    pub(super) fn vault_contract_call_submission_status(&self, request: Request) -> Dispatch {
        self.vault_contract_call_query_operation(
            request,
            "vault.contract_call.submission_status",
            |application, query| {
                application
                    .get_passport_vault_call_submission_status()
                    .execute(query)
                    .map(|status| {
                        json!({
                            "submissionStatus": passport_vault_call_submission_status_value(&status)
                        })
                    })
            },
        )
    }

    pub(super) fn cancel_vault_contract_call_submission(&self, request: Request) -> Dispatch {
        self.vault_contract_call_query_operation(
            request,
            "vault.contract_call.cancel_submission",
            |application, query| {
                application
                    .cancel_passport_vault_call_submission()
                    .execute(query)
                    .map(|status| {
                        json!({
                            "submissionStatus": passport_vault_call_submission_status_value(&status)
                        })
                    })
            },
        )
    }

    pub(super) fn reconcile_vault_contract_call_submission(&self, request: Request) -> Dispatch {
        self.vault_contract_call_query_operation(
            request,
            "vault.contract_call.reconcile_submission",
            |application, query| {
                futures::executor::block_on(
                    application
                        .reconcile_passport_vault_call_submission()
                        .execute(query),
                )
                .map(|status| {
                    json!({
                        "submissionStatus": passport_vault_call_submission_status_value(&status)
                    })
                })
            },
        )
    }

    pub(super) fn vault_contract_call_submission_history(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "vault.contract_call.submission_history");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_passport_vault_call_submissions()
            .execute(profile_id)
        {
            Ok(statuses) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "submissions": statuses
                        .iter()
                        .map(passport_vault_call_submission_status_value)
                        .collect::<Vec<_>>()
                }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_call_error(request.id, error)),
        }
    }

    pub(super) fn vault_contract_call_query_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            PassportVaultCallQuery,
        ) -> Result<Value, PassportVaultCallError>,
    ) -> Dispatch {
        let params = match serde_json::from_value::<VaultContractCallDraftParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    format!("{method} requires only a string draftId"),
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(
            &self.application,
            PassportVaultCallQuery {
                profile_id,
                draft_id: params.draft_id,
            },
        ) {
            Ok(value) => Dispatch::continue_with(Response::success(request.id, value)),
            Err(error) => Dispatch::continue_with(passport_vault_call_error(request.id, error)),
        }
    }

    pub(super) fn create_vault_lock(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CreateVaultLockParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.lock.create requires minimumAgeYears, maximumClaimAmount, initialAmount, confirmed, and intent",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let maximum_claim_amount = match decimal_u128(&params.maximum_claim_amount) {
            Some(value) => value,
            None => return invalid_vault_amount(request.id, "maximumClaimAmount"),
        };
        let initial_amount = match decimal_u128(&params.initial_amount) {
            Some(value) => value,
            None => return invalid_vault_amount(request.id, "initialAmount"),
        };
        let required_issuing_state = match policy_value(params.required_issuing_state) {
            Ok(value) => value,
            Err(()) => return invalid_vault_policy_value(request.id, "requiredIssuingState"),
        };
        let required_document_number = match policy_value(params.required_document_number) {
            Ok(value) => value,
            Err(()) => return invalid_vault_policy_value(request.id, "requiredDocumentNumber"),
        };
        match self.application.create_passport_vault_lock().execute(
            CreatePassportVaultLockCommand {
                profile_id,
                minimum_age_years: params.minimum_age_years,
                required_issuing_state,
                required_document_number,
                maximum_claim_amount,
                initial_amount,
                confirmed: params.confirmed,
                intent: params.intent,
            },
        ) {
            Ok(lock) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "lock": passport_vault_lock_value(&lock) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_error(request.id, error)),
        }
    }

    pub(super) fn deposit_to_vault_lock(&self, request: Request) -> Dispatch {
        self.vault_amount_operation(request, false)
    }

    pub(super) fn withdraw_from_vault_lock(&self, request: Request) -> Dispatch {
        self.vault_amount_operation(request, true)
    }

    pub(super) fn vault_amount_operation(&self, request: Request, withdraw: bool) -> Dispatch {
        let params = match serde_json::from_value::<VaultAmountParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault amount operations require lockId, amount, confirmed, and intent",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let amount = match decimal_u128(&params.amount) {
            Some(value) => value,
            None => return invalid_vault_amount(request.id, "amount"),
        };
        let command = PassportVaultAmountCommand {
            profile_id,
            lock_id: params.lock_id,
            amount,
            confirmed: params.confirmed,
            intent: params.intent,
        };
        let result = if withdraw {
            self.application
                .withdraw_passport_vault_lock()
                .execute(command)
        } else {
            self.application
                .deposit_passport_vault_lock()
                .execute(command)
        };
        match result {
            Ok(lock) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "lock": passport_vault_lock_value(&lock) }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_error(request.id, error)),
        }
    }

    pub(super) fn claim_from_vault_lock(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<ClaimVaultLockParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "vault.claim requires lockId, credentialId, amount, confirmed, and intent",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let amount = match decimal_u128(&params.amount) {
            Some(value) => value,
            None => return invalid_vault_amount(request.id, "amount"),
        };
        match futures::executor::block_on(self.application.claim_passport_vault_lock().execute(
            ClaimPassportVaultLockCommand {
                profile_id,
                lock_id: params.lock_id,
                credential_id: params.credential_id,
                amount,
                confirmed: params.confirmed,
                intent: params.intent,
            },
        )) {
            Ok(claim) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "releasedAmount": claim.released_amount,
                    "currentDay": claim.current_day,
                    "lock": passport_vault_lock_value(&claim.lock),
                }),
            )),
            Err(error) => Dispatch::continue_with(passport_vault_error(request.id, error)),
        }
    }
}
