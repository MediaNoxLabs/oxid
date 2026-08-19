// SPDX-License-Identifier: Apache-2.0

import XCTest

final class ProfileFlowTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        // WKWebView can report a control as hittable when only a tiny strip is
        // visible above Oxid's fixed bottom navigation. Keep content controls
        // clear of that navigation before tapping them.
        let safeBottom = application.frame.maxY - 90
        for _ in 0..<20
            where !element.isHittable || element.frame.maxY > safeBottom
        {
            application.swipeUp()
        }
        XCTAssertTrue(element.isHittable)
        XCTAssertLessThanOrEqual(element.frame.maxY, safeBottom)
    }

    @MainActor
    private func scrollBackTo(_ element: XCUIElement, in application: XCUIApplication) {
        // A completed card can leave a reusable fixture control above the
        // current viewport. Move toward the document start for that case.
        for _ in 0..<20 where !element.isHittable {
            application.swipeDown()
        }
        XCTAssertTrue(element.isHittable)
    }

    @MainActor
    private func ensureProfile(in application: XCUIApplication) {
        application.launch()
        let createButton = application.buttons["Create and continue"]
        if createButton.waitForExistence(timeout: 5) {
            createButton.tap()
        }
        XCTAssertTrue(application.buttons["Scan identity QR code"].waitForExistence(timeout: 15))
    }

    @MainActor
    private func assertHomeComposition(in application: XCUIApplication) {
        let home = application.buttons["Home"]
        XCTAssertTrue(home.waitForExistence(timeout: 15))
        home.tap()
        XCTAssertTrue(
            application.staticTexts["Everything in one place"].waitForExistence(timeout: 15)
        )
        for action in ["Receive", "Send", "Present"] {
            XCTAssertTrue(application.buttons[action].exists)
        }
        XCTAssertTrue(application.buttons["Open Wallet NIGHT account"].exists)
        XCTAssertTrue(application.buttons["Open Wallet shielded account"].exists)
        XCTAssertTrue(application.buttons["Open newest document"].exists)
        XCTAssertTrue(application.buttons["Open Passport Vault"].exists)
        XCTAssertTrue(application.buttons["Open wallet security settings"].exists)
        XCTAssertTrue(application.buttons["See all activity"].exists)
        XCTAssertFalse(application.staticTexts["Backed up"].exists)
    }

    @MainActor
    private func openPassportVault(in application: XCUIApplication) {
        application.buttons["Home"].tap()
        let vault = application.buttons["Open Passport Vault"]
        let firstCard = application.buttons["Open Wallet NIGHT account"]
        XCTAssertTrue(vault.waitForExistence(timeout: 15))
        for _ in 0..<5 where !vault.isHittable {
            firstCard.swipeLeft()
        }
        XCTAssertTrue(vault.isHittable)
        vault.tap()
    }

    @MainActor
    func testCreatesProfileAndCompletesStandaloneWalletFlow() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)
        assertHomeComposition(in: application)
        application.buttons["Present"].tap()
        XCTAssertTrue(application.buttons["Manage identities"].waitForExistence(timeout: 5))
        application.buttons["Home"].tap()
        application.buttons["Receive"].tap()

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
        XCTAssertTrue(application.staticTexts["5 NIGHT"].waitForExistence(timeout: 5))
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
        let continueToAmount = application.buttons["Continue to transfer amount"]
        XCTAssertTrue(continueToAmount.waitForExistence(timeout: 5))
        continueToAmount.tap()

        let shieldedTransfer = application.switches["Use shielded NIGHT transfer"]
        XCTAssertTrue(shieldedTransfer.waitForExistence(timeout: 5))
        scrollTo(shieldedTransfer, in: application)
        shieldedTransfer.tap()
        XCTAssertEqual(shieldedTransfer.value as? String, "1")

        let amount = application.textFields["Amount in NIGHT"]
        XCTAssertTrue(amount.exists)
        scrollTo(amount, in: application)
        amount.tap()
        // The iOS 26 simulator keyboard drops the decimal separator from
        // `typeText("1.5")`; use an exact whole NIGHT for this interaction
        // smoke while Rust unit tests retain fractional conversion coverage.
        amount.typeText("1")
        // The numeric keyboard overlaps the lower WKWebView controls. Blur the
        // field through the fixed, non-interactive center of the app header so
        // the following tap reaches the review button instead of a keypad key.
        application.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.08)).tap()
        let review = application.buttons["Review exact transfer"]
        scrollTo(review, in: application)
        review.tap()

        let continueToConfirm = application.buttons["Continue to NIGHT transfer confirmation"]
        XCTAssertTrue(continueToConfirm.waitForExistence(timeout: 10))
        scrollTo(continueToConfirm, in: application)
        continueToConfirm.tap()
        let authorize = application.buttons["Authorize reviewed NIGHT transfer"]
        XCTAssertTrue(authorize.waitForExistence(timeout: 10))
        scrollTo(authorize, in: application)
        authorize.tap()
        let submit = application.buttons["Prove and submit NIGHT transfer"]
        XCTAssertTrue(submit.waitForExistence(timeout: 10))
        scrollTo(submit, in: application)
        submit.tap()
        let cancelSubmission = application.buttons["Cancel NIGHT transfer submission"]
        if cancelSubmission.waitForExistence(timeout: 5) {
            cancelSubmission.tap()
            let retrySubmission = application.buttons["Retry safely — nothing was broadcast"]
            XCTAssertTrue(retrySubmission.waitForExistence(timeout: 5))
            retrySubmission.tap()
            XCTAssertTrue(submit.waitForExistence(timeout: 5))
            scrollTo(submit, in: application)
            submit.tap()
        }
        XCTAssertTrue(application.staticTexts["Transfer confirmed"].waitForExistence(timeout: 15))

        let documents = application.buttons["Documents"]
        XCTAssertTrue(documents.waitForExistence(timeout: 5))
        documents.tap()
        let manageIdentities = application.buttons["Manage identities"]
        XCTAssertTrue(manageIdentities.waitForExistence(timeout: 5))
        manageIdentities.tap()
        let createDid = application.buttons["Create standalone DID"]
        XCTAssertTrue(createDid.waitForExistence(timeout: 5))
        createDid.tap()
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
        XCTAssertTrue(
            application.descendants(matching: .any)["Manage this DID"]
                .waitForExistence(timeout: 5)
        )
        let demoLogin = application.buttons["Use standalone login request"]
        XCTAssertTrue(demoLogin.waitForExistence(timeout: 5))
        scrollTo(demoLogin, in: application)
        demoLogin.tap()
        let previewLogin = application.buttons["Preview login request"]
        scrollTo(previewLogin, in: application)
        previewLogin.tap()
        XCTAssertTrue(application.staticTexts["DID authentication preview"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Who is asking?"].exists)
        XCTAssertTrue(application.staticTexts["What will you prove?"].exists)
        XCTAssertTrue(application.staticTexts["Which identity?"].exists)
        XCTAssertTrue(application.staticTexts["Why is it requested?"].exists)
        XCTAssertTrue(application.staticTexts["Unverified endpoint"].exists)
        XCTAssertTrue(
            application.staticTexts[
                "Control of the selected managed DID. No credential or document claims will be disclosed."
            ].exists
        )
        let loginConsent = application.descendants(matching: .any)["Consent to DID authentication"]
        XCTAssertTrue(loginConsent.waitForExistence(timeout: 5))
        loginConsent.tap()
        let authenticate = application.buttons["Authenticate with DID"]
        scrollTo(authenticate, in: application)
        authenticate.tap()
        XCTAssertTrue(
            application.staticTexts["DID authentication succeeded and the standalone verifier independently validated the proof."]
                .waitForExistence(timeout: 10)
        )
        let resolveDid = application.buttons["Resolve and save"]
        XCTAssertTrue(resolveDid.waitForExistence(timeout: 5))
        scrollTo(resolveDid, in: application)
        resolveDid.tap()
        XCTAssertTrue(application.staticTexts["standalone-fixture-v2"].waitForExistence(timeout: 10))

        let credentials = application.buttons["Documents"]
        XCTAssertTrue(credentials.waitForExistence(timeout: 5))
        credentials.tap()
        let demoOffer = application.buttons["Use standalone demo offer"]
        XCTAssertTrue(demoOffer.waitForExistence(timeout: 5))
        scrollTo(demoOffer, in: application)
        demoOffer.tap()
        let previewOffer = application.buttons["Preview credential offer"]
        scrollTo(previewOffer, in: application)
        previewOffer.tap()
        XCTAssertTrue(application.staticTexts["Digital Passport"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Credential offer preview"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.staticTexts["Who is issuing it?"].exists)
        XCTAssertTrue(application.staticTexts["What will you receive?"].exists)
        XCTAssertTrue(application.staticTexts["Which identity receives it?"].exists)
        XCTAssertTrue(application.staticTexts["Why add it?"].exists)
        XCTAssertTrue(application.staticTexts["Unverified endpoint"].exists)
        let consent = application.descendants(matching: .any)["Consent to credential issuance"]
        XCTAssertTrue(consent.waitForExistence(timeout: 5))
        consent.tap()
        let issueCredential = application.buttons["Accept and issue credential"]
        scrollTo(issueCredential, in: application)
        issueCredential.tap()
        XCTAssertTrue(
            application.staticTexts["Credential issued, verified, and stored in the protected inventory."]
                .waitForExistence(timeout: 10)
        )
        XCTAssertTrue(
            application.staticTexts[
                "Credential policy · issuer passed · time passed · trust passed · revocation not checked"
            ].waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.staticTexts["Valid"].waitForExistence(timeout: 10))
        XCTAssertFalse(application.staticTexts["Alice"].exists)
        XCTAssertFalse(application.staticTexts["Example"].exists)
        let revealFirst = application.buttons["Reveal First name locally"]
        scrollTo(revealFirst, in: application)
        revealFirst.tap()
        XCTAssertTrue(application.staticTexts["Alice"].waitForExistence(timeout: 5))
        let hideFirst = application.buttons["Hide First name"]
        XCTAssertTrue(hideFirst.waitForExistence(timeout: 5))
        hideFirst.tap()
        XCTAssertFalse(application.staticTexts["Alice"].exists)
        let revealLast = application.buttons["Reveal Last name locally"]
        scrollTo(revealLast, in: application)
        revealLast.tap()
        XCTAssertTrue(application.staticTexts["Example"].waitForExistence(timeout: 5))
        let hideLast = application.buttons["Hide Last name"]
        XCTAssertTrue(hideLast.waitForExistence(timeout: 5))
        hideLast.tap()
        XCTAssertFalse(application.staticTexts["Example"].exists)
        XCTAssertTrue(application.textFields["Age threshold"].exists)
        let previewDisclosure = application.buttons["Preview disclosure plan"]
        scrollTo(previewDisclosure, in: application)
        previewDisclosure.tap()
        XCTAssertTrue(
            application.staticTexts["Disclosure preview ready · local preview only · no presentation generated"]
                .waitForExistence(timeout: 5)
        )
        let reverify = application.buttons["Reverify"]
        scrollTo(reverify, in: application)
        reverify.tap()
        XCTAssertTrue(reverify.waitForExistence(timeout: 10))

        // The standalone credential ID commits to its issuance second. Issue
        // another after that boundary so the presentation request has two
        // distinct matching credentials to choose between.
        Thread.sleep(forTimeInterval: 1.2)
        scrollBackTo(demoOffer, in: application)
        demoOffer.tap()
        scrollTo(previewOffer, in: application)
        previewOffer.tap()
        XCTAssertTrue(
            application.staticTexts["Credential offer preview"].waitForExistence(timeout: 5)
        )
        scrollTo(consent, in: application)
        consent.tap()
        scrollTo(issueCredential, in: application)
        issueCredential.tap()
        XCTAssertTrue(
            application.staticTexts["Credential issued, verified, and stored in the protected inventory."]
                .waitForExistence(timeout: 10)
        )
        let verifierRequest = application.buttons["Use standalone verifier request"]
        XCTAssertTrue(verifierRequest.waitForExistence(timeout: 5))
        scrollTo(verifierRequest, in: application)
        verifierRequest.tap()
        let previewPresentation = application.buttons["Preview presentation request"]
        scrollTo(previewPresentation, in: application)
        previewPresentation.tap()
        XCTAssertTrue(application.staticTexts["Presentation preview"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Who is asking?"].exists)
        XCTAssertTrue(application.staticTexts["What will be shared?"].exists)
        XCTAssertTrue(application.staticTexts["Which document?"].exists)
        XCTAssertTrue(application.staticTexts["Why is it requested?"].exists)
        XCTAssertTrue(application.staticTexts["Unverified endpoint"].exists)
        XCTAssertTrue(
            application.staticTexts[
                "Confirms you're over 18. Your date of birth will not be shared."
            ].exists
        )
        XCTAssertTrue(
            application.staticTexts["No presentation or vp_token has been generated."]
                .waitForExistence(timeout: 5)
        )
        let presentationConsent = application.descendants(matching: .any)["Consent to credential presentation"]
        XCTAssertTrue(presentationConsent.waitForExistence(timeout: 5))
        XCTAssertFalse(presentationConsent.isEnabled)
        let matchingCredentials = application.descendants(matching: .any).matching(
            NSPredicate(format: "label BEGINSWITH %@", "Use Digital Passport issued by")
        )
        XCTAssertEqual(matchingCredentials.count, 2)
        let secondCredential = matchingCredentials.element(boundBy: 1)
        scrollTo(secondCredential, in: application)
        secondCredential.tap()
        let consentEnabled = expectation(
            for: NSPredicate(format: "enabled == true"),
            evaluatedWith: presentationConsent
        )
        wait(for: [consentEnabled], timeout: 5)
        scrollTo(presentationConsent, in: application)
        presentationConsent.tap()
        let presentCredential = application.buttons["Share proof"]
        scrollTo(presentCredential, in: application)
        presentCredential.tap()
        XCTAssertTrue(
            application.staticTexts["The holder authorized this exact presentation, but Compact proving is unavailable. No presentation or vp_token was generated."]
                .waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.staticTexts["Valid"].waitForExistence(timeout: 10))
        XCTAssertFalse(application.staticTexts["Alice"].exists)
        XCTAssertFalse(application.staticTexts["Example"].exists)

        let home = application.buttons["Home"]
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        openPassportVault(in: application)
        XCTAssertTrue(
            application.staticTexts["Owner-private saved conformance ledger · survives app restart · no on-chain transaction submitted"]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(
            application.staticTexts["Simulated — runs locally, nothing on Midnight"].waitForExistence(timeout: 5)
        )
        let readContractState = application.buttons["Read contract state"]
        scrollTo(readContractState, in: application)
        readContractState.tap()
        application.swipeUp()
        XCTAssertTrue(
            application.buttons["Refresh contract state"]
                .waitForExistence(timeout: 10)
        )
        let reviewContractCall = application.buttons["Review contract call"]
        scrollTo(reviewContractCall, in: application)
        reviewContractCall.tap()
        let authorizeContractCall = application.buttons["Authorize exact call"]
        XCTAssertTrue(authorizeContractCall.waitForExistence(timeout: 10))
        scrollTo(authorizeContractCall, in: application)
        authorizeContractCall.tap()
        let submitContractCall = application.buttons["Prove and submit"]
        XCTAssertTrue(submitContractCall.waitForExistence(timeout: 10))
        scrollTo(submitContractCall, in: application)
        submitContractCall.tap()
        XCTAssertTrue(
            application.staticTexts["Passport Vault call completed"]
                .waitForExistence(timeout: 15)
        )
        application.swipeDown()
        XCTAssertTrue(
            application.staticTexts["Mode: Simulated — runs locally, nothing on Midnight. Final DUST fee: 0.000000001 DUST."]
                .waitForExistence(timeout: 5)
        )
        let createLock = application.buttons["Create confirmed lock"]
        XCTAssertTrue(createLock.waitForExistence(timeout: 5))
        scrollTo(createLock, in: application)
        createLock.tap()
        XCTAssertTrue(application.staticTexts["100 NIGHT remaining"].waitForExistence(timeout: 5))
        let deposit = application.buttons["Deposit"]
        scrollTo(deposit, in: application)
        deposit.tap()
        XCTAssertTrue(application.staticTexts["110 NIGHT remaining"].waitForExistence(timeout: 5))
        let claim = application.buttons["Claim with credential"]
        scrollTo(claim, in: application)
        claim.tap()
        XCTAssertTrue(application.staticTexts["100 NIGHT remaining"].waitForExistence(timeout: 10))
        let withdraw = application.buttons["Withdraw"]
        scrollTo(withdraw, in: application)
        withdraw.tap()
        XCTAssertTrue(application.staticTexts["90 NIGHT remaining"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.staticTexts["Claims 1"].exists)

        application.terminate()
        application.launch()

        assertHomeComposition(in: application)
        application.buttons["Wallet"].tap()
        XCTAssertTrue(activateButton.waitForExistence(timeout: 30))
        activateButton.tap()
        XCTAssertTrue(application.staticTexts["Transfer included"].waitForExistence(timeout: 15))
        documents.tap()
        XCTAssertTrue(manageIdentities.waitForExistence(timeout: 5))
        manageIdentities.tap()
        XCTAssertTrue(application.staticTexts["standalone-fixture-v2"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
        credentials.tap()
        XCTAssertTrue(application.staticTexts["Digital Passport"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.buttons["Reveal First name locally"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.buttons["Preview disclosure plan"].exists)
        XCTAssertFalse(application.staticTexts["Alice"].exists)
        XCTAssertFalse(application.staticTexts["Example"].exists)
        XCTAssertTrue(application.buttons["Reverify"].waitForExistence(timeout: 5))
        openPassportVault(in: application)
        XCTAssertTrue(application.staticTexts["90 NIGHT remaining"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Claims 1"].waitForExistence(timeout: 5))
        XCTAssertTrue(
            application.staticTexts["Owner-private saved conformance ledger · survives app restart · no on-chain transaction submitted"]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(application.buttons["Home"].exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }

    @MainActor
    func testIdentityConsentCeremoniesInStandaloneMode() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)

        application.buttons["Wallet"].tap()
        let activateAccount = application.buttons["Activate protected Midnight account"]
        if activateAccount.waitForExistence(timeout: 2) {
            activateAccount.tap()
            XCTAssertTrue(
                application.buttons["Use my receive address"].waitForExistence(timeout: 15)
            )
        }
        application.buttons["Documents"].tap()
        let manageIdentities = application.buttons["Manage identities"]
        XCTAssertTrue(manageIdentities.waitForExistence(timeout: 10))
        manageIdentities.tap()
        if !application.descendants(matching: .any)["Manage this DID"]
            .waitForExistence(timeout: 2)
        {
            let createDid = application.buttons["Create standalone DID"]
            XCTAssertTrue(createDid.waitForExistence(timeout: 5))
            createDid.tap()
            XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
            XCTAssertTrue(
                application.descendants(matching: .any)["Manage this DID"]
                    .waitForExistence(timeout: 10)
            )
        }

        let demoLogin = application.buttons["Use standalone login request"]
        XCTAssertTrue(demoLogin.waitForExistence(timeout: 5))
        scrollTo(demoLogin, in: application)
        demoLogin.tap()
        let previewLogin = application.buttons["Preview login request"]
        scrollTo(previewLogin, in: application)
        previewLogin.tap()
        XCTAssertTrue(
            application.staticTexts["DID authentication preview"]
                .waitForExistence(timeout: 10)
        )
        for heading in [
            "Who is asking?", "What will you prove?", "Which identity?",
            "Why is it requested?", "Unverified endpoint",
        ] {
            XCTAssertTrue(application.staticTexts[heading].exists)
        }
        XCTAssertTrue(
            application.staticTexts[
                "Control of the selected managed DID. No credential or document claims will be disclosed."
            ].exists
        )
        let loginConsent = application.descendants(matching: .any)["Consent to DID authentication"]
        scrollTo(loginConsent, in: application)
        loginConsent.tap()
        let authenticate = application.buttons["Authenticate with DID"]
        scrollTo(authenticate, in: application)
        authenticate.tap()
        XCTAssertTrue(
            application.staticTexts[
                "DID authentication succeeded and the standalone verifier independently validated the proof."
            ].waitForExistence(timeout: 10)
        )

        application.buttons["Documents"].tap()
        let hadCredential = application.staticTexts["Valid"].waitForExistence(timeout: 2)
        let demoOffer = application.buttons["Use standalone demo offer"]
        XCTAssertTrue(demoOffer.waitForExistence(timeout: 5))
        scrollTo(demoOffer, in: application)
        demoOffer.tap()
        let previewOffer = application.buttons["Preview credential offer"]
        scrollTo(previewOffer, in: application)
        previewOffer.tap()
        XCTAssertTrue(
            application.staticTexts["Credential offer preview"].waitForExistence(timeout: 10)
        )
        for heading in [
            "Who is issuing it?", "What will you receive?",
            "Which identity receives it?", "Why add it?", "Unverified endpoint",
        ] {
            XCTAssertTrue(application.staticTexts[heading].exists)
        }
        if hadCredential {
            let refuseOffer = application.buttons["Refuse offer"]
            scrollTo(refuseOffer, in: application)
            refuseOffer.tap()
            XCTAssertTrue(
                application.staticTexts[
                    "Credential offer refused; ephemeral protocol secrets were discarded."
                ].waitForExistence(timeout: 10)
            )
        } else {
            let issuanceConsent = application.descendants(matching: .any)[
                "Consent to credential issuance"
            ]
            scrollTo(issuanceConsent, in: application)
            issuanceConsent.tap()
            let issueCredential = application.buttons["Accept and issue credential"]
            scrollTo(issueCredential, in: application)
            issueCredential.tap()
            XCTAssertTrue(
                application.staticTexts[
                    "Credential issued, verified, and stored in the protected inventory."
                ].waitForExistence(timeout: 10)
            )
        }

        let verifierRequest = application.buttons["Use standalone verifier request"]
        XCTAssertTrue(verifierRequest.waitForExistence(timeout: 5))
        scrollTo(verifierRequest, in: application)
        verifierRequest.tap()
        let previewPresentation = application.buttons["Preview presentation request"]
        scrollTo(previewPresentation, in: application)
        previewPresentation.tap()
        XCTAssertTrue(
            application.staticTexts["Presentation preview"].waitForExistence(timeout: 10)
        )
        for heading in [
            "Who is asking?", "What will be shared?", "Which document?",
            "Why is it requested?", "Unverified endpoint",
        ] {
            XCTAssertTrue(application.staticTexts[heading].exists)
        }
        XCTAssertTrue(
            application.staticTexts[
                "Confirms you're over 18. Your date of birth will not be shared."
            ].exists
        )
        let ageClaim = application.descendants(matching: .any)["Age over 18, required"]
        XCTAssertTrue(ageClaim.exists)
        XCTAssertFalse(ageClaim.isEnabled)
        let presentationConsent = application.descendants(matching: .any)[
            "Consent to credential presentation"
        ]
        if !presentationConsent.isEnabled {
            let matchingCredential = application.descendants(matching: .any).matching(
                NSPredicate(format: "label BEGINSWITH %@", "Use Digital Passport issued by")
            ).firstMatch
            scrollTo(matchingCredential, in: application)
            matchingCredential.tap()
        }
        scrollTo(presentationConsent, in: application)
        presentationConsent.tap()
        let shareProof = application.buttons["Share proof"]
        scrollTo(shareProof, in: application)
        shareProof.tap()
        XCTAssertTrue(
            application.staticTexts[
                "The holder authorized this exact presentation, but Compact proving is unavailable. No presentation or vp_token was generated."
            ].waitForExistence(timeout: 10)
        )
    }

    @MainActor
    func testSimulatorScannerFailsClosedWithoutImportingARequest() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createButton = application.buttons["Create and continue"]
        if createButton.waitForExistence(timeout: 5) {
            createButton.tap()
        }

        let scanIdentityRequest = application.buttons["Scan"]
        XCTAssertTrue(scanIdentityRequest.waitForExistence(timeout: 15))
        scanIdentityRequest.tap()
        XCTAssertTrue(
            application.staticTexts[
                "Camera scanning is unavailable here. Paste or load the request in the identity page instead."
            ].waitForExistence(timeout: 5)
        )
        XCTAssertFalse(
            application.staticTexts[
                "QR recognized as a credential offer. Review the request before consent."
            ].exists
        )
    }

    @MainActor
    func testAppLinksRouteColdAndWarmWithoutConsent() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)

        let offer = try XCTUnwrap(URL(
            string: "openid-credential-offer://?credential_offer=%7B%7D"
        ))
        application.open(offer)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a credential offer. Review the request before consent."
            ].waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
        XCTAssertTrue(application.staticTexts["Credentials"].exists)
        application.buttons["Dismiss identity request"].tap()

        let login = try XCTUnwrap(URL(string:
            "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier%2Frequest"
        ))
        application.open(login)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a DID login request. Review the request before consent."
            ].waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.staticTexts["Your DIDs"].exists)
        application.buttons["Dismiss identity request"].tap()

        application.terminate()
        application.open(offer)
        XCTAssertTrue(
            application.staticTexts[
                "App link recognized as a credential offer. Review the request before consent."
            ].waitForExistence(timeout: 15)
        )
        XCTAssertTrue(application.buttons["Dismiss identity request"].exists)
    }

    @MainActor
    func testNativePublicAddressCopyAndShare() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)
        application.buttons["Wallet"].tap()

        let activate = application.buttons["Activate protected Midnight account"]
        if activate.waitForExistence(timeout: 5) {
            activate.tap()
        }

        let copy = application.buttons["Copy Unshielded receive address"]
        XCTAssertTrue(copy.waitForExistence(timeout: 15))
        scrollTo(copy, in: application)
        copy.tap()
        XCTAssertTrue(
            application.staticTexts[
                "Public receive address copied to the native clipboard."
            ].waitForExistence(timeout: 5)
        )

        let share = application.buttons["Share Unshielded receive address"]
        XCTAssertTrue(share.exists)
        share.tap()
        XCTAssertTrue(
            application.otherElements["ActivityListView"].waitForExistence(timeout: 5)
        )
        let dismiss = application.otherElements["PopoverDismissRegion"]
        if dismiss.exists { dismiss.tap() }
    }

}
