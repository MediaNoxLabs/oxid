// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

final class PortalFlowTests: XCTestCase {
    private let applicationIdentifier = "io.medianox.oxid"

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    private func restoredStatePhase(_ phase: String) {
        XCTContext.runActivity(named: "restored-state phase: \(phase)") { _ in }
    }

    @MainActor
    private func assertSingleValidCredential(in application: XCUIApplication) {
        let policy = "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        let valid = application.staticTexts.matching(NSPredicate(format: "label == %@", policy))
        if !valid.element(boundBy: 0).exists {
            let card = application.buttons["Open Digital Passport document details"]
            XCTAssertTrue(card.waitForExistence(timeout: 15))
            XCTAssertTrue(
                application.buttons.matching(
                    NSPredicate(format: "label == %@", "Open Digital Passport document details")
                ).element(boundBy: 1).waitForNonExistence(timeout: 5)
            )
            scrollTo(card, in: application)
            card.tap()
        }
        XCTAssertTrue(valid.element(boundBy: 0).waitForExistence(timeout: 15))
        XCTAssertTrue(valid.element(boundBy: 1).waitForNonExistence(timeout: 5))
        XCTAssertEqual(valid.count, 1)
    }

    @MainActor
    private func application() -> XCUIApplication {
        let application = XCUIApplication(bundleIdentifier: applicationIdentifier)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let open = springboard.buttons["Open"]
        if open.waitForExistence(timeout: 5) {
            open.tap()
        }
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
        let createWallet = application.buttons["Create private wallet"]
        let home = application.buttons["Home"]
        let readinessDeadline = Date().addingTimeInterval(20)
        while Date() < readinessDeadline, !createWallet.exists, !home.exists {
            RunLoop.current.run(until: Date().addingTimeInterval(0.25))
        }
        if createWallet.exists {
            createWallet.tap()
            let publicDemo = application.buttons["Use public demo wallet"]
            XCTAssertTrue(publicDemo.waitForExistence(timeout: 10))
            publicDemo.tap()
            let createAndContinue = application.buttons["Create and continue"]
            if !createAndContinue.waitForExistence(timeout: 3) {
                createWallet.tap()
            }
            XCTAssertTrue(createAndContinue.waitForExistence(timeout: 7))
            createAndContinue.tap()
            let enableProtection = application.buttons["Enable device protection"]
            XCTAssertTrue(enableProtection.waitForExistence(timeout: 15))
            enableProtection.tap()
        }
        XCTAssertTrue(home.waitForExistence(timeout: 20))
    }

    @MainActor
    private func assertRoutedOffer(in application: XCUIApplication) {
        let dismissRequest = application.buttons["Dismiss identity request"]
        XCTAssertTrue(dismissRequest.waitForExistence(timeout: 20))
        XCTAssertTrue(application.staticTexts["Credentials"].exists)
        XCTAssertTrue(dismissRequest.exists)
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

    private let credentialIssuanceTerminalStatus = "Credential issuance terminal error"
    private let protocolUnavailableCategory = "protocol unavailable"

    private enum ProtocolErrorDiagnosticValue: String {
        case durable
        case absent
        case clearedEarly = "cleared_early"
        case protocolUnavailable = "protocol_unavailable"
        case idle
        case busy
        case cleared
        case retained
        case legacyStaticTextPresent = "legacy_static_text_present"
        case legacyStaticTextAbsent = "legacy_static_text_absent"
    }

    private struct ProtocolErrorDiagnostic {
        let status: ProtocolErrorDiagnosticValue
        let category: ProtocolErrorDiagnosticValue
        let action: ProtocolErrorDiagnosticValue
        let importedOffer: ProtocolErrorDiagnosticValue
        let reviewAdmission: ProtocolErrorDiagnosticValue
        let legacyStaticText: ProtocolErrorDiagnosticValue
    }

    @MainActor
    private func protocolUnavailableTerminalStatus(in application: XCUIApplication) -> XCUIElement {
        let identifier = "\(credentialIssuanceTerminalStatus): \(protocolUnavailableCategory)"
        return application.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@", identifier)
        ).firstMatch
    }

    @MainActor
    private func observeProtocolError(in application: XCUIApplication) -> ProtocolErrorDiagnostic {
        let status = protocolUnavailableTerminalStatus(in: application)
        let appeared = status.waitForExistence(timeout: 35)
        let state: ProtocolErrorDiagnosticValue
        if appeared {
            Thread.sleep(forTimeInterval: 1)
            state = status.exists ? .durable : .clearedEarly
        } else {
            state = .absent
        }
        let legacy = application.staticTexts[
            "This protocol is unavailable in the current build"
        ].exists
        let actionIdle = application.buttons["Preview credential offer"].exists
            && !application.buttons["Checking offer…"].exists
        let importedOfferCleared = !application.descendants(matching: .any)[
            "Imported credential offer retained privately"
        ].exists && application.descendants(matching: .any)["Credential offer URI"].exists
        let reviewAdmissionCleared = !application.buttons["Dismiss identity request"].exists
        return ProtocolErrorDiagnostic(
            status: state,
            category: appeared || legacy ? .protocolUnavailable : .absent,
            action: actionIdle ? .idle : .busy,
            importedOffer: importedOfferCleared ? .cleared : .retained,
            reviewAdmission: reviewAdmissionCleared ? .cleared : .retained,
            legacyStaticText: legacy ? .legacyStaticTextPresent : .legacyStaticTextAbsent
        )
    }

    private func recordProtocolErrorDiagnostic(_ diagnostic: ProtocolErrorDiagnostic) {
        guard let path = ProcessInfo.processInfo.environment[
            "OXID_PORTAL_PROTOCOL_ERROR_DIAGNOSTIC_PATH"
        ] else {
            return
        }
        let json = "{\"schema\":\"oxid-ios-protocol-error-diagnostic-v1\",\"status\":\"\(diagnostic.status.rawValue)\",\"category\":\"\(diagnostic.category.rawValue)\",\"action\":\"\(diagnostic.action.rawValue)\",\"importedOffer\":\"\(diagnostic.importedOffer.rawValue)\",\"reviewAdmission\":\"\(diagnostic.reviewAdmission.rawValue)\",\"legacyStaticText\":\"\(diagnostic.legacyStaticText.rawValue)\"}"
        let destination = URL(fileURLWithPath: path)
        do {
            try Data(json.utf8).write(to: destination, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path)
        } catch {
            XCTFail("Closed protocol-error diagnostic could not be recorded")
        }
    }

    @MainActor
    private func assertProtocolUnavailableTerminalStatus(
        in application: XCUIApplication,
        recordDiagnostic: Bool
    ) {
        let diagnostic = observeProtocolError(in: application)
        if recordDiagnostic {
            recordProtocolErrorDiagnostic(diagnostic)
        }
        XCTAssertEqual(diagnostic.status, .durable)
        XCTAssertEqual(diagnostic.category, .protocolUnavailable)
        XCTAssertEqual(diagnostic.action, .idle)
        XCTAssertEqual(diagnostic.importedOffer, .cleared)
        XCTAssertEqual(diagnostic.reviewAdmission, .cleared)
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
        let activate = application.buttons["Activate protected Midnight account"]
        if activate.waitForExistence(timeout: 5) {
            scrollTo(activate, in: application)
            activate.tap()
            XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 45))
        }
        application.buttons["Documents"].tap()
        let manage = application.buttons["Manage identities"]
        XCTAssertTrue(manage.waitForExistence(timeout: 10))
        scrollTo(manage, in: application)
        manage.tap()
        let createDid = application.buttons["Create a DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 10))
        scrollTo(createDid, in: application)
        createDid.tap()
        let confirmCreateDid = application.buttons["Create DID"]
        XCTAssertTrue(confirmCreateDid.waitForExistence(timeout: 10))
        scrollTo(confirmCreateDid, in: application)
        confirmCreateDid.tap()
        let copyDid = application.buttons["Copy DID"]
        XCTAssertTrue(copyDid.waitForExistence(timeout: 30))
        XCTAssertTrue(application.staticTexts["Managed"].exists)
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
        assertProtocolUnavailableTerminalStatus(in: application, recordDiagnostic: true)
    }

    @MainActor
    func testProtocolTimeout() {
        let application = application()
        assertRoutedOffer(in: application)
        preview(in: application)
        let checking = application.buttons["Checking offer…"]
        XCTAssertTrue(checking.waitForExistence(timeout: 5))
        XCTAssertFalse(checking.isEnabled)
        assertProtocolUnavailableTerminalStatus(in: application, recordDiagnostic: false)
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
        let terminal = protocolUnavailableTerminalStatus(in: application)
        XCTAssertTrue(terminal.waitForExistence(timeout: 40))
        XCTAssertTrue(
            application.staticTexts["Credential offer preview"].waitForNonExistence(timeout: 20)
        )
        XCTAssertTrue(
            application.buttons["Leave credential review"].waitForNonExistence(timeout: 20)
        )
        XCTAssertFalse(application.buttons["Accept and issue credential"].exists)
        XCTAssertFalse(application.buttons["Dismiss identity request"].exists)
        XCTAssertTrue(terminal.exists)
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
        XCTAssertTrue(application.staticTexts["Saved to your wallet"].waitForExistence(timeout: 20))
        XCTAssertTrue(
            application.staticTexts["Credential offer preview"].waitForNonExistence(timeout: 20)
        )
        XCTAssertFalse(application.buttons["Accept and issue credential"].exists)
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].waitForExistence(timeout: 20))
        assertSingleValidCredential(in: application)
        XCTAssertFalse(application.staticTexts["John"].exists)
        XCTAssertFalse(application.staticTexts["Doe"].exists)
    }

    @MainActor
    func testRestored() {
        let application = application()
        restoredStatePhase("foreground")
        application.buttons["Wallet"].tap()
        restoredStatePhase("wallet-selected")
        let reactivate = application.buttons["Activate protected Midnight account"]
        if reactivate.waitForExistence(timeout: 5) {
            scrollTo(reactivate, in: application)
            reactivate.tap()
            restoredStatePhase("custody-reactivation-requested")
        }
        XCTAssertTrue(application.buttons["Use my receive address"].waitForExistence(timeout: 45))
        restoredStatePhase("wallet-ready")
        application.buttons["Documents"].tap()
        restoredStatePhase("documents-selected")
        assertSingleValidCredential(in: application)
        restoredStatePhase("restored-credential-visible")
        let marker = application.staticTexts["Credential reverification applied"]
        XCTAssertFalse(marker.exists)
        let reverify = application.buttons["Reverify"]
        XCTAssertTrue(reverify.waitForExistence(timeout: 15))
        scrollTo(reverify, in: application)
        reverify.tap()
        restoredStatePhase("reverification-requested")
        let completed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"),
            object: reverify
        )
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 35), .completed)
        XCTAssertTrue(marker.waitForExistence(timeout: 35))
        restoredStatePhase("reverification-applied")
        assertSingleValidCredential(in: application)
        XCTAssertTrue(application.staticTexts[
            "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
        ].exists)
        restoredStatePhase("restored-state-asserted")
    }
}
