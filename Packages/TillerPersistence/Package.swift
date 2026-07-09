// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerPersistence",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerPersistence", targets: ["TillerPersistence"])],
    dependencies: [
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.0.0")
    ],
    targets: [
        .target(
            name: "TillerPersistence",
            dependencies: [.product(name: "GRDB", package: "GRDB.swift")]
        ),
        .testTarget(name: "TillerPersistenceTests", dependencies: ["TillerPersistence"])
    ]
)
