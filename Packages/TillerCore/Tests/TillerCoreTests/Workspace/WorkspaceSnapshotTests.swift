import Foundation
import Testing
@testable import TillerCore

@Suite struct WorkspaceSnapshotTests {
    @Test func canonicalPayloadIsByteStableAcrossEncodes() throws {
        let layout = Fixtures.deepMixedOrientationLayout(groups: 8, tabs: 20)
        let snapshot = WorkspaceSnapshot(layout: layout)
        let a = try snapshot.canonicalPayload()
        let b = try snapshot.canonicalPayload()

        #expect(a == b)
    }

    @Test func roundTripPreservesTopologyIDsAndSelections() throws {
        let layout = Fixtures.deepMixedOrientationLayout(groups: 4, tabs: 3)
        let snapshot = WorkspaceSnapshot(layout: layout)
        let payload = try snapshot.canonicalPayload()

        guard case .success(let decoded) = WorkspaceSnapshot.decode(payload) else {
            Issue.record("canonical snapshot should decode")
            return
        }
        #expect(decoded == snapshot)

        guard case .success(let materialized) = decoded.materialize() else {
            Issue.record("decoded snapshot should materialize")
            return
        }
        #expect(materialized.root == layout.root)
        #expect(materialized.orderedGroupIDs == layout.orderedGroupIDs)
        #expect(materialized.activeGroupID == layout.activeGroupID)
        for groupID in layout.orderedGroupIDs {
            #expect(materialized.group(groupID)?.tabs.map(\.id)
                    == layout.group(groupID)?.tabs.map(\.id))
            #expect(materialized.group(groupID)?.activeTabID
                    == layout.group(groupID)?.activeTabID)
        }
    }

    @Test func snapshotExcludesRuntimeOnlyState() throws {
        let layout = Fixtures.singleTerminalTab().layout
        let payload = try WorkspaceSnapshot(layout: layout).canonicalPayload()
        let json = try #require(String(data: payload, encoding: .utf8))

        for forbidden in ["firstResponder", "generation", "hover", "mountPhase",
                          "effectiveFraction"] {
            #expect(!json.contains(forbidden))
        }
    }

    @Test func unknownFutureVersionIsNotClassifiedAsCorruption() throws {
        let layout = Fixtures.singleTerminalTab().layout
        let snapshot = WorkspaceSnapshot(layout: layout)
        let payload = try snapshot.canonicalPayload()

        #expect(WorkspaceSnapshotUpgrader.upgrade(payload, from: 999)
                == .failure(.unsupportedFutureVersion(999)))
    }

    @Test func decodingAMalformedPayloadNeverYieldsAPartialLayout() {
        let malformed = Data(#"{"schemaVersion":1,"groups":[]}"#.utf8)

        guard case .failure = WorkspaceSnapshot.decode(malformed) else {
            Issue.record("malformed snapshot must not decode into a partial layout")
            return
        }
        guard case .failure(.malformed) = WorkspaceSnapshotUpgrader.upgrade(malformed, from: 1) else {
            Issue.record("malformed snapshot must be reported as malformed")
            return
        }
    }
}
