// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerTerminal",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerTerminal", targets: ["TillerTerminal"])],
    dependencies: [
        .package(url: "https://github.com/Lakr233/libghostty-spm.git", from: "1.2.0"),
        .package(path: "../TillerCore")
    ],
    targets: [
        .target(
            name: "TillerTerminal",
            dependencies: [
                .product(name: "GhosttyTerminal", package: "libghostty-spm"),
                "TillerCore"
            ]
        ),
        .testTarget(name: "TillerTerminalTests", dependencies: ["TillerTerminal"])
    ]
)
