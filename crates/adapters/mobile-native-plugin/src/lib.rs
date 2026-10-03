// SPDX-License-Identifier: Apache-2.0

#![deny(unsafe_code)]

pub mod custody;

#[cfg(target_os = "ios")]
use std::sync::Mutex;

#[cfg(any(target_os = "ios", target_os = "android", test))]
use serde::Deserialize;
#[cfg(any(target_os = "ios", target_os = "android", test))]
use serde::Serialize;
#[cfg(any(target_os = "ios", target_os = "android", test))]
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

#[cfg(any(target_os = "android", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AndroidVirtualDeviceProfile {
    #[serde(rename = "androidQemu")]
    android_qemu: bool,
}

#[cfg(any(target_os = "android", test))]
fn decode_android_qemu_profile(value: &str) -> Result<(), NativeBridgeError> {
    let profile: AndroidVirtualDeviceProfile =
        serde_json::from_str(value).map_err(|_| NativeBridgeError::Failed)?;
    if profile.android_qemu {
        Ok(())
    } else {
        Err(NativeBridgeError::Unavailable)
    }
}

#[cfg(target_os = "android")]
pub fn verify_android_qemu_profile() -> Result<(), NativeBridgeError> {
    let response = call_android_activity("oxidVirtualDeviceProfileJson")?;
    decode_android_qemu_profile(&response)
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

/// Closes only the active QR capture handoff after the Rust scanner budget
/// expires. The call carries no payload and cannot route or execute a request.
#[cfg(target_os = "ios")]
pub fn timeout_scan_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    timeoutScanJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn timeout_scan_json() -> Result<String, NativeBridgeError> {
    call_android_activity("oxidTimeoutScanJson")
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
pub fn set_screen_privacy(protected: bool) -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    setScreenPrivacy(&plugin, protected).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn set_screen_privacy(protected: bool) -> Result<String, NativeBridgeError> {
    call_android_activity_with_bool("oxidSetScreenPrivacy", protected)
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

/// Starts one fresh native authorization for a recovery-phrase screen. No
/// profile, secret, or caller-controlled reason crosses this operation.
#[cfg(target_os = "ios")]
pub fn authorize_recovery_phrase_reveal_json() -> Result<String, NativeBridgeError> {
    let plugin = OxidMobilePlugin::new().map_err(|_| NativeBridgeError::Unavailable)?;
    authorizeRecoveryPhraseRevealJson(&plugin).map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "android")]
pub fn authorize_recovery_phrase_reveal_json() -> Result<String, NativeBridgeError> {
    call_android_custody("authorize_recovery_phrase_reveal", "", None, None)
}

// Legacy platform transport. New adapters implement the separate custody
// byte contract; never wrap this JSON path to implement CustodyTransport.
#[cfg(target_os = "android")]
pub fn inspect_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("inspect", profile_id, None, None)
}

#[cfg(target_os = "android")]
pub fn initialize_custody_json(
    profile_id: &str,
    payload: &str,
) -> Result<String, NativeBridgeError> {
    call_android_custody("initialize", profile_id, Some(payload), None)
}

#[cfg(target_os = "android")]
pub fn unlock_custody_json(profile_id: &str, reason: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("unlock", profile_id, None, Some(reason))
}

#[cfg(target_os = "android")]
pub fn load_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("load", profile_id, None, None)
}

#[cfg(target_os = "android")]
pub fn save_custody_json(profile_id: &str, payload: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("save", profile_id, Some(payload), None)
}

#[cfg(target_os = "android")]
pub fn lock_custody_json(profile_id: &str) -> Result<String, NativeBridgeError> {
    call_android_custody("lock", profile_id, None, None)
}

#[cfg(any(target_os = "android", test))]
#[derive(Serialize)]
struct NativeCustodyRequest<'a> {
    operation: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    profile_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[cfg(any(target_os = "android", test))]
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
static IOS_CUSTODY_CALL: Mutex<()> = Mutex::new(());

#[cfg(target_os = "ios")]
#[derive(Serialize)]
struct IosCustodyBufferRequest<'a> {
    profile_id: &'a str,
    address: String,
    length: String,
}

#[cfg(target_os = "ios")]
#[derive(Serialize)]
struct IosCustodyUnlockRequest<'a> {
    profile_id: &'a str,
    reason: &'a str,
}

#[cfg(target_os = "ios")]
#[derive(Serialize)]
struct IosCustodyTakeRequest<'a> {
    operation: &'a str,
    profile_id: &'a str,
    generation: String,
    address: String,
    length: String,
}

#[cfg(target_os = "ios")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IosCustodyLength {
    length: String,
    generation: String,
}

#[cfg(target_os = "ios")]
fn ios_buffer_request(
    profile_id: &str,
    address: u64,
    length: usize,
) -> Result<String, custody::Error> {
    serde_json::to_string(&IosCustodyBufferRequest {
        profile_id,
        address: address.to_string(),
        length: length.to_string(),
    })
    .map_err(|_| custody::Error::Failed)
}

/// Secret-safe iOS custody transport. Only closed control JSON crosses as a
/// string; custody is copied synchronously through a Rust-owned mutable buffer.
#[cfg(target_os = "ios")]
pub fn inspect_custody(profile_id: &str) -> Result<custody::CustodyState, custody::Error> {
    let _call = IOS_CUSTODY_CALL
        .lock()
        .map_err(|_| custody::Error::Failed)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| custody::Error::Unavailable)?;
    let control = custodyInspectControl(&plugin, profile_id.to_owned())
        .map_err(|_| custody::Error::Failed)?;
    match custody::decode_reply(custody::Operation::Inspect, control.as_bytes(), None)? {
        custody::Reply::Uninitialized => Ok(custody::CustodyState::Uninitialized),
        custody::Reply::Locked(protection) => Ok(custody::CustodyState::Locked(protection)),
        custody::Reply::Unlocked(protection) => Ok(custody::CustodyState::Unlocked(protection)),
        _ => Err(custody::Error::Invalid),
    }
}

#[cfg(target_os = "ios")]
pub fn initialize_custody(
    profile_id: &str,
    bytes: &custody::CustodyBytes,
) -> Result<custody::Protection, custody::Error> {
    let _call = IOS_CUSTODY_CALL
        .lock()
        .map_err(|_| custody::Error::Failed)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| custody::Error::Unavailable)?;
    let request = ios_buffer_request(
        profile_id,
        bytes.as_bytes().as_ptr() as usize as u64,
        bytes.as_bytes().len(),
    )?;
    let control = custodyInitializeControl(&plugin, request).map_err(|_| custody::Error::Failed)?;
    match custody::decode_reply(custody::Operation::Initialize, control.as_bytes(), None)? {
        custody::Reply::Stored(protection) => Ok(protection),
        _ => Err(custody::Error::Invalid),
    }
}

#[cfg(target_os = "ios")]
pub fn unlock_custody(
    profile_id: &str,
    reason: &str,
) -> Result<custody::ProtectedCustody, custody::Error> {
    receive_ios_custody(custody::Operation::Unlock, profile_id, |plugin| {
        let request = serde_json::to_string(&IosCustodyUnlockRequest { profile_id, reason })
            .map_err(|_| "invalid request")?;
        custodyPrepareUnlockControl(plugin, request)
    })
}

#[cfg(target_os = "ios")]
pub fn load_custody(profile_id: &str) -> Result<custody::ProtectedCustody, custody::Error> {
    receive_ios_custody(custody::Operation::Load, profile_id, |plugin| {
        custodyPrepareLoadControl(plugin, profile_id.to_owned())
    })
}

#[cfg(target_os = "ios")]
fn receive_ios_custody(
    operation: custody::Operation,
    profile_id: &str,
    prepare: impl FnOnce(&OxidMobilePlugin) -> Result<String, &'static str>,
) -> Result<custody::ProtectedCustody, custody::Error> {
    let _call = IOS_CUSTODY_CALL
        .lock()
        .map_err(|_| custody::Error::Failed)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| custody::Error::Unavailable)?;
    let control = prepare(&plugin).map_err(|_| custody::Error::Failed)?;
    let length = match custodyPendingLengthJson(&plugin)
        .map_err(|_| custody::Error::Failed)
        .and_then(|length| {
            serde_json::from_str::<IosCustodyLength>(&length).map_err(|_| custody::Error::Invalid)
        })
        .and_then(|length| {
            let generation = length
                .generation
                .parse::<u64>()
                .map_err(|_| custody::Error::Invalid)?;
            let length = length
                .length
                .parse::<usize>()
                .map_err(|_| custody::Error::Invalid)?;
            Ok((length, generation))
        }) {
        Ok(length) => length,
        Err(error) => {
            let _ = custodyDiscardPending(&plugin);
            return Err(error);
        }
    };
    let (length, generation) = length;
    let bytes = if length == 0 {
        None
    } else {
        let received = custody::CustodyBytes::receive(length, |destination| {
            let operation = match operation {
                custody::Operation::Unlock => "unlock",
                custody::Operation::Load => "load",
                _ => return Err(custody::Error::Invalid),
            };
            let request = serde_json::to_string(&IosCustodyTakeRequest {
                operation,
                profile_id,
                generation: generation.to_string(),
                address: (destination.as_mut_ptr() as usize as u64).to_string(),
                length: destination.len().to_string(),
            })
            .map_err(|_| custody::Error::Failed)?;
            let accepted =
                custodyTakePending(&plugin, request).map_err(|_| custody::Error::Failed)?;
            if accepted == "taken" {
                Ok(destination.len())
            } else {
                Err(custody::Error::Invalid)
            }
        });
        match received {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                let _ = custodyDiscardPending(&plugin);
                return Err(error);
            }
        }
    };
    let reply = custody::decode_reply(operation, control.as_bytes(), bytes);
    if reply.is_err() {
        let _ = custodyDiscardPending(&plugin);
    }
    match reply? {
        custody::Reply::Material { protection, bytes } => {
            Ok(custody::ProtectedCustody { protection, bytes })
        }
        _ => Err(custody::Error::Invalid),
    }
}

#[cfg(target_os = "ios")]
pub fn save_custody(
    profile_id: &str,
    bytes: &custody::CustodyBytes,
) -> Result<custody::Protection, custody::Error> {
    let _call = IOS_CUSTODY_CALL
        .lock()
        .map_err(|_| custody::Error::Failed)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| custody::Error::Unavailable)?;
    let request = ios_buffer_request(
        profile_id,
        bytes.as_bytes().as_ptr() as usize as u64,
        bytes.as_bytes().len(),
    )?;
    let control = custodySaveControl(&plugin, request).map_err(|_| custody::Error::Failed)?;
    match custody::decode_reply(custody::Operation::Save, control.as_bytes(), None)? {
        custody::Reply::Stored(protection) => Ok(protection),
        _ => Err(custody::Error::Invalid),
    }
}

#[cfg(target_os = "ios")]
pub fn lock_custody(profile_id: &str) -> Result<custody::Protection, custody::Error> {
    let _call = IOS_CUSTODY_CALL
        .lock()
        .map_err(|_| custody::Error::Failed)?;
    let plugin = OxidMobilePlugin::new().map_err(|_| custody::Error::Unavailable)?;
    let control =
        custodyLockControl(&plugin, profile_id.to_owned()).map_err(|_| custody::Error::Failed)?;
    match custody::decode_reply(custody::Operation::Lock, control.as_bytes(), None)? {
        custody::Reply::Locked(protection) => Ok(protection),
        _ => Err(custody::Error::Invalid),
    }
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

/// Initializes every Android certificate-verifier runtime resolved by Oxid.
///
/// The reqwest and Subxt transport graphs currently resolve different
/// `rustls-platform-verifier` versions. Each version owns an independent
/// process-global Android runtime slot, so both must be initialized before any
/// HTTPS-backed wallet or identity capability is allowed to start.
#[cfg(target_os = "android")]
pub fn initialize_android_tls() -> Result<(), NativeBridgeError> {
    manganis::android::with_activity(|environment, activity| {
        let result = (|| {
            let activity_05 = environment.new_local_ref(activity);
            let activity_05 = android_jni_result(environment, activity_05)?;
            let initialized_05 =
                rustls_platform_verifier_05::android::init_with_env(environment, activity_05);
            android_jni_result(environment, initialized_05)?;

            let activity_07 = environment.new_local_ref(activity);
            let activity_07 = android_jni_result(environment, activity_07)?.into_raw();
            initialize_android_tls_07(environment, activity_07).map_err(|error| {
                clear_pending_android_exception(environment);
                error
            })
        })();
        Some(result)
    })
    .ok_or(NativeBridgeError::Unavailable)?
}

/// Bridges the activity reference already authenticated by Manganis from JNI
/// 0.21 into the JNI 0.22 wrapper used by rustls-platform-verifier 0.7.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
fn initialize_android_tls_07(
    environment: &mut manganis::jni::JNIEnv<'_>,
    activity: manganis::jni::sys::jobject,
) -> Result<(), NativeBridgeError> {
    use jni_022::{EnvUnowned, Outcome, objects::JObject};

    let raw_environment = environment
        .get_raw()
        .cast::<std::ffi::c_void>()
        .cast::<jni_022::sys::JNIEnv>();
    let raw_activity: jni_022::sys::jobject = activity.cast();

    // SAFETY: Manganis supplies the current thread's live JNI environment and
    // activity local reference for the duration of this closure. Ownership of
    // the dedicated local reference was transferred out of the 0.21 wrapper;
    // the 0.22 wrapper neither retains nor deletes the raw reference.
    let mut environment_022 = unsafe { EnvUnowned::from_raw(raw_environment) };
    let outcome = environment_022
        .with_env(|environment_022| -> jni_022::errors::Result<()> {
            // SAFETY: `raw_activity` is the same live local reference borrowed
            // above and is scoped to the JNI frame represented by this env.
            let activity_07 = unsafe { JObject::from_raw(environment_022, raw_activity) };
            rustls_platform_verifier_07::android::init_with_env(environment_022, activity_07)
        })
        .into_outcome();

    match outcome {
        Outcome::Ok(()) => Ok(()),
        Outcome::Err(_) | Outcome::Panic(_) => Err(NativeBridgeError::Failed),
    }
}

#[cfg(target_os = "android")]
fn call_android_activity(method: &str) -> Result<String, NativeBridgeError> {
    manganis::android::with_activity(|environment, activity| {
        let result = (|| {
            let value = environment.call_method(activity, method, "()Ljava/lang/String;", &[]);
            let value = android_jni_result(environment, value)?;
            android_string(environment, value)
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
    manganis::android::with_activity(|environment, activity| {
        let result = (|| {
            let value = environment.new_string(value);
            let value = android_jni_result(environment, value)?;
            let argument = manganis::jni::objects::JValue::Object(value.as_ref());
            let result = environment.call_method(
                activity,
                method,
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[argument],
            );
            let result = android_jni_result(environment, result)?;
            android_string(environment, result)
        })();
        Some(result)
    })
    .ok_or(NativeBridgeError::Unavailable)?
}

#[cfg(target_os = "android")]
fn call_android_activity_with_bool(method: &str, value: bool) -> Result<String, NativeBridgeError> {
    manganis::android::with_activity(|environment, activity| {
        let result = environment.call_method(
            activity,
            method,
            "(Z)Ljava/lang/String;",
            &[manganis::jni::objects::JValue::Bool(u8::from(value))],
        );
        let result = android_jni_result(environment, result)
            .and_then(|value| android_string(environment, value));
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
    OxidMobilePlugin, authorizeRecoveryPhraseRevealJson, copyPublicReceiveAddress,
    custodyDiscardPending, custodyInitializeControl, custodyInspectControl, custodyLockControl,
    custodyPendingLengthJson, custodyPrepareLoadControl, custodyPrepareUnlockControl,
    custodySaveControl, custodyTakePending, setScreenPrivacy, sharePublicReceiveAddress,
    startBackupExportJson, startBackupImportJson, startScanJson, takeBackupDocumentResultJson,
    takeScanResultJson, timeoutScanJson,
};

#[cfg(target_os = "ios")]
#[allow(non_snake_case)]
mod ios_bridge {
    #[manganis::ffi("ios")]
    extern "Swift" {
        pub type OxidMobilePlugin;
        pub fn startScanJson(this: &OxidMobilePlugin) -> String;
        pub fn takeScanResultJson(this: &OxidMobilePlugin) -> String;
        pub fn timeoutScanJson(this: &OxidMobilePlugin) -> String;
        pub fn copyPublicReceiveAddress(this: &OxidMobilePlugin, value: String) -> String;
        pub fn sharePublicReceiveAddress(this: &OxidMobilePlugin, value: String) -> String;
        pub fn setScreenPrivacy(this: &OxidMobilePlugin, protected: bool) -> String;
        pub fn startBackupExportJson(this: &OxidMobilePlugin, request: String) -> String;
        pub fn startBackupImportJson(this: &OxidMobilePlugin) -> String;
        pub fn takeBackupDocumentResultJson(this: &OxidMobilePlugin) -> String;
        pub fn authorizeRecoveryPhraseRevealJson(this: &OxidMobilePlugin) -> String;
        pub fn custodyInspectControl(this: &OxidMobilePlugin, profile_id: String) -> String;
        pub fn custodyInitializeControl(this: &OxidMobilePlugin, request: String) -> String;
        pub fn custodyPrepareUnlockControl(this: &OxidMobilePlugin, request: String) -> String;
        pub fn custodyPrepareLoadControl(this: &OxidMobilePlugin, profile_id: String) -> String;
        pub fn custodyPendingLengthJson(this: &OxidMobilePlugin) -> String;
        pub fn custodyTakePending(this: &OxidMobilePlugin, request: String) -> String;
        pub fn custodyDiscardPending(this: &OxidMobilePlugin) -> String;
        pub fn custodySaveControl(this: &OxidMobilePlugin, request: String) -> String;
        pub fn custodyLockControl(this: &OxidMobilePlugin, profile_id: String) -> String;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_phrase_authorization_accepts_only_the_payload_free_operation() {
        let ios = include_str!("../ios/Sources/OxidMobilePlugin.swift");
        assert!(ios.contains("authorizeRecoveryPhraseRevealJson()"));
        assert!(!ios.contains("@objc public func custodyJson"));
        assert!(ios.contains("Confirm to reveal your new wallet recovery phrase"));

        let android =
            include_str!("../android/src/main/kotlin/io/medianox/oxid/mobile/OxidMobilePlugin.kt");
        assert!(android.contains("operation == \"authorize_recovery_phrase_reveal\""));
        assert!(android.contains("setOf(\"operation\")"));
        assert!(
            android.contains("Confirm the device credential to reveal your new recovery phrase")
        );
    }

    #[test]
    fn ios_custody_uses_closed_control_and_mutable_bytes_without_legacy_secret_strings() {
        let ios = include_str!("../ios/Sources/OxidMobilePlugin.swift");
        let storage = include_str!("../../storage-mobile/src/lib.rs");
        assert!(ios.contains("custodyInitializeControl"));
        assert!(ios.contains("custodyTakePending"));
        assert!(ios.contains("UnsafeMutableRawPointer"));
        assert!(ios.contains("material.bytes.resetBytes"));
        assert!(ios.contains("material.profileId == profileId"));
        assert!(ios.contains("material.generation == expectedGeneration"));
        assert!(!ios.contains("plaintext.base64EncodedString()"));
        assert!(!ios.contains("Data(base64Encoded: payload)"));
        assert!(storage.contains("oxid_adapter_mobile_native::initialize_custody"));
        assert!(storage.contains("oxid_adapter_mobile_native::unlock_custody"));
    }

    #[test]
    fn ios_privacy_overlay_is_removed_after_the_scene_becomes_active() {
        let ios = include_str!("../ios/Sources/OxidMobilePlugin.swift");
        assert!(ios.contains("UIScene.didActivateNotification"));
        assert!(ios.contains("UIApplication.shared.applicationState == .active"));
        assert!(ios.contains("removeOverlays()"));
    }

    #[test]
    fn android_plugin_packages_the_pinned_platform_verifier_component() {
        let gradle = include_str!("../android/build.gradle.kts");
        assert!(gradle.contains("rustlsPlatformVerifierMavenPath()"));
        assert!(gradle.contains("rootProject.allprojects"));
        assert!(gradle.contains("implementation(\"rustls:rustls-platform-verifier:0.1.1\")"));
        assert!(
            include_str!("../android/consumer-rules.pro").contains("org.rustls.platformverifier")
        );
    }

    #[test]
    fn android_camera_permission_failure_maps_to_denied() {
        let android =
            include_str!("../android/src/main/kotlin/io/medianox/oxid/mobile/OxidMobilePlugin.kt");
        assert!(android.contains(
            "errorCode == MlKitException.CODE_SCANNER_CAMERA_PERMISSION_NOT_GRANTED ->\n                \"denied\""
        ));
    }

    #[test]
    fn recovery_phrase_authorization_request_is_payload_free() {
        let request =
            custody_request("authorize_recovery_phrase_reveal", "", None, None).expect("request");
        assert_eq!(
            request.as_str(),
            "{\"operation\":\"authorize_recovery_phrase_reveal\"}"
        );
    }

    #[test]
    fn android_portal_profile_accepts_only_the_exact_positive_qemu_attestation() {
        assert_eq!(
            decode_android_qemu_profile(r#"{"androidQemu":true}"#),
            Ok(())
        );
        for rejected in [
            r#"{"androidQemu":false}"#,
            r#"{"androidQemu":true,"physical":false}"#,
            r#"{"androidQemu":"true"}"#,
            r#"{}"#,
            "not-json",
        ] {
            assert!(decode_android_qemu_profile(rejected).is_err());
        }
    }
}
