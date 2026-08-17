// SPDX-License-Identifier: Apache-2.0

import XCTest

final class ProfileFlowTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    private func scrollTo(_ element: XCUIElement, in application: XCUIApplication) {
        for _ in 0..<20 where !element.isHittable {
            application.swipeUp()
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
    func testCreatesProfileAndCompletesStandaloneWalletFlow() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        ensureProfile(in: application)

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
        // The iOS 26 simulator keyboard drops the decimal separator from
        // `typeText("1.5")`; use an exact whole NIGHT for this interaction
        // smoke while Rust unit tests retain fractional conversion coverage.
        amount.typeText("1")
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
        if cancelSubmission.waitForExistence(timeout: 5) {
            cancelSubmission.tap()
            let retrySubmission = application.buttons["Retry safe submission"]
            XCTAssertTrue(retrySubmission.waitForExistence(timeout: 5))
            retrySubmission.tap()
            XCTAssertTrue(submit.waitForExistence(timeout: 5))
            submit.tap()
        }
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
        let demoLogin = application.buttons["Use standalone login request"]
        XCTAssertTrue(demoLogin.waitForExistence(timeout: 5))
        scrollTo(demoLogin, in: application)
        demoLogin.tap()
        let previewLogin = application.buttons["Preview login request"]
        scrollTo(previewLogin, in: application)
        previewLogin.tap()
        XCTAssertTrue(application.staticTexts["DID authentication preview"].waitForExistence(timeout: 10))
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

        let credentials = application.buttons["Credentials"]
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
        let verifierRequest = application.buttons["Use standalone verifier request"]
        XCTAssertTrue(verifierRequest.waitForExistence(timeout: 5))
        scrollTo(verifierRequest, in: application)
        verifierRequest.tap()
        let previewPresentation = application.buttons["Preview presentation request"]
        scrollTo(previewPresentation, in: application)
        previewPresentation.tap()
        XCTAssertTrue(application.staticTexts["Presentation preview"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Requested claims"].exists)
        XCTAssertTrue(
            application.staticTexts["No presentation or vp_token has been generated."]
                .waitForExistence(timeout: 5)
        )
        let presentationConsent = application.descendants(matching: .any)["Consent to credential presentation"]
        XCTAssertTrue(presentationConsent.waitForExistence(timeout: 5))
        scrollTo(presentationConsent, in: application)
        presentationConsent.tap()
        let presentCredential = application.buttons["Consent and present"]
        scrollTo(presentCredential, in: application)
        presentCredential.tap()
        XCTAssertTrue(
            application.staticTexts["The holder authorized this exact presentation, but Compact proving is unavailable. No presentation or vp_token was generated."]
                .waitForExistence(timeout: 10)
        )
        XCTAssertTrue(application.staticTexts["valid"].waitForExistence(timeout: 10))
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
            application.staticTexts["local preview ready · local preview only · no presentation generated"]
                .waitForExistence(timeout: 5)
        )
        let reverify = application.buttons["Reverify"]
        scrollTo(reverify, in: application)
        reverify.tap()
        XCTAssertTrue(reverify.waitForExistence(timeout: 10))

        let vault = application.buttons["Vault"]
        XCTAssertTrue(vault.waitForExistence(timeout: 5))
        vault.tap()
        XCTAssertTrue(
            application.staticTexts["Owner-private durable conformance ledger · survives app restart · no on-chain transaction submitted"]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(
            application.staticTexts["Deterministic simulation"].waitForExistence(timeout: 5)
        )
        let readContractState = application.buttons["Read contract state"]
        scrollTo(readContractState, in: application)
        readContractState.tap()
        application.swipeUp()
        XCTAssertTrue(
            application.buttons["Refresh simulated contract state"]
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
            application.staticTexts["Mode: simulated · deterministic simulation only. Final DUST fee: 1000000 base units."]
                .waitForExistence(timeout: 5)
        )
        let createLock = application.buttons["Create confirmed lock"]
        XCTAssertTrue(createLock.waitForExistence(timeout: 5))
        scrollTo(createLock, in: application)
        createLock.tap()
        XCTAssertTrue(application.staticTexts["100 base units remaining"].waitForExistence(timeout: 5))
        let deposit = application.buttons["Deposit"]
        scrollTo(deposit, in: application)
        deposit.tap()
        XCTAssertTrue(application.staticTexts["110 base units remaining"].waitForExistence(timeout: 5))
        let claim = application.buttons["Claim with credential"]
        scrollTo(claim, in: application)
        claim.tap()
        XCTAssertTrue(application.staticTexts["100 base units remaining"].waitForExistence(timeout: 10))
        let withdraw = application.buttons["Withdraw"]
        scrollTo(withdraw, in: application)
        withdraw.tap()
        XCTAssertTrue(application.staticTexts["90 base units remaining"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.staticTexts["Claims 1"].exists)

        application.terminate()
        application.launch()

        XCTAssertTrue(activateButton.waitForExistence(timeout: 15))
        XCTAssertTrue(application.staticTexts["Transfer included"].waitForExistence(timeout: 15))
        dids.tap()
        XCTAssertTrue(application.staticTexts["standalone-fixture-v2"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["standalone-1"].waitForExistence(timeout: 10))
        credentials.tap()
        XCTAssertTrue(application.staticTexts["Digital Passport"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.buttons["Reveal First name locally"].waitForExistence(timeout: 5))
        XCTAssertTrue(application.buttons["Preview disclosure plan"].exists)
        XCTAssertFalse(application.staticTexts["Alice"].exists)
        XCTAssertFalse(application.staticTexts["Example"].exists)
        XCTAssertTrue(application.buttons["Reverify"].waitForExistence(timeout: 5))
        vault.tap()
        XCTAssertTrue(application.staticTexts["90 base units remaining"].waitForExistence(timeout: 10))
        XCTAssertTrue(application.staticTexts["Claims 1"].waitForExistence(timeout: 5))
        XCTAssertTrue(
            application.staticTexts["Owner-private durable conformance ledger · survives app restart · no on-chain transaction submitted"]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(application.buttons["Assets"].exists)
        XCTAssertFalse(application.buttons["Create and continue"].exists)
    }

    @MainActor
    func testSimulatorScannerFailsClosedWithoutImportingARequest() throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createButton = application.buttons["Create and continue"]
        if createButton.waitForExistence(timeout: 5) {
            createButton.tap()
        }

        let scanIdentityRequest = application.buttons["Scan identity QR code"]
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
                "App link recognized as a DID login. Review the request before consent."
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
