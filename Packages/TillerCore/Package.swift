// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerCore",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerCore", targets: ["TillerCore"])],
    dependencies: [
        .package(path: "../TillerPersistence")
    ],
    targets: [
        .target(name: "TillerCore", dependencies: ["TillerPersistence"]),
        .testTarget(name: "TillerCoreTests", dependencies: ["TillerCore", "TillerPersistence"])
    ]
)
