// SPDX-License-Identifier: Apache-2.0

#[cfg(target_os = "android")]
use std::time::Duration;

#[cfg(target_os = "android")]
use dioxus::prelude::*;

#[cfg(target_os = "android")]
use super::{WalletApp, WalletUiServices, run_ui_blocking};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AndroidPlatformInitialization {
    Ready,
    Retry,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AndroidPlatformState {
    Initializing,
    Ready,
    Failed,
}

fn terminal_state(initialization: AndroidPlatformInitialization) -> Option<AndroidPlatformState> {
    match initialization {
        AndroidPlatformInitialization::Ready => Some(AndroidPlatformState::Ready),
        AndroidPlatformInitialization::Retry => None,
        AndroidPlatformInitialization::Failed => Some(AndroidPlatformState::Failed),
    }
}

/// Prevents network-backed wallet and identity effects from mounting until
/// Android's certificate verifier has both its JVM classes and runtime handles.
#[cfg(target_os = "android")]
#[component]
pub fn App() -> Element {
    let services = consume_context::<WalletUiServices>();
    let initializer = services.android_platform_initializer.clone();
    let mut state = use_signal(|| AndroidPlatformState::Initializing);

    use_future(move || {
        let initializer = initializer.clone();
        async move {
            let Some(initializer) = initializer else {
                state.set(AndroidPlatformState::Ready);
                return;
            };
            for _ in 0..30 {
                if let Some(terminal) = terminal_state(initializer()) {
                    state.set(terminal);
                    return;
                }
                let _ = run_ui_blocking(|| {
                    std::thread::sleep(Duration::from_millis(100));
                })
                .await;
            }
            state.set(AndroidPlatformState::Failed);
        }
    });

    match state() {
        AndroidPlatformState::Ready => rsx! { WalletApp {} },
        AndroidPlatformState::Initializing => rsx! {
            main { role: "status", aria_busy: "true", "Preparing secure Android networking…" }
        },
        AndroidPlatformState::Failed => rsx! {
            main { role: "alert", "Secure Android networking is unavailable. Restart Oxid to retry." }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialization_results_have_closed_terminal_transitions() {
        let transitions = [
            terminal_state(AndroidPlatformInitialization::Ready),
            terminal_state(AndroidPlatformInitialization::Retry),
            terminal_state(AndroidPlatformInitialization::Failed),
        ];
        assert!(!transitions.contains(&Some(AndroidPlatformState::Initializing)));
        assert_eq!(
            transitions,
            [
                Some(AndroidPlatformState::Ready),
                None,
                Some(AndroidPlatformState::Failed),
            ],
        );
    }
}
