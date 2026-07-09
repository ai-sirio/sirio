import Testing
import Foundation
@testable import TillerCore

@Test func defaultProjectsRootIsTillerProjectsUnderHome() {
    #expect(ProjectDefaults.defaultProjectsRoot(home: "/Users/test") == "/Users/test/Tiller/projects")
}
