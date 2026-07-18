// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerACP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerACP", targets: ["TillerACP"])],
    targets: [
        .target(name: "TillerACP"),
        .testTarget(name: "TillerACPTests", dependencies: ["TillerACP"])
    ]
)
