use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sirio_agents::{
    AgentSessionValidator, ClaudeHookMigrator, ClaudeTranscriptSource, CodexTranscriptSource,
    shell_quote,
};

const CURRENT_SIRIOCTL: &str = "/opt/sirio/bin/sirioctl";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sirio-agent-session-test-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temporary directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn hook_migration_still_repairs_pre_rename_tillerctl_paths() {
    // Worktrees prepared before the rebrand hold hooks pointing at a
    // `tillerctl` binary that no longer exists. This rewrite is the only thing
    // that repairs them, so it has to keep recognising the old basename.
    let stale = "'/old/DerivedData/Tiller/tillerctl' notify --session pane-3 --status done";
    let input = serde_json::json!({
        "hooks": {
            "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": stale}]}]
        }
    });

    let rewritten = ClaudeHookMigrator::rewritten_settings(
        serde_json::to_vec(&input).expect("encode settings"),
        CURRENT_SIRIOCTL,
    )
    .expect("a pre-rename tillerctl path is still stale and must be rewritten");
    let output: serde_json::Value = serde_json::from_slice(&rewritten).expect("valid JSON");

    assert_eq!(
        output["hooks"]["Stop"][0]["hooks"][0]["command"],
        format!(
            "{} notify --session pane-3 --status done",
            shell_quote(CURRENT_SIRIOCTL)
        )
    );
}

#[test]
fn hook_migration_rewrites_only_stale_sirioctl_leading_paths() {
    let stale =
        "'/old/DerivedData/Sirio/sirioctl' notify --session pane-7 --status done --stdin-json";
    let current = format!(
        "{} notify --session pane-8 --status running --stdin-json",
        shell_quote(CURRENT_SIRIOCTL)
    );
    let input = serde_json::json!({
        "permissions": {"allow": ["Bash"]},
        "hooks": {
            "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": stale}]}],
            "Notification": [{"matcher": "", "hooks": [{"type": "command", "command": current}]}],
            "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo unchanged"}]}]
        },
        "other": "preserved"
    });

    let rewritten = ClaudeHookMigrator::rewritten_settings(
        serde_json::to_vec(&input).expect("encode settings"),
        CURRENT_SIRIOCTL,
    )
    .expect("stale hook should be rewritten");
    let output: serde_json::Value = serde_json::from_slice(&rewritten).expect("valid JSON");

    assert_eq!(output["permissions"]["allow"][0], "Bash");
    assert_eq!(output["other"], "preserved");
    assert_eq!(
        output["hooks"]["Stop"][0]["hooks"][0]["command"],
        format!(
            "{} notify --session pane-7 --status done --stdin-json",
            shell_quote(CURRENT_SIRIOCTL)
        )
    );
    assert_eq!(
        output["hooks"]["Notification"][0]["hooks"][0]["command"],
        current
    );
    assert_eq!(
        output["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "echo unchanged"
    );
}

#[test]
fn hook_migration_is_a_noop_for_current_non_sirioctl_or_malformed_settings() {
    let current = format!(
        "{} notify --session pane --status done",
        shell_quote(CURRENT_SIRIOCTL)
    );
    let current_settings = serde_json::json!({
        "hooks": {"Stop": [{"hooks": [{"command": current}]}]}
    });
    assert!(
        ClaudeHookMigrator::rewritten_settings(
            serde_json::to_vec(&current_settings).unwrap(),
            CURRENT_SIRIOCTL,
        )
        .is_none()
    );

    let unrelated = serde_json::json!({
        "hooks": {"Stop": [{"hooks": [{"command": "/usr/bin/echo done"}]}]}
    });
    assert!(
        ClaudeHookMigrator::rewritten_settings(
            serde_json::to_vec(&unrelated).unwrap(),
            CURRENT_SIRIOCTL,
        )
        .is_none()
    );
    assert!(ClaudeHookMigrator::rewritten_settings(b"not json", CURRENT_SIRIOCTL).is_none());
}

#[test]
fn hook_migration_updates_a_file_and_missing_files_are_noops() {
    let dir = TempDir::new();
    let path = dir.path().join("settings.local.json");
    let settings = serde_json::json!({
        "hooks": {"Stop": [{"hooks": [{
            "command": "'/old/sirioctl' notify --session pane --status done"
        }]}]}
    });
    std::fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();

    assert!(ClaudeHookMigrator::migrate_file(&path, CURRENT_SIRIOCTL));
    let migrated = std::fs::read_to_string(&path).unwrap();
    assert!(migrated.contains(CURRENT_SIRIOCTL));
    assert!(!migrated.contains("/old/sirioctl"));
    assert!(!ClaudeHookMigrator::migrate_file(
        &dir.path().join("missing.json"),
        CURRENT_SIRIOCTL
    ));
}

#[test]
fn session_validator_checks_claude_and_codex_files_but_trusts_other_agents() {
    let dir = TempDir::new();
    let worktree = "/tmp/demo.project";
    assert_eq!(
        AgentSessionValidator::claude_project_slug(worktree),
        "-tmp-demo-project"
    );
    let claude_file = dir
        .path()
        .join("projects/-tmp-demo-project/claude-ref.jsonl");
    std::fs::create_dir_all(claude_file.parent().unwrap()).unwrap();
    std::fs::write(&claude_file, b"{}").unwrap();

    assert!(AgentSessionValidator::is_likely_valid(
        "claude",
        "claude-ref",
        worktree,
        dir.path(),
        dir.path().join("codex"),
    ));
    assert!(!AgentSessionValidator::is_likely_valid(
        "claude",
        "missing",
        worktree,
        dir.path(),
        dir.path().join("codex"),
    ));

    let codex_file = dir
        .path()
        .join("codex/sessions/2026/08/rollout-deadbeef.jsonl");
    std::fs::create_dir_all(codex_file.parent().unwrap()).unwrap();
    std::fs::write(&codex_file, b"{}").unwrap();
    assert!(AgentSessionValidator::is_likely_valid(
        "codex",
        "deadbeef",
        worktree,
        dir.path().join("missing-claude"),
        dir.path().join("codex"),
    ));
    assert!(!AgentSessionValidator::is_likely_valid(
        "codex",
        "not-present",
        worktree,
        dir.path().join("missing-claude"),
        dir.path().join("codex"),
    ));
    assert!(AgentSessionValidator::is_likely_valid(
        "pi",
        "anything",
        worktree,
        dir.path().join("missing-claude"),
        dir.path().join("missing-codex"),
    ));
}

#[test]
fn claude_transcript_source_joins_string_and_text_block_messages() {
    let dir = TempDir::new();
    let worktree = "/Users/tester/project";
    let session = "claude-ref";
    let path = dir
        .path()
        .join(".claude/projects/-Users-tester-project")
        .join(format!("{session}.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"type":"user","message":{"content":"fix the login bug"}}
{"type":"assistant","message":{"content":[{"type":"thinking","text":"ignored"},{"type":"text","text":"Looking at auth.ts now"}]}}
not-json
{"type":"assistant","message":{"content":[{"type":"text","text":"Done"}]}}
"#,
    )
    .unwrap();

    let text = ClaudeTranscriptSource::new(worktree, session, dir.path())
        .recent_text()
        .expect("transcript text");
    assert_eq!(text, "fix the login bug\nLooking at auth.ts now\nDone");
}

#[test]
fn claude_transcript_source_uses_the_same_non_alphanumeric_slug_as_validation() {
    let dir = TempDir::new();
    let worktree = "/tmp/demo.project";
    let session = "claude-ref";
    let path = dir
        .path()
        .join(".claude/projects/-tmp-demo-project")
        .join(format!("{session}.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"message":{"content":"found the transcript"}}
"#,
    )
    .unwrap();

    assert_eq!(
        ClaudeTranscriptSource::new(worktree, session, dir.path()).recent_text(),
        Some("found the transcript".to_string())
    );
}

#[test]
fn codex_transcript_source_finds_rollout_recursively_and_joins_text_blocks() {
    let dir = TempDir::new();
    let session = "ABCD-1234";
    let path = dir
        .path()
        .join(".codex/sessions/2026/08/13")
        .join("rollout-2026-08-13T12-00-00-abcd-1234.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"content":[{"type":"text","text":"add rate limiting"}]}
{"content":[{"type":"tool","text":"ignored"},{"type":"text","text":"Added a token bucket limiter"}]}
"#,
    )
    .unwrap();

    let text = CodexTranscriptSource::new(session, dir.path())
        .recent_text()
        .expect("rollout text");
    assert_eq!(text, "add rate limiting\nAdded a token bucket limiter");
    assert!(
        CodexTranscriptSource::new("missing", dir.path())
            .recent_text()
            .is_none()
    );
}

#[test]
fn codex_transcript_source_reads_text_blocks_nested_in_rollout_payloads() {
    let dir = TempDir::new();
    let session = "nested-ref";
    let path = dir
        .path()
        .join(".codex/sessions/2026/08/13")
        .join("rollout-2026-08-13T12-00-00-nested-ref.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"type":"response_item","payload":{"type":"message","content":[{"type":"output_text","text":"nested reply"}]}}
"#,
    )
    .unwrap();

    assert_eq!(
        CodexTranscriptSource::new(session, dir.path()).recent_text(),
        Some("nested reply".to_string())
    );
}
