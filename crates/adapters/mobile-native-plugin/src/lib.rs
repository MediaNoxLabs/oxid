// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

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

#[cfg(target_os = "android")]
fn call_android_activity(method: &str) -> Result<String, NativeBridgeError> {
    manganis::android::with_activity(|mut environment, activity| {
        let result = (|| {
            let value = environment
                .call_method(activity, method, "()Ljava/lang/String;", &[])
                .map_err(|_| NativeBridgeError::Failed)?;
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
            let value = environment
                .new_string(value)
                .map_err(|_| NativeBridgeError::Failed)?;
            let argument = manganis::jni::objects::JValue::Object(value.as_ref());
            let result = environment
                .call_method(
                    activity,
                    method,
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[argument],
                )
                .map_err(|_| NativeBridgeError::Failed)?;
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
    let object = value.l().map_err(|_| NativeBridgeError::Failed)?;
    if object.is_null() {
        return Err(NativeBridgeError::Failed);
    }
    let string = manganis::jni::objects::JString::from(object);
    environment
        .get_string(&string)
        .map(Into::into)
        .map_err(|_| NativeBridgeError::Failed)
}

#[cfg(target_os = "ios")]
use ios_bridge::{
    OxidMobilePlugin, copyPublicReceiveAddress, sharePublicReceiveAddress, startScanJson,
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
