// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::time::Duration;

#[cfg(any(target_os = "ios", target_os = "android"))]
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_mobile_native::{
    NativeBridgeError, start_backup_export_json, start_backup_import_json,
    take_backup_document_result_json,
};
#[cfg(any(target_os = "ios", target_os = "android", test))]
use oxid_wallet_application::MAX_PORTABLE_WALLET_BACKUP_BYTES;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_wallet_application::PORTABLE_WALLET_BACKUP_FILE_NAME;
use oxid_wallet_application::{
    PortableWalletBackup, PortableWalletBackupDocumentError, PortableWalletBackupDocumentFuture,
    PortableWalletBackupDocumentPort,
};
#[cfg(any(target_os = "ios", target_os = "android", test))]
use serde::Deserialize;
#[cfg(any(target_os = "ios", target_os = "android"))]
use zeroize::Zeroizing;

#[cfg(any(target_os = "ios", target_os = "android"))]
const DOCUMENT_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(any(target_os = "ios", target_os = "android"))]
const DOCUMENT_POLL_LIMIT: usize = 3_000;
#[cfg(any(target_os = "ios", target_os = "android", test))]
const MAX_NATIVE_DOCUMENT_RESPONSE_BYTES: usize =
    MAX_PORTABLE_WALLET_BACKUP_BYTES.div_ceil(3) * 4 + 128;

/// Native adapter backed only by the iOS and Android document-picker APIs.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePortableWalletBackupDocuments;

impl PortableWalletBackupDocumentPort for NativePortableWalletBackupDocuments {
    fn export<'a>(
        &'a self,
        backup: &'a PortableWalletBackup,
    ) -> PortableWalletBackupDocumentFuture<'a, ()> {
        Box::pin(async move { export_native(backup).await })
    }

    fn import<'a>(&'a self) -> PortableWalletBackupDocumentFuture<'a, PortableWalletBackup> {
        Box::pin(async { import_native().await })
    }
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeDocumentStatus {
    status: String,
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn export_native(
    backup: &PortableWalletBackup,
) -> Result<(), PortableWalletBackupDocumentError> {
    let payload = Zeroizing::new(STANDARD.encode(backup.as_bytes()));
    let started = start_backup_export_json(PORTABLE_WALLET_BACKUP_FILE_NAME, &payload)
        .map_err(map_bridge_error)?;
    require_started(&started, "exporting")?;
    poll_document(false).await.map(|_| ())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn export_native(
    _backup: &PortableWalletBackup,
) -> Result<(), PortableWalletBackupDocumentError> {
    Err(PortableWalletBackupDocumentError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn import_native() -> Result<PortableWalletBackup, PortableWalletBackupDocumentError> {
    let started = start_backup_import_json().map_err(map_bridge_error)?;
    require_started(&started, "importing")?;
    poll_document(true)
        .await?
        .ok_or(PortableWalletBackupDocumentError::InvalidDocument)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn import_native() -> Result<PortableWalletBackup, PortableWalletBackupDocumentError> {
    Err(PortableWalletBackupDocumentError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn require_started(
    response: &str,
    expected: &str,
) -> Result<(), PortableWalletBackupDocumentError> {
    let status = parse_status(response)?;
    if status.payload.is_some() {
        return Err(PortableWalletBackupDocumentError::Failed);
    }
    if status.status == expected {
        Ok(())
    } else {
        Err(map_status(&status.status))
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn poll_document(
    importing: bool,
) -> Result<Option<PortableWalletBackup>, PortableWalletBackupDocumentError> {
    let pending = if importing { "importing" } else { "exporting" };
    for _ in 0..DOCUMENT_POLL_LIMIT {
        tokio::time::sleep(DOCUMENT_POLL_INTERVAL).await;
        let response = take_backup_document_result_json().map_err(map_bridge_error)?;
        let status = parse_status(&response)?;
        match status.status.as_str() {
            value if value == pending && status.payload.is_none() => {}
            "exported" if !importing && status.payload.is_none() => return Ok(None),
            "imported" if importing => {
                let encoded = Zeroizing::new(
                    status
                        .payload
                        .ok_or(PortableWalletBackupDocumentError::InvalidDocument)?,
                );
                let bytes = STANDARD
                    .decode(encoded.as_bytes())
                    .map_err(|_| PortableWalletBackupDocumentError::InvalidDocument)?;
                return PortableWalletBackup::parse(bytes)
                    .map(Some)
                    .map_err(|_| PortableWalletBackupDocumentError::InvalidDocument);
            }
            _ if status.payload.is_some() => {
                return Err(PortableWalletBackupDocumentError::Failed);
            }
            other => return Err(map_status(other)),
        }
    }
    Err(PortableWalletBackupDocumentError::TimedOut)
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn parse_status(response: &str) -> Result<NativeDocumentStatus, PortableWalletBackupDocumentError> {
    if response.is_empty() || response.len() > MAX_NATIVE_DOCUMENT_RESPONSE_BYTES {
        return Err(PortableWalletBackupDocumentError::Failed);
    }
    serde_json::from_str(response).map_err(|_| PortableWalletBackupDocumentError::Failed)
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn map_status(status: &str) -> PortableWalletBackupDocumentError {
    match status {
        "cancelled" => PortableWalletBackupDocumentError::Cancelled,
        "unavailable" => PortableWalletBackupDocumentError::Unavailable,
        "invalid" => PortableWalletBackupDocumentError::InvalidDocument,
        _ => PortableWalletBackupDocumentError::Failed,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
const fn map_bridge_error(error: NativeBridgeError) -> PortableWalletBackupDocumentError {
    match error {
        NativeBridgeError::Unavailable => PortableWalletBackupDocumentError::Unavailable,
        NativeBridgeError::Failed => PortableWalletBackupDocumentError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_adapter_fails_closed() {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            return;
        }
        let adapter = NativePortableWalletBackupDocuments;
        let backup = PortableWalletBackup::parse(vec![1]).expect("bounded backup");
        assert_eq!(
            futures_lite(&adapter, &backup),
            Err(PortableWalletBackupDocumentError::Unavailable)
        );
    }

    #[test]
    fn native_status_parser_is_strict_and_bounded() {
        let imported = parse_status(r#"{"status":"imported","payload":"AQ=="}"#)
            .expect("strict native result");
        assert_eq!(imported.status, "imported");
        assert_eq!(imported.payload.as_deref(), Some("AQ=="));
        assert!(parse_status(r#"{"status":"imported","payload":"AQ==","path":"/tmp/x"}"#).is_err());
        assert!(parse_status(&"x".repeat(MAX_NATIVE_DOCUMENT_RESPONSE_BYTES + 1)).is_err());
    }

    #[test]
    fn native_failures_are_payload_free_categories() {
        assert_eq!(
            map_status("cancelled"),
            PortableWalletBackupDocumentError::Cancelled
        );
        assert_eq!(
            map_status("unavailable"),
            PortableWalletBackupDocumentError::Unavailable
        );
        assert_eq!(
            map_status("invalid"),
            PortableWalletBackupDocumentError::InvalidDocument
        );
        assert_eq!(
            map_status("busy"),
            PortableWalletBackupDocumentError::Failed
        );
    }

    fn futures_lite(
        adapter: &NativePortableWalletBackupDocuments,
        backup: &PortableWalletBackup,
    ) -> Result<(), PortableWalletBackupDocumentError> {
        futures::executor::block_on(adapter.export(backup))
    }
}
