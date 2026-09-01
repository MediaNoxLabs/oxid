// SPDX-License-Identifier: Apache-2.0

use crate::compose_in_memory;

#[test]
fn composition_exposes_every_application_capability() {
    let services = compose_in_memory();

    drop(services.create_wallet_profile());
    drop(services.list_wallet_profiles());
    drop(services.select_wallet_profile());
    drop(services.get_active_wallet_profile());
    drop(services.get_wallet_security_status());
    drop(services.initialize_wallet_security());
    drop(services.unlock_wallet());
    drop(services.lock_wallet());
    drop(services.export_portable_wallet_backup());
    drop(services.recover_portable_wallet_backup());
    drop(services.export_complete_wallet_backup());
    drop(services.recover_complete_wallet_backup());
    drop(services.portable_wallet_backup_documents());
    drop(services.generate_wallet_key());
    drop(services.list_wallet_keys());
    drop(services.sign_wallet_data());
    drop(services.delete_wallet_key());
    drop(services.list_wallet_networks());
    drop(services.select_wallet_network());
    drop(services.derive_wallet_account());
    drop(services.get_wallet_account());
    drop(services.sync_wallet_account());
    drop(services.get_wallet_dust_sync_status());
    drop(services.start_wallet_dust_sync());
    drop(services.cancel_wallet_dust_sync());
    drop(services.get_wallet_shielded_sync_status());
    drop(services.start_wallet_shielded_sync());
    drop(services.cancel_wallet_shielded_sync());
    drop(services.prepare_wallet_transfer());
    drop(services.authorize_wallet_transfer());
    drop(services.submit_wallet_transfer());
    drop(services.get_wallet_transfer_draft());
    drop(services.get_wallet_transfer_submission_status());
    drop(services.cancel_wallet_transfer_submission());
    drop(services.list_wallet_transfer_submissions());
    drop(services.reconcile_wallet_transfer_submission());
    drop(services.resolve_did());
    drop(services.list_did_records());
    drop(services.get_did_record());
    drop(services.forget_did());
    drop(services.receive_credential());
    drop(services.list_credentials());
    drop(services.get_credential());
    drop(services.reverify_credential());
    drop(services.delete_credential());
    drop(services.prepare_credential_issuance());
    drop(services.accept_credential_issuance());
    drop(services.refuse_credential_issuance());
    drop(services.get_credential_issuance());
    drop(services.list_credential_issuances());
    drop(services.prepare_self_issued_authentication());
    drop(services.accept_self_issued_authentication());
    drop(services.refuse_self_issued_authentication());
    drop(services.get_self_issued_authentication());
    drop(services.list_self_issued_authentications());
    drop(services.prepare_passport_vault_call());
    drop(services.authorize_passport_vault_call());
    drop(services.submit_passport_vault_call());
    drop(services.get_passport_vault_call());
    drop(services.get_passport_vault_call_submission_status());
    drop(services.cancel_passport_vault_call_submission());
    drop(services.list_passport_vault_call_submissions());
    drop(services.reconcile_passport_vault_call_submission());
}
