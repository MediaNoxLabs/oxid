// SPDX-License-Identifier: Apache-2.0

use super::*;

impl HeadlessWallet {
    pub(super) fn sync_account(&self, request: Request) -> Dispatch {
        let method = match request.method.as_str() {
            "wallet.connect" => "wallet.connect",
            _ => "wallet.sync.force",
        };
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .sync_wallet_account()
                .execute(WalletAccountQuery { profile_id }),
        ) {
            Ok(account) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "account": account_value(&account) }),
            )),
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }

    pub(super) fn dust_sync_status(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(
            request,
            "wallet.dust.sync.status",
            |application, command| application.get_wallet_dust_sync_status().execute(command),
        )
    }

    pub(super) fn start_dust_sync(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(request, "wallet.dust.sync.start", |application, command| {
            application.start_wallet_dust_sync().execute(command)
        })
    }

    pub(super) fn cancel_dust_sync(&self, request: Request) -> Dispatch {
        self.dust_sync_operation(
            request,
            "wallet.dust.sync.cancel",
            |application, command| application.cancel_wallet_dust_sync().execute(command),
        )
    }

    pub(super) fn dust_sync_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletDustSyncCommand,
        ) -> Result<WalletDustSyncView, WalletDustSyncError>,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(&self.application, WalletDustSyncCommand { profile_id }) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "dustSync": dust_sync_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(dust_sync_error(request.id, error)),
        }
    }

    pub(super) fn prepare_dust_registration(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.dust.registration.prepare");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .prepare_wallet_dust_registration()
            .execute(PrepareWalletDustRegistrationCommand { profile_id })
        {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "registration": dust_registration_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn authorize_dust_registration(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AuthorizeDustRegistrationParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.dust.registration.authorize requires only string draftId and authorizationChallenge fields plus confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .authorize_wallet_dust_registration()
            .execute(AuthorizeWalletDustRegistrationCommand {
                profile_id,
                draft_id: params.draft_id,
                authorization_challenge: params.authorization_challenge,
                confirmation: params.confirmation.into(),
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "registration": dust_registration_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn submit_dust_registration(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.dust.registration.submit requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application.submit_wallet_dust_registration().execute(
                SubmitWalletDustRegistrationCommand {
                    profile_id,
                    draft_id: params.draft_id,
                    confirmation: params.confirmation.into(),
                },
            ),
        ) {
            Ok(submission) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submission": dust_registration_submission_value(&submission) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn start_dust_registration_submission(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.dust.registration.start_submission requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let preview = match self.application.get_wallet_dust_registration().execute(
            GetWalletDustRegistrationCommand {
                profile_id: profile_id.clone(),
                draft_id: params.draft_id.clone(),
            },
        ) {
            Ok(preview) => preview,
            Err(error) => {
                return Dispatch::continue_with(dust_registration_error(request.id, error));
            }
        };
        match preview.state.as_str() {
            "authorized" | "submitting" | "submitted" => {}
            "expired" => {
                return Dispatch::continue_with(dust_registration_port_error(
                    request.id,
                    WalletDustRegistrationPortError::DraftExpired,
                ));
            }
            _ => {
                return Dispatch::continue_with(dust_registration_port_error(
                    request.id,
                    WalletDustRegistrationPortError::DraftConflict,
                ));
            }
        }

        let confirmation: SensitiveOperationConfirmation = params.confirmation.into();
        if let Err(error) = validate_confirmation(&confirmation) {
            return Dispatch::continue_with(sensitive_error(request.id, error));
        }
        let service = self.application.submit_wallet_dust_registration();
        let command = SubmitWalletDustRegistrationCommand {
            profile_id: profile_id.clone(),
            draft_id: params.draft_id.clone(),
            confirmation,
        };
        if thread::Builder::new()
            .name("oxid-headless-dust-register".to_owned())
            .spawn(move || {
                let _ = futures::executor::block_on(service.execute(command));
            })
            .is_err()
        {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unavailable",
                "DUST registration submission worker could not be started",
            ));
        }

        let service = self.application.get_wallet_dust_registration_status();
        let command = GetWalletDustRegistrationStatusCommand {
            profile_id,
            draft_id: params.draft_id,
        };
        for _ in 0..100 {
            match service.execute(command.clone()) {
                Ok(status) if status.state != "not_started" => {
                    return Dispatch::continue_with(Response::success(
                        request.id,
                        json!({
                            "registrationStatus": dust_registration_status_value(&status)
                        }),
                    ));
                }
                Ok(_) => thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Dispatch::continue_with(dust_registration_error(request.id, error));
                }
            }
        }
        Dispatch::continue_with(Response::error(
            request.id,
            "unavailable",
            "DUST registration submission worker did not start",
        ))
    }

    pub(super) fn dust_registration_draft(&self, request: Request) -> Dispatch {
        self.dust_registration_operation(
            request,
            "wallet.dust.registration.draft",
            |application, profile_id, draft_id| {
                application.get_wallet_dust_registration().execute(
                    GetWalletDustRegistrationCommand {
                        profile_id,
                        draft_id,
                    },
                )
            },
            |preview| json!({ "registration": dust_registration_preview_value(&preview) }),
        )
    }

    pub(super) fn dust_registration_status(&self, request: Request) -> Dispatch {
        self.dust_registration_status_operation(
            request,
            "wallet.dust.registration.status",
            |application, command| {
                application
                    .get_wallet_dust_registration_status()
                    .execute(command)
            },
        )
    }

    pub(super) fn cancel_dust_registration_submission(&self, request: Request) -> Dispatch {
        let params = match dust_registration_draft_params(
            request.id.clone(),
            request.params,
            "wallet.dust.registration.cancel_submission",
        ) {
            Ok(params) => params,
            Err(response) => return Dispatch::continue_with(response),
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .cancel_wallet_dust_registration_submission()
            .execute(CancelWalletDustRegistrationSubmissionCommand {
                profile_id,
                draft_id: params.draft_id,
            }) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "registrationStatus": dust_registration_status_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn reconcile_dust_registration_submission(&self, request: Request) -> Dispatch {
        let params = match dust_registration_draft_params(
            request.id.clone(),
            request.params,
            "wallet.dust.registration.reconcile_submission",
        ) {
            Ok(params) => params,
            Err(response) => return Dispatch::continue_with(response),
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .reconcile_wallet_dust_registration_submission()
                .execute(ReconcileWalletDustRegistrationSubmissionCommand {
                    profile_id,
                    draft_id: params.draft_id,
                }),
        ) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "registrationStatus": dust_registration_status_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn dust_registration_status_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            GetWalletDustRegistrationStatusCommand,
        ) -> Result<
            WalletDustRegistrationSubmissionStatusView,
            WalletDustRegistrationError,
        >,
    ) -> Dispatch {
        let params =
            match dust_registration_draft_params(request.id.clone(), request.params, method) {
                Ok(params) => params,
                Err(response) => return Dispatch::continue_with(response),
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(
            &self.application,
            GetWalletDustRegistrationStatusCommand {
                profile_id,
                draft_id: params.draft_id,
            },
        ) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "registrationStatus": dust_registration_status_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn dust_registration_operation<T>(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            String,
            String,
        ) -> Result<T, WalletDustRegistrationError>,
        projection: impl FnOnce(T) -> Value,
    ) -> Dispatch {
        let params =
            match dust_registration_draft_params(request.id.clone(), request.params, method) {
                Ok(params) => params,
                Err(response) => return Dispatch::continue_with(response),
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(&self.application, profile_id, params.draft_id) {
            Ok(value) => Dispatch::continue_with(Response::success(request.id, projection(value))),
            Err(error) => Dispatch::continue_with(dust_registration_error(request.id, error)),
        }
    }

    pub(super) fn shielded_sync_status(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.status",
            |application, command| {
                application
                    .get_wallet_shielded_sync_status()
                    .execute(command)
            },
        )
    }

    pub(super) fn start_shielded_sync(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.start",
            |application, command| application.start_wallet_shielded_sync().execute(command),
        )
    }

    pub(super) fn cancel_shielded_sync(&self, request: Request) -> Dispatch {
        self.shielded_sync_operation(
            request,
            "wallet.shielded.sync.cancel",
            |application, command| application.cancel_wallet_shielded_sync().execute(command),
        )
    }

    pub(super) fn shielded_sync_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletShieldedSyncCommand,
        ) -> Result<WalletShieldedSyncView, WalletShieldedSyncError>,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(&self.application, WalletShieldedSyncCommand { profile_id }) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "shieldedSync": shielded_sync_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(shielded_sync_error(request.id, error)),
        }
    }

    pub(super) fn list_addresses(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.list", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "addresses": account.addresses.iter().map(address_value).collect::<Vec<_>>()
            })
        })
    }

    pub(super) fn unshielded_address(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.unshielded", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "address": account.addresses.iter().find(|address| address.kind == "unshielded").map(address_value)
            })
        })
    }

    pub(super) fn shielded_address(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.address.shielded", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "address": account.addresses.iter().find(|address| address.kind == "shielded").map(address_value)
            })
        })
    }

    pub(super) fn balance_snapshot(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.balance.snapshot", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "balances": account.balances.iter().map(balance_value).collect::<Vec<_>>(),
                "sync": sync_value(account)
            })
        })
    }

    pub(super) fn transaction_history(&self, request: Request) -> Dispatch {
        self.account_projection(request, "wallet.transaction.history", |account| {
            json!({
                "networkId": account.network_id,
                "source": account.source,
                "transactions": account.transactions.iter().map(transaction_value).collect::<Vec<_>>()
            })
        })
    }

    pub(super) fn prepare_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.prepare_unshielded requires only string recipientAddress and amountAtomicUnits fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .prepare_wallet_transfer()
            .execute(PrepareWalletTransferCommand {
                profile_id,
                recipient_address: params.recipient_address,
                amount_atomic_units: params.amount_atomic_units,
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn prepare_shielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareShieldedTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.prepare_shielded requires only string recipientAddress, tokenType, and amountAtomicUnits fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.prepare_shielded_wallet_transfer().execute(
            PrepareShieldedWalletTransferCommand {
                profile_id,
                recipient_address: params.recipient_address,
                token_type: params.token_type,
                amount_atomic_units: params.amount_atomic_units,
            },
        ) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn authorize_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AuthorizeTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet transaction authorization requires only string draftId and authorizationChallenge fields plus confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .authorize_wallet_transfer()
            .execute(AuthorizeWalletTransferCommand {
                profile_id,
                draft_id: params.draft_id,
                authorization_challenge: params.authorization_challenge,
                confirmation: params.confirmation.into(),
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn transaction_draft(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<TransactionDraftParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.draft requires only a string draftId",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_transfer_draft()
            .execute(WalletTransferDraftQuery {
                profile_id,
                draft_id: params.draft_id,
            }) {
            Ok(preview) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "transfer": transfer_preview_value(&preview) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn submit_unshielded(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet transaction submission requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.submit_wallet_transfer().execute(
            SubmitWalletTransferCommand {
                profile_id,
                draft_id: params.draft_id,
                confirmation: params.confirmation.into(),
            },
        )) {
            Ok(submission) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submission": transfer_submission_value(&submission) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn start_submission(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SubmitTransferParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "wallet.transaction.start_submission requires only a string draftId and confirmation",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let preview =
            match self
                .application
                .get_wallet_transfer_draft()
                .execute(WalletTransferDraftQuery {
                    profile_id: profile_id.clone(),
                    draft_id: params.draft_id.clone(),
                }) {
                Ok(preview) => preview,
                Err(error) => return Dispatch::continue_with(transaction_error(request.id, error)),
            };
        match preview.state.as_str() {
            "authorized" | "submitting" | "submitted" => {}
            "expired" => {
                return Dispatch::continue_with(transaction_port_error(
                    request.id,
                    WalletTransactionPortError::DraftExpired,
                ));
            }
            _ => {
                return Dispatch::continue_with(transaction_port_error(
                    request.id,
                    WalletTransactionPortError::DraftConflict,
                ));
            }
        }

        let confirmation: SensitiveOperationConfirmation = params.confirmation.into();
        if let Err(error) = validate_confirmation(&confirmation) {
            return Dispatch::continue_with(sensitive_error(request.id, error));
        }
        let service = self.application.submit_wallet_transfer();
        let command = SubmitWalletTransferCommand {
            profile_id: profile_id.clone(),
            draft_id: params.draft_id.clone(),
            confirmation,
        };
        if thread::Builder::new()
            .name("oxid-headless-submit".to_owned())
            .spawn(move || {
                let _ = futures::executor::block_on(service.execute(command));
            })
            .is_err()
        {
            return Dispatch::continue_with(Response::error(
                request.id,
                "unavailable",
                "transaction submission worker could not be started",
            ));
        }

        let status_service = self.application.get_wallet_transfer_submission_status();
        let query = WalletTransferSubmissionQuery {
            profile_id,
            draft_id: params.draft_id,
        };
        for _ in 0..100 {
            match status_service.execute(query.clone()) {
                Ok(status) if status.state != "not_started" => {
                    return Dispatch::continue_with(Response::success(
                        request.id,
                        json!({ "submissionStatus": transfer_submission_status_value(&status) }),
                    ));
                }
                Ok(_) => thread::sleep(Duration::from_millis(1)),
                Err(error) => {
                    return Dispatch::continue_with(transaction_error(request.id, error));
                }
            }
        }
        Dispatch::continue_with(Response::error(
            request.id,
            "unavailable",
            "transaction submission worker did not start",
        ))
    }

    pub(super) fn submission_status(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.submission_status",
            |application, query| {
                application
                    .get_wallet_transfer_submission_status()
                    .execute(query)
            },
        )
    }

    pub(super) fn submission_history(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "wallet.transaction.submission_history");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_wallet_transfer_submissions()
            .execute(profile_id)
        {
            Ok(statuses) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "submissions": statuses
                        .iter()
                        .map(transfer_submission_status_value)
                        .collect::<Vec<_>>()
                }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn reconcile_submission(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.reconcile_submission",
            |application, query| {
                futures::executor::block_on(
                    application
                        .reconcile_wallet_transfer_submission()
                        .execute(query),
                )
            },
        )
    }

    pub(super) fn cancel_submission(&self, request: Request) -> Dispatch {
        self.submission_operation(
            request,
            "wallet.transaction.cancel_submission",
            |application, query| {
                application
                    .cancel_wallet_transfer_submission()
                    .execute(query)
            },
        )
    }

    pub(super) fn submission_operation(
        &self,
        request: Request,
        method: &'static str,
        operation: impl FnOnce(
            &ApplicationServices,
            WalletTransferSubmissionQuery,
        )
            -> Result<WalletTransferSubmissionStatusView, WalletTransactionError>,
    ) -> Dispatch {
        let params = match serde_json::from_value::<TransactionDraftParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                let message = match method {
                    "wallet.transaction.submission_status" => {
                        "wallet.transaction.submission_status requires only a string draftId"
                    }
                    "wallet.transaction.cancel_submission" => {
                        "wallet.transaction.cancel_submission requires only a string draftId"
                    }
                    "wallet.transaction.reconcile_submission" => {
                        "wallet.transaction.reconcile_submission requires only a string draftId"
                    }
                    _ => "transaction submission method requires only a string draftId",
                };
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    message,
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match operation(
            &self.application,
            WalletTransferSubmissionQuery {
                profile_id,
                draft_id: params.draft_id,
            },
        ) {
            Ok(status) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "submissionStatus": transfer_submission_status_value(&status) }),
            )),
            Err(error) => Dispatch::continue_with(transaction_error(request.id, error)),
        }
    }

    pub(super) fn account_projection(
        &self,
        request: Request,
        method: &'static str,
        projection: impl FnOnce(&WalletAccountView) -> Value,
    ) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, method);
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_wallet_account()
            .execute(WalletAccountQuery { profile_id })
        {
            Ok(account) => {
                Dispatch::continue_with(Response::success(request.id, projection(&account)))
            }
            Err(error) => Dispatch::continue_with(account_error(request.id, error)),
        }
    }
}
