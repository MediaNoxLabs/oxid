// SPDX-License-Identifier: Apache-2.0

import XCTest

final class IdentityIngressTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func ensureProfile(in application: XCUIApplication) {
        application.launch()
        let createWallet = application.buttons["Create new wallet"]
        if createWallet.waitForExistence(timeout: 5) {
            createWallet.tap()
            application.buttons["Create and continue"].tap()
            XCTAssertTrue(application.buttons["Skip for now"].waitForExistence(timeout: 10))
            application.buttons["Skip for now"].tap()
        }
        XCTAssertTrue(application.buttons["Scan"].waitForExistence(timeout: 15))
    }

    @MainActor
    func testSimulatorScannerIsUnavailableAndImportsNothing() {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)

        application.buttons["Scan"].tap()
        XCTAssertTrue(
            application.staticTexts[
                "Camera scanning is unavailable here. Paste or load the request in the identity page instead."
            ].waitForExistence(timeout: 5)
        )
        XCTAssertFalse(
            application.staticTexts[
                "QR recognized as a credential offer. Review the request before consent."
            ].exists
        )
        XCTAssertFalse(application.buttons["Accept offer and issue credential"].exists)
    }

    @MainActor
    func testCustomSchemesRouteWarmAndColdWithoutConsent() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)

        let offer = try XCTUnwrap(URL(
            string: "openid-credential-offer://?credential_offer=%7B%7D"
        ))
        application.open(offer)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a credential offer. Review the request before consent."
            ].waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertFalse(application.buttons["Accept offer and issue credential"].exists)
        application.buttons["Dismiss identity request"].tap()

        let login = try XCTUnwrap(URL(string:
            "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier%2Frequest"
        ))
        application.open(login)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a DID login request. Review the request before consent."
            ].waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertFalse(application.buttons["Accept login"].exists)
        application.buttons["Dismiss identity request"].tap()

        application.terminate()
        application.open(offer)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a credential offer. Review the request before consent."
            ].waitForExistence(timeout: 15)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
    }
}
