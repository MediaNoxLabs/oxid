// SPDX-License-Identifier: Apache-2.0

use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum HeaderMenu {
    #[default]
    Closed,
    ProfileSwitcher,
    Global,
}

impl HeaderMenu {
    pub(super) const fn toggle_profile_switcher(self) -> Self {
        if matches!(self, Self::ProfileSwitcher) {
            Self::Closed
        } else {
            Self::ProfileSwitcher
        }
    }

    pub(super) const fn toggle_global(self) -> Self {
        if matches!(self, Self::Global) {
            Self::Closed
        } else {
            Self::Global
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlobalMenuAction {
    Settings,
    BackupRecovery,
    SessionPrivacy,
    #[cfg(feature = "ui-profile-dev")]
    DeveloperTools,
}

impl GlobalMenuAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::BackupRecovery => "Backup & recovery",
            Self::SessionPrivacy => "Session privacy",
            #[cfg(feature = "ui-profile-dev")]
            Self::DeveloperTools => "Developer tools",
        }
    }

    pub(super) const fn route(self) -> Option<Route> {
        match self {
            Self::Settings => Some(Route::Settings),
            Self::BackupRecovery => Some(Route::BackupRecovery),
            Self::SessionPrivacy => None,
            #[cfg(feature = "ui-profile-dev")]
            Self::DeveloperTools => Some(Route::Developer),
        }
    }
}

#[cfg(feature = "ui-profile-dev")]
const GLOBAL_MENU_ACTIONS: [GlobalMenuAction; 4] = [
    GlobalMenuAction::Settings,
    GlobalMenuAction::BackupRecovery,
    GlobalMenuAction::SessionPrivacy,
    GlobalMenuAction::DeveloperTools,
];
#[cfg(not(feature = "ui-profile-dev"))]
const GLOBAL_MENU_ACTIONS: [GlobalMenuAction; 3] = [
    GlobalMenuAction::Settings,
    GlobalMenuAction::BackupRecovery,
    GlobalMenuAction::SessionPrivacy,
];

#[component]
pub(super) fn GlobalMenuTrigger(open: bool, on_toggle: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: if open { "global-menu-trigger active" } else { "global-menu-trigger" },
            r#type: "button",
            aria_label: if open { "Close global application menu" } else { "Open global application menu" },
            aria_controls: "global-application-menu",
            aria_expanded: if open { "true" } else { "false" },
            aria_haspopup: "menu",
            title: "Global application menu",
            onclick: move |event| on_toggle.call(event),
            span { aria_hidden: "true", "•••" }
        }
    }
}

#[component]
pub(super) fn GlobalApplicationMenu(
    secret_mode: SecretModeController,
    on_action: EventHandler<GlobalMenuAction>,
) -> Element {
    rsx! {
        nav {
            id: "global-application-menu",
            class: "global-menu",
            role: "menu",
            aria_label: "Global application menu",
            p { class: "global-menu__heading", "Application" }
            for action in GLOBAL_MENU_ACTIONS {
                if action == GlobalMenuAction::SessionPrivacy {
                    button {
                        class: "global-menu__item",
                        r#type: "button",
                        role: "menuitemcheckbox",
                        "data-global-action": "{action.label()}",
                        aria_checked: if secret_mode.is_masked() { "true" } else { "false" },
                        onclick: move |_| on_action.call(action),
                        span { "{action.label()}" }
                        small { if secret_mode.is_masked() { "Private values hidden" } else { "Private values revealed" } }
                    }
                } else {
                    button {
                        class: "global-menu__item",
                        r#type: "button",
                        role: "menuitem",
                        "data-global-action": "{action.label()}",
                        autofocus: action == GlobalMenuAction::Settings,
                        onclick: move |_| on_action.call(action),
                        "{action.label()}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_are_mutually_exclusive_and_toggle_closed() {
        assert_eq!(
            HeaderMenu::Closed.toggle_profile_switcher(),
            HeaderMenu::ProfileSwitcher
        );
        assert_eq!(
            HeaderMenu::Global.toggle_profile_switcher(),
            HeaderMenu::ProfileSwitcher
        );
        assert_eq!(
            HeaderMenu::ProfileSwitcher.toggle_profile_switcher(),
            HeaderMenu::Closed
        );
        assert_eq!(HeaderMenu::Closed.toggle_global(), HeaderMenu::Global);
        assert_eq!(
            HeaderMenu::ProfileSwitcher.toggle_global(),
            HeaderMenu::Global
        );
        assert_eq!(HeaderMenu::Global.toggle_global(), HeaderMenu::Closed);
    }

    #[test]
    fn actions_never_route_to_profile_switching_or_management() {
        let labels = GLOBAL_MENU_ACTIONS.map(GlobalMenuAction::label);

        assert_eq!(
            &labels[..3],
            ["Settings", "Backup & recovery", "Session privacy"]
        );
        assert_eq!(GlobalMenuAction::Settings.route(), Some(Route::Settings));
        assert_eq!(
            GlobalMenuAction::BackupRecovery.route(),
            Some(Route::BackupRecovery)
        );
        assert_eq!(GlobalMenuAction::SessionPrivacy.route(), None);
        assert!(
            GLOBAL_MENU_ACTIONS
                .into_iter()
                .all(|action| !matches!(action.route(), Some(Route::Profile | Route::Home)))
        );
    }

    #[cfg(feature = "ui-profile-dev")]
    #[test]
    fn developer_profile_adds_only_the_guarded_tools_shortcut() {
        assert_eq!(GLOBAL_MENU_ACTIONS.len(), 4);
        assert_eq!(GLOBAL_MENU_ACTIONS[3].label(), "Developer tools");
        assert_eq!(GLOBAL_MENU_ACTIONS[3].route(), Some(Route::Developer));
    }

    #[cfg(feature = "ui-profile-demo")]
    #[test]
    fn demo_profile_keeps_developer_tools_out() {
        assert_eq!(GLOBAL_MENU_ACTIONS.len(), 3);
        assert!(
            !GLOBAL_MENU_ACTIONS
                .into_iter()
                .any(|action| action.label() == "Developer tools")
        );
    }

    #[test]
    fn controls_keep_keyboard_focus_and_mobile_touch_targets() {
        let touch_target_rule = BASE_STYLES
            .split(".profile-shortcut,\n.global-menu-trigger {")
            .nth(1)
            .and_then(|styles| styles.split('}').next())
            .expect("header control touch target rule");
        assert!(touch_target_rule.contains("min-height: 2.75rem;"));
        assert!(touch_target_rule.contains("min-width: 2.75rem;"));
        assert!(BASE_STYLES.contains(".profile-shortcut:focus-visible"));
        assert!(BASE_STYLES.contains(".global-menu-trigger:focus-visible"));
        assert!(BASE_STYLES.contains(".global-menu__item:focus-visible"));
    }
}
