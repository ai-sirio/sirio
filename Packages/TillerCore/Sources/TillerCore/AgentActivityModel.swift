import Foundation
import Observation

/// Pure, observable state machine for agent lifecycle status in Tiller.
/// Owns the four agent-activity dictionaries that AppModel previously
/// managed inline, and provides methods for each signal source.
///
/// ## Two-layer evidence model
/// - **Layer A** (explicit): `tillerctl notify` hook pushes, recorded with
///   a wall-clock timestamp for debouncing Layer B.
/// - **Layer B** (derived): terminal-OSC-title-based status detection,
///   applied only when no recent Layer-A signal is authoritative.
///
/// ## Process-exit fallback
/// `applyExitResult` maps a real `waitpid` exit code to done/error — this
/// is the only path to `.done` for agents without native hooks (Codex).
///
/// ## Pane ownership
/// - **Spawn-owned**: panes created through `spawnAgent` (Tiller's own menu).
///   Their exit signal comes solely from `watchExit`.
/// - **Title-owned**: panes discovered by `handleTitleChange` identifying
///   an agent from its title text. They are cleared when the title stops
///   matching any agent.
///
/// Replaces: `AppModel.agentStatus`, `lastHookUpdateAt`, `paneAgents`,
/// `titleOwnedPanes`, and their inline mutation logic.
@Observable
public final class AgentActivityModel {
    /// Per-pane lifecycle status (drives sidebar badge).
    public var agentStatus: [UUID: AgentStatus] = [:]

    /// Wall-clock time of the most recent Layer-A (hook) status write per
    /// pane — used to debounce Layer-B (title) signals.
    public var lastHookUpdateAt: [UUID: Date] = [:]

    /// Adapter id per agent-spawned or title-identified pane.
    public var paneAgents: [UUID: String] = [:]

    /// Panes whose `paneAgents` entry was backfilled by title-identification
    /// rather than by `spawnAgent`. Only these panes get cleared when their
    /// title stops matching any agent.
    public var titleOwnedPanes: Set<UUID> = []

    /// Panes whose `paneAgents` entry was backfilled by foreground-process
    /// identification (Layer D). Cleared only by `processGone` — never by a
    /// title mismatch, because these agents may emit no title at all (Codex).
    public var processOwnedPanes: Set<UUID> = []

    public init() {}

    // MARK: - Layer A: explicit hook push

    /// Apply a `tillerctl notify` (Layer A) hook signal.
    /// Records the wall-clock timestamp so Layer-B signals are debounced.
    /// Returns the transition so the caller can decide on notifications.
    ///
    /// Replaces: `AppModel.handleControl` `"notify"` case.
    /// Layer: A (explicit).
    @discardableResult
    public func notify(paneId: UUID, status: AgentStatus, now: Date) -> AgentTransition {
        let old = agentStatus[paneId]
        agentStatus[paneId] = status
        lastHookUpdateAt[paneId] = now
        return AgentTransition(paneId: paneId, old: old, new: status)
    }

    // MARK: - Spawn

    /// Register a newly spawned agent pane and set its initial status to
    /// `.running`. Does NOT return a transition — running at spawn time is
    /// expected and never triggers a notification.
    ///
    /// Replaces: the status-portion of `AppModel.spawnAgent`.
    /// Layer: A (explicit — the spawn itself is Layer-A-relevant).
    public func agentSpawned(paneId: UUID, agentId: String, now: Date) {
        paneAgents[paneId] = agentId
        agentStatus[paneId] = .running
        lastHookUpdateAt[paneId] = now
    }

    /// Agent id spawned in the given pane, or nil for plain shells.
    public func agentId(paneId: UUID) -> String? { paneAgents[paneId] }

    /// Registers a restored pane's agent identity without claiming a status.
    /// Unlike `agentSpawned`, does NOT set `.running` — a restored chat tab
    /// may be idle, and the real status arrives later via `notify`. Without
    /// this, restored chat tabs never satisfy `AgentTreeBuilder`'s
    /// `paneAgents` guard and silently drop out of the Agents panel.
    public func registerAgentId(paneId: UUID, agentId: String) {
        paneAgents[paneId] = agentId
    }

    // MARK: - Process exit

    /// Map a process exit code to `.done` (code 0) or `.error` (non-zero).
    /// Returns nil when the pane is no longer tracked (already cleaned up
    /// by a prior close).
    ///
    /// Replaces: the status-portion of `AppModel.watchExit`.
    /// Layer: process-exit fallback.
    @discardableResult
    public func applyExitResult(paneId: UUID, exitCode: Int32, now: Date) -> AgentTransition? {
        guard agentStatus[paneId] != nil else { return nil }
        let old = agentStatus[paneId]
        let new: AgentStatus = exitCode == 0 ? .done : .error
        agentStatus[paneId] = new
        return AgentTransition(paneId: paneId, old: old, new: new)
    }

    // MARK: - Layer B: title change

    /// Handle a terminal title change (Layer B). For an already-known pane,
    /// derives status from title text and applies it unless a more recent
    /// Layer-A push is still authoritative. For an unregistered pane, tries
    /// to identify the agent from the title text.
    ///
    /// Returns a transition when the status changed meaningfully, or nil
    /// when the title carries no recognisable status opinion or a recent
    /// hook signal overrides it.
    ///
    /// Replaces: `AppModel.handleTitleChange` entirely.
    /// Layer: B (title-derived), deferred to after Layer A debounce.
    @discardableResult
    public func handleTitleChange(paneId: UUID, title: String, now: Date) -> AgentTransition? {
        // --- Unregistered pane: try to identify the agent from title text ---
        guard let agentId = paneAgents[paneId] else {
            guard let identified = AgentTitleIdentity.identify(title: title) else { return nil }
            paneAgents[paneId] = identified
            titleOwnedPanes.insert(paneId)
            let status = AgentTitleStatus.detect(title: title, agentId: identified) ?? .running
            agentStatus[paneId] = status
            return AgentTransition(paneId: paneId, old: nil, new: status)
        }

        // --- Known pane: detect status from title, or clear if unmatched ---
        guard let status = AgentTitleStatus.detect(title: title, agentId: agentId) else {
            // Title no longer matches this agent's conventions at all.
            // Only clear title-owned panes; spawn-owned panes rely on
            // watchExit and must survive a transient nil classification.
            if titleOwnedPanes.contains(paneId) {
                agentStatus[paneId] = nil
                paneAgents[paneId] = nil
                titleOwnedPanes.remove(paneId)
            }
            return nil
        }

        // --- Debounce: skip if a recent Layer-A push is authoritative ---
        guard AgentSignalMerger.shouldApplyTitleSignal(
            lastHookUpdateAt: lastHookUpdateAt[paneId], now: now
        ) else { return nil }

        let old = agentStatus[paneId]
        agentStatus[paneId] = status
        return AgentTransition(paneId: paneId, old: old, new: status)
    }

    // MARK: - Layer C: content signal

    /// Apply a content-derived (Layer C) status signal — matched against
    /// live pane buffer text via `ScreenManifest`. Unlike Layer B, this is
    /// NOT gated by `AgentSignalMerger.shouldApplyTitleSignal`: a genuine
    /// content match is closer to ground truth than a title convention and
    /// may correct a stale Layer A/B status. A subsequent real hook
    /// (`notify`) still overwrites it unconditionally, so Layer C can never
    /// permanently lock a stale status.
    ///
    /// Replaces: nothing pre-existing — this is a new evidence source.
    /// Layer: C (content-derived), no debounce gate.
    @discardableResult
    public func applyContentSignal(paneId: UUID, status: AgentStatus, now: Date) -> AgentTransition? {
        guard paneAgents[paneId] != nil else { return nil }
        let old = agentStatus[paneId]
        guard old != status else { return nil }
        agentStatus[paneId] = status
        lastHookUpdateAt[paneId] = now
        return AgentTransition(paneId: paneId, old: old, new: status)
    }

    // MARK: - Layer D: foreground-process identification

    /// Register a pane whose shell has a live foreground agent process
    /// (Layer D) — the only signal that catches agents which never emit an
    /// OSC title (Codex). Ignores panes that are already registered by any
    /// other layer: process evidence only says "alive", so it must never
    /// downgrade a richer status.
    ///
    /// Replaces: nothing pre-existing — new evidence source.
    /// Layer: D (process-derived).
    @discardableResult
    public func processIdentified(paneId: UUID, agentId: String, now: Date) -> AgentTransition? {
        guard paneAgents[paneId] == nil else { return nil }
        paneAgents[paneId] = agentId
        agentStatus[paneId] = .running
        processOwnedPanes.insert(paneId)
        return AgentTransition(paneId: paneId, old: nil, new: .running)
    }

    /// The foreground agent process disappeared: the pane is back to a
    /// plain shell. Clears process-owned panes only — spawn- and
    /// title-owned panes have their own exit/clearing paths.
    public func processGone(paneId: UUID) {
        guard processOwnedPanes.contains(paneId) else { return }
        agentStatus[paneId] = nil
        paneAgents[paneId] = nil
        processOwnedPanes.remove(paneId)
    }

    // MARK: - Pane closed

    /// Remove all state for a closed pane. Called when a terminal pane's
    /// onDisappear fires (user closes the tab or the pane exits).
    ///
    /// Replaces: the per-dictionary cleanup in `ContentView.onClose`.
    public func paneClosed(paneId: UUID) {
        agentStatus[paneId] = nil
        lastHookUpdateAt[paneId] = nil
        paneAgents[paneId] = nil
        titleOwnedPanes.remove(paneId)
        processOwnedPanes.remove(paneId)
    }

    // MARK: - Queries

    /// Highest-priority agent status among the given pane ids.
    /// Priority: error > needs-input > running > done.
    /// Returns nil if no agent panes are in the collection.
    ///
    /// Replaces: `AppModel.statusForWorktree`.
    public func statusForWorktree(paneIds: some Collection<UUID>) -> AgentStatus? {
        let statuses = paneIds.compactMap { agentStatus[$0] }
        return AgentStatus.highestPriority(in: statuses)
    }

    /// Agent id of the most relevant pane among the given pane ids (same
    /// priority order as `statusForWorktree`). Returns nil if no agent
    /// panes are in the collection.
    ///
    /// Replaces: `AppModel.agentIdForWorktree`.
    public func agentIdForWorktree(paneIds: some Collection<UUID>) -> String? {
        for wanted in [AgentStatus.error, .needsInput, .running, .done] {
            if let pane = paneIds.first(where: { agentStatus[$0] == wanted }) {
                return paneAgents[pane]
            }
        }
        return paneIds.compactMap { paneAgents[$0] }.first
    }

    /// Distinct agent ids currently `.running` among the given pane ids,
    /// filtered and ordered by `catalogIds` for stable left-to-right icon
    /// order in the worktree row's trailing running-agents badge.
    ///
    /// Replaces: `AppModel.runningAgentIds`.
    public func runningAgentIds(paneIds: some Collection<UUID>, catalogIds: [String]) -> [String] {
        let running = Set(paneIds.filter { agentStatus[$0] == .running }.compactMap { paneAgents[$0] })
        return catalogIds.filter(running.contains)
    }

    // MARK: - Notification payload

    /// Build notification content for a status transition in a pane.
    /// Requires the caller to resolve worktree and project context (which
    /// is outside TillerCore's scope — it lives in AppModel).
    ///
    /// Returns nil when the pane has no registered agent (context gone).
    ///
    /// Replaces: `AppModel.buildPayload`.
    public func buildPayload(
        paneId: UUID,
        status: AgentStatus,
        agentDisplayName: String,
        worktreeId: UUID,
        worktreeBranch: String,
        projectName: String?,
        worktreeComment: String?
    ) -> NotificationPayload? {
        guard paneAgents[paneId] != nil else { return nil }
        var body = worktreeBranch
        if let projectName {
            body += " · \(projectName)"
        }
        if let comment = worktreeComment?.trimmingCharacters(in: .whitespaces), !comment.isEmpty {
            body += "  ·  \(comment)"
        }
        return NotificationPayload(
            paneId: paneId,
            worktreeId: worktreeId,
            title: "\(agentDisplayName) — \(status.humanLabel)",
            body: body
        )
    }
}
