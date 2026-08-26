// SPDX-License-Identifier: Apache-2.0

package dev.dioxus.main

import android.content.Intent
import android.os.Bundle
import io.medianox.oxid.mobile.OxidMobilePlugin

typealias BuildConfig = io.medianox.oxid.BuildConfig

/**
 * Keeps Dioxus/Wry as the host while adding the smallest Android lifecycle seam
 * required for cold and warm identity app links.
 */
class MainActivity : WryActivity() {
    private val oxidMobilePlugin by lazy { OxidMobilePlugin(this) }

    override fun onCreate(savedInstanceState: Bundle?) {
        captureIdentityLink(intent)
        super.onCreate(savedInstanceState)
    }

    override fun onNewIntent(intent: Intent) {
        captureIdentityLink(intent)
        setIntent(intent)
        super.onNewIntent(intent)
    }

    override fun onPause() {
        OxidMobilePlugin.captureScanHostSuspended()
        super.onPause()
    }

    @Deprecated("Android activity-result callback required by the device credential prompt")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (!OxidMobilePlugin.captureBackupDocumentResult(
                this,
                requestCode,
                resultCode,
                data
            ) && !OxidMobilePlugin.captureCustodyAuthorizationResult(requestCode, resultCode)
        ) {
            super.onActivityResult(requestCode, resultCode, data)
        }
    }

    private fun captureIdentityLink(intent: Intent?) {
        if (intent?.action != Intent.ACTION_VIEW) return
        val scheme = intent.data?.scheme ?: return
        if (scheme != "openid-credential-offer" && scheme != "openid4vp") return
        OxidMobilePlugin.captureIdentityLink(intent.dataString)
    }

    /** JNI entry points use the activity instance so Android resolves classes with the app loader. */
    fun oxidStartScanJson(): String = oxidMobilePlugin.startScanJson()

    fun oxidTakeScanResultJson(): String = oxidMobilePlugin.takeScanResultJson()

    fun oxidTimeoutScanJson(): String = oxidMobilePlugin.timeoutScanJson()

    fun oxidTakeIdentityLinkJson(): String = oxidMobilePlugin.takeIdentityLinkJson()

    fun oxidVirtualDeviceProfileJson(): String = oxidMobilePlugin.virtualDeviceProfileJson()

    fun oxidCopyPublicReceiveAddress(value: String): String =
        oxidMobilePlugin.copyPublicReceiveAddress(value)

    fun oxidSharePublicReceiveAddress(value: String): String =
        oxidMobilePlugin.sharePublicReceiveAddress(value)

    fun oxidSetScreenPrivacy(enabled: Boolean): String =
        oxidMobilePlugin.setScreenPrivacy(enabled)

    fun oxidStartBackupExportJson(request: String): String =
        oxidMobilePlugin.startBackupExportJson(request)

    fun oxidStartBackupImportJson(): String = oxidMobilePlugin.startBackupImportJson()

    fun oxidTakeBackupDocumentResultJson(): String =
        oxidMobilePlugin.takeBackupDocumentResultJson()

    fun oxidCustodyJson(request: String): String = oxidMobilePlugin.custodyJson(request)

    /** Smoke-only JNI failure injection; normal builds have no Rust caller for this method. */
    fun oxidThrowForJniRecoveryTest(): String {
        if (BuildConfig.DEBUG) throw IllegalStateException()
        return "{\"status\":\"unavailable\"}"
    }

    /** Side-effect-free second call for the smoke-only JNI recovery probe. */
    fun oxidJniRecoveryProbeJson(): String = "{\"status\":\"ready\"}"
}
