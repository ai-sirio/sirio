import Testing
@testable import TillerCore

@Test func sidebarFilterMatching() {
    #expect(SidebarFilter.matches(projectName: "orca-mac", worktreeBranches: ["master"], query: ""))
    #expect(SidebarFilter.matches(projectName: "orca-mac", worktreeBranches: ["master"], query: "ORCA"))
    #expect(SidebarFilter.matches(projectName: "tiller", worktreeBranches: ["fase-2"], query: "fase"))
    #expect(SidebarFilter.matches(projectName: "tiller", worktreeBranches: ["fase-2"], query: "zzz") == false)
}
