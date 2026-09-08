// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) const fn profile_switch_is_allowed(route: Route) -> bool {
    matches!(route, Route::Home)
}

#[component]
pub(super) fn ProfileQuickSwitcher(
    active_profile: WalletProfileView,
    switching_allowed: bool,
    on_selected: EventHandler<WalletProfileView>,
) -> Element {
    let services = consume_context::<WalletUiServices>();
    let brand = consume_context::<BrandProfile>();
    let mut profiles = use_signal(|| ProfileListState::Loading);
    let mut switching = use_signal(|| false);
    let mut failure = use_signal(|| None::<String>);
    let services_for_load = services.clone();
    use_effect(move || {
        let list = services_for_load.list_wallet_profiles();
        spawn(async move {
            let result = run_ui_blocking(move || list.execute()).await;
            profiles.set(match result {
                Ok(Ok(profiles)) => ProfileListState::Ready(profiles),
                Ok(Err(error)) => ProfileListState::Failed(error.to_string()),
                Err(error) => ProfileListState::Failed(error.to_string()),
            });
        });
    });

    match profiles.read().clone() {
        ProfileListState::Loading => rsx! {
            div { class: "profile-sheet__state", role: "status", aria_busy: "true",
                span { class: "loading-mark", aria_hidden: "true" }
                span { "Loading profiles…" }
            }
        },
        ProfileListState::Failed(message) => rsx! {
            div { class: "profile-sheet__state profile-sheet__state--critical", role: "alert",
                span { "Profiles unavailable: {message}" }
            }
        },
        ProfileListState::Ready(loaded) => {
            let alternatives = loaded
                .into_iter()
                .filter(|profile| profile.id != active_profile.id)
                .collect::<Vec<_>>();
            rsx! {
                if alternatives.is_empty() {
                    p { class: "profile-sheet__hint", "No other profiles on this device." }
                } else {
                    p { class: "profile-sheet__hint", "Switch wallet context" }
                    for profile in alternatives {
                        {
                            let profile_id = profile.id.clone();
                            let profile_name = profile.display_name.clone();
                            let monogram = profile_monogram(&profile_name, brand.wordmark());
                            let select = services.select_wallet_profile();
                            rsx! {
                                button {
                                    class: "profile-sheet__profile",
                                    key: "{profile_id}",
                                    r#type: "button",
                                    aria_label: "Switch to {profile_name}",
                                    disabled: switching() || !switching_allowed,
                                    onclick: move |_| {
                                        let select = Arc::clone(&select);
                                        let profile_id = profile_id.clone();
                                        switching.set(true);
                                        failure.set(None);
                                        spawn(async move {
                                            let selected = run_ui_blocking(move || {
                                                select.execute(SelectWalletProfileCommand { profile_id })
                                            })
                                            .await;
                                            match selected {
                                                Ok(Ok(profile)) => on_selected.call(profile),
                                                Ok(Err(error)) => {
                                                    failure.set(Some(error.to_string()));
                                                    switching.set(false);
                                                }
                                                Err(error) => {
                                                    failure.set(Some(error.to_string()));
                                                    switching.set(false);
                                                }
                                            }
                                        });
                                    },
                                    span { class: "profile-avatar", aria_hidden: "true", "{monogram}" }
                                    span { "{profile_name}" }
                                }
                            }
                        }
                    }
                    if !switching_allowed {
                        p { class: "profile-sheet__hint", role: "status", "Return Home before switching profiles." }
                    }
                }
                if let Some(message) = failure.read().as_deref() {
                    p { class: "profile-sheet__error", role: "alert", "{message}" }
                }
            }
        }
    }
}
