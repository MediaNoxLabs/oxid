// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;
use oxid_passport_vault_application::{
    PassportVaultActivitySource, PassportVaultActivityStatus, PassportVaultActivityView,
    PassportVaultCallKind,
};

use super::activity_observed_at_line;

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
                VaultActivityPageState::Ready(activity) if activity.records.is_empty() => rsx! {
                    p { class: "activity-empty-state", "No Passport Vault operations are available for this profile yet." }
                },
                VaultActivityPageState::Ready(activity) => {
                    let retention = activity.retention.replace('_', " ");
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
