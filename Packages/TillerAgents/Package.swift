// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerAgents",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerAgents", targets: ["TillerAgents"])],
    dependencies: [
        .package(path: "../TillerCore")
    ],
    targets: [
        .target(name: "TillerAgents", dependencies: ["TillerCore"]),
        .testTarget(name: "TillerAgentsTests", dependencies: ["TillerAgents"])
    ]
)
