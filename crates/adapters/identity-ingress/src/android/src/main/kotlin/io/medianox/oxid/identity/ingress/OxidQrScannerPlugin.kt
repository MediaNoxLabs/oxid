// SPDX-License-Identifier: Apache-2.0

package io.medianox.oxid.identity.ingress

import android.app.Activity
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import org.json.JSONObject

class OxidQrScannerPlugin(private val activity: Activity) {
    fun startScanJson(): String = ScannerState.start(activity)

    fun takeScanResultJson(): String = ScannerState.take()
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
