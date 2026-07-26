import Foundation

struct RightPanelFixture: Equatable, Sendable {
    let changedPaths: [String]

    static func make() -> Self {
        var paths: [String] = []
        paths.reserveCapacity(1_200)
        for index in 0..<1_200 {
            let module = index % 12
            let feature = (index / 12) % 10
            let component = (index / 120) % 8
            let path = "Sources/module\(module)/feature\(feature)/component\(component)/file\(index % 300).swift"
            paths.append(path)
            if index.isMultiple(of: 97) {
                paths.append(path)
            }
        }
        return Self(changedPaths: paths)
    }
}
