// swift-tools-version:5.9
// SPDX-License-Identifier: Apache-2.0

import PackageDescription

let package = Package(
    name: "OxidMobilePlugin",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "OxidMobilePlugin", type: .static, targets: ["OxidMobilePlugin"])
    ],
    targets: [
        .target(
            name: "OxidMobilePlugin",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("LocalAuthentication"),
                .linkedFramework("Security"),
                .linkedFramework("UIKit"),
                .linkedFramework("UniformTypeIdentifiers")
            ]
        )
    ]
)
