import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct PackageSmokeTests {
    @Test
    func fakeHostSatisfiesTheContentSeam() {
        let host = FakeContentHost(tabID: WorkspaceTabID())

        #expect(host.fulfill(.focusTab(host.tabID)))
    }
}
