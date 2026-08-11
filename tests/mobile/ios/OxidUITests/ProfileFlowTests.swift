// SPDX-License-Identifier: Apache-2.0

import XCTest

final class ProfileFlowTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testCreatesAndRestoresActiveProfile() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createButton = application.buttons["Create and continue"]
        XCTAssertTrue(createButton.waitForExistence(timeout: 15))
        createButton.tap()

        XCTAssertTrue(application.staticTexts["My wallet is active"].waitForExistence(timeout: 15))
        XCTAssertTrue(application.staticTexts["Assets"].exists)

        application.terminate()
        application.launch()

        XCTAssertTrue(application.staticTexts["My wallet is active"].waitForExistence(timeout: 15))
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }
}
