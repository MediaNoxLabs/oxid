// SPDX-License-Identifier: Apache-2.0

import XCTest

final class NativeCustodyTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testNativeCompositionUsesDeviceCustodyOrFailsClosed() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()
        let createWallet = application.buttons["Create new wallet"]
        if createWallet.waitForExistence(timeout: 5) {
            XCTAssertTrue(
                application.buttons["Restore from backup"].exists,
                "a fresh installation must expose complete-wallet recovery before profile creation"
            )
            createWallet.tap()
            application.buttons["Create and continue"].tap()
            XCTAssertTrue(application.buttons["Skip for now"].waitForExistence(timeout: 10))
            application.buttons["Skip for now"].tap()
        }

        let profileMenu = application.buttons["Open profile menu"]
        XCTAssertTrue(profileMenu.waitForExistence(timeout: 15))
        profileMenu.tap()
        let settings = application.buttons["Open settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 15))
        settings.tap()
        let uninitialized = application.staticTexts["Uninitialized · Operating system"]
        let unavailable = application.staticTexts["Unavailable · Not connected"]
        let capabilityPresent = uninitialized.waitForExistence(timeout: 10)
        XCTAssertTrue(capabilityPresent || unavailable.exists)
        XCTAssertTrue(
            application.staticTexts["One encrypted wallet document"].exists,
            "settings must expose the complete-wallet export surface"
        )

        if !capabilityPresent {
            XCTAssertFalse(
                application.buttons["Use my receive address"].exists,
                "unavailable native custody must not release protected wallet material"
            )
            return
        }

        application.buttons["Wallet"].tap()
        let activate = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(activate.waitForExistence(timeout: 10))
        activate.tap()

        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let cancel = springboard.buttons["Cancel"]
        let promptObserved = cancel.waitForExistence(timeout: 5)
        if promptObserved { cancel.tap() }

        let derived = application.buttons["Use my receive address"]
        let failedClosed = application.staticTexts["wallet authorization was denied"]
            .waitForExistence(timeout: 10)
            || application.staticTexts["wallet protection is unavailable"]
                .waitForExistence(timeout: 1)
        XCTAssertTrue(promptObserved || failedClosed)
        XCTAssertFalse(derived.exists, "cancelling or lacking user presence must not release custody")
    }
}
