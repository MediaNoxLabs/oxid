// SPDX-License-Identifier: Apache-2.0

//! Android platform-verifier bootstrap for the authenticated physical profile.
//!
//! Dioxus seeds `ndk-context` only after entering its mobile launcher, so this
//! module is called by the first rendered gate component, never by `main`.

#![allow(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Initializes the independent verifier globals already linked through
/// jsonrpsee/subxt (0.5) and reqwest (0.7).
///
/// `Ok(false)` means Dioxus has not seeded `ndk-context` yet. No verifier is
/// touched in that state. `Ok(true)` is a prerequisite for rendering the
/// wallet UI and therefore for constructing any first HTTPS request.
pub(crate) fn initialize_after_dioxus_context() -> Result<bool, ()> {
    let context = match catch_unwind(AssertUnwindSafe(ndk_context::android_context)) {
        Ok(context) => context,
        Err(_) => return Ok(false),
    };
    let raw_vm = context.vm();
    let raw_context = context.context();

    // SAFETY: ndk-context is now initialized by Dioxus and returns process-
    // lifetime JVM/activity handles. Each wrapper borrows those handles and
    // each verifier immediately promotes the activity to a global reference.
    let vm_05 = unsafe {
        jni_021::JavaVM::from_raw(raw_vm.cast::<jni_021::sys::JavaVM>()).map_err(|_| ())?
    };
    let mut env_05 = vm_05.attach_current_thread().map_err(|_| ())?;
    let activity_05 = unsafe {
        jni_021::objects::JObject::from_raw(raw_context.cast::<jni_021::sys::_jobject>())
    };
    rustls_platform_verifier_05::android::init_with_env(&mut env_05, activity_05)
        .map_err(|_| ())?;

    // jni 0.22 is a separate type universe, just as verifier 0.7 has a
    // separate initialization cell. Initialize it from the same validated
    // Android handles rather than relying on the 0.5 side effect.
    let vm_07 = unsafe { jni_022::JavaVM::from_raw(raw_vm.cast::<jni_022::sys::JavaVM>()) };
    vm_07
        .attach_current_thread(|env| {
            let activity = unsafe {
                jni_022::objects::JObject::from_raw(
                    env,
                    raw_context.cast::<jni_022::sys::_jobject>(),
                )
            };
            rustls_platform_verifier_07::android::init_with_env(env, activity)
        })
        .map_err(|_| ())?;

    Ok(true)
}
