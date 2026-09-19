// SPDX-License-Identifier: Apache-2.0

import XCTest

final class LifecycleRecoveryTests: XCTestCase {
    private let applicationIdentifier = "io.medianox.oxid"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func openWallet(_ application: XCUIApplication) {
        XCTAssertTrue(application.buttons["Wallet"].waitForExistence(timeout: 15))
        application.buttons["Wallet"].tap()
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        for _ in 0..<20 where !element.isHittable {
            application.swipeUp()
        }
        XCTAssertTrue(element.isHittable)
    }

    @MainActor
    private func assertConsistentProjection(_ application: XCUIApplication) {
        XCTAssertTrue(application.staticTexts["5 NIGHT"].waitForExistence(timeout: 30))
        XCTAssertTrue(application.staticTexts["12 DUST"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["1 shielded notes"].waitForExistence(timeout: 10))
        XCTAssertFalse(application.buttons["Sync DUST"].exists)
        XCTAssertFalse(application.buttons["Sync shielded assets"].exists)
        XCTAssertFalse(application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "last consistent checkpoint")
        ).firstMatch.exists)
    }

    @MainActor
    private func authorizeSimulatorOwnerIfRequested() {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let passcode = springboard.secureTextFields.firstMatch
        guard passcode.waitForExistence(timeout: 5) else { return }
        passcode.tap()
        passcode.typeText("1234\n")
    }

    private func writeClosedDiagnostic() throws {
        let path = try XCTUnwrap(
            ProcessInfo.processInfo.environment["OXID_LIFECYCLE_DIAGNOSTIC_PATH"]
        )
        let document = """
        {"schema":"oxid-ios-wallet-lifecycle-diagnostic-v1","backgroundForeground":"recovered","processRelaunch":"recovered","protectedInteraction":"rearmed","manualFamilySync":"not_used","staleObservation":"not_visible"}
        """
        try Data(document.utf8).write(
            to: URL(fileURLWithPath: path),
            options: [.atomic]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: path
        )
    }

    @MainActor
    func testBackgroundAndColdRelaunchRecoverWithoutManualSync() throws {
        let application = XCUIApplication(bundleIdentifier: applicationIdentifier)
        application.launch()

        XCTAssertTrue(application.buttons["Create private wallet"].waitForExistence(timeout: 15))
        application.buttons["Create private wallet"].tap()
        XCTAssertTrue(application.buttons["Create and continue"].waitForExistence(timeout: 15))
        application.buttons["Create and continue"].tap()

        let generatePhrase = application.buttons["Generate recovery phrase"]
        XCTAssertTrue(generatePhrase.waitForExistence(timeout: 15))
        generatePhrase.tap()
        authorizeSimulatorOwnerIfRequested()
        XCTAssertTrue(
            application.otherElements["New wallet recovery phrase"]
                .waitForExistence(timeout: 30)
        )
        let backupAcknowledgement = application.switches[
            "I have securely saved or verified this recovery phrase."
        ]
        XCTAssertTrue(backupAcknowledgement.waitForExistence(timeout: 10))
        scrollTo(backupAcknowledgement, in: application)
        application.staticTexts[
            "I have securely saved or verified this recovery phrase."
        ].tap()
        let acknowledged = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %@", "1"),
            object: backupAcknowledgement
        )
        XCTAssertEqual(XCTWaiter.wait(for: [acknowledged], timeout: 10), .completed)
        let finishOnboarding = application.buttons["Finish and open wallet"]
        XCTAssertTrue(finishOnboarding.waitForExistence(timeout: 10))
        let enabled = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"),
            object: finishOnboarding
        )
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 10), .completed)
        if !finishOnboarding.isHittable {
            application.swipeUp()
        }
        finishOnboarding.tap()
        authorizeSimulatorOwnerIfRequested()

        openWallet(application)
        let activate = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(activate.waitForExistence(timeout: 15))
        activate.tap()
        XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 90))
        assertConsistentProjection(application)

        let reveal = application.descendants(matching: .any)[
            "Show private values for 30 seconds"
        ]
        XCTAssertTrue(reveal.waitForExistence(timeout: 10))
        reveal.tap()
        XCTAssertTrue(
            application.descendants(matching: .any)["Hide private values"]
                .waitForExistence(timeout: 5)
        )

        XCUIDevice.shared.press(.home)
        RunLoop.current.run(until: Date().addingTimeInterval(2))
        application.activate()
        XCTAssertTrue(reveal.waitForExistence(timeout: 15))
        XCTAssertFalse(application.descendants(matching: .any)["Hide private values"].exists)
        assertConsistentProjection(application)

        application.terminate()
        application.launch()
        openWallet(application)
        assertConsistentProjection(application)

        try writeClosedDiagnostic()
    }
}
