// SPDX-License-Identifier: Apache-2.0

import XCTest

final class DemoProfileTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func staticText(_ label: String, in application: XCUIApplication) -> XCUIElement {
        application.staticTexts.matching(
            NSPredicate(format: "label ==[c] %@", label)
        ).firstMatch
    }

    @MainActor
    func testFullSetupStopsAtExistingCredentialOfferReview() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        XCTAssertTrue(staticText("Standalone demo", in: application).waitForExistence(timeout: 15))
        XCTAssertTrue(application.buttons["Create new wallet"].waitForExistence(timeout: 15))
        XCTAssertTrue(application.buttons["Open standalone demo setup"].exists)
        application.buttons["Open standalone demo setup"].tap()

        let drawer = application.otherElements["Standalone demo bootstrap"]
        XCTAssertTrue(drawer.waitForExistence(timeout: 10))
        XCTAssertTrue(application.buttons["Close standalone demo setup"].exists)
        XCTAssertTrue(application.buttons["Run full demo setup"].exists)
        application.buttons["Run full demo setup"].tap()

        XCTAssertTrue(
            staticText("Accept a credential offer", in: application).waitForExistence(timeout: 60)
        )
        XCTAssertTrue(application.buttons["Preview credential offer"].exists)
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertFalse(application.switches["Consent to credential issuance"].exists)
        XCTAssertFalse(application.buttons["Accept and issue credential"].exists)

        application.buttons["Open standalone demo setup"].tap()
        XCTAssertTrue(
            staticText(
                "Safe setup complete. The credential offer is waiting on its existing review screen.",
                in: application
            ).waitForExistence(timeout: 10)
        )
        XCTAssertTrue(
            staticText(
                "Loaded the deterministic public 5 NIGHT funding snapshot; no chain was contacted.",
                in: application
            ).exists
        )
        XCTAssertTrue(application.buttons["Close standalone demo setup"].exists)
    }
}
