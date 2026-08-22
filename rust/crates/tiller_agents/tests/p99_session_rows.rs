//! P99 exercises for three inventory rows, each driven by its own VERIFY
//! clause against real files (`session_sources.rs` covers the happy paths;
//! these cover the conjuncts it does not):
//!
//! - `F-AGENT-SAFE-02` — migration against matching, stale, malformed and
//!   unrelated hook files, diffing **every** non-path field.
//! - `F-AGENT-SESSION-01` — the validator result as each expected session
//!   file is created **and removed**; Claude, Codex, and uncheckable agents.
//! - `F-AGENT-SESSION-02` — string **and** block content at each supported
//!   transcript path, comparing returned recent text, and the no-usable-text
//!   answer.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tiller_agents::{
    AgentSessionValidator, ClaudeHookMigrator, ClaudeTranscriptSource, CodexTranscriptSource,
    shell_quote,
};

const CURRENT_TILLERCTL: &str = "/opt/tiller/bin/tillerctl";
const PANE: &str = "11111111-2222-3333-4444-555555555555";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiller-p99-session-rows-{}-{id}",
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

/// Flattens a JSON document into `(pointer, leaf)` pairs so two documents
/// can be diffed field by field.
#[cfg(not(windows))]
fn leaves(value: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    fn walk(value: &serde_json::Value, at: String, out: &mut Vec<(String, serde_json::Value)>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, inner) in map {
                    walk(inner, format!("{at}/{key}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, inner) in items.iter().enumerate() {
                    walk(inner, format!("{at}/{index}"), out);
                }
            }
            leaf => out.push((at, leaf.clone())),
        }
    }
    let mut out = Vec::new();
    walk(value, String::new(), &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A realistic worktree-local Claude settings document: permissions, env,
/// three hook events (one stale tillerctl, one current tillerctl, one
/// unrelated command), matchers, a numeric hook timeout, and an unrelated
/// top-level object.
#[cfg(not(windows))]
fn settings_with_stale_hook() -> serde_json::Value {
    serde_json::json!({
        "permissions": {"allow": ["Bash(cargo:*)", "Read"], "deny": []},
        "env": {"TILLER_PANE": PANE, "NO_COLOR": "1"},
        "hooks": {
            "SessionStart": [{"matcher": "", "hooks": [{
                "type": "command",
                "command": format!(
                    "'/stale/DerivedData/tillerctl' notify --session {PANE} --status running --stdin-json"
                ),
                "timeout": 5
            }]}],
            "Stop": [{"matcher": "", "hooks": [{
                "type": "command",
                "command": format!(
                    "{} notify --session {PANE} --status done",
                    shell_quote(CURRENT_TILLERCTL)
                )
            }]}],
            "PreToolUse": [{"matcher": "Bash", "hooks": [{
                "type": "command",
                "command": "/usr/bin/logger tiller-pre"
            }]}]
        },
        "feedbackSurveyState": {"lastShownTime": 1723600000_i64}
    })
}

// ---------------------------------------------------------------- SAFE-02

// On Windows the rewritten hook command is cmd.exe-quoted ("…" with
// doubled embedded quotes), so the `' ` split point this test's suffix
// helper relies on does not exist there. The migration behaviour itself is
// platform-neutral and covered on Windows by the session_sources suite,
// whose expectations derive from `shell_quote`.
#[cfg(not(windows))]
#[test]
fn migration_rewrites_only_the_stale_leading_path_and_no_other_field() {
    let dir = TempDir::new();
    let path = dir.path().join("settings.local.json");
    let before = settings_with_stale_hook();
    std::fs::write(&path, serde_json::to_vec_pretty(&before).unwrap()).unwrap();

    assert!(ClaudeHookMigrator::migrate_file(&path, CURRENT_TILLERCTL));

    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let before_leaves = leaves(&before);
    let after_leaves = leaves(&after);

    // Same set of fields — nothing added, nothing dropped.
    assert_eq!(
        before_leaves.iter().map(|(at, _)| at).collect::<Vec<_>>(),
        after_leaves.iter().map(|(at, _)| at).collect::<Vec<_>>(),
    );

    // Diff every field: exactly one leaf may differ, the stale command.
    let changed: Vec<_> = before_leaves
        .iter()
        .zip(&after_leaves)
        .filter(|(b, a)| b.1 != a.1)
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "only the stale command may change, got {changed:?}"
    );
    let (stale_before, stale_after) = changed[0];
    assert_eq!(stale_before.0, "/hooks/SessionStart/0/hooks/0/command");
    assert_eq!(
        stale_after.1.as_str().unwrap(),
        format!(
            "{} notify --session {PANE} --status running --stdin-json",
            shell_quote(CURRENT_TILLERCTL)
        ),
        "the rewritten command must preserve the pane id and every argument"
    );
    // The suffix after the quoted path — pane id, status, flags — is
    // byte-identical between the stale and rewritten commands.
    let suffix = |command: &str| command[command.find("' ").unwrap()..].to_string();
    assert_eq!(
        suffix(stale_before.1.as_str().unwrap()),
        suffix(stale_after.1.as_str().unwrap())
    );
}

#[test]
fn migration_leaves_matching_malformed_and_unrelated_files_byte_identical() {
    let dir = TempDir::new();

    // Matching: the only tillerctl command already carries the current path.
    let matching = serde_json::json!({
        "hooks": {"Stop": [{"matcher": "", "hooks": [{
            "type": "command",
            "command": format!(
                "{} notify --session {PANE} --status done",
                shell_quote(CURRENT_TILLERCTL)
            )
        }]}]}
    });
    let matching_path = dir.path().join("matching.json");
    std::fs::write(
        &matching_path,
        serde_json::to_vec_pretty(&matching).unwrap(),
    )
    .unwrap();

    // Malformed: truncated JSON.
    let malformed_path = dir.path().join("malformed.json");
    std::fs::write(&malformed_path, b"{\"hooks\": {\"Stop\": [").unwrap();

    // Unrelated: hooks that never mention tillerctl, plus other keys.
    let unrelated = serde_json::json!({
        "permissions": {"allow": ["Read"]},
        "hooks": {"PostToolUse": [{"matcher": "", "hooks": [{
            "type": "command",
            "command": "/usr/bin/logger post-tool"
        }]}]}
    });
    let unrelated_path = dir.path().join("unrelated.json");
    std::fs::write(
        &unrelated_path,
        serde_json::to_vec_pretty(&unrelated).unwrap(),
    )
    .unwrap();

    for path in [&matching_path, &malformed_path, &unrelated_path] {
        let before = std::fs::read(path).unwrap();
        assert!(
            !ClaudeHookMigrator::migrate_file(path, CURRENT_TILLERCTL),
            "{} must be a no-op",
            path.display()
        );
        assert_eq!(
            before,
            std::fs::read(path).unwrap(),
            "{} must be byte-identical after the no-op",
            path.display()
        );
    }
}

// ------------------------------------------------------------- SESSION-01

#[test]
fn validator_flips_as_the_expected_session_file_is_created_and_removed() {
    let dir = TempDir::new();
    let worktree = "/home/user/my project!";
    assert_eq!(
        AgentSessionValidator::claude_project_slug(worktree),
        "-home-user-my-project-",
        "non-alphanumeric path characters become hyphens"
    );

    let claude_config = dir.path().join("claude-config");
    let codex_home = dir.path().join("codex-home");
    let is_valid = |agent: &str, reference: &str| {
        AgentSessionValidator::is_likely_valid(
            agent,
            reference,
            worktree,
            &claude_config,
            &codex_home,
        )
    };

    // Claude: absent → create → remove.
    assert!(!is_valid("claude", "ref-1"));
    let claude_file = claude_config.join("projects/-home-user-my-project-/ref-1.jsonl");
    std::fs::create_dir_all(claude_file.parent().unwrap()).unwrap();
    std::fs::write(&claude_file, b"{}").unwrap();
    assert!(is_valid("claude", "ref-1"));
    std::fs::remove_file(&claude_file).unwrap();
    assert!(!is_valid("claude", "ref-1"));

    // Codex: a nested rollout whose filename contains the reference.
    assert!(!is_valid("codex", "cafe-42"));
    let rollout = codex_home.join("sessions/2026/08/13/rollout-2026-08-13T09-00-00-cafe-42.jsonl");
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    std::fs::write(&rollout, b"{}").unwrap();
    assert!(is_valid("codex", "cafe-42"));
    std::fs::remove_file(&rollout).unwrap();
    assert!(!is_valid("codex", "cafe-42"));

    // Agents without a known on-disk store are trusted as worth trying,
    // with or without anything on disk.
    for agent in ["pi", "opencode", "omp", "some-future-agent"] {
        assert!(is_valid(agent, "anything"), "{agent} must be trusted");
    }
}

// ------------------------------------------------------------- SESSION-02

#[test]
fn transcripts_join_string_and_block_content_at_each_supported_path() {
    let dir = TempDir::new();

    // Claude: `~/.claude/projects/<worktree-slug>/<session>.jsonl`, string
    // content and block content in one file, joined in source order.
    let worktree = "/data/work tree";
    let claude_path = dir
        .path()
        .join(".claude/projects/-data-work-tree/sess-1.jsonl");
    std::fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    std::fs::write(
        &claude_path,
        br#"{"message":{"content":"plain string entry"}}
{"message":{"content":[{"type":"text","text":"block entry"},{"type":"tool_use","text":"skipped"}]}}
{"message":{"content":[{"type":"output_text","text":"output block"}]}}
"#,
    )
    .unwrap();
    assert_eq!(
        ClaudeTranscriptSource::new(worktree, "sess-1", dir.path()).recent_text(),
        Some("plain string entry\nblock entry\noutput block".to_string())
    );

    // Codex: a rollout below `~/.codex/sessions` whose filename ends in the
    // session id (case-insensitive), string content at the top level and
    // block content nested in payloads.
    let rollout = dir
        .path()
        .join(".codex/sessions/2026/08/rollout-2026-08-14T01-00-00-FEED-99.jsonl");
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    std::fs::write(
        &rollout,
        br#"{"content":"top-level string"}
{"payload":{"content":[{"type":"output_text","text":"nested block"}]}}
{"payload":{"payload":{"content":"double nested string"}}}
"#,
    )
    .unwrap();
    assert_eq!(
        CodexTranscriptSource::new("feed-99", dir.path()).recent_text(),
        Some("top-level string\nnested block\ndouble nested string".to_string())
    );
}

#[test]
fn transcripts_without_a_file_or_usable_text_return_none() {
    let dir = TempDir::new();
    let worktree = "/data/work tree";

    // No transcript file at the expected path.
    assert_eq!(
        ClaudeTranscriptSource::new(worktree, "missing", dir.path()).recent_text(),
        None
    );

    // A transcript with lines but no usable text: thinking-only blocks, a
    // line without a message, and a line that is not JSON.
    let claude_path = dir
        .path()
        .join(".claude/projects/-data-work-tree/sess-2.jsonl");
    std::fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    std::fs::write(
        &claude_path,
        br#"{"message":{"content":[{"type":"thinking","text":"not shown"}]}}
{"summary":"no message key"}
not-json
"#,
    )
    .unwrap();
    assert_eq!(
        ClaudeTranscriptSource::new(worktree, "sess-2", dir.path()).recent_text(),
        None
    );

    // Same for a Codex rollout holding only non-text blocks.
    let rollout = dir
        .path()
        .join(".codex/sessions/2026/rollout-2026-08-14T02-00-00-empty-1.jsonl");
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    std::fs::write(&rollout, br#"{"content":[{"type":"tool","text":"x"}]}"#).unwrap();
    assert_eq!(
        CodexTranscriptSource::new("empty-1", dir.path()).recent_text(),
        None
    );
}
