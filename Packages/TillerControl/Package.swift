// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerControl",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "TillerControl", targets: ["TillerControl"]),
        .executable(name: "tillerctl", targets: ["tillerctl"]),
    ],
    dependencies: [
        .package(path: "../TillerCore"),
        .package(url: "https://github.com/apple/swift-argument-parser", from: "1.4.0"),
    ],
    targets: [
        .target(name: "TillerControl", dependencies: [.product(name: "TillerCore", package: "TillerCore")]),
        .executableTarget(
            name: "tillerctl",
            dependencies: [
                "TillerControl",
                .product(name: "ArgumentParser", package: "swift-argument-parser"),
            ]
        ),
        .testTarget(name: "TillerControlTests", dependencies: ["TillerControl"]),
    ]
)
