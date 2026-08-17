use std::collections::{HashMap, HashSet};

use tiller_activity::{
    ActivityStatus, ActivityTab, ActivityTabKind, ActivityWorktreeInput, AgentActivityModel,
    AgentSessionRef, AgentSessionRestorePlan, AgentStatus, AttentionSort, BootstrapRestoreOrder,
    NotificationPolicy, TerminalContentId, WorktreeMountPolicy, build_activity_rows, strip_ansi,
};

#[test]
fn f_core_act_16_strips_csi_and_osc_before_content_matching() {
    let escaped = "\x1b[31mDo you want to proceed?\x1b[0m\x1b]0;agent\x07";
    assert_eq!(strip_ansi(escaped), "Do you want to proceed?");
    assert_eq!(
        tiller_activity::detect_content_status(escaped, "claude"),
        Some(AgentStatus::NeedsInput)
    );
}

#[test]
fn f_core_act_14_requires_word_boundaries_for_generic_title_identity_and_status() {
    assert_eq!(
        tiller_activity::identify_agent_from_title("opencode-experiment"),
        None
    );
    assert_eq!(
        tiller_activity::detect_status_from_title("codex-notes working", "codex"),
        None
    );
    assert_eq!(
        tiller_activity::detect_status_from_title("codex working", "codex"),
        Some(AgentStatus::Running)
    );
}

#[test]
fn f_core_act_19_builds_notification_payload_with_optional_context() {
    let mut model = AgentActivityModel::new();
    model.agent_spawned("pane-1", "claude", std::time::Instant::now());

    let payload = model
        .build_payload(
            "pane-1",
            AgentStatus::NeedsInput,
            "Claude Code",
            "worktree-1",
            "feature/login",
            Some("Tiller"),
            Some("  waiting for approval  "),
        )
        .expect("tracked pane has notification context");

    assert_eq!(payload.title, "Claude Code — needs input");
    assert_eq!(
        payload.body,
        "feature/login · Tiller  ·  waiting for approval"
    );
    assert_eq!(payload.pane_id, "pane-1");
    assert_eq!(payload.worktree_id, "worktree-1");
}

#[test]
fn f_core_act_20_notification_policy_suppresses_noise_and_visible_transitions() {
    assert!(!NotificationPolicy::should_notify(
        None,
        AgentStatus::Running,
        false,
        false
    ));
    assert!(!NotificationPolicy::should_notify(
        Some(AgentStatus::Done),
        AgentStatus::Done,
        false,
        false
    ));
    assert!(!NotificationPolicy::should_notify(
        Some(AgentStatus::Running),
        AgentStatus::NeedsInput,
        true,
        true
    ));
    assert!(NotificationPolicy::should_notify(
        Some(AgentStatus::Running),
        AgentStatus::NeedsInput,
        true,
        false
    ));
}

#[test]
fn f_core_act_21_activity_rows_keep_shells_and_chats_but_omit_non_activity_content() {
    let worktree = ActivityWorktreeInput::new(
        "worktree-1",
        "Tiller/main",
        vec![
            ActivityTab::terminal("terminal-tab", "zsh", ["pane-1"]),
            ActivityTab::chat("chat-tab", "Fix login"),
            ActivityTab::new("document", "README.md", ActivityTabKind::Document),
            ActivityTab::new("diff", "Changes", ActivityTabKind::Diff),
            ActivityTab::new("browser", "Browser", ActivityTabKind::Browser),
            ActivityTab::terminal("unmounted", "not live", Vec::<String>::new()),
        ],
    );
    let statuses = HashMap::from([(String::from("pane-1"), AgentStatus::Running)]);
    let agents = HashMap::from([(String::from("pane-1"), String::from("codex"))]);

    let rows = build_activity_rows(&[worktree], &statuses, &agents);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, tiller_activity::ActivityRowKind::Terminal);
    assert_eq!(rows[0].agent_id.as_deref(), Some("codex"));
    assert_eq!(rows[1].kind, tiller_activity::ActivityRowKind::Chat);
    assert_eq!(rows[1].status, ActivityStatus::Idle);
}

#[test]
fn f_core_act_22_attention_sort_is_stable_and_urgent_first_preserves_manual_nonurgent_order() {
    let items = vec![
        ("idle", None),
        ("done", Some(AgentStatus::Done)),
        ("running", Some(AgentStatus::Running)),
        ("needs", Some(AgentStatus::NeedsInput)),
        ("error", Some(AgentStatus::Error)),
    ];

    let sorted = AttentionSort::sorted(&items, |item| item.1);
    assert_eq!(
        sorted.iter().map(|item| item.0).collect::<Vec<_>>(),
        vec!["error", "needs", "running", "done", "idle"]
    );

    let urgent = AttentionSort::urgent_first(&items, |item| item.1);
    assert_eq!(
        urgent.iter().map(|item| item.0).collect::<Vec<_>>(),
        vec!["error", "needs", "idle", "done", "running"]
    );
}

#[test]
fn f_core_act_23_only_live_activity_statuses_require_close_confirmation() {
    assert!(ActivityStatus::Running.requires_close_confirmation());
    assert!(ActivityStatus::NeedsInput.requires_close_confirmation());
    assert!(ActivityStatus::Error.requires_close_confirmation());
    assert!(!ActivityStatus::Done.requires_close_confirmation());
    assert!(!ActivityStatus::Idle.requires_close_confirmation());
}

#[test]
fn f_core_act_24_restore_plan_keys_refs_by_stable_content_not_live_pane() {
    let alive = TerminalContentId::new("terminal-stable");
    let gone = TerminalContentId::new("terminal-gone");
    let refs = vec![
        AgentSessionRef::new(alive.clone(), "claude", "session-a"),
        AgentSessionRef::new(gone, "codex", "session-b"),
    ];

    let plan = AgentSessionRestorePlan::plan(&refs, &HashSet::from([alive.clone()]));

    assert_eq!(plan.resumable.len(), 1);
    assert_eq!(plan.resumable[0].content_id, alive);
    assert_eq!(plan.prunable.len(), 1);
    assert_eq!(plan.prunable[0].session_ref, "session-b");
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorktreeFixture {
    id: String,
}

#[test]
fn f_core_act_25_bootstrap_prioritizes_selected_open_worktree_and_defers_the_rest() {
    let worktrees = vec![
        WorktreeFixture { id: "w1".into() },
        WorktreeFixture { id: "w2".into() },
        WorktreeFixture { id: "w3".into() },
    ];
    let open = vec!["w1".to_string(), "w3".to_string()];
    let selected = "w3".to_string();

    let order = BootstrapRestoreOrder::partition(&worktrees, &open, Some(&selected), |worktree| {
        worktree.id.clone()
    });

    assert_eq!(
        order
            .priority
            .iter()
            .map(|worktree| worktree.id.as_str())
            .collect::<Vec<_>>(),
        vec!["w3", "w1"]
    );
    assert_eq!(
        order
            .deferred
            .iter()
            .map(|worktree| worktree.id.as_str())
            .collect::<Vec<_>>(),
        vec!["w2"]
    );
}

#[test]
fn f_core_act_26_mount_policy_evicts_only_safe_oldest_worktrees_until_cap() {
    let ids = vec!["w1", "w2", "w3", "w4"];
    let statuses = HashMap::from([
        ("w1", AgentStatus::Done),
        ("w2", AgentStatus::Running),
        ("w3", AgentStatus::Error),
    ]);
    let unsaved = HashSet::from(["w3"]);

    let evicted = WorktreeMountPolicy::ids_to_evict(
        &ids,
        Some(&"w4"),
        2,
        |id| statuses.get(id).copied(),
        |id| unsaved.contains(id),
    );

    assert_eq!(evicted, vec!["w1"]);
}

/// F-CORE-ACT-17, the "one map" half: the live surface's own evidence is a
/// pane status like any other, so `status_for_panes` — the single query the
/// worktree row asks — already accounts for it. Before Layer E the app had
/// to read the chat entity separately for the selected worktree, which is
/// how the same row came to answer the same question two ways.
#[test]
fn f_core_act_17_entity_evidence_resolves_through_the_same_pane_status_query() {
    let now = std::time::Instant::now();
    let mut model = AgentActivityModel::new();
    model.register_agent_id("pane-1", "claude");
    model.notify("pane-1", AgentStatus::Done, now);

    // The surface says it is streaming; the one query agrees immediately.
    assert!(model.set_entity_status("pane-1", Some(AgentStatus::Running)));
    assert_eq!(model.status("pane-1"), Some(AgentStatus::Running));
    assert_eq!(
        model.status_for_panes(&["pane-1"]),
        Some(AgentStatus::Running)
    );
    assert_eq!(
        model.running_agent_ids(&["pane-1"], &["claude", "codex"]),
        vec!["claude"]
    );
    assert_eq!(model.agent_id_for_panes(&["pane-1"]), Some("claude"));

    assert_eq!(model.entity_status("pane-1"), Some(AgentStatus::Running));

    // Re-stating the same claim is not a change, so a per-redraw sync does
    // not churn.
    assert!(!model.set_entity_status("pane-1", Some(AgentStatus::Running)));

    // Withdrawing it falls back on layers A-D rather than sticking.
    assert!(model.set_entity_status("pane-1", None));
    assert_eq!(model.entity_status("pane-1"), None);
    assert_eq!(model.status("pane-1"), Some(AgentStatus::Done));
    assert_eq!(model.status_for_panes(&["pane-1"]), Some(AgentStatus::Done));
}

/// Layer E is first-hand evidence about a pane, so it applies whether or
/// not any layer has identified an agent in it — a terminal that exited
/// non-zero is in an error state either way, and the tab's own status cell
/// already draws that. What it must never do is *invent an identity*: the
/// two identity queries still only name panes with a registered agent.
#[test]
fn f_core_act_17_entity_evidence_reports_status_without_inventing_an_identity() {
    let mut model = AgentActivityModel::new();
    assert!(model.set_entity_status("pane-9", Some(AgentStatus::Error)));
    assert_eq!(model.status("pane-9"), Some(AgentStatus::Error));
    assert_eq!(
        model.status_for_panes(&["pane-9"]),
        Some(AgentStatus::Error)
    );
    assert_eq!(
        model.agent_id_for_panes(&["pane-9"]),
        None,
        "a shell that exited is not an agent"
    );

    model.set_entity_status("pane-9", Some(AgentStatus::Running));
    assert!(
        model
            .running_agent_ids(&["pane-9"], &["claude", "codex"])
            .is_empty(),
        "and it never reaches the running-agents badge either"
    );

    // Closing the pane drops the evidence with everything else.
    model.pane_closed("pane-9");
    assert_eq!(model.status("pane-9"), None);
}
