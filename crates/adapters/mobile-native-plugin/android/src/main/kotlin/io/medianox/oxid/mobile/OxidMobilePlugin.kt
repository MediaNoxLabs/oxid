// SPDX-License-Identifier: Apache-2.0

package io.medianox.oxid.mobile

import android.app.Activity
import android.app.KeyguardManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Looper
import android.os.SystemClock
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyInfo
import android.security.keystore.KeyProperties
import android.security.keystore.UserNotAuthenticatedException
import android.util.AtomicFile
import android.util.Base64
import android.view.WindowManager
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.mlkit.common.MlKitException
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import java.io.File
import java.io.ByteArrayOutputStream
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.SecretKeyFactory
import javax.crypto.spec.GCMParameterSpec
import java.util.Locale
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.json.JSONObject

class OxidMobilePlugin(private val activity: Activity) {
    fun startScanJson(): String = ScannerState.start(activity)

    fun takeScanResultJson(): String = ScannerState.take()

    fun timeoutScanJson(): String = ScannerState.timeout()

    fun takeIdentityLinkJson(): String = IdentityLinkState.take()

    fun virtualDeviceProfileJson(): String {
        val hardware = Build.HARDWARE.orEmpty().lowercase(Locale.ROOT)
        val product = Build.PRODUCT.orEmpty().lowercase(Locale.ROOT)
        val fingerprint = Build.FINGERPRINT.orEmpty().lowercase(Locale.ROOT)
        val qemuHardware = hardware == "ranchu" || hardware == "goldfish"
        val emulatorBuild = product.startsWith("sdk_") ||
            product.contains("emulator") ||
            fingerprint.startsWith("generic") ||
            fingerprint.contains("/emu") ||
            fingerprint.startsWith("google/sdk_gphone")
        return JSONObject(mapOf("androidQemu" to (qemuHardware && emulatorBuild))).toString()
    }

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

    fun setScreenPrivacy(enabled: Boolean): String {
        val changed = onUiThread {
            if (enabled) {
                activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
            } else {
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
        }
        return if (!changed) "failed" else if (enabled) "protected" else "unprotected"
    }

    fun custodyJson(request: String): String = CustodyCoordinator.dispatch(activity, request)

    fun startBackupExportJson(request: String): String =
        BackupDocumentCoordinator.startExport(activity, request)

    fun startBackupImportJson(): String = BackupDocumentCoordinator.startImport(activity)

    fun takeBackupDocumentResultJson(): String = BackupDocumentCoordinator.take()

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

        /** Called only by Oxid's owning activity when an active scanner takes foreground. */
        @JvmStatic
        fun captureScanHostSuspended() {
            ScannerState.captureHostSuspended()
        }

        /** Called by Oxid's repository-owned MainActivity for the credential prompt result. */
        @JvmStatic
        fun captureCustodyAuthorizationResult(requestCode: Int, resultCode: Int): Boolean {
            return CustodyAuthorization.complete(requestCode, resultCode)
        }

        /** Called only by Oxid's repository-owned MainActivity for document-picker results. */
        @JvmStatic
        fun captureBackupDocumentResult(
            activity: Activity,
            requestCode: Int,
            resultCode: Int,
            data: Intent?
        ): Boolean {
            return BackupDocumentCoordinator.complete(activity, requestCode, resultCode, data)
        }
    }
}

private object BackupDocumentCoordinator {
    private const val EXPORT_REQUEST_CODE = 0x0A72
    private const val IMPORT_REQUEST_CODE = 0x0A73
    private const val MAX_PACKAGE_BYTES = 80 * 1024 * 1024
    private val ALLOWED_FILE_NAMES = setOf(
        "oxid-wallet-custody.oxidbak",
        "oxid-wallet.oxidbak",
    )
    private var status = "idle"
    private var resultPayload: String? = null
    private var exportBytes: ByteArray? = null

    @Synchronized
    fun startExport(activity: Activity, request: String): String {
        if (status == "exporting" || status == "importing") return json("busy")
        if (request.isEmpty() || request.length > MAX_PACKAGE_BYTES * 2) return json("invalid")
        val body = runCatching { JSONObject(request) }.getOrNull() ?: return json("invalid")
        if (body.keys().asSequence().toSet() != setOf("file_name", "payload")) {
            return json("invalid")
        }
        val fileName = body.optString("file_name", "")
        if (fileName !in ALLOWED_FILE_NAMES) return json("invalid")
        val encoded = body.optString("payload", "")
        if (encoded.isEmpty() || encoded.length > ((MAX_PACKAGE_BYTES + 2) / 3) * 4) {
            return json("invalid")
        }
        val bytes = runCatching { Base64.decode(encoded, Base64.NO_WRAP) }.getOrNull()
            ?: return json("invalid")
        if (bytes.isEmpty() || bytes.size > MAX_PACKAGE_BYTES ||
            Base64.encodeToString(bytes, Base64.NO_WRAP) != encoded
        ) {
            bytes.fill(0)
            return json("invalid")
        }
        status = "exporting"
        resultPayload = null
        exportBytes = bytes
        activity.runOnUiThread {
            runCatching {
                val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "application/octet-stream"
                    putExtra(Intent.EXTRA_TITLE, fileName)
                }
                @Suppress("DEPRECATION")
                activity.startActivityForResult(intent, EXPORT_REQUEST_CODE)
            }.onFailure { failStart("unavailable") }
        }
        return json("exporting")
    }

    @Synchronized
    fun startImport(activity: Activity): String {
        if (status == "exporting" || status == "importing") return json("busy")
        status = "importing"
        resultPayload = null
        activity.runOnUiThread {
            runCatching {
                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "application/octet-stream"
                }
                @Suppress("DEPRECATION")
                activity.startActivityForResult(intent, IMPORT_REQUEST_CODE)
            }.onFailure { failStart("unavailable") }
        }
        return json("importing")
    }

    @Synchronized
    fun take(): String {
        val result = json(status, resultPayload)
        if (status != "exporting" && status != "importing") {
            status = "idle"
            resultPayload = null
        }
        return result
    }

    fun complete(
        activity: Activity,
        requestCode: Int,
        resultCode: Int,
        data: Intent?
    ): Boolean {
        if (requestCode != EXPORT_REQUEST_CODE && requestCode != IMPORT_REQUEST_CODE) return false
        if (resultCode != Activity.RESULT_OK) {
            finish("cancelled")
            return true
        }
        val uri = data?.data
        if (uri == null) {
            finish("invalid")
            return true
        }
        Thread {
            if (requestCode == EXPORT_REQUEST_CODE) exportTo(activity, uri)
            else importFrom(activity, uri)
        }.start()
        return true
    }

    private fun exportTo(activity: Activity, uri: android.net.Uri) {
        val bytes = synchronized(this) {
            if (status != "exporting") null else exportBytes?.also { exportBytes = null }
        }
        if (bytes == null) {
            finish("failed")
            return
        }
        try {
            val succeeded = runCatching {
                activity.contentResolver.openOutputStream(uri, "wt")?.use { output ->
                    output.write(bytes)
                    output.flush()
                    true
                } ?: false
            }.getOrDefault(false)
            finish(if (succeeded) "exported" else "failed")
        } finally {
            bytes.fill(0)
        }
    }

    private fun importFrom(activity: Activity, uri: android.net.Uri) {
        val declaredSize = runCatching {
            activity.contentResolver.openAssetFileDescriptor(uri, "r")?.use { it.length }
        }.getOrNull()
        if (declaredSize != null && (declaredSize == 0L || declaredSize > MAX_PACKAGE_BYTES)) {
            finish("invalid")
            return
        }
        val bytes = runCatching {
            activity.contentResolver.openInputStream(uri)?.use { input ->
                val output = ByteArrayOutputStream()
                val buffer = ByteArray(16 * 1024)
                while (true) {
                    val read = input.read(buffer)
                    if (read < 0) break
                    output.write(buffer, 0, read)
                    if (output.size() > MAX_PACKAGE_BYTES) return@use null
                }
                output.toByteArray().takeIf { it.isNotEmpty() }
            }
        }.getOrNull()
        if (bytes == null) {
            finish("invalid")
            return
        }
        try {
            finish("imported", Base64.encodeToString(bytes, Base64.NO_WRAP))
        } finally {
            bytes.fill(0)
        }
    }

    @Synchronized
    private fun failStart(next: String) {
        exportBytes?.fill(0)
        exportBytes = null
        status = next
        resultPayload = null
    }

    @Synchronized
    private fun finish(next: String, payload: String? = null) {
        exportBytes?.fill(0)
        exportBytes = null
        status = next
        resultPayload = payload
    }

    private fun json(value: String, payload: String? = null): String {
        return JSONObject().apply {
            put("status", value)
            if (payload != null) put("payload", payload)
        }.toString()
    }
}

private object CustodyCoordinator {
    private const val KEYSTORE = "AndroidKeyStore"
    private const val KEY_PREFIX = "io.medianox.oxid.custody.v1."
    private const val VAULT_VERSION = 1
    private const val MAX_PLAINTEXT_BYTES = 512 * 1024
    private const val MAX_RECORD_BYTES = MAX_PLAINTEXT_BYTES * 2
    private const val AUTH_DURATION_SECONDS = 30
    private val aadDomain = "oxid-mobile-custody-v1\u0000".toByteArray(StandardCharsets.UTF_8)
    private val sessions = mutableMapOf<String, Long>()

    fun dispatch(activity: Activity, request: String): String {
        if (request.isEmpty() || request.length > MAX_PLAINTEXT_BYTES * 2) return json("invalid")
        val body = runCatching { JSONObject(request) }.getOrNull() ?: return json("invalid")
        val operation = body.optString("operation", "")
        val profileId = body.optString("profile_id", "")
        val expected = when (operation) {
            "initialize", "save" -> setOf("operation", "profile_id", "payload")
            "unlock" -> setOf("operation", "profile_id", "reason")
            "inspect", "load", "lock" -> setOf("operation", "profile_id")
            else -> return json("invalid")
        }
        if (body.keys().asSequence().toSet() != expected) return json("invalid")
        return when (operation) {
            "inspect" -> inspect(activity, profileId)
            "initialize" -> initialize(activity, profileId, body.optString("payload", ""))
            "unlock" -> unlock(activity, profileId, body.optString("reason", ""))
            "load" -> load(activity, profileId)
            "save" -> save(activity, profileId, body.optString("payload", ""))
            "lock" -> lock(activity, profileId)
            else -> json("invalid")
        }
    }

    @Synchronized
    fun inspect(activity: Activity, profileId: String): String {
        if (!validProfileId(profileId)) return json("invalid")
        val state = recordState(activity, profileId)
        if (state == RecordState.INVALID) return json("invalid")
        if (state == RecordState.MISSING) return json("uninitialized")
        val protection = protection(activity, profileId) ?: return json("unavailable")
        return if (active(profileId)) json("unlocked", protection)
        else json("locked", protection)
    }

    @Synchronized
    fun initialize(activity: Activity, profileId: String, payload: String): String {
        if (!validProfileId(profileId)) return json("invalid")
        val plaintext = decodePayload(payload) ?: return json("invalid")
        try {
            when (recordState(activity, profileId)) {
                RecordState.PRESENT -> return json("already_initialized")
                RecordState.INVALID -> return json("invalid")
                RecordState.MISSING -> Unit
            }
            val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
            if (!keyguard.isDeviceSecure) return json("unavailable")
            val generated = generateKey(profileId) ?: return json("unavailable")
            val key = generated.first
            val protection = generated.second
            if (!CustodyAuthorization.request(
                    activity,
                    "Protect Oxid wallet",
                    "Confirm the device credential to create device-bound wallet protection"
                )
            ) {
                deleteKey(profileId)
                return json("authorization_denied")
            }
            val record = encrypt(profileId, protection, key, plaintext) ?: run {
                deleteKey(profileId)
                return json("unavailable")
            }
            if (!writeRecord(activity, profileId, record)) {
                deleteKey(profileId)
                return json("unavailable")
            }
            sessions[profileId] = sessionDeadline()
            return json("succeeded", protection)
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    fun unlock(activity: Activity, profileId: String, reason: String): String {
        if (!validProfileId(profileId) || !validReason(reason)) return json("invalid")
        when (recordState(activity, profileId)) {
            RecordState.MISSING -> return json("not_initialized")
            RecordState.INVALID -> return json("invalid")
            RecordState.PRESENT -> Unit
        }
        sessions.remove(profileId)
        if (!CustodyAuthorization.request(activity, "Unlock Oxid", reason)) {
            return json("authorization_denied")
        }
        val plaintext = decrypt(activity, profileId) ?: return json("unavailable")
        sessions[profileId] = sessionDeadline()
        return try {
            json(
                "succeeded",
                protection(activity, profileId) ?: "operating_system",
                Base64.encodeToString(plaintext, Base64.NO_WRAP)
            )
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    fun load(activity: Activity, profileId: String): String {
        if (!validProfileId(profileId)) return json("invalid")
        if (recordState(activity, profileId) == RecordState.MISSING) return json("not_initialized")
        if (!active(profileId)) return json("locked")
        val plaintext = decrypt(activity, profileId) ?: run {
            sessions.remove(profileId)
            return json("locked")
        }
        return try {
            json(
                "succeeded",
                protection(activity, profileId) ?: "operating_system",
                Base64.encodeToString(plaintext, Base64.NO_WRAP)
            )
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    fun save(activity: Activity, profileId: String, payload: String): String {
        if (!validProfileId(profileId)) return json("invalid")
        val plaintext = decodePayload(payload) ?: return json("invalid")
        try {
            if (!active(profileId)) return json("locked")
            val key = key(profileId) ?: return json("not_initialized")
            val protection = protection(activity, profileId) ?: return json("invalid")
            val record = encrypt(profileId, protection, key, plaintext) ?: run {
                sessions.remove(profileId)
                return json("locked")
            }
            if (!writeRecord(activity, profileId, record)) return json("unavailable")
            return json("succeeded", protection)
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    fun lock(activity: Activity, profileId: String): String {
        if (!validProfileId(profileId)) return json("invalid")
        if (recordState(activity, profileId) == RecordState.MISSING) return json("not_initialized")
        sessions.remove(profileId)
        return json("locked", protection(activity, profileId) ?: "operating_system")
    }

    private enum class RecordState { MISSING, PRESENT, INVALID }

    private fun recordState(activity: Activity, profileId: String): RecordState {
        val hasRecord = recordFile(activity, profileId).isFile
        val hasKey = runCatching {
            KeyStore.getInstance(KEYSTORE).apply { load(null) }.containsAlias(alias(profileId))
        }.getOrDefault(false)
        return when {
            hasRecord && hasKey -> RecordState.PRESENT
            !hasRecord && !hasKey -> RecordState.MISSING
            else -> RecordState.INVALID
        }
    }

    private fun generateKey(profileId: String): Pair<SecretKey, String>? {
        val alias = alias(profileId)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val strong = runCatching { generateKey(alias, true) }.getOrNull()
            if (strong != null) {
                val protection = protection(strong)
                if (protection != null) return Pair(strong, protection)
            }
            deleteKey(profileId)
        }
        val fallback = runCatching { generateKey(alias, false) }.getOrNull() ?: return null
        return protection(fallback)?.let { Pair(fallback, it) }
    }

    private fun generateKey(alias: String, strongBox: Boolean): SecretKey {
        val builder = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setRandomizedEncryptionRequired(true)
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            builder.setUserAuthenticationParameters(
                AUTH_DURATION_SECONDS,
                KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL
            )
        } else {
            @Suppress("DEPRECATION")
            builder.setUserAuthenticationValidityDurationSeconds(AUTH_DURATION_SECONDS)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && strongBox) {
            builder.setIsStrongBoxBacked(true)
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(builder.build())
        return generator.generateKey()
    }

    private fun key(profileId: String): SecretKey? = runCatching {
        KeyStore.getInstance(KEYSTORE).apply { load(null) }
            .getKey(alias(profileId), null) as? SecretKey
    }.getOrNull()

    private fun deleteKey(profileId: String) {
        runCatching {
            KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(alias(profileId))
        }
    }

    private fun encrypt(
        profileId: String,
        protection: String,
        key: SecretKey,
        plaintext: ByteArray
    ): JSONObject? {
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, key)
            cipher.updateAAD(aad(profileId, protection))
            val encrypted = cipher.doFinal(plaintext)
            JSONObject().apply {
                put("version", VAULT_VERSION)
                put("protection", protection)
                put("iv", Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
                put("ciphertext", Base64.encodeToString(encrypted, Base64.NO_WRAP))
            }
        } catch (_: UserNotAuthenticatedException) {
            null
        } catch (_: Exception) {
            null
        }
    }

    private fun decrypt(activity: Activity, profileId: String): ByteArray? {
        val record = readRecord(activity, profileId) ?: return null
        val key = key(profileId) ?: return null
        return try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, record.iv))
            cipher.updateAAD(aad(profileId, record.protection))
            cipher.doFinal(record.ciphertext)
                .takeIf { it.isNotEmpty() && it.size <= MAX_PLAINTEXT_BYTES }
        } catch (_: Exception) {
            null
        } finally {
            record.ciphertext.fill(0)
        }
    }

    private data class SealedRecord(
        val protection: String,
        val iv: ByteArray,
        val ciphertext: ByteArray
    )

    private fun readRecord(activity: Activity, profileId: String): SealedRecord? {
        return runCatching {
            val bytes = recordFile(activity, profileId).readBytes()
            if (bytes.isEmpty() || bytes.size > MAX_RECORD_BYTES) return null
            val objectValue = JSONObject(String(bytes, StandardCharsets.UTF_8))
            val keys = objectValue.keys().asSequence().toSet()
            if (keys != setOf("version", "protection", "iv", "ciphertext") ||
                objectValue.getInt("version") != VAULT_VERSION
            ) return null
            val protection = objectValue.getString("protection")
            if (protection != "operating_system" && protection != "hardware_backed") return null
            val iv = Base64.decode(objectValue.getString("iv"), Base64.NO_WRAP)
            val ciphertext = Base64.decode(objectValue.getString("ciphertext"), Base64.NO_WRAP)
            if (iv.size != 12 || ciphertext.size <= 16 || ciphertext.size > MAX_PLAINTEXT_BYTES + 16) {
                ciphertext.fill(0)
                return null
            }
            SealedRecord(protection, iv, ciphertext)
        }.getOrNull()
    }

    private fun writeRecord(activity: Activity, profileId: String, record: JSONObject): Boolean {
        val data = record.toString().toByteArray(StandardCharsets.UTF_8)
        if (data.isEmpty() || data.size > MAX_RECORD_BYTES) return false
        val file = recordFile(activity, profileId)
        if (!file.parentFile!!.exists() && !file.parentFile!!.mkdirs()) return false
        val atomic = AtomicFile(file)
        val output = runCatching { atomic.startWrite() }.getOrNull() ?: return false
        return try {
            output.write(data)
            output.flush()
            output.fd.sync()
            atomic.finishWrite(output)
            true
        } catch (_: Exception) {
            atomic.failWrite(output)
            false
        } finally {
            data.fill(0)
        }
    }

    private fun protection(key: SecretKey): String? {
        return runCatching {
            val factory = SecretKeyFactory.getInstance(key.algorithm, KEYSTORE)
            val info = factory.getKeySpec(key, KeyInfo::class.java) as KeyInfo
            if (info.isInsideSecureHardware) "hardware_backed" else "operating_system"
        }.getOrNull()
    }

    private fun protection(activity: Activity, profileId: String): String? {
        val record = readRecord(activity, profileId) ?: return null
        record.ciphertext.fill(0)
        return record.protection
    }

    private fun recordFile(activity: Activity, profileId: String): File {
        return File(File(activity.noBackupFilesDir, "oxid-custody-v1"), digest(profileId) + ".json")
    }

    private fun alias(profileId: String): String = KEY_PREFIX + digest(profileId)

    private fun digest(value: String): String {
        return MessageDigest.getInstance("SHA-256")
            .digest(value.toByteArray(StandardCharsets.UTF_8))
            .joinToString("") { "%02x".format(it) }
    }

    private fun aad(profileId: String, protection: String): ByteArray =
        aadDomain + profileId.toByteArray(StandardCharsets.UTF_8) +
            byteArrayOf(0) + protection.toByteArray(StandardCharsets.UTF_8)

    private fun active(profileId: String): Boolean {
        val deadline = sessions[profileId] ?: return false
        if (SystemClock.elapsedRealtime() >= deadline) {
            sessions.remove(profileId)
            return false
        }
        return true
    }

    private fun sessionDeadline(): Long =
        SystemClock.elapsedRealtime() + TimeUnit.SECONDS.toMillis(AUTH_DURATION_SECONDS.toLong())

    private fun decodePayload(payload: String): ByteArray? {
        if (payload.isEmpty() || payload.length > MAX_PLAINTEXT_BYTES * 2) return null
        return runCatching { Base64.decode(payload, Base64.NO_WRAP) }
            .getOrNull()
            ?.takeIf { it.isNotEmpty() && it.size <= MAX_PLAINTEXT_BYTES }
    }

    private fun validProfileId(value: String): Boolean {
        return value.isNotEmpty() && value.codePointCount(0, value.length) <= 128 &&
            value.none { it.isWhitespace() || it.isISOControl() }
    }

    private fun validReason(value: String): Boolean {
        return value.isNotEmpty() && value.codePointCount(0, value.length) <= 160 &&
            value.none(Char::isISOControl)
    }

    private fun json(status: String, protection: String? = null, payload: String? = null): String {
        return JSONObject().apply {
            put("status", status)
            if (protection != null) put("protection", protection)
            if (payload != null) put("payload", payload)
        }.toString()
    }
}

private object CustodyAuthorization {
    const val REQUEST_CODE = 0x0A71
    private var pending: Pending? = null

    private data class Pending(val latch: CountDownLatch, var accepted: Boolean = false)

    fun request(activity: Activity, title: String, description: String): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper()) return false
        val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        @Suppress("DEPRECATION")
        val intent = keyguard.createConfirmDeviceCredentialIntent(title, description) ?: return false
        val request = Pending(CountDownLatch(1))
        synchronized(this) {
            if (pending != null) return false
            pending = request
        }
        activity.runOnUiThread {
            runCatching {
                @Suppress("DEPRECATION")
                activity.startActivityForResult(intent, REQUEST_CODE)
            }.onFailure { complete(REQUEST_CODE, Activity.RESULT_CANCELED) }
        }
        val completed = request.latch.await(65, TimeUnit.SECONDS)
        synchronized(this) {
            if (pending === request) pending = null
        }
        return completed && request.accepted
    }

    @Synchronized
    fun complete(requestCode: Int, resultCode: Int): Boolean {
        if (requestCode != REQUEST_CODE) return false
        val request = pending ?: return true
        request.accepted = resultCode == Activity.RESULT_OK
        request.latch.countDown()
        return true
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
    private const val MAX_PAYLOAD_BYTES = 32 * 1024
    private var status: String = "idle"
    private var payload: String? = null
    private var generation: Long = 0
    private var hostSuspendedDuringScan: Boolean = false

    @Synchronized
    fun start(activity: Activity): String {
        if (status == "scanning") return json("failed")
        generation += 1
        val activeGeneration = generation
        status = "scanning"
        payload = null
        hostSuspendedDuringScan = false
        activity.runOnUiThread {
            try {
                val availability = GoogleApiAvailability.getInstance()
                    .isGooglePlayServicesAvailable(activity)
                if (availability != ConnectionResult.SUCCESS) {
                    finish("unavailable", null, activeGeneration)
                    return@runOnUiThread
                }
                val options = GmsBarcodeScannerOptions.Builder()
                    .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
                    .enableAutoZoom()
                    .build()
                GmsBarcodeScanning.getClient(activity, options)
                    .startScan()
                    .addOnSuccessListener { barcode ->
                        val raw = barcode.rawValue
                        if (raw.isNullOrEmpty() ||
                            raw.toByteArray(StandardCharsets.UTF_8).size > MAX_PAYLOAD_BYTES
                        ) {
                            finish("invalid", null, activeGeneration)
                        } else {
                            finish("succeeded", raw, activeGeneration)
                        }
                    }
                    .addOnCanceledListener { finish("cancelled", null, activeGeneration) }
                    .addOnFailureListener { failure ->
                        finishFailure(
                            (failure as? MlKitException)?.errorCode,
                            activeGeneration
                        )
                    }
            } catch (_: Exception) {
                finish("unavailable", null, activeGeneration)
            }
        }
        return json("scanning")
    }

    @Synchronized
    fun captureHostSuspended() {
        if (status == "scanning") hostSuspendedDuringScan = true
    }

    @Synchronized
    fun take(): String {
        val current = json(status, payload)
        if (status != "scanning") {
            status = "idle"
            payload = null
            hostSuspendedDuringScan = false
        }
        return current
    }

    @Synchronized
    fun timeout(): String {
        if (status != "scanning") {
            val current = json(status, payload)
            status = "idle"
            payload = null
            hostSuspendedDuringScan = false
            return current
        }
        generation += 1
        status = "idle"
        payload = null
        hostSuspendedDuringScan = false
        // Google Code Scanner exposes no programmatic dismissal API. This
        // closes Oxid's logical one-item handoff and makes every eventual task
        // callback stale; the system-owned scanner UI may still require the
        // holder to dismiss it.
        return json("timed_out")
    }

    @Synchronized
    private fun finish(next: String, value: String?, activeGeneration: Long) {
        if (status != "scanning" || generation != activeGeneration) return
        status = next
        payload = value
        hostSuspendedDuringScan = false
    }

    @Synchronized
    private fun finishFailure(errorCode: Int?, activeGeneration: Long) {
        if (status != "scanning" || generation != activeGeneration) return
        status = when {
            errorCode == MlKitException.CODE_SCANNER_CANCELLED -> "cancelled"
            // Google Code Scanner 16.1.0 has been observed on Samsung/API 36
            // to return INTERNAL when Back closes its already-presented native
            // activity. Normalize only after Oxid actually lost foreground;
            // pre-presentation internal failures remain fail-closed failures.
            errorCode == MlKitException.INTERNAL && hostSuspendedDuringScan -> "cancelled"
            errorCode == MlKitException.CODE_SCANNER_CAMERA_PERMISSION_NOT_GRANTED ->
                "unavailable"
            errorCode == MlKitException.CODE_SCANNER_UNAVAILABLE ||
                errorCode == MlKitException.CODE_SCANNER_GOOGLE_PLAY_SERVICES_VERSION_TOO_OLD ->
                "unavailable"
            else -> "failed"
        }
        payload = null
        hostSuspendedDuringScan = false
    }

    private fun json(value: String, text: String? = null): String {
        val result = JSONObject()
        result.put("status", value)
        if (text != null) result.put("payload", text)
        return result.toString()
    }
}
