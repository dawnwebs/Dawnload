// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "Dawnload",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "Dawnload", targets: ["Dawnload"])
    ],
    targets: [
        .target(name: "DawnloadCore"),
        .executableTarget(
            name: "Dawnload",
            dependencies: ["DawnloadCore"]
        ),
        .testTarget(
            name: "DawnloadCoreTests",
            dependencies: ["DawnloadCore"]
        )
    ]
)
