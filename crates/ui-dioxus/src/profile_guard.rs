// SPDX-License-Identifier: Apache-2.0

//! Compile-time guards for the non-user UI presentation profiles.
//!
//! The application crate's guards in `apps/oxid/src/main.rs` test *its own*
//! feature names, so enabling `oxid-ui-dioxus/ui-profile-dev` or
//! `oxid-ui-dioxus/ui-profile-demo` directly on the dependency never reached
//! them: the profile code compiled into an otherwise production-composed
//! binary while the release gate reported clean.
//!
//! These guards live with the features they protect. A non-user profile is
//! accepted only alongside the `app-profile-authority` marker, which asserts
//! that the profile was selected deliberately and reviewed: `oxid-app`'s own
//! guarded features forward it automatically, and the repository's
//! adapter-only profile builds state it explicitly. A build that enables a
//! profile through the dependency path without it — the shape that skipped
//! the application guards — no longer compiles. The two profiles also remain
//! mutually exclusive here, not only in the application crate.

#[cfg(all(
    any(feature = "ui-profile-dev", feature = "ui-profile-demo"),
    not(feature = "app-profile-authority")
))]
compile_error!(
    "a non-user UI profile must be selected through oxid-app (ui-profile-dev / \
     ui-profile-demo), which enables oxid-ui-dioxus/app-profile-authority and \
     enforces the composition guards; enabling the feature directly on \
     oxid-ui-dioxus bypasses them"
);

#[cfg(all(feature = "ui-profile-dev", feature = "ui-profile-demo"))]
compile_error!("select at most one non-user UI profile");
