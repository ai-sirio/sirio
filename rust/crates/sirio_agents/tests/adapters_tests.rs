//! Adapter tests against real temporary directories: every command line
//! asserted against what the Swift source specifies, the fake-HOME isolation
//! invariant, and the Codex TOML round trip.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use sirio_agents::{
    ALL, AgentAdapter, ClaudeCodeAdapter, CodexAdapter, GlobalHookInstall, GlobalHookReport,
    OhMyPiAdapter, OpenCodeAdapter, PiAdapter, PrepareError, SKILL_MANAGED_MARKER,
    discover_availability, find_executable_in_path, install_global_hooks, install_skill,
    json_string_literal, shell_quote,
};
#[cfg(unix)]
use sirio_agents::{
    AgentAvailability, DiscoveryError, find_executable_in_path_checked,
    try_discover_availability_in,
};
#[cfg(windows)]
use sirio_agents::try_discover_availability_in;

const PANE_ID: &str = "12345678-1234-1234-1234-123456789abc";
const SIRIOCTL: &str = "/usr/local/bin/sirioctl";
const WORKTREE: &str = "/Users/me/sirio";

/// A throwaway directory, removed on drop. Canonicalized so paths match what
/// the adapters report.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sirio-agents-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(std::fs::canonicalize(&path).expect("canonicalize temp dir"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// Everything under the directory, recursively, as relative paths.
    fn tree(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let entry = entry.expect("entry");
                let relative = entry
                    .path()
                    .strip_prefix(base)
                    .expect("relative")
                    .to_path_buf();
                if entry.file_type().expect("type").is_dir() {
                    walk(&entry.path(), base, out);
                } else {
                    out.push(relative);
                }
            }
        }
        let mut out = Vec::new();
        walk(self.path(), self.path(), &mut out);
        out.sort();
        out
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// Command lines, exactly as the Swift sources specify them
// ---------------------------------------------------------------------------

#[test]
fn claude_command_is_bare() {
    assert_eq!(
        ClaudeCodeAdapter.command(WORKTREE, PANE_ID, SIRIOCTL),
        "claude"
    );
}

#[cfg(not(windows))]
#[test]
fn codex_command_carries_the_notify_override() {
    // The exact expectation from CodexAdapterTests: slashes unescaped,
    // whole override single-quoted for the shell.
    assert_eq!(
        CodexAdapter.command(WORKTREE, PANE_ID, SIRIOCTL),
        "codex -c 'notify=[\"/usr/local/bin/sirioctl\",\"notify\",\"--session\",\"12345678-1234-1234-1234-123456789abc\",\"--status\",\"needs-input\"]'"
    );
}

/// The Windows command() form: the whole `-c` override rides one
/// cmd.exe double-quoted token, with every embedded JSON quote doubled
/// (`""` is cmd's own literal-quote convention, collapsed back to `"`
/// before the child's argv parser sees the token — the shape verified
/// end-to-end against a live cmd.exe in the win-quoting pass).
#[cfg(windows)]
#[test]
fn codex_command_carries_the_notify_override() {
    assert_eq!(
        CodexAdapter.command(WORKTREE, PANE_ID, SIRIOCTL),
        "codex -c \"notify=[\"\"/usr/local/bin/sirioctl\"\",\"\"notify\"\",\"\"--session\"\",\"\"12345678-1234-1234-1234-123456789abc\"\",\"\"--status\"\",\"\"needs-input\"\"]\""
    );
}

#[test]
fn opencode_command_is_bare() {
    assert_eq!(
        OpenCodeAdapter.command(WORKTREE, PANE_ID, SIRIOCTL),
        "opencode"
    );
}

#[test]
fn pi_command_is_bare() {
    assert_eq!(PiAdapter.command(WORKTREE, PANE_ID, SIRIOCTL), "pi");
}

#[cfg(not(windows))]
#[test]
fn omp_command_points_at_the_worktree_local_hook() {
    assert_eq!(
        OhMyPiAdapter.command(WORKTREE, PANE_ID, SIRIOCTL),
        "omp --hook '/Users/me/sirio/.sirio/omp-hook.ts'"
    );
}

/// The Windows form of the launch line: the hook path is a single
/// cmd.exe double-quoted token, so a worktree path with spaces survives.
/// (The adapter joins the hook path with `/` on every platform — the
/// mixed separators are fine for Windows APIs and for cmd; it is the
/// quoting, not the separator, that must match cmd.)
#[cfg(windows)]
#[test]
fn omp_command_points_at_the_worktree_local_hook() {
    assert_eq!(
        OhMyPiAdapter.command("C:\\Users\\me\\my worktree\\sirio", PANE_ID, SIRIOCTL),
        "omp --hook \"C:\\Users\\me\\my worktree\\sirio/.sirio/omp-hook.ts\""
    );
}

#[test]
fn omp_uses_the_distribution_binary_name_for_discovery() {
    assert_eq!(OhMyPiAdapter.executable_name(), "omp");
}

#[test]
fn availability_reports_each_catalog_binary_from_current_path() {
    let availability = discover_availability();
    assert_eq!(
        availability
            .iter()
            .map(|agent| agent.id)
            .collect::<Vec<_>>(),
        vec!["claude", "codex", "opencode", "pi", "omp"]
    );
    for agent in &availability {
        // omp's id and its binary are the same string.
        let expected_program = agent.id;
        assert_eq!(
            agent.executable,
            find_executable_in_path(expected_program, &std::env::var_os("PATH").unwrap()),
            "{} must resolve its distribution executable from the process PATH",
            expected_program
        );
        assert_eq!(agent.is_available(), agent.executable.is_some());
        assert_eq!(
            agent.status_label(),
            if agent.is_available() {
                "Available"
            } else {
                "Not found on PATH"
            }
        );
    }
}

/// unix only: there, an extensionless name with the execute bit set *is* a
/// command, and it is the only shape the search knows. The Windows
/// counterpart is
/// [`path_lookup_ignores_an_extensionless_shim_on_windows`], which pins the
/// opposite rule.
#[cfg(unix)]
#[test]
fn path_lookup_requires_an_executable_file_and_does_not_launch_it() {
    let root = std::env::temp_dir().join(format!("sirio-agent-path-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create fixture directory");
    let executable = root.join("demo-agent");
    std::fs::write(&executable, b"not launched").expect("write fixture");

    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&executable)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).expect("make fixture executable");
    }

    let path = std::ffi::OsString::from(root.as_os_str());
    assert_eq!(
        find_executable_in_path("demo-agent", &path),
        Some(executable)
    );
    assert_eq!(find_executable_in_path("missing-agent", &path), None);
    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// Windows regression, reproduced from a real install: npm lays down three
/// files for a Node-hosted CLI — `pi` (a `#!/bin/sh` shim for Git Bash),
/// `pi.cmd` and `pi.ps1`. Only the `.cmd` is a command as far as Windows is
/// concerned; cmd.exe never executes the extensionless twin.
///
/// A search that probes the bare name first therefore "finds" the sh shim,
/// stops, and hands back a path that fails at spawn time — a worse outcome
/// than reporting the agent missing, because the failure surfaces at launch
/// instead of at discovery. `pi` and `omp` are the entries seen in this
/// shape so far, but it is a property of an npm install rather than of those
/// two ids — any catalog CLI installed that way arrives as the same triple.
///
/// The second half pins the other edge: with no PATHEXT hit at all, the
/// lone extensionless file must NOT rescue the lookup.
#[cfg(windows)]
#[test]
fn path_lookup_ignores_an_extensionless_shim_on_windows() {
    let root = std::env::temp_dir().join(format!("sirio-agent-shim-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture directory");
    let shim = root.join("demo-agent");
    std::fs::write(&shim, b"#!/bin/sh\nexec node cli.js\n").expect("write posix shim fixture");
    let command = root.join("demo-agent.cmd");
    std::fs::write(&command, b"@echo off").expect("write cmd fixture");

    let path = std::ffi::OsString::from(root.as_os_str());
    assert_eq!(
        find_executable_in_path("demo-agent", &path),
        Some(command.clone()),
        "the PATHEXT hit must win over the extensionless sh shim sitting \
         next to it, whatever the directory order returns first"
    );

    std::fs::remove_file(&command).expect("remove cmd fixture");
    assert_eq!(
        find_executable_in_path("demo-agent", &path),
        None,
        "with no PATHEXT hit the bare name is not a command on Windows, so \
         discovery must report the agent missing rather than hand back a \
         path cmd.exe cannot launch"
    );

    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// A name that already carries a PATHEXT extension is explicit and must be
/// probed exactly as written, not suffixed again into `sirioctl.exe.COM`.
#[cfg(windows)]
#[test]
fn path_lookup_takes_an_explicit_extension_verbatim() {
    let root = std::env::temp_dir().join(format!("sirio-agent-explicit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture directory");
    let command = root.join("demo-agent.exe");
    std::fs::write(&command, b"MZ").expect("write exe fixture");

    let path = std::ffi::OsString::from(root.as_os_str());
    assert_eq!(
        find_executable_in_path("demo-agent.exe", &path),
        Some(command),
        "an explicit extension resolves as written"
    );
    // Case-insensitively spelled the other way round: still explicit.
    assert!(
        find_executable_in_path("demo-agent.EXE", &path).is_some(),
        "PATHEXT membership is case-insensitive, so .EXE is explicit too"
    );
    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// A malformed Windows PATH component must not make the complete availability
/// sweep fail. This is the shape produced by an installer that leaves leading
/// whitespace on one PATH entry: the invalid candidate is a clean miss, while
/// valid entries still resolve their agents.
#[cfg(windows)]
#[test]
fn try_discover_ignores_invalid_name_from_whitespace_path_entry() {
    let root = std::env::temp_dir().join(format!(
        "sirio-agent-invalid-path-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let good = root.join("good");
    std::fs::create_dir_all(&good).expect("create fixture directory");
    for name in ["opencode.cmd", "pi.cmd"] {
        std::fs::write(good.join(name), b"@echo off").expect("write shim fixture");
    }

    let malformed = std::ffi::OsString::from(format!("   {}", good.display()));
    let search = std::env::join_paths([malformed, good.as_os_str().to_os_string()])
        .expect("join PATH");
    let discovered = try_discover_availability_in(&search)
        .expect("an invalid PATH component must not fail the availability sweep");

    assert_eq!(
        discovered.iter().map(|agent| agent.id).collect::<Vec<_>>(),
        vec!["claude", "codex", "opencode", "pi", "omp"]
    );
    assert!(discovered
        .iter()
        .find(|agent| agent.id == "claude")
        .expect("claude row")
        .executable
        .is_none());
    assert_eq!(
        discovered
            .iter()
            .find(|agent| agent.id == "opencode")
            .expect("opencode row")
            .executable,
        Some(good.join("opencode.cmd"))
    );
    assert_eq!(
        discovered
            .iter()
            .find(|agent| agent.id == "pi")
            .expect("pi row")
            .executable,
        Some(good.join("pi.cmd"))
    );

    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// Windows resolves commands through `PATHEXT`, and discovery must agree
/// with that or it reports npm shims — `claude.cmd`, `codex.cmd` — as "not
/// found on PATH" even though cmd.exe would launch them. This is the real
/// install shape of the catalog on Windows: the `.cmd` shell script with no
/// extensionless twin.
///
/// The comparison against the fixture path is deliberately exact and
/// case-sensitive. `PATHEXT` spells its extensions in uppercase, so the
/// probe that finds `demo-agent.cmd` on disk is the candidate
/// `demo-agent.CMD` — a case-insensitive filesystem matches them. The
/// result must carry the name the filesystem stored, not the candidate's
/// spelling: the returned path is user-visible (settings, hook configs),
/// and any other surname would make this test fail on purpose.
#[cfg(windows)]
#[test]
fn path_lookup_finds_a_pathext_shim_cmd() {
    let root = std::env::temp_dir().join(format!("sirio-agent-cmd-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create fixture directory");
    let shim = root.join("demo-agent.cmd");
    std::fs::write(&shim, b"@echo off").expect("write shim fixture");

    let path = std::ffi::OsString::from(root.as_os_str());
    assert_eq!(
        find_executable_in_path("demo-agent", &path),
        Some(shim.clone()),
        "the .cmd shim must resolve through the PATHEXT extension list"
    );

    // A file whose extension is not in PATHEXT must stay invisible: only the
    // listed suffixes make a name a command the way Windows defines it.
    let not_a_command = root.join("demo-agent.xyz");
    std::fs::write(&not_a_command, b"not a command").expect("write decoy fixture");
    assert_eq!(
        find_executable_in_path("demo-agent", &path),
        Some(shim),
        "the PATHEXT hit wins over a non-PATHEXT sibling file"
    );

    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// F-SET-17: discovery must be able to say "I could not check" instead of
/// collapsing a probe failure into "Not found on PATH". The checked lookup
/// still lets a hit later in PATH win over an earlier unreadable directory
/// — the error is only surfaced when it makes the absence claim unsafe.
#[cfg(unix)]
#[test]
fn checked_lookup_prefers_a_found_binary_over_an_earlier_probe_error() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("sirio-agent-checked-path-{}", std::process::id()));
    let locked = root.join("locked");
    let good = root.join("good");
    std::fs::create_dir_all(&locked).expect("create locked dir");
    std::fs::create_dir_all(&good).expect("create good dir");
    let executable = good.join("demo-agent");
    std::fs::write(&executable, b"not launched").expect("write fixture");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
        .expect("make fixture executable");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
        .expect("lock the directory");

    let search = std::env::join_paths([&locked, &good]).expect("join paths");

    // Found later in PATH → the earlier unreadable directory is irrelevant.
    let found = find_executable_in_path_checked("demo-agent", &search)
        .expect("a real hit later in PATH must win over an earlier probe error");
    assert_eq!(found, Some(executable));

    // Absent everywhere, with a probe error on the way → claiming absence
    // would be a guess, so the lookup says what it could not check.
    let error = find_executable_in_path_checked("missing-agent", &search)
        .expect_err("an unreadable PATH directory makes absence unknowable");
    match &error {
        DiscoveryError::Probe {
            program, source, ..
        } => {
            assert_eq!(program, "missing-agent");
            assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
        }
        other => panic!("unexpected discovery error variant: {other:?}"),
    }
    let message = error.to_string();
    assert!(
        message.contains("missing-agent") && message.contains("locked"),
        "the error names the program and the unprobeable path: {message}"
    );

    // The legacy infallible lookup keeps its collapsed behavior for callers
    // that have not opted into the distinction.
    assert_eq!(find_executable_in_path("missing-agent", &search), None);

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("unlock for cleanup");
    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

/// F-SET-17: the sweep-level entry points. A PATH made entirely of an
/// unreadable directory fails the sweep; a PATH holding all five
/// distribution binaries resolves every adapter (omp's binary is
/// `omp`, per `OhMyPiAdapter::executable_name`); recovery after the
/// permission is fixed is
/// the "observe the next result" half of the clause.
#[cfg(unix)]
#[test]
fn try_discover_fails_on_unsafe_absence_and_recovers_when_the_cause_is_fixed() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("sirio-agent-try-discover-{}", std::process::id()));
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("create bin dir");
    for name in ["claude", "codex", "opencode", "pi", "omp"] {
        let file = bin.join(name);
        std::fs::write(&file, b"not launched").expect("write fixture");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
            .expect("make fixture executable");
    }
    let search = std::ffi::OsString::from(bin.as_os_str());

    let discovered =
        try_discover_availability_in(&search).expect("a readable PATH discovers cleanly");
    assert_eq!(discovered.len(), 5);
    assert!(
        discovered.iter().all(AgentAvailability::is_available),
        "all five distribution binaries resolve"
    );

    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o000))
        .expect("lock the directory");
    let error = try_discover_availability_in(&search)
        .expect_err("an unprobeable PATH fails the sweep instead of claiming absence");
    assert!(
        error.to_string().contains("could not probe"),
        "the sweep error is renderable: {error}"
    );

    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
        .expect("unlock the directory");
    let recovered = try_discover_availability_in(&search)
        .expect("fixing the cause makes the next refresh succeed");
    assert!(recovered.iter().all(AgentAvailability::is_available));

    std::fs::remove_dir_all(root).expect("remove fixture directory");
}

// ---------------------------------------------------------------------------
// Resume commands
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
#[test]
fn claude_resume_command() {
    assert_eq!(
        ClaudeCodeAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("claude --resume 'sess-abc'".to_string())
    );
}

/// Acceptance (win-quoting): a resume with a session reference containing
/// spaces must emerge as ONE cmd.exe double-quoted token — the form
/// verified against a live cmd.exe to arrive intact at the child.
#[cfg(windows)]
#[test]
fn claude_resume_command() {
    assert_eq!(
        ClaudeCodeAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("claude --resume \"sess-abc\"".to_string())
    );

    let spaced = ClaudeCodeAdapter
        .resume_command(WORKTREE, PANE_ID, SIRIOCTL, "my session abc")
        .expect("claude resumes");
    assert_eq!(spaced, "claude --resume \"my session abc\"");
}

#[cfg(not(windows))]
#[test]
fn codex_resume_command() {
    assert_eq!(
        CodexAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some(
            "codex -c 'notify=[\"/usr/local/bin/sirioctl\",\"notify\",\"--session\",\"12345678-1234-1234-1234-123456789abc\",\"--status\",\"needs-input\"]' resume 'sess-abc'"
                .to_string()
        )
    );
}

/// The Windows resume form: the `-c` override is one cmd double-quoted
/// token with its embedded JSON quotes doubled, and the spaced session
/// reference is a second one. cmd.exe collapses the doubled quotes back
/// and the child receives `-c notify=[...] resume <ref>` exactly.
#[cfg(windows)]
#[test]
fn codex_resume_command() {
    assert_eq!(
        CodexAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some(
            "codex -c \"notify=[\"\"/usr/local/bin/sirioctl\"\",\"\"notify\"\",\"\"--session\"\",\"\"12345678-1234-1234-1234-123456789abc\"\",\"\"--status\"\",\"\"needs-input\"\"]\" resume \"sess-abc\""
                .to_string()
        )
    );
}

#[cfg(not(windows))]
#[test]
fn opencode_resume_command() {
    assert_eq!(
        OpenCodeAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("opencode --session 'sess-abc'".to_string())
    );
}

#[cfg(windows)]
#[test]
fn opencode_resume_command() {
    assert_eq!(
        OpenCodeAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("opencode --session \"sess-abc\"".to_string())
    );
}

#[cfg(not(windows))]
#[test]
fn pi_resume_command() {
    assert_eq!(
        PiAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("pi --session 'sess-abc'".to_string())
    );
}

#[cfg(windows)]
#[test]
fn pi_resume_command() {
    assert_eq!(
        PiAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("pi --session \"sess-abc\"".to_string())
    );
}

#[cfg(not(windows))]
#[test]
fn omp_resume_command() {
    assert_eq!(
        OhMyPiAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("omp --hook '/Users/me/sirio/.sirio/omp-hook.ts' --resume='sess-abc'".to_string(),)
    );
}

/// Acceptance (win-quoting): a resume whose worktree path contains spaces
/// must produce a line cmd.exe parses into ONE hook-path argument — the
/// exact case that previously split the path at the space and handed the
/// single-quote characters to omp.
#[cfg(windows)]
#[test]
fn omp_resume_command() {
    assert_eq!(
        OhMyPiAdapter.resume_command(WORKTREE, PANE_ID, SIRIOCTL, "sess-abc"),
        Some("omp --hook \"/Users/me/sirio/.sirio/omp-hook.ts\" --resume=\"sess-abc\"".to_string())
    );

    let spaced = OhMyPiAdapter
        .resume_command(
            "C:\\Users\\me\\my worktree\\sirio",
            PANE_ID,
            SIRIOCTL,
            "sess-abc",
        )
        .expect("omp resumes");
    assert_eq!(
        spaced,
        "omp --hook \"C:\\Users\\me\\my worktree\\sirio/.sirio/omp-hook.ts\" --resume=\"sess-abc\""
    );
}

// ---------------------------------------------------------------------------
// The Codex TOML trap: slashes must survive unescaped
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
#[test]
fn codex_notify_override_survives_shell_and_json_round_trip() {
    // Mirror of the Swift `verifyCodexRoundTrip`: strip `codex -c `, shell-
    // unquote the single-quoted value, then decode `notify=[...]` as JSON
    // and compare with the intended arguments. A `\/`-escaped path would
    // still JSON-decode here — the failure would happen earlier, in Codex's
    // TOML parser — so additionally assert no slash escaping anywhere.
    let cmd = CodexAdapter.command(WORKTREE, PANE_ID, SIRIOCTL);

    let after_prefix = cmd.strip_prefix("codex -c ").expect("prefix");
    assert!(
        !after_prefix.contains("\\/"),
        "TOML would reject the override"
    );

    let inner = shell_unquote(after_prefix);
    let notify = inner.strip_prefix("notify=").expect("notify= prefix");
    let args: Vec<String> = serde_json::from_str(notify).expect("override decodes as JSON");
    assert_eq!(
        args,
        vec![
            SIRIOCTL.to_string(),
            "notify".to_string(),
            "--session".to_string(),
            PANE_ID.to_string(),
            "--status".to_string(),
            "needs-input".to_string(),
        ]
    );
}

#[test]
fn codex_override_with_slashy_sirioctl_path_keeps_slashes_unescaped() {
    let literal = json_string_literal("/Users/John Smith/bin/sirioctl");
    assert_eq!(literal, "\"/Users/John Smith/bin/sirioctl\"");
    assert!(!literal.contains("\\/"));
}

/// Strips the outer single quotes and unescapes embedded `'\''` sequences —
/// the inverse of the POSIX [`shell_quote`].
#[cfg(not(windows))]
fn shell_unquote(quoted: &str) -> String {
    assert!(
        quoted.starts_with('\'') && quoted.ends_with('\''),
        "single-quoted: {quoted}"
    );
    quoted[1..quoted.len() - 1].replace("'\\''", "'")
}

/// cmd.exe's unquoting for the double-quote form: outer pair stripped,
/// every `""` collapsed to `"`. This is the inverse of the Windows
/// [`shell_quote`], mirroring what cmd does on its way to the child's
/// argv parser.
#[cfg(windows)]
fn shell_unquote(quoted: &str) -> String {
    assert!(
        quoted.starts_with('"') && quoted.ends_with('"'),
        "double-quoted: {quoted}"
    );
    quoted[1..quoted.len() - 1].replace("\"\"", "\"")
}

/// The Windows mirror of `codex_notify_override_survives_shell_and_json_round_trip`,
/// with a sirioctl path that contains spaces and backslashes — the case
/// that previously split at the spaces and handed the single-quote
/// characters to Codex. The command string is cmd-unquoted the way a
/// spawned child would receive it, then JSON-decoded.
#[cfg(windows)]
#[test]
fn codex_notify_override_survives_shell_and_json_round_trip() {
    let sirioctl = r"C:\.herdr\sirioctl bin\\sirioctl.exe";
    let cmd = CodexAdapter.command(WORKTREE, PANE_ID, sirioctl);

    let after_prefix = cmd.strip_prefix("codex -c ").expect("prefix");
    assert!(
        !after_prefix.contains("\\/"),
        "TOML would reject the override"
    );

    let inner = shell_unquote(after_prefix);
    let notify = inner.strip_prefix("notify=").expect("notify= prefix");
    let args: Vec<String> = serde_json::from_str(notify).expect("override decodes as JSON");
    assert_eq!(
        args,
        vec![
            sirioctl.to_string(),
            "notify".to_string(),
            "--session".to_string(),
            PANE_ID.to_string(),
            "--status".to_string(),
            "needs-input".to_string(),
        ]
    );
}

#[test]
fn claude_prepare_writes_worktree_local_settings_with_all_five_hooks() {
    let worktree = TempDir::new();
    ClaudeCodeAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    let settings = std::fs::read_to_string(worktree.path().join(".claude/settings.local.json"))
        .expect("settings.local.json written inside the worktree");
    let json: serde_json::Value = serde_json::from_str(&settings).expect("valid JSON");

    let hooks = json["hooks"].as_object().expect("hooks object");
    // Sort explicitly: a JSON object's key order is not semantically
    // meaningful, and it actually changes here depending on whether cargo's
    // feature unification has turned on serde_json/preserve_order for the
    // workspace build (insertion order) or not (BTreeMap order).
    let mut hook_keys = hooks.keys().collect::<Vec<_>>();
    hook_keys.sort();
    assert_eq!(
        hook_keys,
        [
            "Notification",
            "SessionEnd",
            "SessionStart",
            "Stop",
            "UserPromptSubmit"
        ],
        "sorted keys, exactly the five hook events"
    );

    let command_of = |event: &str| {
        hooks[event][0]["hooks"][0]["command"]
            .as_str()
            .expect("command")
            .to_string()
    };
    let quoted = shell_quote(SIRIOCTL);
    assert_eq!(
        command_of("Stop"),
        format!("{quoted} notify --session {PANE_ID} --status needs-input --stdin-json")
    );
    assert_eq!(
        command_of("Notification"),
        format!("{quoted} notify --session {PANE_ID} --status needs-input --stdin-json")
    );
    assert_eq!(
        command_of("SessionStart"),
        format!("{quoted} notify --session {PANE_ID} --status needs-input --stdin-json")
    );
    assert_eq!(
        command_of("UserPromptSubmit"),
        format!("{quoted} notify --session {PANE_ID} --status running --stdin-json")
    );
    assert_eq!(
        command_of("SessionEnd"),
        format!("{quoted} notify --session {PANE_ID} --status done --stdin-json")
    );
}

#[test]
fn claude_prepare_preserves_existing_settings_keys() {
    let worktree = TempDir::new();
    let settings_path = worktree.path().join(".claude/settings.local.json");
    std::fs::create_dir_all(settings_path.parent().expect("parent")).expect("dir");
    std::fs::write(
        &settings_path,
        r#"{
  "model": "opus",
  "hooks": {
    "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "echo hi"}]}],
    "Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "echo old"}]}]
  }
}"#,
    )
    .expect("seed");

    ClaudeCodeAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).expect("read back"))
            .expect("valid JSON");

    assert_eq!(json["model"], "opus", "unrelated top-level key preserved");
    let hooks = json["hooks"].as_object().expect("hooks object");
    assert_eq!(
        hooks["PreToolUse"][0]["hooks"][0]["command"], "echo hi",
        "unrelated hooks key preserved"
    );
    assert_ne!(
        hooks["Stop"][0]["hooks"][0]["command"], "echo old",
        "the five hook events are replaced"
    );
}

#[test]
fn opencode_prepare_writes_the_session_plugin() {
    let worktree = TempDir::new();
    OpenCodeAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    let plugin = std::fs::read_to_string(worktree.path().join(".opencode/plugin/sirio-session.js"))
        .expect("plugin written inside the worktree");
    assert!(plugin.contains(&json_string_literal(SIRIOCTL)));
    assert!(plugin.contains(PANE_ID));
    assert!(plugin.contains("session-ref"));
    assert!(plugin.contains("export const SirioSession"));
}

#[test]
fn omp_prepare_writes_the_hook_file() {
    let worktree = TempDir::new();
    OhMyPiAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    let hook = std::fs::read_to_string(worktree.path().join(".sirio/omp-hook.ts"))
        .expect("hook written inside the worktree");
    assert!(hook.contains(&json_string_literal(SIRIOCTL)));
    assert!(hook.contains(PANE_ID));
    assert!(hook.contains("pi.on(\"turn_start\""));
    assert!(hook.contains("--agent-session"));
}

#[test]
fn codex_and_pi_prepare_leave_the_worktree_untouched() {
    let worktree = TempDir::new();
    CodexAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");
    PiAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    assert!(worktree.tree().is_empty(), "no-op prepares write nothing");
}

#[test]
fn each_adapter_prepare_creates_only_its_own_files() {
    let worktree = TempDir::new();
    let path = worktree.path().to_str().unwrap();
    ClaudeCodeAdapter
        .prepare(path, PANE_ID, SIRIOCTL)
        .expect("claude");
    OpenCodeAdapter
        .prepare(path, PANE_ID, SIRIOCTL)
        .expect("opencode");
    OhMyPiAdapter.prepare(path, PANE_ID, SIRIOCTL).expect("omp");
    CodexAdapter
        .prepare(path, PANE_ID, SIRIOCTL)
        .expect("codex");
    PiAdapter.prepare(path, PANE_ID, SIRIOCTL).expect("pi");

    assert_eq!(
        worktree.tree(),
        vec![
            // OpenCode and OMP share the non-Claude skill destination
            // (F-AGENT-SAFE-01); the second `prepare` to run overwrites the
            // first's byte-identical file rather than being refused —
            // `install_skill` only refuses a file lacking the marker.
            PathBuf::from(".agents/skills/sirio/SKILL.md"),
            PathBuf::from(".claude/settings.local.json"),
            PathBuf::from(".claude/skills/sirio/SKILL.md"),
            PathBuf::from(".opencode/plugin/sirio-session.js"),
            PathBuf::from(".sirio/omp-hook.ts"),
        ]
    );
}

#[test]
fn prepare_keeps_real_global_config_mtimes_unchanged() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is set for this test"));
    let global_files = [
        home.join(".claude/settings.json"),
        home.join(".codex/config.toml"),
    ];
    let before = global_files
        .iter()
        .map(|path| {
            (
                path.clone(),
                std::fs::metadata(path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok()),
            )
        })
        .collect::<Vec<_>>();
    println!("global config mtimes before: {before:?}");

    let worktree = TempDir::new();
    for adapter in ALL {
        adapter
            .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
            .unwrap_or_else(|error| panic!("{} prepare failed: {error}", adapter.id()));
    }

    let after = global_files
        .iter()
        .map(|path| {
            (
                path.clone(),
                std::fs::metadata(path)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok()),
            )
        })
        .collect::<Vec<(PathBuf, Option<SystemTime>)>>();
    println!("global config mtimes after: {after:?}");
    assert_eq!(
        before, after,
        "prepare must not create or modify user-global agent config"
    );
}

// ---------------------------------------------------------------------------
// F-AGENT-SAFE-01: the skill provisioner refuses to overwrite a file it
// did not itself write. Ported from `SirioSkillProvisionerTests` in the
// Swift app.
// ---------------------------------------------------------------------------

#[test]
fn install_skill_writes_the_marker_bearing_file_for_claude() {
    let worktree = TempDir::new();
    let markdown = format!("{SKILL_MANAGED_MARKER}\n# Sirio\n");
    install_skill(&markdown, "claude", worktree.path().to_str().unwrap()).expect("install");

    let written = std::fs::read_to_string(worktree.path().join(".claude/skills/sirio/SKILL.md"))
        .expect("skill file written at claude's destination");
    assert_eq!(written, markdown);
}

#[test]
fn install_skill_shares_one_destination_across_codex_opencode_pi_and_omp() {
    for id in ["codex", "opencode", "pi", "omp"] {
        let worktree = TempDir::new();
        let markdown = format!("{SKILL_MANAGED_MARKER}\n# Sirio\n");
        install_skill(&markdown, id, worktree.path().to_str().unwrap())
            .unwrap_or_else(|error| panic!("{id} install failed: {error}"));
        assert!(
            worktree
                .path()
                .join(".agents/skills/sirio/SKILL.md")
                .exists(),
            "{id} installs to the shared .agents destination"
        );
    }
}

#[test]
fn install_skill_refuses_markdown_missing_its_own_marker() {
    let worktree = TempDir::new();
    let error = install_skill(
        "# Sirio\nno marker here\n",
        "claude",
        worktree.path().to_str().unwrap(),
    )
    .expect_err("markdown without the marker must be refused");
    assert!(matches!(error, PrepareError::MissingSkillMarker));
    assert!(
        !worktree
            .path()
            .join(".claude/skills/sirio/SKILL.md")
            .exists(),
        "a refused install must not write anything"
    );
}

#[test]
fn install_skill_refuses_an_unsupported_agent_id() {
    let worktree = TempDir::new();
    let markdown = format!("{SKILL_MANAGED_MARKER}\n# Sirio\n");
    let error = install_skill(
        &markdown,
        "not-a-real-agent",
        worktree.path().to_str().unwrap(),
    )
    .expect_err("an unknown agent id must be refused");
    assert!(matches!(error, PrepareError::UnsupportedSkillAgent(id) if id == "not-a-real-agent"));
}

#[test]
fn install_skill_refuses_to_overwrite_a_file_it_did_not_write() {
    let worktree = TempDir::new();
    let destination = worktree.path().join(".claude/skills/sirio/SKILL.md");
    std::fs::create_dir_all(destination.parent().unwrap()).expect("dir");
    std::fs::write(&destination, "hand-authored notes, not Sirio's\n").expect("seed file");

    let markdown = format!("{SKILL_MANAGED_MARKER}\n# Sirio\n");
    let error = install_skill(&markdown, "claude", worktree.path().to_str().unwrap())
        .expect_err("an unmanaged existing file must be refused");
    assert!(matches!(error, PrepareError::UnmanagedSkillFile(path) if path == destination));

    let untouched = std::fs::read_to_string(&destination).expect("still there");
    assert_eq!(
        untouched, "hand-authored notes, not Sirio's\n",
        "the user's own file must survive a refused install byte for byte"
    );
}

#[test]
fn install_skill_overwrites_its_own_previously_installed_file() {
    let worktree = TempDir::new();
    let path = worktree.path().to_str().unwrap();
    // Simulate a file an older Sirio build already installed: it carries
    // the shared prefix but not today's exact marker sentence, the way a
    // real prior release's wording would.
    let destination = worktree.path().join(".claude/skills/sirio/SKILL.md");
    std::fs::create_dir_all(destination.parent().unwrap()).expect("dir");
    std::fs::write(
        &destination,
        "<!-- Machine-managed by Sirio (build 1). Do not edit. -->\nold content\n",
    )
    .expect("seed a prior-release install");

    let markdown = format!("{SKILL_MANAGED_MARKER}\n# Sirio v2\n");
    install_skill(&markdown, "claude", path).expect(
        "a file recognized by the shared managed-file prefix, even with older marker wording, \
         is Sirio's own and may be overwritten",
    );

    let written = std::fs::read_to_string(worktree.path().join(".claude/skills/sirio/SKILL.md"))
        .expect("rewritten");
    assert_eq!(written, markdown);
}

#[test]
fn claude_prepare_installs_the_bundled_skill_with_its_marker() {
    let worktree = TempDir::new();
    ClaudeCodeAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect("prepare");

    let skill = std::fs::read_to_string(worktree.path().join(".claude/skills/sirio/SKILL.md"))
        .expect("prepare installs the skill alongside the hook settings");
    assert!(skill.contains(SKILL_MANAGED_MARKER));
}

#[test]
fn claude_prepare_refuses_to_run_over_an_unmanaged_skill_file() {
    let worktree = TempDir::new();
    let destination = worktree.path().join(".claude/skills/sirio/SKILL.md");
    std::fs::create_dir_all(destination.parent().unwrap()).expect("dir");
    std::fs::write(&destination, "not Sirio's file\n").expect("seed file");

    let error = ClaudeCodeAdapter
        .prepare(worktree.path().to_str().unwrap(), PANE_ID, SIRIOCTL)
        .expect_err("prepare must refuse, not silently skip the skill and proceed");
    assert!(matches!(error, PrepareError::UnmanagedSkillFile(_)));
    assert!(
        !worktree.path().join(".claude/settings.local.json").exists(),
        "the skill install runs before hook settings, so a refusal must leave the worktree \
         exactly as it found it"
    );
}

// ---------------------------------------------------------------------------
// User-global hooks (Settings → Install Hooks)
// ---------------------------------------------------------------------------

fn environment(values: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read json")).expect("valid json")
}

const CLAUDE_EVENTS: [&str; 5] = [
    "Stop",
    "Notification",
    "SessionStart",
    "UserPromptSubmit",
    "SessionEnd",
];

/// Settings → Install Hooks writes the five hook arrays into the user's own
/// `~/.claude/settings.json`. One file serves every pane, so the commands
/// carry no `--session`: sirioctl resolves the pane from `SIRIO_PANE_ID`.
#[test]
fn claude_global_hooks_land_in_the_user_settings_without_a_pane_id() {
    let home = TempDir::new();
    let outcome = ClaudeCodeAdapter
        .install_global_hooks(home.path(), &environment(&[]), SIRIOCTL)
        .expect("install");
    let settings_path = home.path().join(".claude/settings.json");
    assert_eq!(outcome, GlobalHookInstall::Written(settings_path.clone()));

    let root = read_json(&settings_path);
    let hooks = root["hooks"].as_object().expect("hooks object");
    for event in CLAUDE_EVENTS {
        let command = hooks[event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_else(|| panic!("{event} has a command hook"));
        assert!(
            command.starts_with(&format!("{} notify --status ", shell_quote(SIRIOCTL))),
            "{event}: {command}"
        );
        assert!(
            !command.contains("--session"),
            "{event} must not pin a pane: {command}"
        );
        assert!(command.ends_with("--stdin-json"), "{event}: {command}");
    }
    assert_eq!(
        hooks["UserPromptSubmit"][0]["hooks"][0]["command"],
        format!(
            "{} notify --status running --stdin-json",
            shell_quote(SIRIOCTL)
        )
    );
    assert_eq!(
        hooks["SessionEnd"][0]["hooks"][0]["command"],
        format!(
            "{} notify --status done --stdin-json",
            shell_quote(SIRIOCTL)
        )
    );
}

/// The user's global settings hold far more than hooks; only the five
/// Sirio events are replaced, everything else keeps its meaning.
#[test]
fn claude_global_hooks_preserve_the_users_other_settings() {
    let home = TempDir::new();
    let dir = home.path().join(".claude");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("settings.json"),
        r#"{"model":"opus","permissions":{"allow":["Bash(ls)"]},"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[]}]}}"#,
    )
    .expect("seed settings");

    ClaudeCodeAdapter
        .install_global_hooks(home.path(), &environment(&[]), SIRIOCTL)
        .expect("install");

    let root = read_json(&dir.join("settings.json"));
    assert_eq!(root["model"], "opus");
    assert_eq!(root["permissions"]["allow"][0], "Bash(ls)");
    assert_eq!(root["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    for event in CLAUDE_EVENTS {
        assert!(root["hooks"][event].is_array(), "{event} installed");
    }
}

/// `CLAUDE_CONFIG_DIR` relocates Claude's whole config directory; the hooks
/// must follow it, or they land where Claude never looks.
#[test]
fn claude_global_hooks_honour_claude_config_dir() {
    let home = TempDir::new();
    let config_dir = TempDir::new();
    let outcome = ClaudeCodeAdapter
        .install_global_hooks(
            home.path(),
            &environment(&[("CLAUDE_CONFIG_DIR", config_dir.path().to_str().unwrap())]),
            SIRIOCTL,
        )
        .expect("install");
    assert_eq!(
        outcome,
        GlobalHookInstall::Written(config_dir.path().join("settings.json"))
    );
    assert!(
        home.tree().is_empty(),
        "nothing lands under ~ when the override is set"
    );
}

/// Codex has one hook: the `notify` argv in `~/.codex/config.toml`. The
/// user's config is TOML with comments and tables written by hand —
/// setting one key must leave the rest, comments included, in place.
#[test]
fn codex_global_hooks_set_notify_in_the_user_config_and_keep_the_rest() {
    let home = TempDir::new();
    let dir = home.path().join(".codex");
    std::fs::create_dir_all(&dir).expect("dir");
    let config_path = dir.join("config.toml");
    std::fs::write(
        &config_path,
        "# my config\nmodel = \"o3\"\n\n[sandbox]\nmode = \"read-only\"\n",
    )
    .expect("seed config");

    let outcome = CodexAdapter
        .install_global_hooks(home.path(), &environment(&[]), SIRIOCTL)
        .expect("install");
    assert_eq!(outcome, GlobalHookInstall::Written(config_path.clone()));

    let text = std::fs::read_to_string(&config_path).expect("read config");
    assert!(text.contains("# my config"), "comments survive: {text}");
    let document: toml_edit::DocumentMut = text.parse().expect("still valid TOML");
    assert_eq!(document["model"].as_str(), Some("o3"));
    assert_eq!(document["sandbox"]["mode"].as_str(), Some("read-only"));
    let notify = document
        .as_table()
        .get("notify")
        .and_then(|item| item.as_array())
        .expect("notify is a top-level array, not a [sandbox] key");
    let argv = notify
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect::<Vec<_>>();
    assert_eq!(argv, [SIRIOCTL, "notify", "--status", "needs-input"]);
}

/// A Windows install path carries backslashes and a space: the TOML must
/// escape them so Codex reads back the exact path.
#[test]
fn codex_global_hooks_survive_a_windows_sirioctl_path() {
    let home = TempDir::new();
    let sirioctl = r"C:\Program Files\Sirio\sirioctl.exe";
    CodexAdapter
        .install_global_hooks(home.path(), &environment(&[]), sirioctl)
        .expect("install");

    let text = std::fs::read_to_string(home.path().join(".codex/config.toml")).expect("read");
    let document: toml_edit::DocumentMut = text.parse().expect("valid TOML");
    assert_eq!(document["notify"][0].as_str(), Some(sirioctl));
}

/// `CODEX_HOME` relocates Codex's config the same way `CLAUDE_CONFIG_DIR`
/// does for Claude — the same precedence `codex` itself applies.
#[test]
fn codex_global_hooks_honour_codex_home() {
    let home = TempDir::new();
    let codex_home = TempDir::new();
    let outcome = CodexAdapter
        .install_global_hooks(
            home.path(),
            &environment(&[("CODEX_HOME", codex_home.path().to_str().unwrap())]),
            SIRIOCTL,
        )
        .expect("install");
    assert_eq!(
        outcome,
        GlobalHookInstall::Written(codex_home.path().join("config.toml"))
    );
    assert!(home.tree().is_empty());
}

/// OpenCode loads user-global plugins from `~/.config/opencode/plugins/`;
/// the session-ref plugin goes there, pane-less like the Claude hooks.
#[test]
fn opencode_global_plugin_lands_in_the_user_plugins_dir_without_a_pane_id() {
    let home = TempDir::new();
    let outcome = OpenCodeAdapter
        .install_global_hooks(home.path(), &environment(&[]), SIRIOCTL)
        .expect("install");
    let plugin_path = home
        .path()
        .join(".config/opencode/plugins/sirio-session.js");
    assert_eq!(outcome, GlobalHookInstall::Written(plugin_path.clone()));

    let plugin = std::fs::read_to_string(&plugin_path).expect("read plugin");
    assert!(plugin.contains(&json_string_literal(SIRIOCTL)), "{plugin}");
    assert!(plugin.contains("session-ref --ref"), "{plugin}");
    assert!(
        !plugin.contains("--session"),
        "must not pin a pane: {plugin}"
    );
    assert!(!plugin.contains("__PANE__"), "{plugin}");
    assert!(!plugin.contains("__SIRIOCTL__"), "{plugin}");
}

/// OpenCode's config directory follows `OPENCODE_CONFIG_DIR`, then
/// `XDG_CONFIG_HOME`, then `~/.config` — in that order.
#[test]
fn opencode_global_plugin_honours_the_config_dir_overrides() {
    let home = TempDir::new();
    let xdg = TempDir::new();
    let outcome = OpenCodeAdapter
        .install_global_hooks(
            home.path(),
            &environment(&[("XDG_CONFIG_HOME", xdg.path().to_str().unwrap())]),
            SIRIOCTL,
        )
        .expect("install");
    assert_eq!(
        outcome,
        GlobalHookInstall::Written(xdg.path().join("opencode/plugins/sirio-session.js"))
    );

    let explicit = TempDir::new();
    let outcome = OpenCodeAdapter
        .install_global_hooks(
            home.path(),
            &environment(&[
                ("XDG_CONFIG_HOME", xdg.path().to_str().unwrap()),
                ("OPENCODE_CONFIG_DIR", explicit.path().to_str().unwrap()),
            ]),
            SIRIOCTL,
        )
        .expect("install");
    assert_eq!(
        outcome,
        GlobalHookInstall::Written(explicit.path().join("plugins/sirio-session.js"))
    );
    assert!(home.tree().is_empty());
}

/// Pi has no hook mechanism and omp takes hooks only through `--hook
/// <file>` on its command line: neither has a user-global place to put
/// one, and the honest answer is to say so — not to write a file nothing
/// reads.
#[test]
fn agents_without_a_global_hook_mechanism_say_so() {
    let home = TempDir::new();
    for adapter in [&PiAdapter as &dyn AgentAdapter, &OhMyPiAdapter] {
        let outcome = adapter
            .install_global_hooks(home.path(), &environment(&[]), SIRIOCTL)
            .unwrap_or_else(|error| panic!("{}: {error}", adapter.id()));
        assert_eq!(outcome, GlobalHookInstall::NotSupported, "{}", adapter.id());
    }
    assert!(home.tree().is_empty());
}

/// The Settings button runs every adapter and reports each one, in display
/// order, so the screen can say per agent where the hooks went.
#[test]
fn install_global_hooks_reports_every_adapter_in_display_order() {
    let home = TempDir::new();
    let reports = install_global_hooks(home.path(), &environment(&[]), SIRIOCTL);
    let names = reports
        .iter()
        .map(|report| report.display_name)
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["Claude Code", "Codex", "OpenCode", "Pi", "Oh-My-Pi"]
    );
    assert!(matches!(
        reports[0].outcome,
        Ok(GlobalHookInstall::Written(_))
    ));
    assert!(matches!(
        reports[1].outcome,
        Ok(GlobalHookInstall::Written(_))
    ));
    assert!(matches!(
        reports[2].outcome,
        Ok(GlobalHookInstall::Written(_))
    ));
    assert!(matches!(
        reports[3].outcome,
        Ok(GlobalHookInstall::NotSupported)
    ));
    assert!(matches!(
        reports[4].outcome,
        Ok(GlobalHookInstall::NotSupported)
    ));
}

/// One line per agent: the file that was written, the reason nothing was,
/// or the error — the text the Settings card shows under the button.
#[test]
fn global_hook_report_summary_names_the_file_or_the_reason() {
    let written = GlobalHookReport {
        display_name: "Claude Code",
        outcome: Ok(GlobalHookInstall::Written(PathBuf::from(
            "/home/me/.claude/settings.json",
        ))),
    };
    assert_eq!(
        written.summary_line(),
        "Claude Code: /home/me/.claude/settings.json"
    );

    let unsupported = GlobalHookReport {
        display_name: "Pi",
        outcome: Ok(GlobalHookInstall::NotSupported),
    };
    assert_eq!(
        unsupported.summary_line(),
        "Pi: no user-global hook mechanism"
    );

    let failed = GlobalHookReport {
        display_name: "Codex",
        outcome: Err(PrepareError::UnsupportedSkillAgent("codex".into())),
    };
    assert_eq!(
        failed.summary_line(),
        "Codex: failed — unsupported Sirio agent: codex"
    );
}
