// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use dioxus::prelude::*;

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

/// Prevents network-backed wallet and identity effects from mounting until
/// Android's certificate verifier has both its JVM classes and runtime handles.
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
                match initializer() {
                    AndroidPlatformInitialization::Ready => {
                        state.set(AndroidPlatformState::Ready);
                        return;
                    }
                    AndroidPlatformInitialization::Retry => {
                        let _ = run_ui_blocking(|| {
                            std::thread::sleep(Duration::from_millis(100));
                        })
                        .await;
                    }
                    AndroidPlatformInitialization::Failed => {
                        state.set(AndroidPlatformState::Failed);
                        return;
                    }
                }
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
