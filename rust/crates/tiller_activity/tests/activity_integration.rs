//! Model-level scenario tests: the layered resolution and pane-ownership
//! invariants that matter for the sidebar badge. All timestamps are
//! explicit `Instant`s for determinism.

use std::time::{Duration, Instant};

use tiller_activity::{
    AgentActivityModel, AgentStatus, CATALOG_IDS, detect_content_status,
    identify_agent_from_process_names, identify_agent_from_title, inspect_process_names,
};

const P1: &str = "pane-1";
const P2: &str = "pane-2";
const P3: &str = "pane-3";

fn now() -> Instant {
    Instant::now()
}

#[cfg(target_os = "linux")]
#[test]
fn linux_process_inspection_reads_agent_names_from_proc_children() {
    use std::os::unix::fs::symlink;
    use std::process::Command;
    use std::thread;

    let root = std::env::temp_dir().join(format!("tiller-activity-proc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create process fixture directory");
    let agent = root.join("codex");
    symlink("/bin/sleep", &agent).expect("create codex test alias");
    let mut child = Command::new("sh")
        .args(["-c", &format!("{} 2", agent.display())])
        .spawn()
        .expect("spawn shell with agent child");
    thread::sleep(Duration::from_millis(50));

    let names = inspect_process_names(child.id()).expect("read Linux process tree");
    assert!(names.iter().any(|name| name == "codex"));

    let mut model = AgentActivityModel::new();
    let transition = model
        .refresh_process_signal(P1, child.id())
        .expect("refresh process signal");
    assert_eq!(
        transition.map(|value| value.new),
        Some(AgentStatus::Running)
    );
    assert!(model.is_process_owned(P1));

    let _ = child.kill();
    let _ = child.wait();
    model
        .refresh_process_signal(P1, child.id())
        .expect("refresh disappeared process signal");
    assert_eq!(model.status(P1), None);
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_process_inspection_reads_a_real_child_process_name() {
    use std::process::Command;

    let mut child = Command::new("/bin/sleep")
        .arg("5")
        .spawn()
        .expect("spawn child process");
    let names = inspect_process_names(std::process::id()).expect("read macOS process tree");

    assert!(
        names.iter().any(|name| name == "sleep"),
        "macOS process inspection must report the spawned sleep process, got {names:?}"
    );
    child.kill().expect("kill child process");
    child.wait().expect("reap child process");
}

// The Windows counterpart of the Linux test above: no privilege-free
// symlinks on Windows, so cmd.exe is *copied* under an agent's catalog id and
// spawned with a slow grandchild (ping) still running when the walk happens.
#[cfg(target_os = "windows")]
#[test]
fn windows_process_inspection_reads_agent_names_from_a_real_child() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let comspec = std::env::var("ComSpec").expect("ComSpec is set on every Windows install");
    let root = std::env::temp_dir().join(format!("tiller-activity-win-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create process fixture directory");
    let agent = root.join("codex.exe");
    std::fs::copy(&comspec, &agent).expect("copy shell as codex.exe test alias");
    // Spawned as a direct child of THIS test process, which is what the walk
    // below starts from (the macOS test's trick): the copied alias is a real
    // depth-1 node under it and its own ping grandchild stays alive ~5s so
    // the nested walk has something at depth > 1 to find.
    let mut child = Command::new(&agent)
        .args(["/c", "ping -n 6 127.0.0.1 >nul"])
        .spawn()
        .expect("spawn shell with agent child");
    thread::sleep(Duration::from_millis(200));

    let names = inspect_process_names(std::process::id()).expect("read Windows process tree");
    assert!(
        names.iter().any(|name| name == "codex"),
        "the .exe suffix must be stripped down to the catalog shape, got {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "ping"),
        "nested descendants must be walked too, got {names:?}"
    );

    let mut model = AgentActivityModel::new();
    let transition = model
        .refresh_process_signal(P1, std::process::id())
        .expect("refresh process signal");
    assert_eq!(
        transition.map(|value| value.new),
        Some(AgentStatus::Running)
    );
    assert!(model.is_process_owned(P1));

    let _ = child.kill();
    let _ = child.wait();
    // The alias is dead; whatever orphaned ping remains carries no catalog
    // name, so the pane's Layer-D evidence is gone either way.
    model
        .refresh_process_signal(P1, std::process::id())
        .expect("refresh disappeared process signal");
    assert_eq!(model.status(P1), None);
    let _ = std::fs::remove_dir_all(root);
}

/// `pty_shell_pid` stands `0` in for an unresolvable ConPTY child pid. Layer D
/// must refuse to enumerate it — Toolhelp32 would happily report processes
/// whose parent field is 0 — and the refusal must arrive as `Err` so that
/// `refresh_process_signal` never routes an unknown pid through
/// `process_gone`: that would let a resolution failure clear a
/// legitimately identified process-owned pane.
#[cfg(target_os = "windows")]
#[test]
fn windows_layer_d_refuses_an_unknown_shell_pid_without_clearing_state() {
    use std::io::ErrorKind;

    let error = inspect_process_names(0).expect_err("pid 0 must be refused");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);

    let mut model = AgentActivityModel::new();
    model.process_identified(P1, "codex");
    assert!(model.refresh_process_signal(P1, 0).is_err());
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Running),
        "an unknown pid must not clear a process-owned pane"
    );
    assert!(model.is_process_owned(P1));
}

fn spinner() -> String {
    "\u{280B}".to_string() // ⠋
}

// ---------------------------------------------------------------------------
// The five required scenarios
// ---------------------------------------------------------------------------

#[test]
fn layer_a_push_suppresses_contradicting_title_inside_debounce_and_stops_after() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    model.agent_spawned(P1, "claude", t0);
    // A hook push lands; a contradicting title arrives 0.5 s later — inside
    // the 1.5 s debounce window.
    model.notify(P1, AgentStatus::Running, t0 + Duration::from_millis(500));
    let suppressed = model.handle_title_change(P1, "✳ idle", t0 + Duration::from_millis(1000));
    assert_eq!(
        suppressed, None,
        "contradicting title dropped inside the debounce window"
    );
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Running),
        "hook status stands"
    );

    // Once the window expires, the title signal applies.
    let applied = model.handle_title_change(P1, "✳ idle", t0 + Duration::from_millis(2500));
    assert_eq!(applied.map(|t| t.new), Some(AgentStatus::NeedsInput));
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));
}

#[test]
fn bare_braille_spinner_assigns_no_identity() {
    // Codex 0.144+ writes the same braille "dots" cycle as Claude while
    // working — a bare spinner alone must not claim either identity.
    assert_eq!(
        identify_agent_from_title(&format!("{} Fix login bug", spinner())),
        None
    );

    // And through the model: an unregistered pane with a spinner title gets
    // no agent identity and no status.
    let mut model = AgentActivityModel::new();
    assert_eq!(
        model.handle_title_change(P1, &format!("{} working", spinner()), now()),
        None
    );
    assert_eq!(model.agent_id(P1), None);
    assert_eq!(model.status(P1), None);
}

#[test]
fn pi_and_omp_titles_resolve_to_different_agents() {
    // pi = "π - <cwd>", omp = "π: <cwd>" — the colon is the only mark.
    assert_eq!(identify_agent_from_title("π - tiller"), Some("pi"));
    assert_eq!(identify_agent_from_title("π: tiller"), Some("omp"));

    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.handle_title_change(P1, "π - tiller", t0);
    model.handle_title_change(P2, "π: tiller", t0);
    assert_eq!(model.agent_id(P1), Some("pi"));
    assert_eq!(model.agent_id(P2), Some("omp"));
}

#[test]
fn content_match_overrides_stale_title_even_inside_debounce_window() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    model.agent_spawned(P1, "claude", t0);
    // A hook fired 'running' 0.2 s ago — well inside Layer B's 1.5 s
    // debounce — but a genuine content match still applies (Layer C is not
    // gated by the title debounce).
    let transition =
        model.apply_content_signal(P1, AgentStatus::NeedsInput, t0 + Duration::from_millis(200));
    assert_eq!(
        transition.map(|t| (t.old, t.new)),
        Some((Some(AgentStatus::Running), AgentStatus::NeedsInput))
    );
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));

    // A subsequent real hook still overwrites it unconditionally.
    let hook = model.notify(P1, AgentStatus::Running, t0 + Duration::from_millis(400));
    assert_eq!(hook.new, AgentStatus::Running);
    assert_eq!(model.status(P1), Some(AgentStatus::Running));
}

#[test]
fn each_ownership_kind_is_cleared_only_by_its_own_condition() {
    // Spawn-owned: cleared by process exit, never by an unmatched title.
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);
    assert_eq!(
        model.handle_title_change(P1, "zsh", t0),
        None,
        "unmatched title ignored"
    );
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Running),
        "spawn-owned survives"
    );
    model.process_gone(P1);
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Running),
        "process_gone must not clear spawn-owned"
    );
    model.apply_exit_result(P1, 0, t0 + Duration::from_secs(1));
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Done),
        "cleared by process exit"
    );

    // Title-owned: cleared only when the title stops matching.
    model.handle_title_change(P2, "opencode ready", t0);
    assert!(model.is_title_owned(P2));
    model.process_gone(P2);
    assert_eq!(
        model.status(P2),
        Some(AgentStatus::NeedsInput),
        "process_gone must not clear title-owned"
    );
    model.handle_title_change(P2, "zsh", t0);
    assert_eq!(
        model.status(P2),
        None,
        "title-owned cleared by unmatched title"
    );
    assert_eq!(model.agent_id(P2), None);

    // Process-owned: cleared ONLY by process_gone, never by an unrelated
    // title change.
    model.process_identified(P3, "codex");
    assert!(model.is_process_owned(P3));
    assert_eq!(model.status(P3), Some(AgentStatus::Running));

    // A title that is unrelated (no match) must not clear it…
    assert_eq!(model.handle_title_change(P3, "zsh", t0), None);
    assert_eq!(
        model.status(P3),
        Some(AgentStatus::Running),
        "process-owned survives unrelated title"
    );
    assert_eq!(model.agent_id(P3), Some("codex"));

    // …and neither must a title that MATCHES another agent's conventions.
    // (process-owned panes are registered, so a matching title updates
    // status but never the identity; an unmatched one changes nothing.)
    assert_eq!(
        model.handle_title_change(P3, "π - tiller", t0 + Duration::from_secs(2)),
        None
    );
    assert_eq!(
        model.agent_id(P3),
        Some("codex"),
        "identity never re-assigned"
    );

    model.process_gone(P3);
    assert_eq!(
        model.status(P3),
        None,
        "process-owned cleared only by process_gone"
    );
    assert_eq!(model.agent_id(P3), None);
}

// ---------------------------------------------------------------------------
// Layer A: hook pushes
// ---------------------------------------------------------------------------

#[test]
fn notify_sets_status_and_records_timestamp() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    let transition = model.notify(P1, AgentStatus::NeedsInput, t0);
    assert_eq!(transition.old, None);
    assert_eq!(transition.new, AgentStatus::NeedsInput);
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));

    let second = model.notify(P1, AgentStatus::Done, t0);
    assert_eq!(second.old, Some(AgentStatus::NeedsInput));
    assert_eq!(second.new, AgentStatus::Done);
    assert_eq!(model.status(P1), Some(AgentStatus::Done));
}

#[test]
fn out_of_order_hook_push_does_not_regress_a_pane_status() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    model.agent_spawned(P1, "claude", t0);
    // The reviewer's sequence: Done at t_new, then Running at t_old — a
    // full second OLDER — arriving late. tiller_control serves each socket
    // connection on its own OS thread, so a real agent's lifecycle events
    // fired close together can reach the model in either order.
    model.notify(P1, AgentStatus::Done, t0 + Duration::from_secs(10));
    let stale = model.notify(P1, AgentStatus::Running, t0 + Duration::from_secs(9));

    assert_eq!(
        stale.old,
        Some(AgentStatus::Done),
        "the stale push reports the standing status"
    );
    assert_eq!(
        stale.new,
        AgentStatus::Done,
        "the rejected push changes nothing"
    );
    assert_eq!(
        model.status(P1),
        Some(AgentStatus::Done),
        "the pane must not regress from Done back to Running"
    );
}

#[test]
fn a_stale_push_does_not_resurrect_a_cleared_pane() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    // A title-identified pane that received a hook push, then was cleared
    // by its title stopping to match, must not be resurrected by stale
    // hook evidence arriving later.
    model.handle_title_change(P1, "opencode ready", t0);
    model.notify(P1, AgentStatus::Running, t0 + Duration::from_secs(5));
    model.handle_title_change(P1, "zsh", t0 + Duration::from_secs(6));
    assert_eq!(model.status(P1), None, "title-owned pane cleared");

    let stale = model.notify(P1, AgentStatus::Done, t0 + Duration::from_secs(4));
    assert_eq!(stale.old, None);
    assert_eq!(model.status(P1), None, "cleared pane stays cleared");
    assert_eq!(model.agent_id(P1), None);
}

#[test]
fn a_push_with_equal_timestamp_still_applies() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    model.agent_spawned(P1, "claude", t0);
    let applied = model.notify(P1, AgentStatus::NeedsInput, t0);
    assert_eq!(applied.new, AgentStatus::NeedsInput);
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));
}

// ---------------------------------------------------------------------------
// Spawn and restore
// ---------------------------------------------------------------------------

#[test]
fn agent_spawned_sets_running_and_agent_id_without_transition() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);
    assert_eq!(model.status(P1), Some(AgentStatus::Running));
    assert_eq!(model.agent_id(P1), Some("claude"));
    assert!(!model.is_title_owned(P1));
}

#[test]
fn register_agent_id_sets_identity_without_status() {
    let mut model = AgentActivityModel::new();
    model.register_agent_id(P1, "claude");
    assert_eq!(model.agent_id(P1), Some("claude"));
    assert_eq!(
        model.status(P1),
        None,
        "restored tabs are not claimed as running"
    );

    // A later real notify resolves the pane fully.
    model.notify(P1, AgentStatus::Running, now());
    assert_eq!(model.status(P1), Some(AgentStatus::Running));
}

// ---------------------------------------------------------------------------
// Process exit
// ---------------------------------------------------------------------------

#[test]
fn exit_results_map_codes_and_respect_tracking() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "codex", t0);

    let done = model.apply_exit_result(P1, 0, t0).expect("tracked pane");
    assert_eq!(done.old, Some(AgentStatus::Running));
    assert_eq!(done.new, AgentStatus::Done);

    model.agent_spawned(P2, "codex", t0);
    let error = model.apply_exit_result(P2, 1, t0).expect("tracked pane");
    assert_eq!(error.new, AgentStatus::Error);

    assert_eq!(model.apply_exit_result(P3, 0, t0), None, "untracked pane");

    model.pane_closed(P1);
    assert_eq!(model.apply_exit_result(P1, 0, t0), None, "closed pane");
}

// ---------------------------------------------------------------------------
// Layer B: identification and clearing
// ---------------------------------------------------------------------------

#[test]
fn title_change_identifies_unregistered_panes() {
    let mut model = AgentActivityModel::new();
    let t0 = now();

    let transition = model
        .handle_title_change(P1, "✳ Fix login bug", t0)
        .expect("identified");
    assert_eq!(transition.old, None);
    assert_eq!(transition.new, AgentStatus::NeedsInput);
    assert_eq!(model.agent_id(P1), Some("claude"));
    assert!(model.is_title_owned(P1));

    // An unrecognizable title on an unregistered pane changes nothing.
    assert_eq!(model.handle_title_change(P2, "zsh", t0), None);
    assert_eq!(model.agent_id(P2), None);

    // A title that identifies an agent but carries no status opinion falls
    // back to running.
    let transition = model
        .handle_title_change(P2, "opencode", t0)
        .expect("identified");
    assert_eq!(transition.new, AgentStatus::Running);
}

#[test]
fn known_pane_status_updates_respect_debounce() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);

    // Inside the debounce window (spawn counts as a hook update).
    assert_eq!(
        model.handle_title_change(P1, "✳ idle", t0 + Duration::from_millis(500)),
        None
    );
    assert_eq!(model.status(P1), Some(AgentStatus::Running));

    // After the window.
    let transition = model.handle_title_change(P1, "✳ idle", t0 + Duration::from_secs(2));
    assert_eq!(transition.map(|t| t.new), Some(AgentStatus::NeedsInput));
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));
}

#[test]
fn title_owned_panes_are_cleared_by_unmatched_titles() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.handle_title_change(P1, "opencode ready", t0);
    assert_eq!(model.agent_id(P1), Some("opencode"));

    assert_eq!(model.handle_title_change(P1, "zsh", t0), None);
    assert_eq!(model.status(P1), None);
    assert_eq!(model.agent_id(P1), None);
    assert!(!model.is_title_owned(P1));
}

// ---------------------------------------------------------------------------
// Layer C
// ---------------------------------------------------------------------------

#[test]
fn content_signal_no_ops_for_unregistered_or_unchanged() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    assert_eq!(
        model.apply_content_signal(P1, AgentStatus::NeedsInput, t0),
        None
    );

    model.agent_spawned(P1, "claude", t0);
    assert_eq!(
        model.apply_content_signal(P1, AgentStatus::Running, t0),
        None,
        "no change"
    );
}

#[test]
fn content_signal_updates_last_hook_timestamp() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);

    // A content signal at t0+1s must debounce a title arriving before
    // t0+2.5s, exactly like a hook push would.
    model.apply_content_signal(P1, AgentStatus::NeedsInput, t0 + Duration::from_secs(1));
    assert_eq!(
        model.handle_title_change(P1, ". working", t0 + Duration::from_secs(2)),
        None
    );
    let applied = model.handle_title_change(P1, ". working", t0 + Duration::from_secs(3));
    assert_eq!(applied.map(|t| t.new), Some(AgentStatus::Running));
}

#[test]
fn content_detector_feeds_the_model() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);

    let status = detect_content_status("Do you want to proceed?\n1. Yes", "claude");
    model.apply_content_signal(P1, status.expect("detected"), t0 + Duration::from_secs(1));
    assert_eq!(model.status(P1), Some(AgentStatus::NeedsInput));
}

// ---------------------------------------------------------------------------
// Layer D
// ---------------------------------------------------------------------------

#[test]
fn process_identification_never_downgrades_existing_identity() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);
    assert_eq!(
        model.process_identified(P1, "codex"),
        None,
        "already registered"
    );
    assert_eq!(model.agent_id(P1), Some("claude"));
    assert!(!model.is_process_owned(P1));
}

#[test]
fn process_matcher_finds_first_catalog_match() {
    let names = std::collections::HashSet::from([
        "zsh".to_string(),
        "codex".to_string(),
        "claude".to_string(),
    ]);
    assert_eq!(
        identify_agent_from_process_names(&names, &CATALOG_IDS),
        Some("claude")
    );

    let no_agents = std::collections::HashSet::from(["zsh".to_string(), "node".to_string()]);
    assert_eq!(
        identify_agent_from_process_names(&no_agents, &CATALOG_IDS),
        None
    );
}

// ---------------------------------------------------------------------------
// Pane closed
// ---------------------------------------------------------------------------

#[test]
fn pane_closed_clears_all_state_and_is_idempotent() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);
    model.process_identified(P2, "codex");
    model.handle_title_change(P3, "opencode ready", t0);

    model.pane_closed(P1);
    model.pane_closed(P1); // idempotent
    assert_eq!(model.status(P1), None);
    assert_eq!(model.agent_id(P1), None);

    model.pane_closed(P2);
    assert_eq!(model.status(P2), None);
    assert!(!model.is_process_owned(P2));

    model.pane_closed(P3);
    assert!(!model.is_title_owned(P3));
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

#[test]
fn status_for_panes_uses_priority_order() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0); // running
    model.agent_spawned(P2, "codex", t0); // running
    model.notify(P3, AgentStatus::NeedsInput, t0);

    assert_eq!(
        model.status_for_panes(&[P1, P2, P3]),
        Some(AgentStatus::NeedsInput)
    );
    assert_eq!(
        model.status_for_panes(&[P1, P2]),
        Some(AgentStatus::Running)
    );
    assert_eq!(model.status_for_panes(&[]), None);

    model.notify(P1, AgentStatus::Error, t0);
    assert_eq!(
        model.status_for_panes(&[P1, P2, P3]),
        Some(AgentStatus::Error)
    );
}

#[test]
fn agent_id_for_panes_picks_the_highest_priority_pane() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "claude", t0);
    model.agent_spawned(P2, "codex", t0);
    assert_eq!(model.agent_id_for_panes(&[P1, P2]), Some("claude"));

    model.notify(P2, AgentStatus::NeedsInput, t0);
    assert_eq!(model.agent_id_for_panes(&[P1, P2]), Some("codex"));

    assert_eq!(model.agent_id_for_panes(&[]), None);
}

#[test]
fn running_agent_ids_follow_catalog_order() {
    let mut model = AgentActivityModel::new();
    let t0 = now();
    model.agent_spawned(P1, "codex", t0);
    model.agent_spawned(P2, "claude", t0);
    model.notify(P3, AgentStatus::Done, t0);

    let running = model.running_agent_ids(&[P1, P2, P3], &CATALOG_IDS);
    assert_eq!(
        running,
        vec!["claude", "codex"],
        "catalog order, not pane order"
    );
}
