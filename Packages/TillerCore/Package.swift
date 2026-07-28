// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerCore",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerCore", targets: ["TillerCore"])],
    dependencies: [
        .package(path: "../TillerPersistence"),
        // ProjectStore.swift imports GRDB directly. Relying on TillerPersistence to
        // re-export it works for `swift build`, but when Xcode builds TillerCore as a
        // dynamic PackageFramework the link edge must be explicit, otherwise every
        // GRDB symbol comes back undefined at Ld time.
        .package(url: "https://github.com/groue/GRDB.swift.git", from: "7.0.0")
    ],
    targets: [
        .target(name: "TillerCore", dependencies: [
            "TillerPersistence",
            .product(name: "GRDB", package: "GRDB.swift")
        ]),
        .testTarget(name: "TillerCoreTests", dependencies: ["TillerCore", "TillerPersistence"])
    ]
)
