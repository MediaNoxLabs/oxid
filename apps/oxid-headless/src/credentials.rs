// SPDX-License-Identifier: Apache-2.0

use super::*;

impl HeadlessWallet {
    pub(super) fn receive_credential(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "credential.receive does not accept parameters",
            ));
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .receive_credential()
                .execute(CredentialProfileQuery { profile_id }),
        ) {
            Ok(credential) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "credential": credential_value(&credential) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    pub(super) fn list_credentials(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return Dispatch::continue_with(Response::error(
                request.id,
                "invalid_params",
                "credential.list does not accept parameters",
            ));
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credentials()
            .execute(CredentialProfileQuery { profile_id })
        {
            Ok(credentials) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "credentials": credentials.iter().map(credential_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    pub(super) fn get_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.get requires only a string credentialId field",
                ));
            }
        };
        self.credential_query(request.id, params.credential_id, false)
    }

    pub(super) fn reverify_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.reverify requires only a string credentialId field",
                ));
            }
        };
        self.credential_query(request.id, params.credential_id, true)
    }

    pub(super) fn credential_query(
        &self,
        id: Option<String>,
        credential_id: String,
        reverify: bool,
    ) -> Dispatch {
        let profile_id = match self.active_profile_id(id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        let query = CredentialQuery {
            profile_id,
            credential_id,
        };
        let result = if reverify {
            futures::executor::block_on(self.application.reverify_credential().execute(query))
        } else {
            self.application.get_credential().execute(query)
        };
        match result {
            Ok(credential) => Dispatch::continue_with(Response::success(
                id,
                json!({ "credential": credential_value(&credential) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(id, error)),
        }
    }

    pub(super) fn delete_credential(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DeleteCredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.delete requires credentialId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .delete_credential()
            .execute(DeleteCredentialCommand {
                profile_id,
                credential_id: params.credential_id,
                confirmed: params.confirmed,
                intent: params.intent,
            }) {
            Ok(()) => {
                Dispatch::continue_with(Response::success(request.id, json!({ "deleted": true })))
            }
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    pub(super) fn credential_disclosure_candidates(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.disclosure.candidates requires only a string credentialId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_disclosure()
            .execute(CredentialDisclosureQuery {
                profile_id,
                credential_id: params.credential_id,
            }) {
            Ok(disclosure) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "disclosure": credential_disclosure_value(&disclosure) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    pub(super) fn preview_credential_disclosure(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<DisclosurePreviewParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.disclosure.preview requires credentialId, revealClaimPaths, and predicates fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.preview_credential_disclosure().execute(
            PreviewCredentialDisclosureCommand {
                profile_id,
                credential_id: params.credential_id,
                reveal_claim_paths: params.reveal_claim_paths,
                predicates: params
                    .predicates
                    .into_iter()
                    .map(|predicate| CredentialPredicateInput {
                        claim_path: predicate.claim_path,
                        kind: predicate.kind,
                        threshold: predicate.threshold,
                    })
                    .collect(),
            },
        ) {
            Ok(plan) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "plan": credential_disclosure_plan_value(&plan) }),
            )),
            Err(error) => Dispatch::continue_with(credential_error(request.id, error)),
        }
    }

    pub(super) fn prepare_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<PrepareCredentialIssuanceParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.prepare requires only a string offer field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.prepare_credential_issuance().execute(
            PrepareCredentialIssuanceCommand {
                profile_id,
                offer: params.offer,
            },
        )) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    pub(super) fn route_identity_request(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<RouteIdentityRequestParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.request.route requires only a string requestUri field",
                ));
            }
        };
        match self
            .application
            .route_identity_request()
            .execute(RouteIdentityRequestCommand {
                request_uri: params.request_uri,
            }) {
            Ok(kind) => Dispatch::continue_with(Response::success(
                request.id,
                json!({
                    "route": {
                        "kind": kind.code(),
                        "destination": match kind {
                            IdentityRequestKind::SelfIssuedAuthentication => "dids",
                            IdentityRequestKind::CredentialIssuance
                            | IdentityRequestKind::CredentialPresentation => "credentials",
                        },
                    }
                }),
            )),
            Err(error) => {
                Dispatch::continue_with(identity_request_routing_error(request.id, error))
            }
        }
    }

    pub(super) fn accept_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptCredentialIssuanceParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.accept requires issuanceId, holderDid, methodId, holderBindingMethodId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(self.application.accept_credential_issuance().execute(
            AcceptCredentialIssuanceCommand {
                profile_id,
                issuance_id: params.issuance_id,
                holder_did: params.holder_did,
                method_id: params.method_id,
                holder_binding_method_id: params.holder_binding_method_id,
                confirmed: params.confirmed,
                intent: params.intent,
            },
        )) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    pub(super) fn refuse_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialIssuanceParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.refuse requires only a string issuanceId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.refuse_credential_issuance().execute(
            RefuseCredentialIssuanceCommand {
                profile_id,
                issuance_id: params.issuance_id,
            },
        ) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    pub(super) fn get_credential_issuance(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialIssuanceParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.issuance.get requires only a string issuanceId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_issuance()
            .execute(CredentialIssuanceQuery {
                profile_id,
                issuance_id: params.issuance_id,
            }) {
            Ok(issuance) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuance": credential_issuance_value(&issuance) }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    pub(super) fn list_credential_issuances(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "credential.issuance.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credential_issuances()
            .execute(CredentialIssuanceProfileQuery { profile_id })
        {
            Ok(issuances) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "issuances": issuances.iter().map(credential_issuance_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_issuance_error(request.id, error)),
        }
    }

    pub(super) fn prepare_credential_presentation(&self, request: Request) -> Dispatch {
        let params =
            match serde_json::from_value::<PrepareCredentialPresentationParams>(request.params) {
                Ok(params) => params,
                Err(_) => {
                    return Dispatch::continue_with(Response::error(
                        request.id,
                        "invalid_params",
                        "credential.presentation.prepare requires only a string request field",
                    ));
                }
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application.prepare_credential_presentation().execute(
                PrepareCredentialPresentationCommand {
                    profile_id,
                    request: params.request,
                },
            ),
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    pub(super) fn accept_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptCredentialPresentationParams>(
            request.params,
        ) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.accept requires presentationId, credentialId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application.accept_credential_presentation().execute(
                AcceptCredentialPresentationCommand {
                    profile_id,
                    presentation_id: params.presentation_id,
                    credential_id: params.credential_id,
                    confirmed: params.confirmed,
                    intent: params.intent,
                },
            ),
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    pub(super) fn refuse_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialPresentationParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.refuse requires only a string presentationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.refuse_credential_presentation().execute(
            RefuseCredentialPresentationCommand {
                profile_id,
                presentation_id: params.presentation_id,
            },
        ) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    pub(super) fn get_credential_presentation(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<CredentialPresentationParams>(request.params) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "credential.presentation.get requires only a string presentationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .get_credential_presentation()
            .execute(CredentialPresentationQuery {
                profile_id,
                presentation_id: params.presentation_id,
            }) {
            Ok(presentation) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentation": credential_presentation_value(&presentation) }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    pub(super) fn list_credential_presentations(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "credential.presentation.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_credential_presentations()
            .execute(CredentialPresentationProfileQuery { profile_id })
        {
            Ok(presentations) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "presentations": presentations.iter().map(credential_presentation_value).collect::<Vec<_>>() }),
            )),
            Err(error) => Dispatch::continue_with(credential_presentation_error(request.id, error)),
        }
    }

    pub(super) fn prepare_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params =
            match serde_json::from_value::<PrepareSelfIssuedAuthenticationParams>(request.params) {
                Ok(params) => params,
                Err(_) => {
                    return Dispatch::continue_with(Response::error(
                        request.id,
                        "invalid_params",
                        "identity.authentication.prepare requires only a string request field",
                    ));
                }
            };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .prepare_self_issued_authentication()
                .execute(PrepareSelfIssuedAuthenticationCommand {
                    profile_id,
                    request: params.request,
                }),
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    pub(super) fn accept_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<AcceptSelfIssuedAuthenticationParams>(
            request.params,
        ) {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.accept requires authenticationId, holderDid, methodId, confirmed, and intent fields",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match futures::executor::block_on(
            self.application
                .accept_self_issued_authentication()
                .execute(AcceptSelfIssuedAuthenticationCommand {
                    profile_id,
                    authentication_id: params.authentication_id,
                    holder_did: params.holder_did,
                    method_id: params.method_id,
                    confirmed: params.confirmed,
                    intent: params.intent,
                }),
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    pub(super) fn refuse_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelfIssuedAuthenticationParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.refuse requires only a string authenticationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .refuse_self_issued_authentication()
            .execute(RefuseSelfIssuedAuthenticationCommand {
                profile_id,
                authentication_id: params.authentication_id,
            }) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    pub(super) fn get_self_issued_authentication(&self, request: Request) -> Dispatch {
        let params = match serde_json::from_value::<SelfIssuedAuthenticationParams>(request.params)
        {
            Ok(params) => params,
            Err(_) => {
                return Dispatch::continue_with(Response::error(
                    request.id,
                    "invalid_params",
                    "identity.authentication.get requires only a string authenticationId field",
                ));
            }
        };
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self.application.get_self_issued_authentication().execute(
            SelfIssuedAuthenticationQuery {
                profile_id,
                authentication_id: params.authentication_id,
            },
        ) {
            Ok(authentication) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentication": self_issued_authentication_value(&authentication) }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }

    pub(super) fn list_self_issued_authentications(&self, request: Request) -> Dispatch {
        if !params_are_empty(&request.params) {
            return invalid_empty_params(request.id, "identity.authentication.list");
        }
        let profile_id = match self.active_profile_id(request.id.clone()) {
            Ok(profile_id) => profile_id,
            Err(response) => return Dispatch::continue_with(response),
        };
        match self
            .application
            .list_self_issued_authentications()
            .execute(SelfIssuedAuthenticationProfileQuery { profile_id })
        {
            Ok(authentications) => Dispatch::continue_with(Response::success(
                request.id,
                json!({ "authentications": authentications.iter().map(self_issued_authentication_value).collect::<Vec<_>>() }),
            )),
            Err(error) => {
                Dispatch::continue_with(self_issued_authentication_error(request.id, error))
            }
        }
    }
}
