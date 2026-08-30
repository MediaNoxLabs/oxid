// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

final class PortalFlowTests: XCTestCase {
    private let applicationIdentifier = "io.medianox.oxid"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func application() -> XCUIApplication {
        let application = XCUIApplication(bundleIdentifier: applicationIdentifier)
        application.activate()
        XCTAssertTrue(application.wait(for: .runningForeground, timeout: 15))
        return application
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        let safeTop = application.frame.minY + 90
        let safeBottom = application.frame.maxY - 90
        for _ in 0..<20 {
            if element.isHittable,
               element.frame.minY >= safeTop,
               element.frame.maxY <= safeBottom {
                return
            }
            if element.exists, element.frame.minY < safeTop {
                application.swipeDown()
            } else {
                application.swipeUp()
            }
        }
        XCTAssertTrue(element.isHittable)
        XCTAssertGreaterThanOrEqual(element.frame.minY, safeTop)
        XCTAssertLessThanOrEqual(element.frame.maxY, safeBottom)
    }

    @MainActor
    private func ensureProfile(in application: XCUIApplication) {
        let createWallet = application.buttons["Create new wallet"]
        if createWallet.waitForExistence(timeout: 5) {
            createWallet.tap()
            let createAndContinue = application.buttons["Create and continue"]
            XCTAssertTrue(createAndContinue.waitForExistence(timeout: 10))
            createAndContinue.tap()
            let skip = application.buttons["Skip for now"]
            XCTAssertTrue(skip.waitForExistence(timeout: 15))
            skip.tap()
        }
        XCTAssertTrue(application.buttons["Home"].waitForExistence(timeout: 20))
    }

    @MainActor
    private func assertRoutedOffer(in application: XCUIApplication) {
        XCTAssertTrue(application.staticTexts[
            "App link recognized as a credential offer. Review the request before consent."
        ].waitForExistence(timeout: 20))
        XCTAssertTrue(application.staticTexts["Credentials"].exists)
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertTrue(application.descendants(matching: .any)[
            "Imported credential offer retained privately"
        ].exists)
        XCTAssertFalse(application.descendants(matching: .any)["Credential offer URI"].exists)
        XCTAssertFalse(application.descendants(matching: .any)["Consent to credential issuance"].exists)
        XCTAssertFalse(application.buttons["Accept and issue credential"].exists)
    }

    @MainActor
    private func preview(in application: XCUIApplication) {
        let preview = application.buttons["Preview credential offer"]
        XCTAssertTrue(preview.waitForExistence(timeout: 10))
        scrollTo(preview, in: application)
        preview.tap()
    }

    @MainActor
    private func assertExactPreview(in application: XCUIApplication) -> XCUIElement {
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 30))
        XCTAssertTrue(
            application.descendants(matching: .any)["Imported credential offer retained privately"]
                .waitForNonExistence(timeout: 10)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].waitForNonExistence(timeout: 10))
        for heading in [
            "Who is issuing it?", "What will you receive?", "Which identity receives it?",
            "Why add it?", "Unverified endpoint",
        ] {
            XCTAssertTrue(application.staticTexts[heading].exists, heading)
        }
        let consent = application.descendants(matching: .any)["Consent to credential issuance"]
        XCTAssertTrue(consent.waitForExistence(timeout: 10))
        XCTAssertEqual(consent.value as? String, "0")
        let issue = application.buttons["Accept and issue credential"]
        XCTAssertTrue(issue.exists)
        XCTAssertFalse(issue.isEnabled)
        return consent
    }

    @MainActor
    private func preparePreview(in application: XCUIApplication) -> XCUIElement {
        assertRoutedOffer(in: application)
        preview(in: application)
        return assertExactPreview(in: application)
    }

    private func unavailableStatus(in application: XCUIApplication) -> XCUIElement {
        application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "protocol is unavailable")
        ).firstMatch
    }

    private func signalIssueErrorBoundary() throws {
        let environment = ProcessInfo.processInfo.environment
        let directory = try XCTUnwrap(environment["OXID_PORTAL_PHASE_DIRECTORY"])
        let request = URL(fileURLWithPath: directory).appendingPathComponent("issue-error-ready")
        let acknowledgement = URL(fileURLWithPath: directory).appendingPathComponent("issue-error-armed")
        try Data("ready\n".utf8).write(to: request, options: .atomic)
        let deadline = Date().addingTimeInterval(15)
        while Date() < deadline {
            if FileManager.default.fileExists(atPath: acknowledgement.path) { return }
            Thread.sleep(forTimeInterval: 0.1)
        }
        XCTFail("Shell-mediated issuance failure was not armed")
    }

    @MainActor
    func testColdRoute() {
        let application = application()
        ensureProfile(in: application)
        assertRoutedOffer(in: application)
        application.buttons["Dismiss identity request"].tap()
        XCTAssertTrue(application.buttons["Dismiss identity request"].waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testPrepareHolder() {
        let application = application()
        ensureProfile(in: application)
        application.buttons["Wallet"].tap()
        let activate = application.buttons["Activate development wallet"]
        if activate.waitForExistence(timeout: 5) {
            activate.tap()
            XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 45))
        }
        application.buttons["Documents"].tap()
        let manage = application.buttons["Manage identities"]
        XCTAssertTrue(manage.waitForExistence(timeout: 10))
        manage.tap()
        let createDid = application.buttons["Create standalone DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 10))
        createDid.tap()
        XCTAssertTrue(application.descendants(matching: .any)["Manage this DID"].waitForExistence(timeout: 30))
    }

    @MainActor
    func testRouteRefuse() {
        let application = application()
        _ = preparePreview(in: application)
        let refuse = application.buttons["Refuse offer"]
        scrollTo(refuse, in: application)
        refuse.tap()
        XCTAssertTrue(application.staticTexts[
            "Credential offer refused; ephemeral protocol secrets were discarded."
        ].waitForExistence(timeout: 15))
        XCTAssertFalse(application.buttons["Dismiss identity request"].exists)
    }

    @MainActor
    func testMalformed() {
        let application = application()
        assertRoutedOffer(in: application)
        preview(in: application)
        XCTAssertTrue(application.staticTexts[
            "The issuer metadata is not valid"
        ].waitForExistence(timeout: 20))
        XCTAssertTrue(application.buttons["Dismiss identity request"].waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testProtocolError() {
        let application = application()
        assertRoutedOffer(in: application)
        preview(in: application)
        XCTAssertTrue(unavailableStatus(in: application).waitForExistence(timeout: 35))
        XCTAssertTrue(application.buttons["Dismiss identity request"].waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testProtocolTimeout() {
        let application = application()
        assertRoutedOffer(in: application)
        preview(in: application)
        let checking = application.buttons["Checking offer…"]
        XCTAssertTrue(checking.waitForExistence(timeout: 5))
        XCTAssertFalse(checking.isEnabled)
        XCTAssertTrue(unavailableStatus(in: application).waitForExistence(timeout: 40))
        XCTAssertTrue(application.buttons["Dismiss identity request"].waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testIssueError() throws {
        let application = application()
        let consent = preparePreview(in: application)
        scrollTo(consent, in: application)
        consent.tap()
        let issue = application.buttons["Accept and issue credential"]
        XCTAssertTrue(issue.isEnabled)
        try signalIssueErrorBoundary()
        scrollTo(issue, in: application)
        issue.tap()
        let leave = application.buttons["Leave credential review"]
        XCTAssertTrue(leave.waitForExistence(timeout: 40))
        XCTAssertTrue(leave.isEnabled)
        XCTAssertTrue(unavailableStatus(in: application).exists)
        XCTAssertEqual(consent.value as? String, "0")
        XCTAssertFalse(issue.isEnabled)
        XCTAssertTrue(application.staticTexts["Credential offer preview"].exists)
        XCTAssertFalse(application.buttons["Dismiss identity request"].exists)
        application.buttons["Wallet"].tap()
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 10))
        leave.tap()
        XCTAssertTrue(leave.waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testIssue() {
        let application = application()
        let consent = preparePreview(in: application)
        scrollTo(consent, in: application)
        consent.tap()
        let issue = application.buttons["Accept and issue credential"]
        XCTAssertTrue(issue.isEnabled)
        scrollTo(issue, in: application)
        issue.tap()
        XCTAssertTrue(application.staticTexts[
            "Credential issued, verified, and stored in the protected inventory."
        ].waitForExistence(timeout: 100))
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].waitForExistence(timeout: 20))
        XCTAssertEqual(
            application.staticTexts.matching(NSPredicate(format: "label == %@", "Valid")).count,
            1
        )
        XCTAssertFalse(application.staticTexts["John"].exists)
        XCTAssertFalse(application.staticTexts["Doe"].exists)
    }

    @MainActor
    func testRestored() {
        let application = application()
        application.buttons["Wallet"].tap()
        let reactivate = application.buttons["Activate development wallet"]
        XCTAssertTrue(reactivate.waitForExistence(timeout: 15))
        reactivate.tap()
        XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 45))
        application.buttons["Documents"].tap()
        XCTAssertEqual(
            application.staticTexts.matching(NSPredicate(format: "label == %@", "Valid")).count,
            1
        )
        let marker = application.staticTexts["Credential reverification applied"]
        XCTAssertFalse(marker.exists)
        let reverify = application.buttons["Reverify"]
        XCTAssertTrue(reverify.waitForExistence(timeout: 15))
        scrollTo(reverify, in: application)
        reverify.tap()
        let completed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"),
            object: reverify
        )
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 35), .completed)
        XCTAssertTrue(marker.waitForExistence(timeout: 35))
        XCTAssertEqual(
            application.staticTexts.matching(NSPredicate(format: "label == %@", "Valid")).count,
            1
        )
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].exists)
    }
}
