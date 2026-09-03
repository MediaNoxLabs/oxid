// SPDX-License-Identifier: Apache-2.0

use super::*;

#[component]
pub(super) fn DidsPage(
    active_profile: WalletProfileView,
    pending_identity_request: Signal<Option<PendingIdentityRequest>>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| DidPageState::Loading);
    let mut did_input = use_signal(String::new);
    let mut did_creation = use_signal(|| DidCreationState::Ready);
    let mut did_creation_notice = use_signal(|| None::<String>);
    let mut did_publication_busy = use_signal(|| false);
    let mut did_publication_notice = use_signal(|| None::<String>);
    let mut authentication_input = use_signal(String::new);
    let mut prepared_authentication = use_signal(|| None::<SelfIssuedAuthenticationView>);
    let mut authentication_consent = use_signal(|| false);
    let mut authentication_busy = use_signal(|| false);
    let mut authentication_notice = use_signal(|| None::<String>);
    use_effect(move || {
        let pending = pending_identity_request.read().clone();
        if let Some(request) = pending
            && request.kind == IdentityRequestKind::SelfIssuedAuthentication
        {
            authentication_input.set(request.request_uri);
            prepared_authentication.set(None);
            authentication_consent.set(false);
            authentication_notice.set(Some(
                "Imported login request loaded. Preview it before authenticating.".to_owned(),
            ));
        }
    });
    let profile_id = active_profile.id.clone();
    let load_services = services.clone();
    let load_profile = profile_id.clone();
    use_effect(move || {
        let services = load_services.clone();
        let profile_id = load_profile.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || load_did_page(&services, &profile_id))
                    .await
                    .unwrap_or_else(|error| DidPageState::Failed(error.to_string())),
            );
        });
    });

    let state_snapshot = state.read().clone();
    match state_snapshot {
        DidPageState::Loading => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Decentralized identity" }
                h1 { "Your DIDs" }
                p { "Loading public DID records for this wallet profile…" }
            }
        },
        DidPageState::Failed(message) => rsx! {
            section { class: "page-heading",
                p { class: "eyebrow", "Decentralized identity" }
                h1 { "Your DIDs" }
                p { "DID inventory is an independently composed identity capability." }
            }
            article { class: "empty-state surface-card", role: "alert",
                span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                h2 { "DID capability unavailable" }
                p { "{message}" }
                button {
                    class: "secondary-action", r#type: "button",
                    onclick: move |_| {
                        let services = services.clone();
                        let profile_id = profile_id.clone();
                        state.set(DidPageState::Loading);
                        spawn(async move {
                            state.set(
                                run_ui_blocking(move || {
                                    load_did_page(&services, &profile_id)
                                })
                                .await
                                .unwrap_or_else(|error| {
                                    DidPageState::Failed(error.to_string())
                                }),
                            );
                        });
                    },
                    "Retry"
                }
            }
        },
        DidPageState::Ready {
            records,
            resolving,
            operation_error,
        } => {
            let creation = did_creation();
            let creating_did = creation == DidCreationState::Creating;
            let can_resolve = !resolving
                && !creating_did
                && !did_input.read().trim().is_empty()
                && did_input.read().len() <= 8_192;
            let resolve_services = services.clone();
            let resolve_profile = profile_id.clone();
            let retained_records = records.clone();
            let create_services = services.clone();
            let create_profile = profile_id.clone();
            let create_records = records.clone();
            let active_managed_did =
                active_managed_issuance_methods(&records).map(|(did, _, _)| did);
            let issuance_did_ready = active_managed_did.is_some();
            let publication_service = services.publish_did.clone();
            let publication_profile = profile_id.clone();
            let standalone_authentication_request = services.standalone_self_issued_request();
            rsx! {
                section { class: "page-heading",
                    p { class: "eyebrow", "Decentralized identity" }
                    h1 { "Your DIDs" }
                    p { "Create, resolve, update, sign with, and deactivate standards-shaped did:midnight documents under the active profile." }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "Managed identity" }
                    h2 { "Create a standalone DID" }
                    p { class: "form-hint", "Creates protected Ed25519 authentication, P-256 assertion, and Jubjub holder-binding keys. Only the public DID document is persisted." }
                    if creation == DidCreationState::Ready {
                        button {
                            class: "primary-action", r#type: "button", disabled: resolving,
                            onclick: move |_| {
                                {
                                    let mut creation = did_creation.write();
                                    if !begin_did_creation_value(&mut creation) {
                                        return;
                                    }
                                }
                                did_creation_notice.set(None);
                                let service = create_services.create_did();
                                let profile_id = create_profile.clone();
                                let records = create_records.clone();
                                spawn(async move {
                                    let result = run_ui_blocking(move || {
                                        service.execute(CreateDidCommand {
                                            profile_id,
                                            network: "undeployed".to_owned(),
                                        })
                                    })
                                    .await;
                                    match result {
                                        Ok(Ok(record)) => {
                                            let mut updated = records;
                                            updated.retain(|existing| existing.document.id != record.document.id);
                                            updated.push(record);
                                            updated.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                            state.set(DidPageState::Ready { records: updated, resolving: false, operation_error: None });
                                            did_creation.set(DidCreationState::Created);
                                            did_creation_notice.set(Some("Standalone DID created. Review it below before creating another DID.".to_owned()));
                                        }
                                        Ok(Err(error)) => {
                                            did_creation.set(DidCreationState::Failed);
                                            did_creation_notice.set(Some(did_operation_message(error)));
                                        }
                                        Err(error) => {
                                            did_creation.set(DidCreationState::Failed);
                                            did_creation_notice.set(Some(error.to_string()));
                                        }
                                    }
                                });
                            },
                            "Create standalone DID"
                        }
                    } else if creation == DidCreationState::Creating {
                        p {
                            class: "form-hint",
                            role: "status",
                            aria_busy: true,
                            aria_live: "polite",
                            "Creating DID…"
                        }
                    } else if creation == DidCreationState::AwaitingConfirmation {
                        p { class: "form-hint", role: "status", aria_live: "polite", "Confirm before another protected DID creation command is sent." }
                        div { class: "action-row",
                            button {
                                class: "primary-action", r#type: "button",
                                onclick: move |_| {
                                    let mut creation = did_creation.write();
                                    if confirm_another_did_creation_value(&mut creation) {
                                        did_creation_notice.set(Some("Ready to create another standalone DID.".to_owned()));
                                    }
                                },
                                "Confirm create another DID"
                            }
                            button {
                                class: "secondary-action", r#type: "button",
                                onclick: move |_| {
                                    did_creation.set(DidCreationState::Created);
                                    did_creation_notice.set(Some("No additional DID creation command was sent.".to_owned()));
                                },
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "secondary-action", r#type: "button",
                            onclick: move |_| {
                                let mut creation = did_creation.write();
                                if arm_another_did_creation_value(&mut creation) {
                                    did_creation_notice.set(None);
                                }
                            },
                            "Create another DID"
                        }
                    }
                    if let Some(message) = did_creation_notice.read().as_deref() {
                        p { class: "form-hint", role: "status", aria_live: "polite", "{message}" }
                    }
                    if issuance_did_ready {
                        p {
                            class: "credential-reverification-success",
                            role: "status",
                            aria_live: "polite",
                            "A protected managed DID is ready for credential issuance. Its management metadata is available only in this running wallet process."
                        }
                    }
                    if let (Some(service), Some(did)) = (publication_service, active_managed_did) {
                        div { class: "did-resolver-card",
                            h3 { "Tailnet demo bootstrap" }
                            p { class: "form-hint", "Make this DID's public document available to the current test issuer so it can verify holder proofs. This sends no private keys or credentials and does not publish the DID on chain." }
                            button {
                                class: "secondary-action",
                                r#type: "button",
                                disabled: did_publication_busy(),
                                onclick: move |_| {
                                    if did_publication_busy() {
                                        return;
                                    }
                                    did_publication_busy.set(true);
                                    did_publication_notice.set(None);
                                    let service = service.clone();
                                    let profile_id = publication_profile.clone();
                                    let did = did.clone();
                                    spawn(async move {
                                        let result = run_ui_future(async move {
                                            service.execute(PublishDidCommand {
                                                profile_id,
                                                did,
                                                confirmed: true,
                                                intent: PUBLISH_DID_TO_TEST_ISSUER_INTENT.to_owned(),
                                            }).await
                                        })
                                        .await;
                                        did_publication_busy.set(false);
                                        match result {
                                            Ok(Ok(())) => did_publication_notice.set(Some(
                                                "Public DID document is available to the current test issuer. You can accept its credential offer now.".to_owned(),
                                            )),
                                            Ok(Err(error)) => did_publication_notice
                                                .set(Some(did_operation_message(error))),
                                            Err(error) => did_publication_notice
                                                .set(Some(error.to_string())),
                                        }
                                    });
                                },
                                if did_publication_busy() { "Publishing holder DID…" } else { "Publish active holder DID to test issuer" }
                            }
                            if let Some(message) = did_publication_notice.read().as_deref() {
                                p { class: "form-hint", role: "status", aria_live: "polite", "{message}" }
                            }
                        }
                    }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "SIOPv2 draft 13 · standalone" }
                    h2 { "Authenticate with a DID" }
                    p { class: "form-hint", "Preview the verifier and purpose before consent. This flow proves control of a managed DID; it does not disclose a credential. Nonce, state, and the signed ID token remain inside the protocol adapter." }
                    label { r#for: "self-issued-authentication-request", "Authentication request URI" }
                    textarea {
                        id: "self-issued-authentication-request",
                        maxlength: 32768,
                        rows: 4,
                        autocomplete: "off",
                        spellcheck: false,
                        value: "{authentication_input}",
                        oninput: move |event| authentication_input.set(event.value()),
                    }
                    if let Some(request) = standalone_authentication_request {
                        button {
                            class: "secondary-action",
                            r#type: "button",
                            disabled: authentication_busy(),
                            onclick: move |_| {
                                authentication_input.set(request.clone());
                                prepared_authentication.set(None);
                                authentication_consent.set(false);
                                authentication_notice.set(Some("Standalone login request loaded. Preview it before authenticating.".to_owned()));
                            },
                            "Use standalone login request"
                        }
                    }
                    button {
                        class: "primary-action",
                        r#type: "button",
                        disabled: authentication_busy() || authentication_input.read().trim().is_empty(),
                        onclick: {
                            let service = services.prepare_self_issued_authentication();
                            let profile_id = profile_id.clone();
                            move |_| {
                                let service = service.clone();
                                let profile_id = profile_id.clone();
                                let request = authentication_input.read().trim().to_owned();
                                authentication_busy.set(true);
                                authentication_notice.set(None);
                                spawn(async move {
                                    match run_ui_future(async move {
                                        service.execute(PrepareSelfIssuedAuthenticationCommand { profile_id, request }).await
                                    })
                                    .await
                                    {
                                        Ok(Ok(preview)) => {
                                            prepared_authentication.set(Some(preview));
                                            authentication_consent.set(false);
                                            authentication_notice.set(Some("Login preview ready. Review the verifier and purpose before consenting.".to_owned()));
                                        }
                                        Ok(Err(error)) => {
                                            prepared_authentication.set(None);
                                            authentication_notice.set(Some(self_issued_authentication_message(error)));
                                        }
                                        Err(error) => {
                                            prepared_authentication.set(None);
                                            authentication_notice.set(Some(error.to_string()));
                                        }
                                    }
                                    authentication_busy.set(false);
                                });
                            }
                        },
                        if authentication_busy() { "Checking request…" } else { "Preview login request" }
                    }
                    if let Some(preview) = prepared_authentication.read().clone() {
                        div { class: "credential-offer-preview",
                            div { class: "consent-preview__heading",
                                h3 { "DID authentication preview" }
                                span { class: "status-pill", "{ui::protocol_state(&preview.state)}" }
                            }
                            if preview.state == "awaiting_consent" {
                                p { class: "privacy-consent-exemption", "Details shown for authorization." }
                                ol { class: "consent-questions", aria_label: "DID authentication consent questions",
                                    li { class: "consent-question",
                                        p { class: "card-eyebrow", "Who" }
                                        h4 { "Who is asking?" }
                                        code { title: "{preview.verifier}", "{preview.verifier}" }
                                        div { class: "consent-trust",
                                            span { class: "status-pill warning", "Unverified endpoint" }
                                            p { "Standalone mode has no production trust-registry or verified-domain signal." }
                                        }
                                    }
                                    li { class: "consent-question",
                                        p { class: "card-eyebrow", "What" }
                                        h4 { "What will you prove?" }
                                        p { "Control of the selected managed DID. No credential or document claims will be disclosed." }
                                    }
                                    li { class: "consent-question",
                                        p { class: "card-eyebrow", "From" }
                                        h4 { "Which identity?" }
                                        if let Some((holder_did, _)) = active_managed_authentication_method(&records) {
                                            code { title: "{holder_did}", "{holder_did}" }
                                            p { class: "form-hint", "A protected authentication method stays inside wallet custody." }
                                        } else {
                                            p { class: "field-error", role: "alert", "Create an active managed DID before authenticating." }
                                        }
                                    }
                                    li { class: "consent-question",
                                        p { class: "card-eyebrow", "Why" }
                                        h4 { "Why is it requested?" }
                                        p { "{preview.purpose}" }
                                    }
                                }
                                label { class: "confirmation-check",
                                    input {
                                        id: "self-issued-authentication-consent",
                                        r#type: "checkbox",
                                        aria_label: "Consent to DID authentication",
                                        disabled: active_managed_authentication_method(&records).is_none(),
                                        checked: authentication_consent(),
                                        onchange: move |event| authentication_consent.set(event.checked()),
                                    }
                                    span { "I reviewed this verifier and consent to authenticate with my active managed DID." }
                                }
                                div { class: "action-row",
                                    button {
                                        class: "primary-action",
                                        r#type: "button",
                                        disabled: authentication_busy() || !authentication_consent(),
                                        onclick: {
                                            let service = services.accept_self_issued_authentication();
                                            let profile_id = profile_id.clone();
                                            let authentication_id = preview.id.clone();
                                            let records = records.clone();
                                            move |_| {
                                                let Some((holder_did, method_id)) = active_managed_authentication_method(&records) else {
                                                    authentication_notice.set(Some("Create an active managed DID before authenticating.".to_owned()));
                                                    return;
                                                };
                                                let service = service.clone();
                                                let profile_id = profile_id.clone();
                                                let authentication_id = authentication_id.clone();
                                                authentication_busy.set(true);
                                                authentication_notice.set(None);
                                                spawn(async move {
                                                    match run_ui_future(async move {
                                                        service.execute(AcceptSelfIssuedAuthenticationCommand {
                                                            profile_id,
                                                            authentication_id,
                                                            holder_did,
                                                            method_id,
                                                            confirmed: true,
                                                            intent: "ACCEPT_SELF_ISSUED_AUTHENTICATION".to_owned(),
                                                        }).await
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(result)) => {
                                                            prepared_authentication.set(Some(result));
                                                            authentication_notice.set(Some("DID authentication succeeded and the standalone verifier independently validated the proof.".to_owned()));
                                                        }
                                                        Ok(Err(error)) => authentication_notice.set(Some(self_issued_authentication_message(error))),
                                                        Err(error) => authentication_notice.set(Some(error.to_string())),
                                                    }
                                                    authentication_busy.set(false);
                                                });
                                            }
                                        },
                                        if authentication_busy() { "Authenticating…" } else { "Authenticate with DID" }
                                    }
                                    button {
                                        class: "secondary-action",
                                        r#type: "button",
                                        disabled: authentication_busy(),
                                        onclick: {
                                            let service = services.refuse_self_issued_authentication();
                                            let profile_id = profile_id.clone();
                                            let authentication_id = preview.id.clone();
                                            move |_| {
                                                let service = service.clone();
                                                let profile_id = profile_id.clone();
                                                let authentication_id = authentication_id.clone();
                                                authentication_busy.set(true);
                                                authentication_notice.set(None);
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        service.execute(RefuseSelfIssuedAuthenticationCommand {
                                                            profile_id,
                                                            authentication_id,
                                                        })
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(result)) => {
                                                            prepared_authentication.set(Some(result));
                                                            authentication_consent.set(false);
                                                            authentication_notice.set(Some("Login request refused; ephemeral protocol secrets were discarded.".to_owned()));
                                                        }
                                                        Ok(Err(error)) => authentication_notice.set(Some(self_issued_authentication_message(error))),
                                                        Err(error) => authentication_notice.set(Some(error.to_string())),
                                                    }
                                                    authentication_busy.set(false);
                                                });
                                            }
                                        },
                                        "Refuse login"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(message) = authentication_notice.read().as_deref() {
                        p { class: "form-hint", role: "status", "{message}" }
                    }
                }
                article { class: "surface-card did-resolver-card",
                    p { class: "card-eyebrow", "Resolve a DID" }
                    label { r#for: "did-identifier", "Midnight DID" }
                    input {
                        id: "did-identifier", r#type: "text", maxlength: 8192,
                        autocomplete: "off", spellcheck: false,
                        value: "{did_input}",
                        oninput: move |event| did_input.set(event.value()),
                    }
                    p { class: "form-hint", "Start with an empty resolver input. A live resolver is used only when its base URL is explicitly configured." }
                    button {
                        class: "secondary-action", r#type: "button", disabled: resolving || creating_did,
                        onclick: move |_| did_input.set(STANDALONE_DID_FIXTURE.to_owned()),
                        "Load standalone example DID"
                    }
                    button {
                        class: "primary-action", r#type: "button", disabled: !can_resolve,
                        onclick: move |_| {
                            state.set(DidPageState::Ready { records: retained_records.clone(), resolving: true, operation_error: None });
                            let service = resolve_services.resolve_did();
                            let profile_id = resolve_profile.clone();
                            let did = did_input.read().trim().to_owned();
                            let mut records = retained_records.clone();
                            spawn(async move {
                                match run_ui_future(async move {
                                    service.execute(ResolveDidCommand { profile_id, did }).await
                                })
                                .await
                                {
                                    Ok(Ok(record)) => {
                                        records.retain(|existing| existing.document.id != record.document.id);
                                        records.push(record);
                                        records.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                        state.set(DidPageState::Ready { records, resolving: false, operation_error: None });
                                    }
                                    Ok(Err(error)) => state.set(DidPageState::Ready { records, resolving: false, operation_error: Some(did_operation_message(error)) }),
                                    Err(error) => state.set(DidPageState::Ready { records, resolving: false, operation_error: Some(error.to_string()) }),
                                }
                            });
                        },
                        if resolving { "Resolving…" } else { "Resolve and save" }
                    }
                    if let Some(error) = operation_error {
                        p { class: "field-error", role: "alert", "{error}" }
                    }
                }
                if records.is_empty() {
                    article { class: "empty-state surface-card",
                        span { class: "empty-state__mark", aria_hidden: "true", "◇" }
                        h2 { "No saved DIDs" }
                        p { "Resolve a did:midnight identifier to add its public document to this profile." }
                        span { class: "status-pill", "Profile scoped" }
                    }
                } else {
                    section { class: "did-inventory", aria_label: "Saved decentralized identifiers",
                        for record in records.clone() {
                            {
                                let did = record.document.id.clone();
                                let forget_did = did.clone();
                                let forget_profile = profile_id.clone();
                                let forget_services = services.clone();
                                let retained = records.clone();
                                let source = ui::did_source(&record.source);
                                let management = did_record_management_label(
                                    &record.source,
                                    &record.managed_method_ids,
                                );
                                let version = record.document_metadata.version_id.clone().unwrap_or_else(|| "Unversioned".to_owned());
                                rsx! {
                                    article { class: "surface-card did-record", key: "{did}",
                                        div { class: "did-record__heading",
                                            div {
                                                p { class: "card-eyebrow", "{ui::midnight_network(&record.document.network)} · {source}" }
                                                p { class: "form-hint", "{management}" }
                                                h2 { class: "privacy-value", "{truncate_middle(&did, 22, 12)}" }
                                            }
                                            span { class: if record.document_metadata.deactivated == Some(true) { "status-pill" } else { "status-pill success" },
                                                if record.document_metadata.deactivated == Some(true) { "Deactivated" } else { "Resolved" }
                                            }
                                        }
                                        dl { class: "did-record__facts",
                                            div { dt { "Version" } dd { "{version}" } }
                                            div { dt { "Public methods" } dd { "{record.document.verification_methods.len()}" } }
                                            div { dt { "Services" } dd { "{record.document.services.len()}" } }
                                        }
                                        if !record.document.verification_methods.is_empty() {
                                            ul { class: "did-method-list",
                                                for method in record.document.verification_methods.clone() {
                                                    li { key: "{method.id}",
                                                        strong { "{ui::key_curve(&method.public_key_jwk.curve)}" }
                                                        code { class: "privacy-value", "{truncate_middle(&method.id, 16, 8)}" }
                                                    }
                                                }
                                            }
                                        }
                                        {
                                            let managed_did = did.clone();
                                            let retained = records.clone();
                                            rsx! {
                                                ManagedDidControls {
                                                    profile_id: profile_id.clone(),
                                                    record: record.clone(),
                                                    on_record: move |result: Result<DidRecordView, String>| {
                                                        match result {
                                                            Ok(updated) => {
                                                                let mut next = retained.clone();
                                                                next.retain(|entry| entry.document.id != managed_did);
                                                                next.push(updated);
                                                                next.sort_by(|left, right| left.document.id.cmp(&right.document.id));
                                                                state.set(DidPageState::Ready { records: next, resolving: false, operation_error: None });
                                                            }
                                                            Err(message) => state.set(DidPageState::Ready { records: retained.clone(), resolving: false, operation_error: Some(message) }),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        button {
                                            class: "secondary-action", r#type: "button",
                                            aria_label: "Forget saved DID {did}",
                                            onclick: move |_| {
                                                let service = forget_services.forget_did();
                                                let profile_id = forget_profile.clone();
                                                let did = forget_did.clone();
                                                let target = did.clone();
                                                let records = retained.clone();
                                                state.set(DidPageState::Ready { records: records.clone(), resolving: true, operation_error: None });
                                                spawn(async move {
                                                    let result = run_ui_blocking(move || {
                                                        service.execute(DidRecordQuery {
                                                            profile_id,
                                                            did,
                                                        })
                                                    })
                                                    .await;
                                                    match result {
                                                        Ok(Ok(())) => state.set(DidPageState::Ready {
                                                            records: records.iter().filter(|record| record.document.id != target).cloned().collect(),
                                                            resolving: false,
                                                            operation_error: None,
                                                        }),
                                                        Ok(Err(error)) => state.set(DidPageState::Ready {
                                                            records,
                                                            resolving: false,
                                                            operation_error: Some(did_operation_message(error)),
                                                        }),
                                                        Err(error) => state.set(DidPageState::Ready {
                                                            records,
                                                            resolving: false,
                                                            operation_error: Some(error.to_string()),
                                                        }),
                                                    }
                                                });
                                            },
                                            "Forget from profile"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
