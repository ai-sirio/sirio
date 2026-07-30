// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerWorkspace",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerWorkspace", targets: ["TillerWorkspace"])],
    dependencies: [.package(path: "../TillerCore")],
    targets: [
        .target(name: "TillerWorkspace", dependencies: ["TillerCore"]),
        .testTarget(name: "TillerWorkspaceTests", dependencies: ["TillerWorkspace"])
    ]
)
