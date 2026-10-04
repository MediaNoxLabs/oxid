// SPDX-License-Identifier: Apache-2.0

import XCTest

enum WalletOnboardingFixture {
    /// Completes the reviewed recovery ceremony used by development-custody
    /// simulator compositions. Native-custody tests must exercise device
    /// authorization directly and must not call this fixture.
    @MainActor
    static func completeDevelopmentRecoveryCeremony(
        in application: XCUIApplication,
        timeout: TimeInterval = 120
    ) {
        let generate = application.buttons["Generate recovery phrase"]
        XCTAssertTrue(
            generate.waitForExistence(timeout: timeout),
            "development onboarding must expose the recovery ceremony"
        )
        XCTAssertFalse(
            application.buttons["Skip for now"].exists,
            "private-wallet onboarding must not expose the retired protection bypass"
        )
        generate.tap()

        XCTAssertTrue(
            application.otherElements["New wallet recovery phrase"]
                .waitForExistence(timeout: timeout),
            "development custody must produce a reviewable recovery phrase within the bounded root-preparation budget"
        )

        let acknowledgement = application.descendants(matching: .any)[
            "I have securely saved or verified this recovery phrase."
        ].firstMatch
        XCTAssertTrue(acknowledgement.waitForExistence(timeout: 10))
        for _ in 0..<12 where !acknowledgement.isHittable {
            application.swipeUp()
        }
        XCTAssertTrue(acknowledgement.isHittable)
        acknowledgement.tap()

        let finish = application.buttons["Finish and open wallet"]
        XCTAssertTrue(finish.waitForExistence(timeout: 10))
        for _ in 0..<12 where !finish.isHittable {
            application.swipeUp()
        }
        XCTAssertTrue(finish.isHittable)
        let deadline = Date().addingTimeInterval(10)
        while !finish.isEnabled && Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        }
        XCTAssertTrue(finish.isEnabled)
        finish.tap()
        XCTAssertTrue(application.buttons["Home"].waitForExistence(timeout: timeout))
    }
}
