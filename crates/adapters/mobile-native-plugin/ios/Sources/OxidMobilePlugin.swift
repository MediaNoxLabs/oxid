// SPDX-License-Identifier: Apache-2.0

import AVFoundation
import Foundation
import LocalAuthentication
import Security
import UIKit
import UniformTypeIdentifiers

@objc(OxidMobilePlugin)
public final class OxidMobilePlugin: NSObject {
    @objc public func startScanJson() -> String {
        ScanCoordinator.shared.start()
    }

    @objc public func takeScanResultJson() -> String {
        ScanCoordinator.shared.take()
    }

    @objc public func timeoutScanJson() -> String {
        ScanCoordinator.shared.timeout()
    }

    @objc public func copyPublicReceiveAddress(_ value: String) -> String {
        onMain {
            UIPasteboard.general.string = value
            return "copied"
        }
    }

    @objc public func sharePublicReceiveAddress(_ value: String) -> String {
        onMain {
            guard let presenter = Self.topViewController() else { return "unavailable" }
            let controller = UIActivityViewController(
                activityItems: [value],
                applicationActivities: nil
            )
            if let popover = controller.popoverPresentationController {
                popover.sourceView = presenter.view
                popover.sourceRect = CGRect(
                    x: presenter.view.bounds.midX,
                    y: presenter.view.bounds.midY,
                    width: 1,
                    height: 1
                )
                popover.permittedArrowDirections = []
            }
            presenter.present(controller, animated: true)
            return "presented"
        }
    }

    @objc public func setScreenPrivacy(_ enabled: Bool) -> String {
        onMain { ScreenPrivacyCoordinator.shared.setEnabled(enabled) }
    }

    @objc public func startBackupExportJson(_ request: String) -> String {
        onMain { BackupDocumentCoordinator.shared.startExport(request: request) }
    }

    @objc public func startBackupImportJson() -> String {
        onMain { BackupDocumentCoordinator.shared.startImport() }
    }

    @objc public func takeBackupDocumentResultJson() -> String {
        BackupDocumentCoordinator.shared.take()
    }

    @objc public func custodyJson(_ request: String) -> String {
        CustodyCoordinator.shared.dispatch(request: request)
    }

    private func onMain(_ operation: @escaping () -> String) -> String {
        if Thread.isMainThread { return operation() }
        return DispatchQueue.main.sync(execute: operation)
    }

    fileprivate static func topViewController() -> UIViewController? {
        let root = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: { $0.isKeyWindow })?.rootViewController
        var current = root
        while let presented = current?.presentedViewController { current = presented }
        return current
    }
}

private final class ScreenPrivacyCoordinator: NSObject {
    static let shared = ScreenPrivacyCoordinator()

    private let overlayTag = 0x0A71D
    private var enabled = false

    override private init() {
        super.init()
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(sceneDidEnterBackground),
            name: UIScene.didEnterBackgroundNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(sceneWillEnterForeground),
            name: UIScene.willEnterForegroundNotification,
            object: nil
        )
    }

    func setEnabled(_ next: Bool) -> String {
        enabled = next
        if next && UIApplication.shared.applicationState != .active {
            installOverlays()
        } else if !next {
            removeOverlays()
        }
        return next ? "protected" : "unprotected"
    }

    @objc private func sceneDidEnterBackground() {
        if enabled { installOverlays() }
    }

    @objc private func sceneWillEnterForeground() {
        removeOverlays()
    }

    private func installOverlays() {
        for window in applicationWindows() where window.viewWithTag(overlayTag) == nil {
            let overlay = UIView(frame: window.bounds)
            overlay.tag = overlayTag
            overlay.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            overlay.backgroundColor = .black
            overlay.isAccessibilityElement = true
            overlay.accessibilityLabel = "Protected wallet preview"
            window.addSubview(overlay)
        }
    }

    private func removeOverlays() {
        for window in applicationWindows() {
            window.viewWithTag(overlayTag)?.removeFromSuperview()
        }
    }

    private func applicationWindows() -> [UIWindow] {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
    }
}

private final class BackupDocumentCoordinator: NSObject, UIDocumentPickerDelegate {
    static let shared = BackupDocumentCoordinator()

    private enum Operation {
        case export
        case importFile
    }

    private let lock = NSLock()
    private let maximumPackageBytes = 80 * 1024 * 1024
    private let allowedFileNames = Set([
        "oxid-wallet-custody.oxidbak",
        "oxid-wallet.oxidbak",
    ])
    private var operation: Operation?
    private var status = "idle"
    private var payload: String?
    private var temporaryDirectory: URL?

    func startExport(request: String) -> String {
        guard request.utf8.count <= maximumPackageBytes * 2,
              let requestData = request.data(using: .utf8),
              let body = try? JSONSerialization.jsonObject(with: requestData) as? [String: Any],
              Set(body.keys) == Set(["file_name", "payload"]),
              let fileName = body["file_name"] as? String,
              allowedFileNames.contains(fileName),
              let encoded = body["payload"] as? String,
              encoded.utf8.count <= ((maximumPackageBytes + 2) / 3) * 4,
              let bytes = Data(base64Encoded: encoded),
              !bytes.isEmpty,
              bytes.count <= maximumPackageBytes,
              bytes.base64EncodedString() == encoded else {
            return Self.json(status: "invalid")
        }
        lock.lock()
        guard operation == nil else {
            lock.unlock()
            return Self.json(status: "busy")
        }
        operation = .export
        status = "exporting"
        payload = nil
        lock.unlock()

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("oxid-backup-\(UUID().uuidString)", isDirectory: true)
        let file = directory.appendingPathComponent(fileName, isDirectory: false)
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700]
            )
            try bytes.write(to: file, options: [.atomic, .completeFileProtection])
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            var protectedFile = file
            try protectedFile.setResourceValues(values)
            lock.lock()
            temporaryDirectory = directory
            lock.unlock()
            guard let presenter = OxidMobilePlugin.topViewController() else {
                finish("unavailable")
                return Self.json(status: "unavailable")
            }
            let picker = UIDocumentPickerViewController(forExporting: [file], asCopy: true)
            picker.delegate = self
            presenter.present(picker, animated: true)
            return Self.json(status: "exporting")
        } catch {
            finish("failed")
            return Self.json(status: "failed")
        }
    }

    func startImport() -> String {
        lock.lock()
        guard operation == nil else {
            lock.unlock()
            return Self.json(status: "busy")
        }
        operation = .importFile
        status = "importing"
        payload = nil
        lock.unlock()
        guard let presenter = OxidMobilePlugin.topViewController() else {
            finish("unavailable")
            return Self.json(status: "unavailable")
        }
        let picker = UIDocumentPickerViewController(
            forOpeningContentTypes: [.data],
            asCopy: true
        )
        picker.allowsMultipleSelection = false
        picker.delegate = self
        presenter.present(picker, animated: true)
        return Self.json(status: "importing")
    }

    func take() -> String {
        lock.lock()
        defer { lock.unlock() }
        let result = Self.json(status: status, payload: payload)
        if status != "exporting" && status != "importing" {
            status = "idle"
            payload = nil
        }
        return result
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish("cancelled")
    }

    func documentPicker(
        _ controller: UIDocumentPickerViewController,
        didPickDocumentsAt urls: [URL]
    ) {
        lock.lock()
        let current = operation
        lock.unlock()
        switch current {
        case .export:
            finish(urls.count == 1 ? "exported" : "failed")
        case .importFile:
            guard urls.count == 1 else {
                finish("invalid")
                return
            }
            importDocument(urls[0])
        case .none:
            finish("failed")
        }
    }

    private func importDocument(_ url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let values = try url.resourceValues(forKeys: [
                .isRegularFileKey,
                .isSymbolicLinkKey,
                .fileSizeKey
            ])
            guard values.isRegularFile == true,
                  values.isSymbolicLink != true,
                  let fileSize = values.fileSize,
                  fileSize > 0,
                  fileSize <= maximumPackageBytes else {
                finish("invalid")
                return
            }
            let bytes = try Data(contentsOf: url, options: .uncached)
            guard !bytes.isEmpty, bytes.count <= maximumPackageBytes else {
                finish("invalid")
                return
            }
            finish("imported", payload: bytes.base64EncodedString())
        } catch {
            finish("failed")
        }
    }

    private func finish(_ next: String, payload nextPayload: String? = nil) {
        lock.lock()
        status = next
        payload = nextPayload
        operation = nil
        let directory = temporaryDirectory
        temporaryDirectory = nil
        lock.unlock()
        if let directory {
            try? FileManager.default.removeItem(at: directory)
        }
    }

    private static func json(status: String, payload: String? = nil) -> String {
        var body: [String: String] = ["status": status]
        if let payload { body["payload"] = payload }
        guard let data = try? JSONSerialization.data(withJSONObject: body),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"status\":\"failed\"}"
        }
        return text
    }
}

private final class CustodyCoordinator {
    static let shared = CustodyCoordinator()

    private struct Session {
        let context: LAContext
        let expiresAt: Date
    }

    private let lock = NSLock()
    private let service = "io.medianox.oxid.mobile-custody.v1"
    private let sessionDuration: TimeInterval = 30
    private let maximumPayloadBytes = 512 * 1024
    private var sessions: [String: Session] = [:]

    func dispatch(request: String) -> String {
        guard !request.isEmpty,
              request.utf8.count <= maximumPayloadBytes * 2,
              let data = request.data(using: .utf8),
              let body = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let operation = body["operation"] as? String,
              let profileId = body["profile_id"] as? String else {
            return json(status: "invalid")
        }
        let expected: Set<String>
        switch operation {
        case "initialize", "save":
            expected = ["operation", "profile_id", "payload"]
        case "unlock":
            expected = ["operation", "profile_id", "reason"]
        case "inspect", "load", "lock":
            expected = ["operation", "profile_id"]
        default:
            return json(status: "invalid")
        }
        guard Set(body.keys) == expected else { return json(status: "invalid") }
        switch operation {
        case "inspect":
            return inspect(profileId: profileId)
        case "initialize":
            guard let payload = body["payload"] as? String else { return json(status: "invalid") }
            return initialize(profileId: profileId, payload: payload)
        case "unlock":
            guard let reason = body["reason"] as? String else { return json(status: "invalid") }
            return unlock(profileId: profileId, reason: reason)
        case "load":
            return load(profileId: profileId)
        case "save":
            guard let payload = body["payload"] as? String else { return json(status: "invalid") }
            return save(profileId: profileId, payload: payload)
        case "lock":
            return lock(profileId: profileId)
        default:
            return json(status: "invalid")
        }
    }

    func inspect(profileId: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId) else { return json(status: "invalid") }
        let existence = itemExistence(profileId: profileId)
        guard existence != .unavailable else { return json(status: "unavailable") }
        guard existence == .present else { return json(status: "uninitialized") }
        if activeSession(profileId: profileId) != nil {
            return json(status: "unlocked", protection: "operating_system")
        }
        return json(status: "locked", protection: "operating_system")
    }

    func initialize(profileId: String, payload: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId), var plaintext = decodePayload(payload) else {
            return json(status: "invalid")
        }
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        switch itemExistence(profileId: profileId) {
        case .present:
            return json(status: "already_initialized")
        case .unavailable:
            return json(status: "unavailable")
        case .missing:
            break
        }

        let capability = LAContext()
        var capabilityError: NSError?
        guard capability.canEvaluatePolicy(.deviceOwnerAuthentication, error: &capabilityError) else {
            return json(status: "unavailable")
        }
        var accessError: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly,
            .userPresence,
            &accessError
        ) else {
            return json(status: "unavailable")
        }
        let add: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: profileId,
            kSecAttrAccessControl: access,
            kSecValueData: plaintext
        ]
        let added = SecItemAdd(add as CFDictionary, nil)
        guard added == errSecSuccess else {
            return json(status: added == errSecDuplicateItem ? "already_initialized" : "unavailable")
        }

        let context = LAContext()
        context.touchIDAuthenticationAllowableReuseDuration = 0
        let opened = read(
            profileId: profileId,
            context: context,
            prompt: "Protect this Oxid wallet",
            allowAuthenticationUI: true
        )
        switch opened {
        case .success:
            sessions[profileId] = Session(
                context: context,
                expiresAt: Date().addingTimeInterval(sessionDuration)
            )
            return json(status: "succeeded", protection: "operating_system")
        case .failure(let status):
            SecItemDelete(baseQuery(profileId: profileId) as CFDictionary)
            context.invalidate()
            return json(status: status)
        }
    }

    func unlock(profileId: String, reason: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId), validReason(reason) else { return json(status: "invalid") }
        switch itemExistence(profileId: profileId) {
        case .missing:
            return json(status: "not_initialized")
        case .unavailable:
            return json(status: "unavailable")
        case .present:
            break
        }
        sessions.removeValue(forKey: profileId)?.context.invalidate()
        let context = LAContext()
        context.touchIDAuthenticationAllowableReuseDuration = 0
        switch read(
            profileId: profileId,
            context: context,
            prompt: reason,
            allowAuthenticationUI: true
        ) {
        case .success(var plaintext):
            defer { plaintext.resetBytes(in: 0..<plaintext.count) }
            sessions[profileId] = Session(
                context: context,
                expiresAt: Date().addingTimeInterval(sessionDuration)
            )
            return json(
                status: "succeeded",
                protection: "operating_system",
                payload: plaintext.base64EncodedString()
            )
        case .failure(let status):
            context.invalidate()
            return json(status: status)
        }
    }

    func load(profileId: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId) else { return json(status: "invalid") }
        guard let session = activeSession(profileId: profileId) else {
            return itemExistence(profileId: profileId) == .missing
                ? json(status: "not_initialized")
                : json(status: "locked")
        }
        switch read(
            profileId: profileId,
            context: session.context,
            prompt: "",
            allowAuthenticationUI: false
        ) {
        case .success(var plaintext):
            defer { plaintext.resetBytes(in: 0..<plaintext.count) }
            return json(
                status: "succeeded",
                protection: "operating_system",
                payload: plaintext.base64EncodedString()
            )
        case .failure:
            sessions.removeValue(forKey: profileId)?.context.invalidate()
            return json(status: "locked")
        }
    }

    func save(profileId: String, payload: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId), var plaintext = decodePayload(payload) else {
            return json(status: "invalid")
        }
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        guard let session = activeSession(profileId: profileId) else {
            return json(status: "locked")
        }
        var query = baseQuery(profileId: profileId)
        query[kSecUseAuthenticationContext] = session.context
        query[kSecUseAuthenticationUI] = kSecUseAuthenticationUIFail
        let updated = SecItemUpdate(
            query as CFDictionary,
            [kSecValueData: plaintext] as CFDictionary
        )
        guard updated == errSecSuccess else {
            sessions.removeValue(forKey: profileId)?.context.invalidate()
            return json(status: updated == errSecItemNotFound ? "not_initialized" : "locked")
        }
        return json(status: "succeeded", protection: "operating_system")
    }

    func lock(profileId: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        guard validProfileId(profileId) else { return json(status: "invalid") }
        guard itemExistence(profileId: profileId) == .present else {
            return json(status: "not_initialized")
        }
        sessions.removeValue(forKey: profileId)?.context.invalidate()
        return json(status: "locked", protection: "operating_system")
    }

    private enum Existence { case missing, present, unavailable }

    private enum ReadResult {
        case success(Data)
        case failure(String)
    }

    private func baseQuery(profileId: String) -> [CFString: Any] {
        [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: profileId
        ]
    }

    private func itemExistence(profileId: String) -> Existence {
        var query = baseQuery(profileId: profileId)
        query[kSecMatchLimit] = kSecMatchLimitOne
        query[kSecReturnAttributes] = true
        query[kSecUseAuthenticationUI] = kSecUseAuthenticationUIFail
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess, errSecInteractionNotAllowed, errSecAuthFailed:
            return .present
        case errSecItemNotFound:
            return .missing
        default:
            return .unavailable
        }
    }

    private func read(
        profileId: String,
        context: LAContext,
        prompt: String,
        allowAuthenticationUI: Bool
    ) -> ReadResult {
        var query = baseQuery(profileId: profileId)
        query[kSecMatchLimit] = kSecMatchLimitOne
        query[kSecReturnData] = true
        query[kSecUseAuthenticationContext] = context
        query[kSecUseAuthenticationUI] = allowAuthenticationUI
            ? kSecUseAuthenticationUIAllow
            : kSecUseAuthenticationUIFail
        if allowAuthenticationUI { query[kSecUseOperationPrompt] = prompt }
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess, let data = result as? Data else {
            let safeStatus: String
            switch status {
            case errSecItemNotFound:
                safeStatus = "not_initialized"
            case errSecUserCanceled, errSecAuthFailed:
                safeStatus = "authorization_denied"
            case errSecInteractionNotAllowed:
                safeStatus = "locked"
            default:
                safeStatus = "unavailable"
            }
            return .failure(safeStatus)
        }
        guard !data.isEmpty, data.count <= maximumPayloadBytes else {
            return .failure("invalid")
        }
        return .success(data)
    }

    private func activeSession(profileId: String) -> Session? {
        guard let session = sessions[profileId] else { return nil }
        guard Date() < session.expiresAt else {
            sessions.removeValue(forKey: profileId)?.context.invalidate()
            return nil
        }
        return session
    }

    private func decodePayload(_ payload: String) -> Data? {
        guard !payload.isEmpty,
              payload.utf8.count <= maximumPayloadBytes * 2,
              let data = Data(base64Encoded: payload),
              !data.isEmpty,
              data.count <= maximumPayloadBytes else {
            return nil
        }
        return data
    }

    private func validProfileId(_ value: String) -> Bool {
        guard !value.isEmpty, value.count <= 128 else { return false }
        return !value.unicodeScalars.contains {
            CharacterSet.whitespacesAndNewlines.contains($0)
                || CharacterSet.controlCharacters.contains($0)
        }
    }

    private func validReason(_ value: String) -> Bool {
        !value.isEmpty && value.count <= 160 && !value.unicodeScalars.contains {
            CharacterSet.controlCharacters.contains($0)
        }
    }

    private func json(
        status: String,
        protection: String? = nil,
        payload: String? = nil
    ) -> String {
        var body = ["status": status]
        if let protection { body["protection"] = protection }
        if let payload { body["payload"] = payload }
        guard let data = try? JSONSerialization.data(withJSONObject: body, options: [.sortedKeys]),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"status\":\"invalid\"}"
        }
        return text
    }
}

private final class ScanCoordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate {
    static let shared = ScanCoordinator()

    private let lock = NSLock()
    private let maximumPayloadBytes = 32 * 1024
    private var status = "idle"
    private var payload: String?
    private var generation: UInt64 = 0
    private var session: AVCaptureSession?
    private var metadataOutput: AVCaptureMetadataOutput?
    private weak var controller: UIViewController?

    func start() -> String {
#if targetEnvironment(simulator)
        return Self.json(status: "unavailable")
#else
        lock.lock()
        guard status != "scanning" else {
            lock.unlock()
            return Self.json(status: "failed")
        }
        generation &+= 1
        let activeGeneration = generation
        status = "scanning"
        payload = nil
        lock.unlock()

        DispatchQueue.main.async {
            [weak self] in self?.requestCameraAndPresent(activeGeneration)
        }
        return Self.json(status: "scanning")
#endif
    }

    func take() -> String {
        lock.lock()
        defer { lock.unlock() }
        let result = Self.json(status: status, payload: payload)
        if status != "scanning" {
            status = "idle"
            payload = nil
        }
        return result
    }

    func timeout() -> String {
        lock.lock()
        if status != "scanning" {
            let result = Self.json(status: status, payload: payload)
            status = "idle"
            payload = nil
            lock.unlock()
            return result
        }
        generation &+= 1
        status = "timed_out"
        payload = nil
        let capture = session
        let presented = controller
        session = nil
        metadataOutput = nil
        controller = nil
        lock.unlock()

        DispatchQueue.main.async {
            capture?.stopRunning()
            presented?.dismiss(animated: true)
        }
        return Self.json(status: "timed_out")
    }

    private func requestCameraAndPresent(_ activeGeneration: UInt64) {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            presentScanner(activeGeneration)
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    if granted {
                        self?.presentScanner(activeGeneration)
                    } else {
                        self?.finish("denied", generation: activeGeneration)
                    }
                }
            }
        case .denied, .restricted:
            finish("denied", generation: activeGeneration)
        @unknown default:
            finish("unavailable", generation: activeGeneration)
        }
    }

    private func presentScanner(_ activeGeneration: UInt64) {
        lock.lock()
        let isActive = status == "scanning" && generation == activeGeneration
        lock.unlock()
        guard isActive else { return }

        guard let camera = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: camera) else {
            finish("unavailable", generation: activeGeneration)
            return
        }
        let capture = AVCaptureSession()
        guard capture.canAddInput(input) else {
            finish("failed", generation: activeGeneration)
            return
        }
        capture.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard capture.canAddOutput(output) else {
            finish("failed", generation: activeGeneration)
            return
        }
        capture.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        guard let presenter = OxidMobilePlugin.topViewController() else {
            finish("failed", generation: activeGeneration)
            return
        }
        let scanner = ScannerViewController(session: capture) { [weak self] in
            self?.finish("cancelled", generation: activeGeneration)
        }

        lock.lock()
        guard status == "scanning" && generation == activeGeneration else {
            lock.unlock()
            return
        }
        session = capture
        metadataOutput = output
        controller = scanner
        lock.unlock()
        presenter.present(scanner, animated: true) { [weak self] in
            guard let self else { return }
            self.lock.lock()
            let shouldStart = self.status == "scanning"
                && self.generation == activeGeneration
                && self.session === capture
            self.lock.unlock()
            if shouldStart { capture.startRunning() }
        }
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        lock.lock()
        let activeGeneration = generation
        let isActive = status == "scanning" && metadataOutput === output
        lock.unlock()
        guard isActive else { return }

        guard let code = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
              code.type == .qr,
              let value = code.stringValue,
              !value.isEmpty,
              value.utf8.count <= maximumPayloadBytes else {
            finish("invalid", generation: activeGeneration)
            return
        }
        finish("succeeded", payload: value, generation: activeGeneration)
    }

    private func finish(
        _ next: String,
        payload value: String? = nil,
        generation activeGeneration: UInt64
    ) {
        lock.lock()
        guard status == "scanning" && generation == activeGeneration else {
            lock.unlock()
            return
        }
        status = next
        payload = value
        let capture = session
        let presented = controller
        session = nil
        metadataOutput = nil
        controller = nil
        lock.unlock()

        DispatchQueue.main.async {
            capture?.stopRunning()
            presented?.dismiss(animated: true)
        }
    }

    private static func json(status: String, payload: String? = nil) -> String {
        var body: [String: String] = ["status": status]
        if let payload { body["payload"] = payload }
        guard let data = try? JSONSerialization.data(withJSONObject: body),
              let text = String(data: data, encoding: .utf8) else {
            return "{\"status\":\"failed\"}"
        }
        return text
    }
}

private final class ScannerViewController: UIViewController {
    private let session: AVCaptureSession
    private let onCancel: () -> Void

    init(session: AVCaptureSession, onCancel: @escaping () -> Void) {
        self.session = session
        self.onCancel = onCancel
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.videoGravity = .resizeAspectFill
        preview.frame = view.bounds
        view.layer.addSublayer(preview)

        let cancel = UIButton(type: .system)
        cancel.setTitle("Cancel", for: .normal)
        cancel.setTitleColor(.white, for: .normal)
        cancel.titleLabel?.font = .preferredFont(forTextStyle: .headline)
        cancel.addTarget(self, action: #selector(cancelScan), for: .touchUpInside)
        cancel.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(cancel)
        NSLayoutConstraint.activate([
            cancel.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
            cancel.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20)
        ])
    }

    @objc private func cancelScan() { onCancel() }
}
