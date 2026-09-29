// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "sound_flutter",
    platforms: [
        .iOS("13.0")
    ],
    products: [
        .library(
            name: "sound-flutter",
            targets: ["sound_flutter"]
        )
    ],
    dependencies: [
        .package(name: "FlutterFramework", path: "../FlutterFramework")
    ],
    targets: [
        .target(
            name: "sound_flutter",
            dependencies: [
                .product(name: "FlutterFramework", package: "FlutterFramework")
            ],
            path: "Sources/sound_flutter"
        )
    ]
)