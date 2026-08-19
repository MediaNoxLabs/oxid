// SPDX-License-Identifier: Apache-2.0

import XCTest

final class DeveloperProfileTests: XCTestCase {
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
    func testBannerAndSharedCapabilityManifestRemainVisible() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        XCTAssertTrue(
            staticText("Developer profile", in: application).waitForExistence(timeout: 15)
        )
        XCTAssertTrue(application.buttons["Create new wallet"].waitForExistence(timeout: 15))
        application.buttons["Create new wallet"].tap()
        application.buttons["Create and continue"].tap()
        XCTAssertTrue(application.buttons["Skip for now"].waitForExistence(timeout: 10))
        application.buttons["Skip for now"].tap()

        XCTAssertTrue(application.buttons["Open profile menu"].waitForExistence(timeout: 15))
        application.buttons["Open profile menu"].tap()
        XCTAssertTrue(
            application.buttons["Open developer capabilities"].waitForExistence(timeout: 10)
        )
        application.buttons["Open developer capabilities"].tap()

        XCTAssertTrue(staticText("Capability manifest", in: application).waitForExistence(timeout: 10))
        XCTAssertTrue(staticText("Developer profile", in: application).exists)
        XCTAssertTrue(staticText("wallet.key.sign", in: application).waitForExistence(timeout: 10))
        XCTAssertTrue(staticText("confirmationRequired", in: application).exists)
        XCTAssertTrue(staticText("oxid_capabilities_application", in: application).exists)
        XCTAssertFalse(application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "credential_offer")
        ).firstMatch.exists)
    }
}
