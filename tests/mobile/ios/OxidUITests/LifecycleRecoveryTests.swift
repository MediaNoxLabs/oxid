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
    private func assertAutomaticReconciliation(_ application: XCUIApplication) -> Int {
        XCTAssertTrue(application.staticTexts["Synced"].waitForExistence(timeout: 30))
        XCTAssertTrue(application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "Simulated source")
        ).firstMatch.exists)
        XCTAssertFalse(application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "last consistent checkpoint")
        ).firstMatch.exists)
        let lifecycle = application.descendants(matching: .any).matching(
            NSPredicate(format: "label BEGINSWITH %@", "Wallet lifecycle generation ")
        ).firstMatch
        XCTAssertTrue(lifecycle.waitForExistence(timeout: 10))
        return Int(lifecycle.label.split(separator: " ").last ?? "") ?? -1
    }

    @MainActor
    private func revealAndAssertConsistentProjection(_ application: XCUIApplication) {
        // WebKit may expose ARIA menu controls as buttons, menu buttons, or
        // checkboxes on different iOS runtimes. Select by the stable
        // accessible name instead of coupling the diagnostic to that mapping.
        let menu = application.descendants(matching: .any)[
            "Open global application menu"
        ]
        XCTAssertTrue(menu.waitForExistence(timeout: 10))
        menu.tap()
        let privacy = application.descendants(matching: .any).matching(
            NSPredicate(format: "label BEGINSWITH[c] %@", "Session privacy")
        ).firstMatch
        XCTAssertTrue(privacy.waitForExistence(timeout: 5))
        privacy.tap()
        XCTAssertTrue(application.staticTexts["5 NIGHT"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["12 DUST"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "1 protected notes")
        ).firstMatch.waitForExistence(timeout: 10))
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
        let publicDemo = application.buttons["Use public demo wallet"]
        XCTAssertTrue(publicDemo.waitForExistence(timeout: 10))
        publicDemo.tap()
        XCTAssertTrue(application.buttons["Create and continue"].waitForExistence(timeout: 15))
        application.buttons["Create and continue"].tap()
        let enableProtection = application.buttons["Enable device protection"]
        XCTAssertTrue(enableProtection.waitForExistence(timeout: 15))
        enableProtection.tap()
        authorizeSimulatorOwnerIfRequested()

        openWallet(application)
        let activate = application.buttons["Activate protected Midnight account"]
        if activate.waitForExistence(timeout: 3) {
            activate.tap()
            XCTAssertTrue(
                application.buttons["Use my receive address"].waitForExistence(timeout: 90)
            )
        }
        let initialGeneration = assertAutomaticReconciliation(application)
        XCTAssertGreaterThanOrEqual(initialGeneration, 0)
        revealAndAssertConsistentProjection(application)

        XCUIDevice.shared.press(.home)
        RunLoop.current.run(until: Date().addingTimeInterval(2))
        application.activate()
        XCTAssertTrue(application.descendants(matching: .any)[
            "Open global application menu"
        ].waitForExistence(timeout: 15))
        XCTAssertFalse(application.staticTexts["5 NIGHT"].exists)
        let foregroundGeneration = assertAutomaticReconciliation(application)
        XCTAssertGreaterThan(foregroundGeneration, initialGeneration)
        revealAndAssertConsistentProjection(application)

        application.terminate()
        application.launch()
        openWallet(application)
        let relaunchedGeneration = assertAutomaticReconciliation(application)
        XCTAssertGreaterThan(relaunchedGeneration, foregroundGeneration)
        revealAndAssertConsistentProjection(application)

        try writeClosedDiagnostic()
    }
}
