// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;
use oxid_passport_vault_application::{
    PassportVaultActivitySource, PassportVaultActivityStatus, PassportVaultActivityView,
    PassportVaultCallKind,
};

use super::{activity_observed_at_line, labels};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum VaultActivityPageState {
    Loading,
    Ready(PassportVaultActivityView),
    Unavailable(String),
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
                    let retention = labels::vault_activity_retention(&activity.retention);
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

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::dioxus_core::Mutation;

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
