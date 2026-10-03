// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;
use oxid_passport_vault_application::{
    PassportVaultActivitySource, PassportVaultActivityStatus, PassportVaultActivityView,
    PassportVaultCallKind,
};
use oxid_presentation_application::{
    CredentialPresentationActivitySource, CredentialPresentationActivityStatus,
    CredentialPresentationActivityView,
};
use oxid_protocol_application::{
    CredentialIssuanceActivitySource, CredentialIssuanceActivityStatus,
    CredentialIssuanceActivityView,
};

use super::{WalletUiServices, activity_observed_at_line, labels, run_ui_blocking};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VaultActivityPageState {
    Loading,
    Ready(PassportVaultActivityView),
    Unavailable(String),
}

#[component]
pub(super) fn PassportVaultActivitySection(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| VaultActivityPageState::Loading);
    let service = services.list_passport_vault_activity();
    use_effect(move || {
        let service = service.clone();
        let profile_id = profile_id.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || service.execute(profile_id))
                    .await
                    .map_or_else(
                        |error| VaultActivityPageState::Unavailable(error.to_string()),
                        |result| {
                            result.map_or_else(
                                |error| VaultActivityPageState::Unavailable(error.to_string()),
                                VaultActivityPageState::Ready,
                            )
                        },
                    ),
            );
        });
    });
    rsx! { PassportVaultActivityCard { state: state.read().clone() } }
}

#[component]
pub(super) fn PassportVaultActivityCard(state: VaultActivityPageState) -> Element {
    rsx! {
        article { class: "surface-card",
            p { class: "card-eyebrow", "Passport Vault activity" }
            h2 { "Vault operations" }
            p { class: "activity-source-note", "Source: application-owned Vault operation lifecycle. Sensitive protocol and credential payloads are never retained." }
            match state {
                VaultActivityPageState::Loading => rsx! {
                    p { class: "activity-empty-state", role: "status", "Loading Vault activity…" }
                },
                VaultActivityPageState::Unavailable(error) => rsx! {
                    p { class: "activity-empty-state", role: "status", "Vault activity is unavailable. {error}" }
                },
                VaultActivityPageState::Ready(activity) if activity.records.is_empty() => {
                    let empty_message = "No Passport Vault operations are available for this profile yet.";
                    rsx! {
                        p { class: "activity-empty-state", "{empty_message}" }
                    }
                },
                VaultActivityPageState::Ready(activity) => {
                    let retention = labels::activity_retention(&activity.retention);
                    rsx! {
                        div { class: "activity-list", aria_label: "Passport Vault activity",
                            for record in activity.records {
                                article { class: "activity-row", key: "{record.id.value()}",
                                    span { class: "activity-row__mark", aria_hidden: "true", "◇" }
                                    div {
                                        strong { "{vault_activity_operation(record.operation)}" }
                                        small { "{vault_activity_status(record.status)}" }
                                        small { class: "privacy-value", "{activity_observed_at_line(record.observed_at_millis)}" }
                                    }
                                    code { "#{record.id.value()}" }
                                    details { class: "activity-row__details",
                                        summary { "Operation details" }
                                        dl { class: "preview-list",
                                            div { dt { "Source" } dd { "{vault_activity_source(record.source)}" } }
                                            div { dt { "Status" } dd { "{vault_activity_status(record.status)}" } }
                                            div { dt { "Finality" } dd { "{record.finality.name()}" } }
                                            if let Some(lock_id) = record.lock_id {
                                                div { dt { "Lock" } dd { "#{lock_id}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "field-hint", "Retention: {retention}." }
                    }
                },
            }
        }
    }
}

const fn vault_activity_operation(operation: PassportVaultCallKind) -> &'static str {
    match operation {
        PassportVaultCallKind::CreateLock => "Create lock",
        PassportVaultCallKind::DepositToLock => "Deposit",
        PassportVaultCallKind::ClaimFromLock => "Claim",
        PassportVaultCallKind::WithdrawFromLock => "Withdraw",
    }
}

const fn vault_activity_status(status: PassportVaultActivityStatus) -> &'static str {
    match status {
        PassportVaultActivityStatus::Pending => "Pending",
        PassportVaultActivityStatus::Confirmed => "Confirmed",
        PassportVaultActivityStatus::Failed => "Failed",
        PassportVaultActivityStatus::Refused => "Refused",
        PassportVaultActivityStatus::Cancelled => "Cancelled",
        PassportVaultActivityStatus::TimedOut => "Timed out",
        PassportVaultActivityStatus::OutcomeUnknown => "Outcome unknown",
    }
}

const fn vault_activity_source(source: PassportVaultActivitySource) -> &'static str {
    match source {
        PassportVaultActivitySource::StandaloneVault => "Standalone Vault",
        PassportVaultActivitySource::MidnightContractCall => "Midnight contract call",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CredentialIssuanceActivityPageState {
    Loading,
    Ready(CredentialIssuanceActivityView),
    Unavailable(String),
}

#[component]
pub(super) fn CredentialIssuanceActivitySection(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| CredentialIssuanceActivityPageState::Loading);
    let service = services.list_credential_issuance_activity();
    use_effect(move || {
        let service = service.clone();
        let profile_id = profile_id.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || service.execute(profile_id))
                    .await
                    .map_or_else(
                        |error| CredentialIssuanceActivityPageState::Unavailable(error.to_string()),
                        |result| {
                            result.map_or_else(
                                |error| {
                                    CredentialIssuanceActivityPageState::Unavailable(
                                        error.to_string(),
                                    )
                                },
                                CredentialIssuanceActivityPageState::Ready,
                            )
                        },
                    ),
            );
        });
    });
    rsx! { CredentialIssuanceActivityCard { state: state.read().clone() } }
}

#[component]
pub(super) fn CredentialIssuanceActivityCard(
    state: CredentialIssuanceActivityPageState,
) -> Element {
    rsx! {
        article { class: "surface-card", aria_label: "Credential issuance activity",
            p { class: "card-eyebrow", "Credential issuance activity" }
            h2 { "Credential issuance" }
            p { class: "activity-source-note", "Source: application-owned issuance lifecycle. Offers, claims, proofs, keys, credential bytes, protocol identifiers, and raw errors are never retained." }
            match state {
                CredentialIssuanceActivityPageState::Loading => rsx! {
                    p { class: "activity-empty-state", role: "status", "Loading credential issuance activity…" }
                },
                CredentialIssuanceActivityPageState::Unavailable(error) => {
                    let status_role = "status";
                    rsx! {
                        p { class: "activity-empty-state", role: "{status_role}", "Credential issuance activity is unavailable. {error}" }
                    }
                },
                CredentialIssuanceActivityPageState::Ready(activity) if activity.records.is_empty() => {
                    let empty_message = "No credential issuance activity is available for this profile yet.";
                    let status_role = "status";
                    rsx! {
                        p { class: "activity-empty-state", role: "{status_role}", "{empty_message}" }
                    }
                },
                CredentialIssuanceActivityPageState::Ready(activity) => {
                    let retention = labels::activity_retention(&activity.retention);
                    rsx! {
                        div { class: "activity-list", aria_label: "Credential issuance activity",
                            for record in activity.records {
                                article { class: "activity-row", key: "{record.id.value()}",
                                    span { class: "activity-row__mark", aria_hidden: "true", "◇" }
                                    div {
                                        strong { "Credential issuance" }
                                        small { "Issuer endpoint: {record.issuer}" }
                                        small { "{credential_issuance_activity_status(record.status)}" }
                                        small { class: "privacy-value", "{activity_observed_at_line(record.observed_at_millis)}" }
                                    }
                                    details { class: "activity-row__details",
                                        summary { "Issuance details" }
                                        dl { class: "preview-list",
                                            div { dt { "Source" } dd { "{credential_issuance_activity_source(record.source)}" } }
                                            div { dt { "Status" } dd { "{credential_issuance_activity_status(record.status)}" } }
                                            div { dt { "Finality" } dd { "{record.finality.name()}" } }
                                            div { dt { "Configurations" } dd {
                                                for configuration in record.credential_configuration_ids {
                                                    span { "{configuration} " }
                                                }
                                            } }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "field-hint", "Retention: {retention}." }
                    }
                },
            }
        }
    }
}

const fn credential_issuance_activity_status(
    status: CredentialIssuanceActivityStatus,
) -> &'static str {
    match status {
        CredentialIssuanceActivityStatus::Pending => "Pending",
        CredentialIssuanceActivityStatus::Stored => "Stored",
        CredentialIssuanceActivityStatus::Failed => "Failed",
        CredentialIssuanceActivityStatus::Refused => "Refused",
        CredentialIssuanceActivityStatus::Cancelled => "Cancelled",
        CredentialIssuanceActivityStatus::TimedOut => "Timed out",
        CredentialIssuanceActivityStatus::OutcomeUnknown => "Outcome unknown",
    }
}

const fn credential_issuance_activity_source(
    source: CredentialIssuanceActivitySource,
) -> &'static str {
    match source {
        CredentialIssuanceActivitySource::OpenId4Vci => "OpenID4VCI",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CredentialPresentationActivityPageState {
    Loading,
    Ready(CredentialPresentationActivityView),
    Unavailable(String),
}

#[component]
pub(super) fn CredentialPresentationActivitySection(profile_id: String) -> Element {
    let services = consume_context::<WalletUiServices>();
    let mut state = use_signal(|| CredentialPresentationActivityPageState::Loading);
    let service = services.list_credential_presentation_activity();
    use_effect(move || {
        let service = service.clone();
        let profile_id = profile_id.clone();
        spawn(async move {
            state.set(
                run_ui_blocking(move || service.execute(profile_id))
                    .await
                    .map_or_else(
                        |error| {
                            CredentialPresentationActivityPageState::Unavailable(error.to_string())
                        },
                        |result| {
                            result.map_or_else(
                                |error| {
                                    CredentialPresentationActivityPageState::Unavailable(
                                        error.to_string(),
                                    )
                                },
                                CredentialPresentationActivityPageState::Ready,
                            )
                        },
                    ),
            );
        });
    });
    rsx! { CredentialPresentationActivityCard { state: state.read().clone() } }
}

#[component]
pub(super) fn CredentialPresentationActivityCard(
    state: CredentialPresentationActivityPageState,
) -> Element {
    rsx! {
        article { class: "surface-card", aria_label: "Credential presentation activity",
            p { class: "card-eyebrow", "Credential presentation activity" }
            h2 { "Credential sharing" }
            p { class: "activity-source-note", "Source: application-owned consented presentation lifecycle. Claims, disclosed values, proofs, keys, protocol payloads, identifiers, and raw errors are never retained." }
            match state {
                CredentialPresentationActivityPageState::Loading => rsx! {
                    p { class: "activity-empty-state", role: "{credential_presentation_activity_status_role()}", "Loading credential presentation activity…" }
                },
                CredentialPresentationActivityPageState::Unavailable(error) => rsx! {
                    p { class: "activity-empty-state", role: "{credential_presentation_activity_status_role()}", "Credential presentation activity is unavailable. {error}" }
                },
                CredentialPresentationActivityPageState::Ready(activity) if activity.records.is_empty() => rsx! {
                    p { class: "activity-empty-state", role: "{credential_presentation_activity_status_role()}", "{credential_presentation_activity_empty_message()}" }
                },
                CredentialPresentationActivityPageState::Ready(activity) => {
                    let retention = labels::activity_retention(&activity.retention);
                    rsx! {
                        div { class: "activity-list", aria_label: "Credential presentation activity",
                            for record in activity.records {
                                article { class: "activity-row", key: "{record.id.value()}",
                                    span { class: "activity-row__mark", aria_hidden: "true", "◇" }
                                    div {
                                        strong { "Credential presentation" }
                                        small { "Purpose: {record.purpose}" }
                                        small { "{credential_presentation_activity_status(record.status)}" }
                                        small { class: "privacy-value", "{activity_observed_at_line(record.observed_at_millis)}" }
                                    }
                                    details { class: "activity-row__details",
                                        summary { "Presentation details" }
                                        dl { class: "preview-list",
                                            div { dt { "Source" } dd { "{credential_presentation_activity_source(record.source)}" } }
                                            div { dt { "Status" } dd { "{credential_presentation_activity_status(record.status)}" } }
                                            div { dt { "Finality" } dd { "{record.finality.name()}" } }
                                            div { dt { "Type" } dd { "{record.presentation_type}" } }
                                            if let Some(verifier) = record.verifier {
                                                div { dt { "Verifier" } dd { "{verifier}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        p { class: "field-hint", "Retention: {retention}." }
                    }
                },
            }
        }
    }
}

const fn credential_presentation_activity_empty_message() -> &'static str {
    "No credential presentation activity is available for this profile yet."
}

const fn credential_presentation_activity_status_role() -> &'static str {
    "status"
}

const fn credential_presentation_activity_status(
    status: CredentialPresentationActivityStatus,
) -> &'static str {
    match status {
        CredentialPresentationActivityStatus::Pending => "Pending",
        CredentialPresentationActivityStatus::Shared => "Shared",
        CredentialPresentationActivityStatus::Failed => "Failed",
        CredentialPresentationActivityStatus::Refused => "Refused",
        CredentialPresentationActivityStatus::Cancelled => "Cancelled",
        CredentialPresentationActivityStatus::TimedOut => "Timed out",
        CredentialPresentationActivityStatus::OutcomeUnknown => "Outcome unknown",
    }
}

const fn credential_presentation_activity_source(
    source: CredentialPresentationActivitySource,
) -> &'static str {
    match source {
        CredentialPresentationActivitySource::OpenId4Vp => "OpenID4VP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::dioxus_core::Mutation;
    use oxid_presentation_application::{
        CredentialPresentationActivityFinality, CredentialPresentationActivityId,
        CredentialPresentationActivityRecord,
    };
    use oxid_protocol_application::{
        CredentialIssuanceActivityFinality, CredentialIssuanceActivityId,
        CredentialIssuanceActivityRecord,
    };

    fn rendered_text(state: VaultActivityPageState) -> Vec<String> {
        #[derive(Clone, PartialEq, Props)]
        struct HarnessProps {
            state: VaultActivityPageState,
        }

        fn harness(props: HarnessProps) -> Element {
            rsx! { PassportVaultActivityCard { state: props.state } }
        }

        let mut dom = VirtualDom::new_with_props(harness, HarnessProps { state });
        dom.rebuild_to_vec()
            .edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::CreateTextNode { value, .. } => Some(value.to_string()),
                _ => None,
            })
            .collect()
    }

    fn credential_rendered_dom(state: CredentialIssuanceActivityPageState) -> Vec<Mutation> {
        #[derive(Clone, PartialEq, Props)]
        struct HarnessProps {
            state: CredentialIssuanceActivityPageState,
        }

        fn harness(props: HarnessProps) -> Element {
            rsx! { CredentialIssuanceActivityCard { state: props.state } }
        }

        let mut dom = VirtualDom::new_with_props(harness, HarnessProps { state });
        dom.rebuild_to_vec().edits
    }

    fn credential_rendered_text(state: CredentialIssuanceActivityPageState) -> Vec<String> {
        credential_rendered_dom(state)
            .iter()
            .filter_map(|edit| match edit {
                Mutation::CreateTextNode { value, .. } => Some(value.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn empty_and_unavailable_states_remain_explicit() {
        let empty = rendered_text(VaultActivityPageState::Ready(PassportVaultActivityView {
            source: "application_event_projection".to_owned(),
            retention: "process_local_bounded_not_backed_up".to_owned(),
            records: Vec::new(),
        }));
        assert!(
            empty
                .iter()
                .any(|text| text.contains("No Passport Vault operations"))
        );

        let unavailable = rendered_text(VaultActivityPageState::Unavailable(
            "Passport Vault activity is unavailable".to_owned(),
        ));
        assert!(unavailable.iter().any(|text| text.contains("unavailable")));
    }

    #[test]
    fn credential_issuance_empty_unavailable_and_privacy_states_are_explicit() {
        let empty = CredentialIssuanceActivityPageState::Ready(CredentialIssuanceActivityView {
            source: "application_event_projection".to_owned(),
            retention: "process_local_bounded_not_backed_up".to_owned(),
            records: Vec::new(),
        });
        let empty_text = credential_rendered_text(empty.clone());
        let empty_dom = format!("{:?}", credential_rendered_dom(empty));
        assert!(
            empty_text
                .iter()
                .any(|text| text.contains("No credential issuance activity")),
            "{empty_text:?}"
        );
        assert!(
            empty_dom.contains("role") && empty_dom.contains("status"),
            "{empty_dom}"
        );

        let unavailable = CredentialIssuanceActivityPageState::Unavailable(
            "credential issuance activity is unavailable".to_owned(),
        );
        let unavailable_text = credential_rendered_text(unavailable.clone());
        let unavailable_dom = format!("{:?}", credential_rendered_dom(unavailable));
        assert!(
            unavailable_text
                .iter()
                .any(|text| text.contains("activity is unavailable"))
        );
        assert!(unavailable_dom.contains("role") && unavailable_dom.contains("status"));
        assert_eq!(
            credential_issuance_activity_status(CredentialIssuanceActivityStatus::Stored),
            "Stored"
        );
        assert_eq!(
            credential_issuance_activity_status(CredentialIssuanceActivityStatus::OutcomeUnknown),
            "Outcome unknown"
        );
        assert_eq!(
            credential_issuance_activity_source(CredentialIssuanceActivitySource::OpenId4Vci),
            "OpenID4VCI"
        );
    }

    #[test]
    fn populated_credential_issuance_card_shows_only_safe_summary_and_details() {
        let text = credential_rendered_text(CredentialIssuanceActivityPageState::Ready(
            CredentialIssuanceActivityView {
                source: "application_event_projection".to_owned(),
                retention: "process_local_bounded_not_backed_up".to_owned(),
                records: vec![CredentialIssuanceActivityRecord {
                    id: CredentialIssuanceActivityId::from_value(7).expect("activity id"),
                    profile_id: "profile_1".to_owned(),
                    source: CredentialIssuanceActivitySource::OpenId4Vci,
                    issuer: "https://issuer.example".to_owned(),
                    credential_configuration_ids: vec!["identity".to_owned()],
                    status: CredentialIssuanceActivityStatus::Stored,
                    finality: CredentialIssuanceActivityFinality::Final,
                    observed_at_millis: Some(1_000),
                }],
            },
        ));
        assert!(
            text.iter()
                .any(|value| value.contains("https://issuer.example"))
        );
        assert!(text.iter().any(|value| value.contains("Issuer endpoint")));
        assert!(text.iter().any(|value| value.contains("Stored")));
        assert!(text.iter().any(|value| value.contains("identity")));
        assert!(!text.iter().any(|value| value == "#7"));
        assert!(
            text.iter()
                .any(|value| value.contains("This session only; bounded and not backed up"))
        );
        assert!(!text.iter().any(|value| value.contains("profile_1")));
    }

    #[test]
    fn presentation_activity_empty_unavailable_and_safe_details_are_accessible() {
        #[derive(Clone, PartialEq, Props)]
        struct HarnessProps {
            state: CredentialPresentationActivityPageState,
        }
        fn harness(props: HarnessProps) -> Element {
            rsx! { CredentialPresentationActivityCard { state: props.state } }
        }
        fn render(state: CredentialPresentationActivityPageState) -> (Vec<String>, String) {
            let mut dom = VirtualDom::new_with_props(harness, HarnessProps { state });
            let edits = dom.rebuild_to_vec().edits;
            let text = edits
                .iter()
                .filter_map(|edit| match edit {
                    Mutation::CreateTextNode { value, .. } => Some(value.to_string()),
                    _ => None,
                })
                .collect();
            (text, format!("{edits:?}"))
        }

        let (empty, empty_dom) = render(CredentialPresentationActivityPageState::Ready(
            CredentialPresentationActivityView {
                source: "application_event_projection".to_owned(),
                retention: "process_local_bounded_not_backed_up".to_owned(),
                records: Vec::new(),
            },
        ));
        assert!(
            empty
                .iter()
                .any(|value| value.contains("No credential presentation activity")),
            "{empty:?} {empty_dom}"
        );
        assert!(empty_dom.contains("role") && empty_dom.contains("status"));

        let (unavailable, unavailable_dom) =
            render(CredentialPresentationActivityPageState::Unavailable(
                "projection unavailable".to_owned(),
            ));
        assert!(
            unavailable
                .iter()
                .any(|value| value.contains("unavailable"))
        );
        assert!(unavailable_dom.contains("role") && unavailable_dom.contains("status"));

        let (populated, _) = render(CredentialPresentationActivityPageState::Ready(
            CredentialPresentationActivityView {
                source: "application_event_projection".to_owned(),
                retention: "process_local_bounded_not_backed_up".to_owned(),
                records: vec![CredentialPresentationActivityRecord {
                    id: CredentialPresentationActivityId::from_value(9).expect("id"),
                    profile_id: "profile_one".to_owned(),
                    source: CredentialPresentationActivitySource::OpenId4Vp,
                    purpose: "Age assurance".to_owned(),
                    presentation_type: "digital_passport".to_owned(),
                    verifier: Some("https://verifier.example".to_owned()),
                    status: CredentialPresentationActivityStatus::Shared,
                    finality: CredentialPresentationActivityFinality::Final,
                    observed_at_millis: Some(1_000),
                }],
            },
        ));
        assert!(
            populated
                .iter()
                .any(|value| value.contains("Age assurance"))
        );
        assert!(
            populated
                .iter()
                .any(|value| value.contains("digital_passport"))
        );
        assert!(
            populated
                .iter()
                .any(|value| value.contains("https://verifier.example"))
        );
        assert!(populated.iter().any(|value| value.contains("Shared")));
        assert!(!populated.iter().any(|value| value.contains("profile_one")));
        assert!(!populated.iter().any(|value| value == "#9"));
    }

    #[test]
    fn safe_labels_cover_every_supported_lifecycle_state() {
        assert_eq!(
            vault_activity_operation(PassportVaultCallKind::CreateLock),
            "Create lock"
        );
        assert_eq!(
            vault_activity_operation(PassportVaultCallKind::DepositToLock),
            "Deposit"
        );
        assert_eq!(
            vault_activity_operation(PassportVaultCallKind::ClaimFromLock),
            "Claim"
        );
        assert_eq!(
            vault_activity_operation(PassportVaultCallKind::WithdrawFromLock),
            "Withdraw"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::Pending),
            "Pending"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::Confirmed),
            "Confirmed"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::Failed),
            "Failed"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::Refused),
            "Refused"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::Cancelled),
            "Cancelled"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::TimedOut),
            "Timed out"
        );
        assert_eq!(
            vault_activity_status(PassportVaultActivityStatus::OutcomeUnknown),
            "Outcome unknown"
        );
        assert_eq!(
            vault_activity_source(PassportVaultActivitySource::StandaloneVault),
            "Standalone Vault"
        );
        assert_eq!(
            vault_activity_source(PassportVaultActivitySource::MidnightContractCall),
            "Midnight contract call"
        );
    }
}
