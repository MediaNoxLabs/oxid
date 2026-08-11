// SPDX-License-Identifier: Apache-2.0

import XCTest

final class ProfileFlowTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testCreatesAndRestoresActiveProfile() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createButton = application.buttons["Create and continue"]
        XCTAssertTrue(createButton.waitForExistence(timeout: 15))
        createButton.tap()

        application.terminate()
        application.launch()

        let unavailableAccount = application.buttons["Midnight account unavailable"]
        XCTAssertTrue(unavailableAccount.waitForExistence(timeout: 15))
        XCTAssertFalse(unavailableAccount.isEnabled)
        XCTAssertTrue(application.buttons["Assets"].exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }
}
