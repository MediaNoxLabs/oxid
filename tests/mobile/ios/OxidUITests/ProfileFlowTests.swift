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

        let syncShielded = application.buttons["Sync shielded assets"]
        XCTAssertTrue(syncShielded.waitForExistence(timeout: 5))
        scrollTo(syncShielded, in: application)
        syncShielded.tap()
        XCTAssertTrue(application.staticTexts["1 shielded notes"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.staticTexts["5000000 atomic units"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.buttons["Resync shielded assets"].exists)

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
        let cancelSubmission = application.buttons["Cancel NIGHT transfer submission"]
        XCTAssertTrue(cancelSubmission.waitForExistence(timeout: 5))
        cancelSubmission.tap()
        let retrySubmission = application.buttons["Retry safe submission"]
        XCTAssertTrue(retrySubmission.waitForExistence(timeout: 5))
        retrySubmission.tap()
        XCTAssertTrue(submit.waitForExistence(timeout: 5))
        submit.tap()
        XCTAssertTrue(application.staticTexts["Transfer submitted"].waitForExistence(timeout: 15))

        let dids = application.buttons["DIDs"]
        XCTAssertTrue(dids.waitForExistence(timeout: 5))
        dids.tap()
        let createDid = application.buttons["Create standalone DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 5))
        createDid.tap()
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
        XCTAssertTrue(
            application.descendants(matching: .any)["Manage this DID"]
                .waitForExistence(timeout: 5)
        )
        let resolveDid = application.buttons["Resolve and save"]
        XCTAssertTrue(resolveDid.waitForExistence(timeout: 5))
        scrollTo(resolveDid, in: application)
        resolveDid.tap()
        XCTAssertTrue(application.staticTexts["standalone-fixture-v2"].waitForExistence(timeout: 10))

        let credentials = application.buttons["Credentials"]
        XCTAssertTrue(credentials.waitForExistence(timeout: 5))
        credentials.tap()
        let receiveCredential = application.buttons["Receive standalone credential"]
        XCTAssertTrue(receiveCredential.waitForExistence(timeout: 5))
        receiveCredential.tap()
        XCTAssertTrue(application.staticTexts["Identity credential"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["valid"].waitForExistence(timeout: 10))
        let reverify = application.buttons["Reverify"]
        scrollTo(reverify, in: application)
        reverify.tap()
        XCTAssertTrue(reverify.waitForExistence(timeout: 10))

        application.terminate()
        application.launch()

        XCTAssertTrue(activateButton.waitForExistence(timeout: 15))
        XCTAssertTrue(application.staticTexts["Transfer included"].waitForExistence(timeout: 15))
        dids.tap()
        XCTAssertTrue(application.staticTexts["standalone-fixture-v2"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
        credentials.tap()
        XCTAssertTrue(application.staticTexts["Identity credential"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.buttons["Reverify"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.buttons["Assets"].exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }
}
