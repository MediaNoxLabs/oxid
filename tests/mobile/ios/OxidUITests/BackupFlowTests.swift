// SPDX-License-Identifier: Apache-2.0

import XCTest

final class BackupFlowTests: XCTestCase {
    private let applicationIdentifier = "io.medianox.oxid"
    private let backupFileName = "oxid-wallet.oxidbak"
    private let recoverySecret = "oxidsimulatorbackup2026"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func typeControlledText(
        _ text: String,
        into element: XCUIElement,
        in application: XCUIApplication
    ) {
        element.tap()
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        for character in text {
            application.typeText(String(character))
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertEqual((element.value as? String)?.count, text.count)
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        let keyboard = application.keyboards.firstMatch
        let safeTop = application.frame.minY + 70
        func safeBottom() -> CGFloat {
            if keyboard.exists {
                return keyboard.frame.minY - 12
            }
            let fixedNavigationClearance: CGFloat = application.buttons["Home"].exists ? 90 : 12
            return application.frame.maxY - fixedNavigationClearance
        }
        for _ in 0..<24
            where !element.isHittable || element.frame.maxY > safeBottom()
        {
            let targetIsAboveViewport = element.exists && element.frame.maxY < safeTop
            if keyboard.exists && targetIsAboveViewport {
                application.coordinate(
                    withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)
                ).press(
                    forDuration: 0.01,
                    thenDragTo: application.coordinate(
                        withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)
                    )
                )
            } else if keyboard.exists {
                application.coordinate(
                    withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)
                ).press(
                    forDuration: 0.01,
                    thenDragTo: application.coordinate(
                        withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)
                    )
                )
            } else if targetIsAboveViewport {
                application.swipeDown()
            } else {
                application.swipeUp()
            }
        }
        if !element.isHittable {
            let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
            screenshot.lifetime = .keepAlways
            add(screenshot)
        }
        XCTAssertTrue(element.isHittable)
        XCTAssertLessThanOrEqual(element.frame.maxY, safeBottom())
    }

    @MainActor
    private func systemElement(
        _ identifier: String,
        application: XCUIApplication,
        timeout: TimeInterval = 30
    ) -> XCUIElement {
        if let element = optionalSystemElement(
            identifier,
            application: application,
            timeout: timeout
        ) {
            return element
        }
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.lifetime = .keepAlways
        add(screenshot)
        XCTFail("system document picker did not expose \(identifier)")
        return application.descendants(matching: .any)[identifier].firstMatch
    }

    @MainActor
    private func systemElement(
        containing labelFragment: String,
        application: XCUIApplication,
        timeout: TimeInterval = 30
    ) -> XCUIElement {
        let files = XCUIApplication(bundleIdentifier: "com.apple.DocumentsApp")
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let predicate = NSPredicate(format: "label CONTAINS[c] %@", labelFragment)
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            for root in [application, files, springboard] {
                let element = root.descendants(matching: .any)
                    .matching(predicate)
                    .firstMatch
                if element.exists { return element }
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline

        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.lifetime = .keepAlways
        add(screenshot)
        XCTFail("system document picker did not expose a label containing \(labelFragment)")
        return files.descendants(matching: .any).matching(predicate).firstMatch
    }

    @MainActor
    private func optionalSystemElement(
        _ identifier: String,
        application: XCUIApplication,
        timeout: TimeInterval
    ) -> XCUIElement? {
        let files = XCUIApplication(bundleIdentifier: "com.apple.DocumentsApp")
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            for root in [application, files, springboard] {
                let element = root.descendants(matching: .any)[identifier].firstMatch
                if element.exists { return element }
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return nil
    }

    @MainActor
    private func createCompleteWallet(in application: XCUIApplication) {
        application.launch()
        let create = application.buttons["Create and continue"]
        XCTAssertTrue(create.waitForExistence(timeout: 15))
        create.tap()
        application.buttons["Wallet"].tap()

        let activate = application.buttons["Activate protected Midnight account"]
        XCTAssertTrue(activate.waitForExistence(timeout: 30))
        activate.tap()
        XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 90))

        application.buttons["Documents"].tap()
        application.buttons["Manage identities"].tap()
        let createDid = application.buttons["Create standalone DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 15))
        createDid.tap()
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 30))

        application.buttons["Documents"].tap()
        let offer = application.buttons["Use standalone demo offer"]
        XCTAssertTrue(offer.waitForExistence(timeout: 15))
        offer.tap()
        let preview = application.buttons["Preview credential offer"]
        scrollTo(preview, in: application)
        preview.tap()
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 15))
        let consent = application.descendants(matching: .any)["Consent to credential issuance"]
        XCTAssertTrue(consent.waitForExistence(timeout: 10))
        consent.tap()
        let issue = application.buttons["Accept and issue credential"]
        scrollTo(issue, in: application)
        issue.tap()
        XCTAssertTrue(
            application.staticTexts[
                "Credential issued, verified, and stored in the protected inventory."
            ].waitForExistence(timeout: 30)
        )
    }

    @MainActor
    func testExportsCompleteWalletBackupThroughDocumentPicker() throws {
        let application = XCUIApplication(bundleIdentifier: applicationIdentifier)
        createCompleteWallet(in: application)

        application.buttons["Open profile menu"].tap()
        application.buttons["Open settings"].tap()
        XCTAssertTrue(application.staticTexts["One encrypted wallet document"].waitForExistence(timeout: 15))

        let secret = application.secureTextFields["Recovery secret"].firstMatch
        XCTAssertTrue(secret.waitForExistence(timeout: 10))
        scrollTo(secret, in: application)
        typeControlledText(recoverySecret, into: secret, in: application)

        let repeated = application.secureTextFields["Repeat recovery secret"]
        XCTAssertTrue(repeated.exists)
        scrollTo(repeated, in: application)
        typeControlledText(recoverySecret, into: repeated, in: application)

        let confirmation = application.staticTexts[
            "I confirm this complete wallet export and will store its recovery secret separately."
        ].firstMatch
        scrollTo(confirmation, in: application)
        confirmation.tap()

        let export = application.buttons["Choose file and export"]
        scrollTo(export, in: application)
        XCTAssertTrue(export.isEnabled)
        export.tap()

        let save = systemElement("Save", application: application, timeout: 120)
        XCTAssertTrue(save.isHittable)
        save.tap()
        if let replace = optionalSystemElement(
            "Replace",
            application: application,
            timeout: 2
        ), replace.isHittable {
            replace.tap()
        }

        XCTAssertTrue(
            application.staticTexts[
                "Encrypted complete wallet backup saved to the selected document."
            ].waitForExistence(timeout: 120)
        )
    }

    @MainActor
    func testRecoversCompleteWalletBackupThroughDocumentPicker() throws {
        let application = XCUIApplication(bundleIdentifier: applicationIdentifier)
        application.launch()

        XCTAssertTrue(application.buttons["Create and continue"].waitForExistence(timeout: 15))
        let secret = application.secureTextFields["Recovery secret"].firstMatch
        XCTAssertTrue(secret.waitForExistence(timeout: 10))
        scrollTo(secret, in: application)
        typeControlledText(recoverySecret, into: secret, in: application)

        let confirmation = application.staticTexts[
            "I confirm complete recovery into this empty Oxid installation."
        ].firstMatch
        scrollTo(confirmation, in: application)
        confirmation.tap()

        let recover = application.buttons["Choose complete wallet backup and recover"]
        scrollTo(recover, in: application)
        XCTAssertTrue(recover.isEnabled)
        recover.tap()

        if let browse = optionalSystemElement(
            "Browse",
            application: application,
            timeout: 10
        ), browse.isHittable {
            browse.tap()
        }
        if let onMyIPhone = optionalSystemElement(
            "On My iPhone",
            application: application,
            timeout: 5
        ), onMyIPhone.isHittable {
            onMyIPhone.tap()
        }

        let backup = systemElement(
            containing: backupFileName.replacingOccurrences(of: ".oxidbak", with: ""),
            application: application,
            timeout: 30
        )
        XCTAssertTrue(backup.isHittable)
        backup.tap()
        if let open = optionalSystemElement(
            "Open",
            application: application,
            timeout: 2
        ), open.isHittable {
            open.tap()
        }

        let home = application.buttons["Home"]
        let recoveryAlert = application.descendants(matching: .any)["alert"].firstMatch
        let deadline = Date().addingTimeInterval(120)
        while !home.exists && !recoveryAlert.exists && Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
        if recoveryAlert.exists {
            let message = recoveryAlert.staticTexts.firstMatch.label
            XCTFail("complete wallet recovery failed: \(message)")
        }
        XCTAssertTrue(home.exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
        XCTAssertTrue(
            application.staticTexts["My wallet · Standalone"]
                .waitForExistence(timeout: 30)
        )
        application.buttons["Wallet"].tap()
        XCTAssertTrue(
            application.buttons["Copy Unshielded receive address"]
                .waitForExistence(timeout: 30)
        )
        XCTAssertTrue(
            application.buttons["Copy Shielded receive address"]
                .waitForExistence(timeout: 30)
        )

        application.buttons["Documents"].tap()
        application.buttons["Manage identities"].tap()
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 30))
        XCTAssertTrue(
            application.descendants(matching: .any)["Manage this DID"]
                .waitForExistence(timeout: 10)
        )

        application.buttons["Documents"].tap()
        XCTAssertTrue(application.staticTexts["Digital Passport"].waitForExistence(timeout: 30))
        XCTAssertTrue(application.buttons["Reverify"].waitForExistence(timeout: 10))
    }
}
