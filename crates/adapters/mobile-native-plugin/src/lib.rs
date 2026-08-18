// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(any(target_os = "ios", target_os = "android"))]
use serde::Serialize;
#[cfg(any(target_os = "ios", target_os = "android"))]
use zeroize::Zeroizing;

/// Payload-free failure from the repository-owned native bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeBridgeError {
    Unavailable,
    Failed,
}

#[cfg(target_os = "ios")]
pub fn start_scan_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    startScanJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn start_scan_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidStartScanJson")
}

#[cfg(target_os = "ios")]
pub fn take_scan_result_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    takeScanResultJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn take_scan_result_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidTakeScanResultJson")
}

#[cfg(target_os = "android")]
pub fn take_identity_link_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidTakeIdentityLinkJson")
}

#[cfg(target_os = "ios")]
pub fn copy_public_receive_address(value: &str) -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    copyPublicReceiveAddress(&plugin, value.to_owned()).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn copy_public_receive_address(value: &str) -> Result<String, NativeBridgeError> {
    call_android_activity_with_string("oxidCopyPublicReceiveAddress", value)
}

#[cfg(target_os = "ios")]
pub fn share_public_receive_address(value: &str) -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    sharePublicReceiveAddress(&plugin, value.to_owned()).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn share_public_receive_address(value: &str) -> Result<String, NativeBridgeError> {
    call_android_activity_with_string("oxidSharePublicReceiveAddress", value)
}

#[cfg(target_os = "ios")]
pub fn start_backup_export_json(
    file_name: &str,
    payload: &str,
) -> Result<String, NativeBridgeError> {
    let request = backup_export_request(file_name, payload)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    startBackupExportJson(&plugin, request.to_string()).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn start_backup_export_json(
    file_name: &str,
    payload: &str,
) -> Result<String, NativeBridgeError> {
    let request = backup_export_request(file_name, payload)?;
    call_android_activity_with_string("oxidStartBackupExportJson", &request)
}

#[cfg(target_os = "ios")]
pub fn start_backup_import_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    startBackupImportJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn start_backup_import_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidStartBackupImportJson")
}

#[cfg(target_os = "ios")]
pub fn take_backup_document_result_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    takeBackupDocumentResultJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn take_backup_document_result_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidTakeBackupDocumentResultJson")
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Serialize)]
struct NativeBackupExportRequest<'a> {
    file_name: &'a str,
    payload: &'a str,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn backup_export_request(
    file_name: &str,
    payload: &str,
) -> Result<Zeroizing<String>, NativeBridgeError> {
    serde_json::to_string(&NativeBackupExportRequest { file_name, payload })
        .map(Zeroizing::new)
        .map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "ios")]
pub fn inspect_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_ios_custody("inspect", profile_id, None, None)
}

#[cfg(target_os = "android")]
pub fn inspect_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("inspect", profile_id, None, None)
}

#[cfg(target_os = "ios")]
pub fn initialize_custody_json(
    profile_id: &str,
    payload: &str,
) -> Result<String, NativeBridgeError> {
    call_ios_custody("initialize", profile_id, Some(payload), None)
}

#[cfg(target_os = "android")]
pub fn initialize_custody_json(
    profile_id: &str,
    payload: &str,
) -> Result<String, NativeBridgeError> {
    call_android_custody("initialize", profile_id, Some(payload), None)
}

#[cfg(target_os = "ios")]
pub fn unlock_custody_json(profile_id: &str, reason: &str) -> Result<String, NativeBridgeError> {
    call_ios_custody("unlock", profile_id, None, Some(reason))
}

#[cfg(target_os = "android")]
pub fn unlock_custody_json(profile_id: &str, reason: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("unlock", profile_id, None, Some(reason))
}

#[cfg(target_os = "ios")]
pub fn load_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_ios_custody("load", profile_id, None, None)
}

#[cfg(target_os = "android")]
pub fn load_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("load", profile_id, None, None)
}

#[cfg(target_os = "ios")]
pub fn save_custody_json(profile_id: &str, payload: &str) -> Result<String, NativeBridgeError> {
    call_ios_custody("save", profile_id, Some(payload), None)
}

#[cfg(target_os = "android")]
pub fn save_custody_json(profile_id: &str, payload: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("save", profile_id, Some(payload), None)
}

#[cfg(target_os = "ios")]
pub fn lock_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_ios_custody("lock", profile_id, None, None)
}

#[cfg(target_os = "android")]
pub fn lock_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("lock", profile_id, None, None)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Serialize)]
struct NativeCustodyRequest<'a> {
    operation: &'a str,
    profile_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn custody_request(
    operation: &str,
    profile_id: &str,
    payload: Option<&str>,
    reason: Option<&str>,
) -> Result<Zeroizing<String>, NativeBridgeError> {
    serde_json::to_string(&NativeCustodyRequest {
        operation,
        profile_id,
        payload,
        reason,
    })
    .map(Zeroizing::new)
    .map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "ios")]
fn call_ios_custody(
    operation: &str,
    profile_id: &str,
    payload: Option<&str>,
    reason: Option<&str>,
) -> Result<String, NativeBridgeError> {
    let request = custody_request(operation, profile_id, payload, reason)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    custodyJson(&plugin, request.to_string()).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
fn call_android_custody(
    operation: &str,
    profile_id: &str,
    payload: Option<&str>,
    reason: Option<&str>,
) -> Result<String, NativeBridgeError> {
    let request = custody_request(operation, profile_id, payload, reason)?;
    call_android_activity_with_string("oxidCustodyJson", &request)
}

#[cfg(target_os = "android")]
fn call_android_activity(method: &str) -> Result<String, NativeBridgeError> {
    manganis::android::with_activity(|mut environment, activity| {
        let result = (|| {
            let value = environment.call_method(activity, method, "()Ljava/lang/String;", &[]);
            let value = android_jni_result(&mut environment, value)?;
            android_string(&mut environment, value)
        })();
        Some(result)
    })
    .ok_or(NativeBridgeError::Unavailable)?
}

#[cfg(target_os = "android")]
fn call_android_activity_with_string(
    method: &str,
    value: &str,
) -> Result<String, NativeBridgeError> {
    manganis::android::with_activity(|mut environment, activity| {
        let result = (|| {
            let value = environment.new_string(value);
            let value = android_jni_result(&mut environment, value)?;
            let argument = manganis::jni::objects::JValue::Object(value.as_ref());
            let result = environment.call_method(
                activity,
                method,
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[argument],
            );
            let result = android_jni_result(&mut environment, result)?;
            android_string(&mut environment, result)
        })();
        Some(result)
    })
    .ok_or(NativeBridgeError::Unavailable)?
}

#[cfg(target_os = "android")]
fn android_string<'local>(
    environment: &mut manganis::jni::JNIEnv<'local>,
    value: manganis::jni::objects::JValueOwned<'local>,
) -> Result<String, NativeBridgeError> {
    let object = value.l();
    let object = android_jni_result(environment, object)?;
    if object.is_null() {
        return Err(NativeBridgeError::Failed);
    }
    let string = manganis::jni::objects::JString::from(object);
    let string = environment.get_string(&string);
    android_jni_result(environment, string).map(Into::into)
}

#[cfg(target_os = "android")]
fn android_jni_result<T>(
    environment: &mut manganis::jni::JNIEnv<'_>,
    result: manganis::jni::errors::Result<T>,
) -> Result<T, NativeBridgeError> {
    result.map_err(|_| {
        clear_pending_android_exception(environment);
        NativeBridgeError::Failed
    })
}

#[cfg(target_os = "android")]
fn clear_pending_android_exception(environment: &mut manganis::jni::JNIEnv<'_>) {
    if matches!(environment.exception_check(), Ok(true)) {
        let _ = environment.exception_clear();
    }
}

/// Proves that a thrown activity exception is cleared before the next bridge call.
///
/// This is compiled only into the explicit Android smoke-test composition. It
/// returns the same payload-free failure as every other native bridge error and
/// never inspects, describes, or logs the Java exception.
#[cfg(all(target_os = "android", feature = "android-jni-exception-recovery-test"))]
pub fn verify_android_jni_exception_recovery() -> Result<(), NativeBridgeError> {
    if call_android_activity("oxidThrowForJniRecoveryTest") != Err(NativeBridgeError::Failed) {
        return Err(NativeBridgeError::Failed);
    }
    call_android_activity("oxidJniRecoveryProbeJson").map(|_| ())
}

#[cfg(target_os = "ios")]
use ios_bridge::{
    OxidMobilePlugin, copyPublicReceiveAddress, custodyJson, sharePublicReceiveAddress,
    startBackupExportJson, startBackupImportJson, startScanJson, takeBackupDocumentResultJson,
    takeScanResultJson,
};

#[cfg(target_os = "ios")]
#[allow(non_snake_case)]
mod ios_bridge {
    #[manganis::ffi("ios")]
    extern "Swift" {
        pub type OxidMobilePlugin;
        pub fn startScanJson(this: &OxidMobilePlugin) -> String;
        pub fn takeScanResultJson(this: &OxidMobilePlugin) -> String;
        pub fn copyPublicReceiveAddress(this: &OxidMobilePlugin, value: String) -> String;
        pub fn sharePublicReceiveAddress(this: &OxidMobilePlugin, value: String) -> String;
        pub fn startBackupExportJson(this: &OxidMobilePlugin, request: String) -> String;
        pub fn startBackupImportJson(this: &OxidMobilePlugin) -> String;
        pub fn takeBackupDocumentResultJson(this: &OxidMobilePlugin) -> String;
        pub fn custodyJson(this: &OxidMobilePlugin, request: String) -> String;
    }
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
mod android_bridge {
    #[manganis::ffi("android")]
    extern "Kotlin" {
        pub type OxidMobilePlugin;
    }
}
