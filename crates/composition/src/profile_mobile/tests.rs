// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn mobile_development_routes_require_tls_for_remote_proving() {
    drop(
        compose_mobile_development_standalone_from_routes(
            "wss://laptop.example.invalid:8443/api/v4/graphql/ws",
            "https://laptop.example.invalid:8443/api/v4/graphql",
            "wss://laptop.example.invalid:10000",
            "https://laptop.example.invalid",
        )
        .expect("explicit TLS standalone routes compose without network I/O"),
    );
    assert!(matches!(
        compose_mobile_development_standalone_from_routes(
            "ws://100.64.0.1:8088/api/v4/graphql/ws",
            "http://100.64.0.1:8088/api/v4/graphql",
            "ws://100.64.0.1:9944",
            "http://100.64.0.1:6300",
        ),
        Err(
            HeadlessCompositionError::InvalidMidnightStandaloneConfiguration(
                MidnightStandaloneConfigError::InvalidProofEndpoint
            )
        )
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn mobile_development_routes_accept_the_reviewed_loopback_stack() {
    drop(
        compose_mobile_development_standalone_from_routes(
            "ws://127.0.0.1:8088/api/v4/graphql/ws",
            "http://127.0.0.1:8088/api/v4/graphql",
            "ws://127.0.0.1:9944",
            "http://127.0.0.1:6300",
        )
        .expect("reviewed localhost standalone routes compose without network I/O"),
    );
}
