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
        let createWallet = application.buttons["Create private wallet"]
        XCTAssertTrue(
            createWallet.waitForExistence(timeout: 15),
            "the reset native composition must begin at private-wallet onboarding"
        )
        XCTAssertTrue(
            application.buttons["Restore from backup"].exists,
            "a fresh installation must expose complete-wallet recovery before profile creation"
        )
        createWallet.tap()
        let createAndContinue = application.buttons["Create and continue"]
        XCTAssertTrue(createAndContinue.waitForExistence(timeout: 10))
        createAndContinue.tap()
        let generateRecoveryPhrase = application.buttons["Generate recovery phrase"]
        XCTAssertTrue(generateRecoveryPhrase.waitForExistence(timeout: 10))
        XCTAssertFalse(
            application.buttons["Skip for now"].exists,
            "native onboarding must not bypass protected root creation"
        )
        generateRecoveryPhrase.tap()

        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let passcodePrompt = springboard.staticTexts.matching(
            NSPredicate(format: "label BEGINSWITH %@", "Enter iPhone Passcode")
        ).firstMatch
        let promptObserved = passcodePrompt.waitForExistence(timeout: 5)

        if promptObserved {
            XCTAssertFalse(
                application.otherElements["New wallet recovery phrase"].exists,
                "device authorization must complete before the recovery phrase is released"
            )
            return
        }

        let failedClosed = application.staticTexts["wallet authorization was denied"]
            .waitForExistence(timeout: 10)
            || application.staticTexts["wallet protection is unavailable"]
                .waitForExistence(timeout: 1)
        XCTAssertTrue(promptObserved || failedClosed)
        XCTAssertFalse(
            application.otherElements["New wallet recovery phrase"].exists,
            "cancelling or lacking user presence must not release the recovery phrase"
        )
        XCTAssertFalse(
            application.buttons["Finish and open wallet"].exists,
            "cancelling or lacking user presence must not complete onboarding"
        )
    }
}
