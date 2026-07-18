// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerACP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerACP", targets: ["TillerACP"])],
    dependencies: [
        .package(path: "../TillerPersistence")
    ],
    targets: [
        .target(name: "TillerACP", dependencies: ["TillerPersistence"]),
        .testTarget(name: "TillerACPTests", dependencies: ["TillerACP", "TillerPersistence"])
    ]
)
