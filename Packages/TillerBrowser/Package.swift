// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerBrowser",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerBrowser", targets: ["TillerBrowser"])],
    targets: [
        .target(name: "TillerBrowser"),
        .testTarget(name: "TillerBrowserTests", dependencies: ["TillerBrowser"])
    ]
)
