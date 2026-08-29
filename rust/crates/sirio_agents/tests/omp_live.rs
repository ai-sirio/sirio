//! F-AGENT-OMP-01/-02/-03 — the live drive, against a real `omp`.
//!
//! Ignored by default: it launches the actual agent and spends an API
//! call. Run it deliberately:
//!
//! ```sh
//! SIRIO_LIVE_OMP=1 cargo test -p sirio_agents --test omp_live -- --ignored --nocapture
//! ```
//!
//! Why this test exists rather than a hand-typed shell transcript: every
//! previous verdict on these rows was reached by typing a command that
//! *resembled* the adapter's output. This one asks the adapter for its
//! command and runs exactly that, so a regression in `command()`,
//! `prepare()` or `summarizer_command()` fails the test rather than
//! quietly passing a paraphrase. It is also what would have caught the
//! `oh-my-pi`/`omp` package-name collision three passes earlier: the
//! wrong binary cannot start, so nothing would have reached `notify`.

use std::path::Path;
use std::process::Command;

use sirio_agents::{AgentAdapter, OhMyPiAdapter};

const PANE: &str = "11111111-2222-3333-4444-555555555555";

fn live_enabled() -> bool {
    std::env::var("SIRIO_LIVE_OMP").is_ok_and(|value| value == "1")
}

/// A stand-in for `sirioctl` that appends its argv to `notify.log`, so
/// the hook's calls are observable without a running Sirio.
fn write_fake_sirioctl(dir: &Path) -> String {
    let path = dir.join("fake-sirioctl");
    std::fs::write(
        &path,
        "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/notify.log\"\n",
    )
    .expect("write fake sirioctl");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake sirioctl");
    }
    path.to_string_lossy().into_owned()
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sirio-omp-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch worktree");
    dir
}

fn run(command: &str, cwd: &Path) -> std::process::Output {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn shell")
}

/// F-AGENT-OMP-01 and F-AGENT-OMP-02: `prepare` writes a worktree-local
/// hook, the adapter's launch command loads it, and a real session drives
/// the badge through start, turn and shutdown.
#[test]
#[ignore = "launches a real omp session; set SIRIO_LIVE_OMP=1"]
fn live_session_drives_the_badge_through_start_turn_and_shutdown() {
    if !live_enabled() {
        eprintln!("skipped: set SIRIO_LIVE_OMP=1 to drive a real omp session");
        return;
    }
    let worktree = scratch_dir("session");
    let sirioctl = write_fake_sirioctl(&worktree);
    let worktree_str = worktree.to_string_lossy().into_owned();

    OhMyPiAdapter
        .prepare(&worktree_str, PANE, &sirioctl)
        .expect("prepare writes the hook");

    // The adapter never writes user-global config (F-AGENT-SAFE-*): the
    // hook is inside the worktree, and omp is told about it by flag
    // rather than by dropping it in ~/.omp/agent/hooks.
    let hook = worktree.join(".sirio/omp-hook.ts");
    assert!(hook.is_file(), "prepare must write {}", hook.display());

    // Exactly what Sirio would launch, made noninteractive so the test
    // can observe a whole session. Nothing else about it is rewritten.
    let launch = OhMyPiAdapter.command(&worktree_str, PANE, &sirioctl);
    let output = run(
        &format!("{launch} --print --no-tools 'Reply with exactly the word: ALIVE'"),
        &worktree,
    );
    assert!(
        output.status.success(),
        "omp exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let log = std::fs::read_to_string(worktree.join("notify.log")).unwrap_or_default();
    for status in ["--status running", "--status needs-input", "--status done"] {
        assert!(
            log.contains(status),
            "the hook never reported `{status}`; notify.log was:\n{log}"
        );
    }
    assert!(
        log.contains(PANE),
        "notify must carry this pane's id; notify.log was:\n{log}"
    );
}

/// F-AGENT-OMP-03: the summarizer command produces a summary on stdout.
#[test]
#[ignore = "launches a real omp session; set SIRIO_LIVE_OMP=1"]
fn live_summarizer_command_prints_to_stdout() {
    if !live_enabled() {
        eprintln!("skipped: set SIRIO_LIVE_OMP=1 to drive a real omp session");
        return;
    }
    let worktree = scratch_dir("summarizer");
    let command = OhMyPiAdapter
        .summarizer_command("Reply with exactly the word: ALIVE")
        .expect("omp has a summarizer");
    let output = run(&command, &worktree);

    assert!(
        output.status.success(),
        "`{command}` exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "the summarizer must print a name to stdout; got nothing"
    );
}
