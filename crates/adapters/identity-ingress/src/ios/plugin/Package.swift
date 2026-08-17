// swift-tools-version:5.9
// SPDX-License-Identifier: Apache-2.0

import PackageDescription

let package = Package(
    name: "OxidQrScannerPlugin",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "OxidQrScannerPlugin", type: .static, targets: ["OxidQrScannerPlugin"])
    ],
    targets: [
        .target(
            name: "OxidQrScannerPlugin",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("UIKit")
            ]
        )
    ]
)
