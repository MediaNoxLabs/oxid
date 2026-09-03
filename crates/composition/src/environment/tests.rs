// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "headless-portal-local", feature = "desktop-portal-test")
))]
#[test]
fn headless_process_portal_policy_accepts_only_the_canonical_standalone_bundle() {
    let placeholder = oxid_adapter_midnight::standalone_configuration_placeholder_address()
        .expect("public standalone placeholder")
        .value()
        .to_owned();
    let canonical = [
        Some("undeployed".to_owned()),
        Some("ws://127.0.0.1:8088/api/v4/graphql/ws".to_owned()),
        Some("http://127.0.0.1:8088/api/v4/graphql".to_owned()),
        Some("ws://127.0.0.1:9944".to_owned()),
        Some("http://127.0.0.1:6300".to_owned()),
        Some(placeholder),
        None,
    ];
    let no_adjacent_settings = PortalAdjacentEnvironmentSettings::default();

    assert_eq!(
        validate_portal_environment_combination(
            HeadlessEnvironmentPolicy::General,
            &canonical,
            &no_adjacent_settings,
        ),
        Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
    );
    assert_eq!(
        validate_portal_environment_combination(
            HeadlessEnvironmentPolicy::NativeHeadlessProcess,
            &canonical,
            &no_adjacent_settings,
        ),
        Ok(())
    );
    for policy in [
        HeadlessEnvironmentPolicy::General,
        HeadlessEnvironmentPolicy::NativeHeadlessProcess,
    ] {
        assert_eq!(
            validate_portal_environment_combination(
                policy,
                &[None, None, None, None, None, None, None],
                &no_adjacent_settings,
            ),
            Ok(())
        );
    }

    let replacements = [
        (0, "devnet"),
        (1, "ws://localhost:8088/api/v4/graphql/ws"),
        (1, "ws://127.0.0.1:8088/api/v3/graphql/ws"),
        (2, "http://127.0.0.1:8089/api/v4/graphql"),
        (3, "ws://127.0.0.1:9945"),
        (4, "http://127.0.0.1:6301"),
        (5, "mn_addr_undeployed1alternate"),
    ];
    for (index, replacement) in replacements {
        let mut values = canonical.clone();
        values[index] = Some(replacement.to_owned());
        assert_eq!(
            validate_portal_environment_combination(
                HeadlessEnvironmentPolicy::NativeHeadlessProcess,
                &values,
                &no_adjacent_settings,
            ),
            Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
        );
    }
    for index in 0..6 {
        let mut values = canonical.clone();
        values[index] = None;
        assert_eq!(
            validate_portal_environment_combination(
                HeadlessEnvironmentPolicy::NativeHeadlessProcess,
                &values,
                &no_adjacent_settings,
            ),
            Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
        );
    }
    let read_only = [
        canonical[0].clone(),
        canonical[1].clone(),
        None,
        None,
        None,
        canonical[5].clone(),
        None,
    ];
    assert_eq!(
        validate_portal_environment_combination(
            HeadlessEnvironmentPolicy::NativeHeadlessProcess,
            &read_only,
            &no_adjacent_settings,
        ),
        Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
    );

    for adjacent in PortalAdjacentEnvironmentSettings::each_conflict() {
        assert_eq!(
            validate_portal_environment_combination(
                HeadlessEnvironmentPolicy::NativeHeadlessProcess,
                &canonical,
                &adjacent,
            ),
            Err(HeadlessCompositionError::PortalRequiresStandaloneSimulation)
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn standalone_live_configuration_is_all_or_nothing() {
    const ADDRESS: &str =
        "mn_addr_devnet1asujt0dayj4pelgq97wv75hjhscqv9epmzzpapkf8sy8c87jhh9syn2j3y";
    assert!(matches!(
        parse_optional_midnight_config([None, None, None, None, None, None, None]),
        Ok(None)
    ));
    assert!(matches!(
        parse_optional_midnight_config([
            Some("devnet".to_owned()),
            Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
            None,
            None,
            None,
            Some(ADDRESS.to_owned()),
            None,
        ]),
        Ok(Some(HeadlessMidnightConfig::Indexer(_)))
    ));
    assert!(matches!(
        parse_optional_midnight_config([
            Some("devnet".to_owned()),
            Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
            Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
            Some("ws://127.0.0.1:9944".to_owned()),
            Some("http://127.0.0.1:6300".to_owned()),
            Some(ADDRESS.to_owned()),
            None,
        ]),
        Ok(Some(HeadlessMidnightConfig::Standalone(_)))
    ));
    let local_cache = std::env::temp_dir().join("oxid-composition-proving-cache");
    assert!(matches!(
        parse_optional_midnight_config([
            Some("devnet".to_owned()),
            Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
            Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
            Some("ws://127.0.0.1:9944".to_owned()),
            None,
            Some(ADDRESS.to_owned()),
            Some(local_cache.to_string_lossy().into_owned()),
        ]),
        Ok(Some(HeadlessMidnightConfig::Standalone(_)))
    ));
    assert_eq!(
        parse_optional_midnight_config([
            Some("undeployed".to_owned()),
            None,
            None,
            None,
            None,
            None,
            None,
        ])
        .err(),
        Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
    );
    assert_eq!(
        parse_optional_midnight_config([
            Some("devnet".to_owned()),
            Some("ws://127.0.0.1:8088/api/v1/graphql/ws".to_owned()),
            Some("http://127.0.0.1:8088/api/v1/graphql".to_owned()),
            Some("ws://127.0.0.1:9944".to_owned()),
            Some("http://127.0.0.1:6300".to_owned()),
            Some(ADDRESS.to_owned()),
            Some(local_cache.to_string_lossy().into_owned()),
        ])
        .err(),
        Some(HeadlessCompositionError::IncompleteMidnightIndexerConfiguration)
    );
    assert_eq!(
        parse_optional_passport_vault_deployment_height(None),
        Ok(None)
    );
    assert_eq!(
        parse_optional_passport_vault_deployment_height(Some("42".to_owned())),
        Ok(Some(42))
    );
    for invalid in ["", "0", "-1", " 42", "18446744073709551616"] {
        assert_eq!(
            parse_optional_passport_vault_deployment_height(Some(invalid.to_owned())),
            Err(HeadlessCompositionError::InvalidPassportVaultDeploymentHeight)
        );
    }
}
