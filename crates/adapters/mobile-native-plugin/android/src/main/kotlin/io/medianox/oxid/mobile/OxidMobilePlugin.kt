// SPDX-License-Identifier: Apache-2.0

package io.medianox.oxid.mobile

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Looper
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.json.JSONObject

class OxidMobilePlugin(private val activity: Activity) {
    fun startScanJson(): String = ScannerState.start(activity)

    fun takeScanResultJson(): String = ScannerState.take()

    fun takeIdentityLinkJson(): String = IdentityLinkState.take()

    fun copyPublicReceiveAddress(value: String): String {
        return if (onUiThread {
            val clipboard = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboard.setPrimaryClip(ClipData.newPlainText("Oxid receive address", value))
        }) "copied" else "failed"
    }

    fun sharePublicReceiveAddress(value: String): String {
        return if (onUiThread {
            val send = Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, value)
            }
            activity.startActivity(Intent.createChooser(send, "Share Oxid receive address"))
        }) "presented" else "unavailable"
    }

    private fun onUiThread(operation: () -> Unit): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return runCatching(operation).isSuccess
        }
        val completed = CountDownLatch(1)
        var succeeded = false
        activity.runOnUiThread {
            succeeded = runCatching(operation).isSuccess
            completed.countDown()
        }
        return completed.await(2, TimeUnit.SECONDS) && succeeded
    }

    companion object {
        /** Called only by Oxid's repository-owned MainActivity for ACTION_VIEW intents. */
        @JvmStatic
        fun captureIdentityLink(value: String?) {
            IdentityLinkState.capture(value)
        }
    }
}

private object IdentityLinkState {
    private const val MAX_LINK_LENGTH = 32 * 1024
    private const val MAX_PENDING_LINKS = 1
    private val pending = ArrayDeque<String>()
    private var pendingError: String? = null

    @Synchronized
    fun capture(value: String?) {
        if (value == null || value.isEmpty() || value.length > MAX_LINK_LENGTH ||
            value.trim() != value || value.any(Char::isISOControl)
        ) {
            pendingError = "invalid"
            return
        }
        if (pending.size >= MAX_PENDING_LINKS) {
            pendingError = "queue_full"
            return
        }
        pending.addLast(value)
    }

    @Synchronized
    fun take(): String {
        val value = pending.removeFirstOrNull()
        if (value != null) return json("succeeded", value)
        val error = pendingError
        if (error != null) {
            pendingError = null
            return json(error)
        }
        return json("empty")
    }

    private fun json(status: String, payload: String? = null): String {
        val result = JSONObject()
        result.put("status", status)
        if (payload != null) result.put("payload", payload)
        return result.toString()
    }
}

private object ScannerState {
    private var status: String = "idle"
    private var payload: String? = null

    @Synchronized
    fun start(activity: Activity): String {
        if (status == "scanning") return json("failed")
        status = "scanning"
        payload = null
        activity.runOnUiThread {
            try {
                val options = GmsBarcodeScannerOptions.Builder()
                    .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
                    .enableAutoZoom()
                    .build()
                GmsBarcodeScanning.getClient(activity, options)
                    .startScan()
                    .addOnSuccessListener { barcode ->
                        val raw = barcode.rawValue
                        if (raw == null) finish("invalid", null) else finish("succeeded", raw)
                    }
                    .addOnCanceledListener { finish("cancelled", null) }
                    .addOnFailureListener { finish("failed", null) }
            } catch (_: Exception) {
                finish("unavailable", null)
            }
        }
        return json("scanning")
    }

    @Synchronized
    fun take(): String {
        val current = json(status, payload)
        if (status != "scanning") {
            status = "idle"
            payload = null
        }
        return current
    }

    @Synchronized
    private fun finish(next: String, value: String?) {
        status = next
        payload = value
    }

    private fun json(value: String, text: String? = null): String {
        val result = JSONObject()
        result.put("status", value)
        if (text != null) result.put("payload", text)
        return result.toString()
    }
}
