// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use oxid_diagnostics_application::{ClearDiagnosticsUseCase, GetDiagnosticSnapshotUseCase};
#[cfg(feature = "ui-profile-dev")]
use oxid_ui_dioxus::CapabilityManifestContext;
use oxid_ui_dioxus::{
    App, BrandProfile, CredentialDisclosureUiServices, CredentialInventoryUiServices,
    CredentialIssuanceUiServices, CredentialPresentationUiServices, CredentialUiServices,
    DiagnosticsUiServices, DidUiServices, IdentityIngressUiServices, IdentityUiServices,
    PassportVaultContractCallRecoveryUiServices, PassportVaultContractCallUiServices,
    PassportVaultUiServices, SecurityCopySnapshot, SelfIssuedAuthenticationUiServices,
    WalletAccountUiServices, WalletBackupUiServices, WalletDustRegistrationRecoveryUiServices,
    WalletDustRegistrationUiServices, WalletDustSyncUiServices, WalletOperationalUiServices,
    WalletProfileUiServices, WalletSecurityUiServices, WalletShieldedSyncUiServices,
    WalletTransactionPreparationUiServices, WalletTransactionRecoveryUiServices,
    WalletTransactionUiServices, WalletUiServices, security_copy_snapshot,
};

fn assert_public_path<Item>(_item: Item) {}

fn assert_public_type<Type>() {
    assert!(!std::any::type_name::<Type>().is_empty());
}

#[test]
fn service_facade_type_and_constructor_paths_remain_at_the_crate_root() {
    assert_public_type::<WalletUiServices>();
    assert_public_type::<DiagnosticsUiServices>();
    assert_public_type::<PassportVaultUiServices>();
    assert_public_type::<PassportVaultContractCallRecoveryUiServices>();
    assert_public_type::<PassportVaultContractCallUiServices>();
    assert_public_type::<WalletOperationalUiServices>();
    assert_public_type::<DidUiServices>();
    assert_public_type::<CredentialUiServices>();
    assert_public_type::<CredentialInventoryUiServices>();
    assert_public_type::<CredentialIssuanceUiServices>();
    assert_public_type::<CredentialPresentationUiServices>();
    assert_public_type::<CredentialDisclosureUiServices>();
    assert_public_type::<SelfIssuedAuthenticationUiServices>();
    assert_public_type::<IdentityUiServices>();
    assert_public_type::<IdentityIngressUiServices>();
    assert_public_type::<WalletProfileUiServices>();
    assert_public_type::<WalletSecurityUiServices>();
    assert_public_type::<WalletBackupUiServices>();
    assert_public_type::<WalletAccountUiServices>();
    assert_public_type::<WalletDustSyncUiServices>();
    assert_public_type::<WalletDustRegistrationUiServices>();
    assert_public_type::<WalletDustRegistrationRecoveryUiServices>();
    assert_public_type::<WalletShieldedSyncUiServices>();
    assert_public_type::<WalletTransactionUiServices>();
    assert_public_type::<WalletTransactionPreparationUiServices>();
    assert_public_type::<WalletTransactionRecoveryUiServices>();

    assert_public_path(WalletUiServices::new);
    let _: fn(
        Arc<dyn GetDiagnosticSnapshotUseCase>,
        Arc<dyn ClearDiagnosticsUseCase>,
    ) -> DiagnosticsUiServices = DiagnosticsUiServices::new;
    let _: fn(_, _, _, _, _, String, _) -> PassportVaultUiServices = PassportVaultUiServices::new;
    assert_public_path(PassportVaultContractCallRecoveryUiServices::new);
    let _: fn(_, _, _, _, _, String, _) -> PassportVaultContractCallUiServices =
        PassportVaultContractCallUiServices::new;
    let _: fn(
        WalletDustSyncUiServices,
        WalletDustRegistrationUiServices,
        WalletShieldedSyncUiServices,
        WalletTransactionUiServices,
        PassportVaultUiServices,
    ) -> WalletOperationalUiServices = WalletOperationalUiServices::new;
    assert_public_path(DidUiServices::new);
    let _: fn(
        CredentialInventoryUiServices,
        CredentialIssuanceUiServices,
        CredentialPresentationUiServices,
        CredentialDisclosureUiServices,
    ) -> CredentialUiServices = CredentialUiServices::new;
    assert_public_path(CredentialInventoryUiServices::new);
    assert_public_path(CredentialIssuanceUiServices::new);
    assert_public_path(CredentialPresentationUiServices::new);
    assert_public_path(CredentialDisclosureUiServices::new);
    assert_public_path(SelfIssuedAuthenticationUiServices::new);
    let _: fn(
        DidUiServices,
        CredentialUiServices,
        SelfIssuedAuthenticationUiServices,
        IdentityIngressUiServices,
    ) -> IdentityUiServices = IdentityUiServices::new;
    assert_public_path(IdentityIngressUiServices::new);
    assert_public_path(WalletProfileUiServices::new);
    assert_public_path(WalletSecurityUiServices::new);
    assert_public_path(WalletBackupUiServices::new);
    assert_public_path(WalletAccountUiServices::new);
    assert_public_path(WalletDustSyncUiServices::new);
    assert_public_path(WalletDustRegistrationUiServices::new);
    assert_public_path(WalletDustRegistrationRecoveryUiServices::new);
    assert_public_path(WalletShieldedSyncUiServices::new);
    assert_public_path(WalletTransactionUiServices::new);
    assert_public_path(WalletTransactionPreparationUiServices::new);
    assert_public_path(WalletTransactionRecoveryUiServices::new);
}

#[test]
fn other_stable_facade_paths_remain_at_the_crate_root() {
    assert_public_type::<BrandProfile>();
    assert_public_type::<SecurityCopySnapshot>();
    assert_public_path(BrandProfile::new);
    assert_public_path(security_copy_snapshot);
    assert_public_path(App);
}

#[cfg(feature = "ui-profile-dev")]
#[test]
fn developer_manifest_context_remains_at_the_crate_root() {
    assert_public_type::<CapabilityManifestContext>();
    assert_public_path(CapabilityManifestContext::new);
    assert_public_path(WalletUiServices::with_developer_capabilities);
}
