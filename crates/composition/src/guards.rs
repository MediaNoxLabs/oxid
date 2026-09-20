// SPDX-License-Identifier: Apache-2.0

#[cfg(all(
    feature = "mobile-portal",
    not(any(target_os = "ios", target_os = "android"))
))]
compile_error!("mobile-portal is available only on iOS and Android");

#[cfg(all(feature = "mobile-portal-tailnet", not(target_os = "android")))]
compile_error!("mobile-portal-tailnet is available only on Android");
