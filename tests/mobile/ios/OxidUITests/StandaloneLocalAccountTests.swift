// SPDX-License-Identifier: Apache-2.0

import Foundation
import XCTest

final class StandaloneLocalAccountTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testSynchronizesProtectedAccountFromLocalStandaloneStack() async throws {
        let application = XCUIApplication(bundleIdentifier: "io.medianox.oxid")
        application.launch()

        let createWallet = application.buttons["Create private wallet"]
        XCTAssertTrue(createWallet.waitForExistence(timeout: 15))
        createWallet.tap()
        application.buttons["Create and continue"].tap()
        WalletOnboardingFixture.completeDevelopmentRecoveryCeremony(in: application, timeout: 90)

        XCTAssertTrue(application.buttons["Wallet"].waitForExistence(timeout: 15))
        application.buttons["Wallet"].tap()
        let activate = application.buttons["Activate protected Midnight account"]
        if activate.waitForExistence(timeout: 3) {
            activate.tap()
        }

        XCTAssertTrue(
            application.buttons["Use my receive address"].waitForExistence(timeout: 90)
        )
        let liveAccount = application.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] %@", "Live source")
        ).firstMatch
        XCTAssertTrue(
            liveAccount.waitForExistence(timeout: 90),
            "Protected account activation did not converge to a live Midnight source."
        )
        XCTAssertTrue(application.buttons["Copy Unshielded receive address"].exists)
        XCTAssertTrue(application.buttons["Copy Shielded receive address"].exists)

        application.buttons["Home"].tap()
        let receive = application.buttons["Receive"]
        XCTAssertTrue(receive.waitForExistence(timeout: 10))
        receive.tap()
        XCTAssertTrue(application.staticTexts["Receive assets"].waitForExistence(timeout: 10))

        let addressPrefix = "Full validated raw Unshielded receive address "
        let addressElement = application.descendants(matching: .any)
            .matching(NSPredicate(format: "label BEGINSWITH %@", addressPrefix))
            .firstMatch
        XCTAssertTrue(addressElement.waitForExistence(timeout: 15))
        let address = String(addressElement.label.dropFirst(addressPrefix.count))
        XCTAssertTrue(address.hasPrefix("mn_addr_undeployed1"))

        try await Self.requestFixedGrant(for: address)
        XCTAssertTrue(
            application.staticTexts["Transfer confirmed"].waitForExistence(timeout: 180),
            "The bounded receive watcher did not observe the finalized NIGHT grant."
        )

        application.buttons["Close Receive"].firstMatch.tap()
        application.buttons["Wallet"].tap()

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
        XCTAssertTrue(application.staticTexts["50000"].waitForExistence(timeout: 30))

        XCTAssertFalse(
            application.staticTexts["Simulated — runs locally, nothing on Midnight"].exists
        )
        XCTAssertFalse(application.staticTexts["12 DUST"].exists)
        XCTAssertFalse(application.staticTexts["1 shielded notes"].exists)
        XCTAssertFalse(application.staticTexts["5 NIGHT"].exists)
        XCTAssertFalse(
            application.staticTexts["Account state could not be loaded safely."].exists
        )
    }

    private static func requestFixedGrant(for address: String) async throws {
        var request = URLRequest(url: URL(string: "http://127.0.0.1:36301/fund")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: [
            "requestId": "ios-standalone-\(UUID().uuidString.lowercased())",
            "recipientAddress": address,
        ])

        let (data, response) = try await URLSession.shared.data(for: request)
        let http = try XCTUnwrap(response as? HTTPURLResponse)
        guard http.statusCode == 200 else {
            throw NSError(
                domain: "OxidUITests.StandaloneFaucet",
                code: http.statusCode,
                userInfo: [NSLocalizedDescriptionKey: String(decoding: data, as: UTF8.self)]
            )
        }
        let body = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        XCTAssertNotNil(body["result"], String(decoding: data, as: UTF8.self))
    }
}
