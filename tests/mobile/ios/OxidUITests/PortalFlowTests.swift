// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

private final class PortalControlResult: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Result<Data, Error>?

    func store(_ result: Result<Data, Error>) {
        lock.lock()
        value = result
        lock.unlock()
    }

    func load() -> Result<Data, Error>? {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

final class PortalFlowTests: XCTestCase {
    private var acceptExternalLinks = false

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        let safeBottom = application.frame.maxY - 90
        for _ in 0..<20 where !element.isHittable || element.frame.maxY > safeBottom {
            application.swipeUp()
        }
        XCTAssertTrue(element.isHittable)
        XCTAssertLessThanOrEqual(element.frame.maxY, safeBottom)
    }

    private let controlOrigin = "http://127.0.0.1:18091"

    private func control(
        _ route: String,
        method: String = "GET",
        body: Data? = nil,
        timeout: TimeInterval = 35
    ) throws -> Data {
        guard let url = URL(string: controlOrigin + route) else {
            XCTFail("Portal control route is invalid")
            throw NSError(domain: "PortalControl", code: 2)
        }
        var request = URLRequest(url: url, timeoutInterval: timeout)
        request.httpMethod = method
        request.httpBody = body
        request.cachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        let completed = expectation(description: "Portal control \(route)")
        let result = PortalControlResult()
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error {
                result.store(.failure(error))
            } else if let response = response as? HTTPURLResponse,
                      (200..<300).contains(response.statusCode), let data {
                result.store(.success(data))
            } else {
                result.store(.failure(NSError(domain: "PortalControl", code: 1)))
            }
            completed.fulfill()
        }.resume()
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: timeout + 2), .completed)
        return try XCTUnwrap(result.load()).get()
    }

    @MainActor
    private func deliver(_ kind: String, in application: XCUIApplication) throws {
        acceptExternalLinks = true
        _ = try control("/deliver-ios", method: "POST", body: Data(kind.utf8))
        // A host-driven custom scheme can present SpringBoard's one-time
        // confirmation. This payload-free tap invokes the registered monitor.
        application.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08)).tap()
    }

    private func setProxyMode(_ mode: String) throws {
        _ = try control("/proxy-mode", method: "POST", body: Data(mode.utf8))
    }

    private func counters() throws -> [String: Int] {
        let value = try JSONSerialization.jsonObject(with: control("/counters"))
        return try XCTUnwrap(value as? [String: Int])
    }

    private func holderGeneration() throws -> Int {
        let value = try JSONSerialization.jsonObject(with: control("/holder-generation"))
        return try XCTUnwrap((value as? [String: Int])?["generation"])
    }

    private func waitForHolderGeneration(after prior: Int = 0) throws {
        let deadline = Date().addingTimeInterval(20)
        while Date() < deadline {
            if try holderGeneration() > prior { return }
            Thread.sleep(forTimeInterval: 0.2)
        }
        XCTFail("The public managed DID did not reach the test resolver")
    }

    @MainActor
    private func ensureProfileAndManagedDid(in application: XCUIApplication) throws {
        application.launch()
        let createWallet = application.buttons["Create new wallet"]
        if createWallet.waitForExistence(timeout: 5) {
            createWallet.tap()
            let createAndContinue = application.buttons["Create and continue"]
            if !createAndContinue.waitForExistence(timeout: 3) {
                // A stale host-link confirmation from an interrupted prior run
                // can consume the first synthesized tap. The monitor has now
                // dismissed it, so retry only the bounded onboarding action.
                createWallet.tap()
            }
            XCTAssertTrue(createAndContinue.waitForExistence(timeout: 5))
            createAndContinue.tap()
            XCTAssertTrue(application.buttons["Skip for now"].waitForExistence(timeout: 10))
            application.buttons["Skip for now"].tap()
        }
        XCTAssertTrue(application.buttons["Wallet"].waitForExistence(timeout: 20))
        application.buttons["Wallet"].tap()
        let activate = application.buttons["Activate protected Midnight account"]
        if activate.waitForExistence(timeout: 5) {
            activate.tap()
            XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 30))
        }
        application.buttons["Documents"].tap()
        let manage = application.buttons["Manage identities"]
        XCTAssertTrue(manage.waitForExistence(timeout: 10))
        manage.tap()
        let createDid = application.buttons["Create standalone DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 10))
        createDid.tap()
        XCTAssertTrue(application.descendants(matching: .any)["Manage this DID"].waitForExistence(timeout: 15))
        try waitForHolderGeneration()
    }

    @MainActor
    private func assertRoutedOffer(in application: XCUIApplication) {
        XCTAssertTrue(application.staticTexts[
            "App link recognized as a credential offer. Review the request before consent."
        ].waitForExistence(timeout: 15))
        XCTAssertTrue(application.staticTexts["Credentials"].exists)
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertTrue(application.descendants(matching: .any)[
            "Imported credential offer retained privately"
        ].exists)
        XCTAssertFalse(application.descendants(matching: .any)["Credential offer URI"].exists)
        XCTAssertFalse(application.descendants(matching: .any)["Consent to credential issuance"].exists)
    }

    @MainActor
    private func previewImportedOffer(in application: XCUIApplication) {
        let preview = application.buttons["Preview credential offer"]
        XCTAssertTrue(preview.waitForExistence(timeout: 10))
        scrollTo(preview, in: application)
        preview.tap()
    }

    @MainActor
    func testRealPortalOfferUsesStrictWarmColdConsentAndRestoresEncryptedCredential() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        addUIInterruptionMonitor(withDescription: "External Oxid link") { [weak self] alert in
            let label = self?.acceptExternalLinks == true ? "Open" : "Cancel"
            let button = alert.buttons[label]
            guard button.exists else { return false }
            button.tap()
            return true
        }
        try ensureProfileAndManagedDid(in: application)

        // Warm OS delivery reaches the one-item router but never auto-previews or consents.
        try deliver("real", in: application)
        assertRoutedOffer(in: application)
        previewImportedOffer(in: application)
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 20))
        for heading in [
            "Who is issuing it?", "What will you receive?",
            "Which identity receives it?", "Why add it?", "Unverified endpoint",
        ] {
            XCTAssertTrue(application.staticTexts[heading].exists)
        }
        XCTAssertEqual(try counters()["token"], 0)
        XCTAssertEqual(try counters()["nonce"], 0)
        XCTAssertEqual(try counters()["credential"], 0)
        let refuse = application.buttons["Refuse offer"]
        scrollTo(refuse, in: application)
        refuse.tap()
        XCTAssertTrue(application.staticTexts[
            "Credential offer refused; ephemeral protocol secrets were discarded."
        ].waitForExistence(timeout: 10))
        XCTAssertEqual(try counters()["token"], 0)
        application.buttons["Dismiss identity request"].tap()

        // Adapter transport failures remain payload-free and fail closed in the mobile framework.
        try setProxyMode("unavailable")
        try deliver("real", in: application)
        assertRoutedOffer(in: application)
        previewImportedOffer(in: application)
        XCTAssertTrue(application.staticTexts[
            "This protocol is unavailable in the current build"
        ].waitForExistence(timeout: 10))
        try setProxyMode("normal")
        application.buttons["Dismiss identity request"].tap()

        try setProxyMode("timeout")
        try deliver("real", in: application)
        assertRoutedOffer(in: application)
        previewImportedOffer(in: application)
        XCTAssertTrue(application.staticTexts[
            "This protocol is unavailable in the current build"
        ].waitForExistence(timeout: 30))
        try setProxyMode("normal")
        application.buttons["Dismiss identity request"].tap()

        // The unchanged explicit consent path selects managed authentication and a distinct Jubjub assertion method.
        try deliver("real", in: application)
        assertRoutedOffer(in: application)
        previewImportedOffer(in: application)
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 20))
        let consent = application.descendants(matching: .any)["Consent to credential issuance"]
        scrollTo(consent, in: application)
        consent.tap()
        let issue = application.buttons["Accept and issue credential"]
        scrollTo(issue, in: application)
        issue.tap()
        XCTAssertTrue(application.staticTexts[
            "Credential issued, verified, and stored in the protected inventory."
        ].waitForExistence(timeout: 90))
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].waitForExistence(timeout: 20))
        XCTAssertEqual(
            application.staticTexts.matching(NSPredicate(format: "label == %@", "Valid")).count,
            1
        )
        XCTAssertFalse(application.staticTexts["John"].exists)
        XCTAssertFalse(application.staticTexts["Doe"].exists)
        XCTAssertEqual(try counters()["token"], 1)
        XCTAssertEqual(try counters()["nonce"], 1)
        XCTAssertEqual(try counters()["credential"], 1)

        // Cold OS delivery is still routed without consent. The consumed offer is not executed.
        try deliver("real-cold", in: application)
        assertRoutedOffer(in: application)
        application.buttons["Dismiss identity request"].tap()

        // Development custody truthfully resets; reactivation does not affect encrypted credential restore/reverification.
        application.buttons["Wallet"].tap()
        let reactivate = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(reactivate.waitForExistence(timeout: 15))
        reactivate.tap()
        XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 30))
        application.buttons["Documents"].tap()
        XCTAssertEqual(
            application.staticTexts.matching(NSPredicate(format: "label == %@", "Valid")).count,
            1
        )
        // The restored credential already shows this exact policy summary
        // before the tap, so that text alone cannot prove reverification ran.
        // Require the issuer-resolver success count to strictly increase and
        // the button to pass through its busy state and back.
        let issuerResolutionSuccessBeforeReverify = try counters()["issuerResolutionSuccess"] ?? 0
        let reverify = application.buttons["Reverify"]
        XCTAssertTrue(reverify.waitForExistence(timeout: 15))
        scrollTo(reverify, in: application)
        reverify.tap()
        XCTAssertTrue(application.buttons["Verifying…"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.buttons["Reverify"].waitForExistence(timeout: 30))
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].waitForExistence(timeout: 30))
        let issuerResolutionSuccessAfterReverify = try counters()["issuerResolutionSuccess"] ?? 0
        XCTAssertGreaterThan(issuerResolutionSuccessAfterReverify, issuerResolutionSuccessBeforeReverify)

        application.buttons["Scan identity QR code"].tap()
        XCTAssertTrue(application.staticTexts[
            "Camera scanning is unavailable here. Paste or load the request in the identity page instead."
        ].waitForExistence(timeout: 10))
    }
}
