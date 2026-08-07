import Foundation
import Testing
import TillerCore
@testable import Tiller

@Suite(.serialized)
struct WorkspaceMigrationV15Tests {
    @Test func newWorktreeWithNoLegacyRowsProducesTheEmptyRootLayout() throws {
        let fixture = try load("empty.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func validV15SinglePaneTerminalKeepsItsTabIdAndLeafUuid() throws {
        let fixture = try load("single-terminal.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func nestedV15SplitTreeBecomesTheTopologyWithDepthFirstLeafIdentities() throws {
        let fixture = try load("nested-split.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func mixedLegacyTabsFlattenIntoThePrimaryGroupInGlobalOrder() throws {
        let fixture = try load("mixed-tabs.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func chatRowWithNullSessionGetsAFreshChatContentId() throws {
        let fixture = try load("chat-null-session.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func missingFilesStayAsValidTabsInTheMissingPlaceholderState() throws {
        let fixture = try load("missing-document.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func duplicateDocumentIdentitiesKeepTheActiveTabAndReportTheRemoval() throws {
        let fixture = try load("duplicate-document.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func corruptTreeJsonIsSkippedAndReported() throws {
        let fixture = try load("corrupt-tree.json")
        let result = migrate(fixture)
        #expect(summary(result) == fixture.expected)
    }

    @Test func migrationIsPureAndDeterministicForTheSameInjectedIds() throws {
        let fixture = try load("deterministic.json")
        let first = migrate(fixture)
        let second = migrate(fixture)
        #expect(first.layout == second.layout)
        #expect(first.tabs == second.tabs)
        #expect(first.terminalContents == second.terminalContents)
        #expect(first.diagnostics == second.diagnostics)
        #expect(summary(first) == fixture.expected)
    }

    private struct Fixture: Decodable {
        let worktreeID: String
        let rows: [Row]
        let expected: Expected
    }

    private struct Row: Decodable {
        let id: String
        let worktreeID: String
        let title: String
        let orderIdx: Int
        let isActive: Bool
        let treeJSON: String
        let kind: String
        let filePath: String?
        let chatAgentId: String?
        let chatSessionId: String?
        let titleIsAutoNamed: Bool

        func legacy() -> WorkspaceMigrationV15.LegacyTabRow {
            WorkspaceMigrationV15.LegacyTabRow(
                id: UUID(uuidString: id)!, worktreeId: UUID(uuidString: worktreeID)!,
                title: title, orderIdx: orderIdx, isActive: isActive, treeJSON: treeJSON,
                kind: kind, filePath: filePath, chatAgentId: chatAgentId,
                chatSessionId: chatSessionId, titleIsAutoNamed: titleIsAutoNamed)
        }
    }

    private struct Expected: Equatable, Decodable {
        let root: String
        let orderedGroupIDs: [String]
        let activeGroupID: String
        let groups: [ExpectedGroup]
        let tabs: [ExpectedTab]
        let terminalContents: [ExpectedTerminalContent]
        let diagnostics: [String]
    }

    private struct ExpectedGroup: Equatable, Decodable {
        let id: String
        let tabIDs: [String]
        let activeTabID: String?
    }

    private struct ExpectedTab: Equatable, Decodable {
        let id: String
        let title: String
        let titleIsAutoNamed: Bool
        let contentKind: String
        let contentID: String
        let editor: String?
    }

    private struct ExpectedTerminalContent: Equatable, Decodable {
        let id: String
        let worktreeID: String
        let launchKind: String
        let commandJSON: String?
    }

    private func load(_ name: String, file: StaticString = #filePath) throws -> Fixture {
        let testURL = URL(fileURLWithPath: String(describing: file))
        let fixtureURL = testURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Fixtures/WorkspaceMigration")
            .appendingPathComponent(name)
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: fixtureURL))
    }

    private func migrate(_ fixture: Fixture) -> WorkspaceMigrationV15.MigrationResult {
        var tabIndex = 0
        var groupIndex = 0
        var splitIndex = 0
        var chatIndex = 0
        let worktreeID = UUID(uuidString: fixture.worktreeID)!
        return WorkspaceMigrationV15.migrate(
            rows: fixture.rows.map { $0.legacy() }, worktreeID: worktreeID,
            freshTabID: {
                tabIndex += 1
                return WorkspaceTabID(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", 100 + tabIndex))!)
            },
            freshGroupID: {
                groupIndex += 1
                return PaneGroupID(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", groupIndex))!)
            },
            freshSplitID: {
                splitIndex += 1
                return SplitID(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", splitIndex))!)
            },
            freshChatID: {
                chatIndex += 1
                return ChatContentID("fresh-chat-\(chatIndex)")
            },
            agentDisplayName: { id in
                ["codex": "Codex", "claude": "Claude"].first { $0.key == id }?.value
            })
    }

    private func summary(_ result: WorkspaceMigrationV15.MigrationResult) -> Expected {
        let root = describe(result.layout.root)
        let groups = result.layout.orderedGroupIDs.map { id in
            let group = result.layout.group(id)!
            return ExpectedGroup(
                id: id.rawValue.uuidString, tabIDs: group.tabs.map { $0.id.rawValue.uuidString },
                activeTabID: group.activeTabID?.rawValue.uuidString)
        }
        let tabs = result.layout.orderedGroupIDs.flatMap { result.layout.group($0)!.tabs }.map { tab in
            switch tab.content {
            case .terminal(let id):
                return ExpectedTab(id: tab.id.rawValue.uuidString, title: tab.title,
                                   titleIsAutoNamed: tab.titleIsAutoNamed, contentKind: "terminal",
                                   contentID: id.rawValue.uuidString, editor: nil)
            case .chat(let id):
                return ExpectedTab(id: tab.id.rawValue.uuidString, title: tab.title,
                                   titleIsAutoNamed: tab.titleIsAutoNamed, contentKind: "chat",
                                   contentID: id.rawValue, editor: nil)
            case .document(let id, editor: let editor):
                return ExpectedTab(id: tab.id.rawValue.uuidString, title: tab.title,
                                   titleIsAutoNamed: tab.titleIsAutoNamed, contentKind: "document",
                                   contentID: id.canonicalPath, editor: editor.rawValue)
            case .browser:
                preconditionFailure("v15 migration cannot produce browser content")
            }
        }
        let contents = result.terminalContents.map {
            let launchKind: String
            switch $0.launchKind {
            case .shell: launchKind = "shell"
            case .agent(let id): launchKind = "agent:\(id)"
            }
            return ExpectedTerminalContent(id: $0.id.rawValue.uuidString,
                                           worktreeID: $0.worktreeID.uuidString,
                                           launchKind: launchKind, commandJSON: $0.commandJSON)
        }
        return Expected(root: root, orderedGroupIDs: result.layout.orderedGroupIDs.map { $0.rawValue.uuidString },
                        activeGroupID: result.layout.activeGroupID.rawValue.uuidString,
                        groups: groups, tabs: tabs, terminalContents: contents,
                        diagnostics: result.diagnostics.map(diagnosticDescription))
    }

    private func describe(_ node: LayoutNode) -> String {
        switch node {
        case .group(let id): return "group:\(id.rawValue.uuidString)"
        case .split(let id, let axis, let fraction, let first, let second):
            return "split:\(id.rawValue.uuidString):\(axis.rawValue):\(fraction):[\(describe(first)),\(describe(second))]"
        }
    }

    private func diagnosticDescription(_ diagnostic: RestoreDiagnostic) -> String {
        switch diagnostic {
        case .quarantinedSnapshot(let reason): return "quarantinedSnapshot:\(reason)"
        case .removedMissingTab(let id): return "removedMissingTab:\(id.rawValue.uuidString)"
        case .repairedSelection(let id): return "repairedSelection:\(id.rawValue.uuidString)"
        case .unavailableContent(let id): return "unavailableContent:\(id.rawValue.uuidString)"
        case .importedRecoverySidecar(let revision): return "importedRecoverySidecar:\(revision)"
        case .futureSchemaVersion(let version): return "futureSchemaVersion:\(version)"
        }
    }
}
