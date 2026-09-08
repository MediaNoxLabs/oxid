// SPDX-License-Identifier: Apache-2.0

use dioxus::prelude::*;

#[cfg(feature = "public-standalone-genesis")]
use crate::PUBLIC_STANDALONE_GENESIS_MARKER;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SessionNoticeState {
    dismissed: bool,
}

#[cfg(any(
    feature = "ui-profile-dev",
    feature = "public-standalone-genesis",
    test
))]
impl SessionNoticeState {
    const fn is_visible(self) -> bool {
        !self.dismissed
    }

    fn dismiss(&mut self) {
        self.dismissed = true;
    }
}

#[cfg(feature = "ui-profile-dev")]
#[component]
pub(super) fn DeveloperProfileBanner(mut state: Signal<SessionNoticeState>) -> Element {
    if !state().is_visible() {
        return rsx! {};
    }
    rsx! {
        aside {
            class: "developer-profile-banner",
            role: "status",
            "data-ui-profile": "OXID_UI_PROFILE_DEVELOPMENT",
            div { class: "developer-profile-banner__copy",
                strong { "Developer profile" }
                span { "Standalone composition · public capability facts only · telemetry off" }
            }
            button {
                class: "developer-profile-banner__dismiss",
                r#type: "button",
                aria_label: "Dismiss developer profile notice for this session",
                title: "Dismiss notice",
                onclick: move |_| state.write().dismiss(),
                span { aria_hidden: "true", "×" }
            }
        }
    }
}

#[cfg(not(feature = "ui-profile-dev"))]
#[component]
pub(super) fn DeveloperProfileBanner(state: Signal<SessionNoticeState>) -> Element {
    let _ = state;
    rsx! {}
}

#[cfg(feature = "public-standalone-genesis")]
#[component]
pub(super) fn PublicStandaloneGenesisBanner(mut state: Signal<SessionNoticeState>) -> Element {
    if !state().is_visible() {
        return rsx! {};
    }
    rsx! {
        aside {
            class: "developer-profile-banner",
            role: "alert",
            "data-wallet-authority": PUBLIC_STANDALONE_GENESIS_MARKER,
            div { class: "developer-profile-banner__copy",
                strong { "Public genesis wallet capability" }
                span { "Only the unique “Demo Wallet” profile can use shared, publicly spendable test authority; other profiles remain random. No privacy or ownership is implied." }
            }
            button {
                class: "developer-profile-banner__dismiss",
                r#type: "button",
                aria_label: "Dismiss public genesis wallet notice for this session",
                title: "Dismiss notice",
                onclick: move |_| state.write().dismiss(),
                span { aria_hidden: "true", "×" }
            }
        }
    }
}

#[cfg(not(feature = "public-standalone-genesis"))]
#[component]
pub(super) fn PublicStandaloneGenesisBanner(state: Signal<SessionNoticeState>) -> Element {
    let _ = state;
    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::SessionNoticeState;
    use crate::BASE_STYLES;

    #[test]
    fn notices_are_visible_per_session_until_explicitly_dismissed() {
        let mut first_session = SessionNoticeState::default();
        assert!(first_session.is_visible());

        first_session.dismiss();
        assert!(!first_session.is_visible());

        let restarted_session = SessionNoticeState::default();
        assert!(restarted_session.is_visible());

        let dismiss_control = BASE_STYLES
            .split(".developer-profile-banner__dismiss {")
            .nth(1)
            .and_then(|styles| styles.split('}').next())
            .expect("development notice dismiss control rule");
        assert!(dismiss_control.contains("width: 2.75rem;"));
        assert!(dismiss_control.contains("height: 2.75rem;"));
    }
}
