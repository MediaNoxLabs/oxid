// SPDX-License-Identifier: Apache-2.0

import XCTest

final class ProfileFlowTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        for _ in 0..<10 where !element.isHittable {
            application.swipeUp()
        }
        XCTAssertTrue(element.isHittable)
    }

    @MainActor
    func testCreatesProfileAndCompletesStandaloneWalletFlow() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createButton = application.buttons["Create and continue"]
        XCTAssertTrue(createButton.waitForExistence(timeout: 15))
        createButton.tap()

        let activateButton = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(activateButton.waitForExistence(timeout: 15))
        activateButton.tap()

        let useReceiveAddress = application.buttons["Use my receive address"]
        XCTAssertTrue(useReceiveAddress.waitForExistence(timeout: 15))

        let syncDust = application.buttons["Sync DUST"]
        XCTAssertTrue(syncDust.waitForExistence(timeout: 5))
        scrollTo(syncDust, in: application)
        syncDust.tap()
        XCTAssertTrue(application.staticTexts["12 DUST"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.buttons["Resync DUST"].exists)

        let showQrButton = application.buttons["Show receive QR"].firstMatch
        XCTAssertTrue(showQrButton.exists)
        scrollTo(showQrButton, in: application)
        showQrButton.tap()
        XCTAssertTrue(
            application.images["QR code for Unshielded receive address"]
                .waitForExistence(timeout: 5)
        )
        application.buttons["Hide receive QR"].firstMatch.tap()

        let showShieldedQrButton = application.buttons
            .matching(identifier: "Show receive QR")
            .element(boundBy: 1)
        XCTAssertTrue(showShieldedQrButton.exists)
        scrollTo(showShieldedQrButton, in: application)
        showShieldedQrButton.tap()
        XCTAssertTrue(
            application.images["QR code for Shielded receive address"]
                .waitForExistence(timeout: 5)
        )
        application.buttons["Hide receive QR"].firstMatch.tap()

        scrollTo(useReceiveAddress, in: application)
        useReceiveAddress.tap()
        let amount = application.textFields["Amount in NIGHT"]
        XCTAssertTrue(amount.exists)
        scrollTo(amount, in: application)
        amount.tap()
        amount.typeText("1.5")
        let review = application.buttons["Review transfer"]
        scrollTo(review, in: application)
        review.tap()

        let authorize = application.buttons["Authorize reviewed NIGHT transfer"]
        XCTAssertTrue(authorize.waitForExistence(timeout: 10))
        scrollTo(authorize, in: application)
        authorize.tap()
        let submit = application.buttons["Prove and submit NIGHT transfer"]
        XCTAssertTrue(submit.waitForExistence(timeout: 10))
        scrollTo(submit, in: application)
        submit.tap()
        XCTAssertTrue(application.staticTexts["Transfer submitted"].waitForExistence(timeout: 15))

        application.terminate()
        application.launch()

        XCTAssertTrue(activateButton.waitForExistence(timeout: 15))
        XCTAssertTrue(application.buttons["Assets"].exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }
}
