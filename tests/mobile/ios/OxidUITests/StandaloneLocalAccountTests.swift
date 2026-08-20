// SPDX-License-Identifier: Apache-2.0

import XCTest

final class StandaloneLocalAccountTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testSynchronizesProtectedAccountFromLocalStandaloneStack() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createWallet = application.buttons["Create new wallet"]
        XCTAssertTrue(createWallet.waitForExistence(timeout: 15))
        createWallet.tap()
        application.buttons["Create and continue"].tap()
        XCTAssertTrue(application.buttons["Skip for now"].waitForExistence(timeout: 10))
        application.buttons["Skip for now"].tap()

        XCTAssertTrue(application.buttons["Wallet"].waitForExistence(timeout: 15))
        application.buttons["Wallet"].tap()
        let activate = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(activate.waitForExistence(timeout: 15))
        activate.tap()

        XCTAssertTrue(
            application.buttons["Use my receive address"].waitForExistence(timeout: 90)
        )
        XCTAssertTrue(application.staticTexts["Live"].waitForExistence(timeout: 10))
        XCTAssertTrue(
            application.staticTexts.matching(
                NSPredicate(format: "label CONTAINS %@", "Synced · Live source")
            ).firstMatch.waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.buttons["Copy Unshielded receive address"].exists)
        XCTAssertTrue(application.buttons["Copy Shielded receive address"].exists)

        XCTAssertFalse(
            application.staticTexts["Simulated — runs locally, nothing on Midnight"].exists
        )
        XCTAssertFalse(application.staticTexts["12 DUST"].exists)
        XCTAssertFalse(application.staticTexts["1 shielded notes"].exists)
        XCTAssertFalse(application.staticTexts["5 NIGHT"].exists)
        XCTAssertFalse(
            application.staticTexts["Account state could not be loaded safely."].exists
        )
    }
}
