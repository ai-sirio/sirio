// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerCode",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerCode", targets: ["TillerCode"])],
    dependencies: [
        .package(path: "../TillerCore"),
        .package(
            url: "https://github.com/CodeEditApp/CodeEditLanguages.git",
            exact: "0.1.20"),
        .package(
            url: "https://github.com/ChimeHQ/SwiftTreeSitter.git",
            from: "0.9.0")
    ],
    targets: [
        .target(name: "TillerCode", dependencies: [
            "TillerCore", "CodeEditLanguages", "SwiftTreeSitter"
        ]),
        .testTarget(name: "TillerCodeTests", dependencies: [
            "TillerCode", "CodeEditLanguages"
        ])
    ]
)
