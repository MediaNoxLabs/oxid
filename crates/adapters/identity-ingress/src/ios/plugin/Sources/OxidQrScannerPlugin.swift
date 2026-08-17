// SPDX-License-Identifier: Apache-2.0

import AVFoundation
import Foundation
import UIKit

@objc(OxidQrScannerPlugin)
public final class OxidQrScannerPlugin: NSObject {
    @objc public func startScanJson() -> String {
        ScanCoordinator.shared.start()
    }

    @objc public func takeScanResultJson() -> String {
        ScanCoordinator.shared.take()
    }
}

private final class ScanCoordinator: NSObject, AVCaptureMetadataOutputObjectsDelegate {
    static let shared = ScanCoordinator()

    private let lock = NSLock()
    private var status = "idle"
    private var payload: String?
    private var session: AVCaptureSession?
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
        status = "scanning"
        payload = nil
        lock.unlock()

        DispatchQueue.main.async { [weak self] in self?.requestCameraAndPresent() }
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

    private func requestCameraAndPresent() {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized:
            presentScanner()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { [weak self] granted in
                DispatchQueue.main.async {
                    if granted { self?.presentScanner() } else { self?.finish("unavailable") }
                }
            }
        case .denied, .restricted:
            finish("unavailable")
        @unknown default:
            finish("unavailable")
        }
    }

    private func presentScanner() {
        guard let camera = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: camera) else {
            finish("unavailable")
            return
        }

        let capture = AVCaptureSession()
        guard capture.canAddInput(input) else {
            finish("failed")
            return
        }
        capture.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard capture.canAddOutput(output) else {
            finish("failed")
            return
        }
        capture.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        guard let presenter = Self.topViewController() else {
            finish("failed")
            return
        }
        let scanner = ScannerViewController(session: capture) { [weak self] in
            self?.finish("cancelled")
        }
        session = capture
        controller = scanner
        presenter.present(scanner, animated: true) { capture.startRunning() }
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        guard let code = metadataObjects.first as? AVMetadataMachineReadableCodeObject,
              code.type == .qr,
              let value = code.stringValue else {
            return
        }
        finish("succeeded", payload: value)
    }

    private func finish(_ next: String, payload value: String? = nil) {
        lock.lock()
        guard status == "scanning" else {
            lock.unlock()
            return
        }
        status = next
        payload = value
        let capture = session
        let presented = controller
        session = nil
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

    private static func topViewController() -> UIViewController? {
        let root = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
            .first(where: { $0.isKeyWindow })?.rootViewController
        var current = root
        while let presented = current?.presentedViewController { current = presented }
        return current
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
