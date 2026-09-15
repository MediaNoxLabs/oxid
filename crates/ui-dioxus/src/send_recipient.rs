// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use dioxus::prelude::*;
use oxid_platform_ports::{QrScanError, QrScannerPort};
use oxid_wallet_application::import_midnight_night_receive_request;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SendWizardStep {
    Recipient,
    Amount,
}

impl SendWizardStep {
    pub(super) const fn number(self) -> u8 {
        match self {
            Self::Recipient => 1,
            Self::Amount => 2,
        }
    }

    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Recipient => "Recipient",
            Self::Amount => "Amount",
        }
    }
}

#[component]
pub(super) fn SendWizardProgress(current: SendWizardStep) -> Element {
    let steps = [SendWizardStep::Recipient, SendWizardStep::Amount];
    rsx! {
        ol { class: "send-wizard__progress", aria_label: "Send progress",
            for step in steps {
                {
                    let class = if step == current {
                        "send-wizard__step is-active"
                    } else if step.number() < current.number() {
                        "send-wizard__step is-complete"
                    } else {
                        "send-wizard__step"
                    };
                    rsx! {
                        li {
                            key: "{step.number()}",
                            class,
                            aria_current: if step == current { "step" } else { "false" },
                            span { class: "send-wizard__step-mark", aria_hidden: "true", "{step.number()}" }
                            strong { "{step.title()}" }
                        }
                    }
                }
            }
        }
    }
}

/// The only state change a wallet-recipient scan may request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RecipientScanUpdate {
    pub recipient: String,
    pub advances_wizard: bool,
}

/// Validates a scanned NIGHT receive request for the selected network.
/// Scanning deliberately only fills the recipient field; the user still
/// chooses the amount and begins the normal review flow explicitly.
pub(super) fn scanned_recipient_update(
    active_network_id: &str,
    payload: String,
) -> Result<RecipientScanUpdate, String> {
    import_midnight_night_receive_request(active_network_id, &payload)
        .map(|address| RecipientScanUpdate {
            recipient: address.value().to_owned(),
            advances_wizard: false,
        })
        .map_err(|_| {
            "This is not a valid public NIGHT receive request for the active network. Nothing was imported."
                .to_owned()
        })
}

pub(super) fn start_recipient_scan(
    scanner: Arc<dyn QrScannerPort>,
    active_network_id: String,
    mut busy: Signal<bool>,
    mut notice: Signal<Option<String>>,
    mut recipient: Signal<String>,
    mut using_own_address: Signal<bool>,
) {
    if busy() {
        return;
    }
    busy.set(true);
    notice.set(None);
    spawn(async move {
        match scanner.scan().await {
            Ok(payload) => match scanned_recipient_update(&active_network_id, payload.into_inner())
            {
                Ok(update) => {
                    using_own_address.set(false);
                    recipient.set(update.recipient);
                    notice.set(Some(
                        "Recipient imported. Continue when you are ready.".to_owned(),
                    ));
                }
                Err(message) => notice.set(Some(message)),
            },
            Err(error) => notice.set(Some(recipient_qr_scan_message(error))),
        }
        busy.set(false);
    });
}

fn recipient_qr_scan_message(error: QrScanError) -> String {
    match error {
        QrScanError::Cancelled => "QR scan cancelled.".to_owned(),
        QrScanError::Denied => "Camera access was denied; no recipient was imported.".to_owned(),
        QrScanError::Unavailable => {
            "Camera scanning is unavailable; paste a recipient instead.".to_owned()
        }
        QrScanError::TimedOut => "QR scan timed out; no recipient was imported.".to_owned(),
        QrScanError::InvalidPayload | QrScanError::Failed => {
            "QR scanning failed; no recipient was imported.".to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_scan_populates_only_the_recipient() {
        let update = scanned_recipient_update(
            "undeployed",
            "midnight-receive:v1|network=undeployed|asset=NIGHT|address=mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh".to_owned(),
        )
        .expect("request is valid");

        assert_eq!(
            update.recipient,
            "mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh"
        );
        assert!(!update.advances_wizard);
    }

    #[test]
    fn scan_rejects_an_identity_request_and_wrong_network_without_mutation() {
        assert!(
            scanned_recipient_update(
                "undeployed",
                "openid4vp://authorize?request_uri=https://verifier.invalid/request".to_owned(),
            )
            .is_err()
        );
        assert!(
            scanned_recipient_update(
                "preprod",
                "midnight-receive:v1|network=undeployed|asset=NIGHT|address=mn_addr_undeployed1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9smkp9zh".to_owned(),
            )
            .is_err()
        );
    }
}
