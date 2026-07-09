// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerGit",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerGit", targets: ["TillerGit"])],
    targets: [
        .target(name: "TillerGit"),
        .testTarget(name: "TillerGitTests", dependencies: ["TillerGit"])
    ]
)
