# ADE project 1 — a `sirioctl` that is always there — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `sirioctl` is always current and reachable — in the user's own shell and in every Sirio pane, before and after a quit or an update; every child process Sirio starts runs in the user's environment rather than the AppImage launcher's; the hooks are proven end to end; the skill is true, installed for all five agents, and checked against what a release build serves.

**Architecture:** A new leaf crate `sirio_env` holds the *user environment* (captured from a snapshot the AppImage launcher records, or from the process at the top of `main`) and applies it to child processes; every spawn site calls it. The host protocol gains an additive `env_exact` flag so a session's environment can be exactly the client's. The stable `sirioctl` copy is refreshed by content at every launch and linked onto the user's PATH. The skill is checked by running the real `sirioctl` in a new `--dry-run` mode and comparing the methods with a release build's served list, which moves into `sirio_control`.

**Tech Stack:** Rust 2024 workspace (`rust/`), gpui, portable-pty, bash E2E scripts, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-17-ade-programme-design.md` — §4 (rules) and §5 (project 1). Read both before any task.

## Global Constraints

- Workspace version stays `0.33.0`: the cycle's first `feat:` (#635) already moved it. No task runs `Scripts/set-workspace-version.sh`.
- Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh` — only the user launches them. Iterate with `cargo build -p <crate>` / `cargo nextest run -p <crate>` (`cargo test` shows process-sharing failures in `sirio`).
- Never run `cargo fmt` on a crate or `rustfmt` on a file that declares modules (the workspace is not fmt-clean).
- Commit messages: Conventional Commits, lower-case imperative; **no** `Co-Authored-By` or session lines (project rule).
- A public struct field breaks literals downstream: after changing one, build `cargo build --workspace --all-targets`.
- New `#[test]`s are for units proven in isolation and are written before the code (failure list first); no tautological or change-detector tests.
- Stable copy: `<data>/Sirio/bin/sirioctl` (release), `<data>/Sirio/bin-debug/sirioctl` (debug); `<data>` = absolute `$XDG_DATA_HOME`, else `$HOME/.local/share` (unix) / `%LOCALAPPDATA%` (Windows).
- User-global hooks always name the **release** stable path, from any build.
- The app process keeps the launcher's environment (its WebKit pane needs it); only children get the user environment. `APPIMAGE` stays in the app (`sirio_apply` self-locates through it).
- A startup step never asks for a password, shows a dialog, or blocks the launch.
- Never replace a file Sirio did not create at a PATH-link place or a user-level skill without `SKILL_MANAGED_MARKER`. The stable path is Sirio's and is replaced freely.
- Sirio's names in a `sirio_perf` trace stay content-free `&'static str`.

**Refinements of the spec made by this plan** (Claude's calls, reflected in the spec's §5.3 by Task 13):
- The user environment lives in a leaf crate with a process-wide setter, like `sirio_perf`, rather than a parameter threaded through every API: twelve crates spawn processes.
- With an adopted host that lacks `term.env_exact`, the client sends the user environment **plus an empty value for every variable the app has and the user environment lacks** — best effort, instead of today's behaviour.
- The stable copy is always a copy, never a symlink.
- The launcher snapshot is `env -0 | od -An -tx1 -v` (hex, POSIX tools), so `sirio_env` needs no dependency.

## Review Focus

1. **A pane opened before an update keeps working.** After the stable copy is replaced, an agent's hook running from that pane still reaches Sirio, and a bare `sirioctl` in the pane resolves to the stable directory, not the dead mount — Task 11 cases 2 and 5.
2. **`~/.local/bin` not on the login PATH** (some distros): the card says so, never "installed" — Task 5 test `link_in_a_directory_off_the_login_path_is_reported`.
3. **A second Sirio launching at the same moment** races the copy: one whole file results, never a truncated one — Task 4 test `two_concurrent_installs_leave_one_whole_file`.
4. **Wayland user**: a terminal in Sirio still has `WAYLAND_DISPLAY` although the app removed it from its own process — Task 1 test `launch_environment_is_taken_before_the_app_changes_its_own` and Task 11 case 4.
5. **A user-level skill the user wrote themselves** at `~/.agents/skills/sirio/SKILL.md` is never overwritten, by the button or at startup — Task 8 test `user_skill_without_the_marker_is_left_alone`.

---

### Task 1: `sirio_env` — the user environment

**Files:**
- Create: `rust/crates/sirio_env/Cargo.toml`, `rust/crates/sirio_env/src/lib.rs`
- Modify: `rust/Cargo.toml` (members), `Scripts/build-appimage.sh:228-247` (move the AppRun heredoc to a file), Create: `Scripts/appimage/AppRun`
- Modify: `rust/crates/sirio/src/main.rs:25667-25723` (`fn main` order), `rust/crates/sirio/src/login_path.rs:17-27`
- Test: `rust/crates/sirio_env/src/lib.rs` (`mod tests`), `Scripts/Tests/test-build-appimage.sh`

**Interfaces:**
- Produces (in `sirio_env`, no dependencies, like `sirio_perf`):
  - `pub const LAUNCH_SNAPSHOT_VAR: &str = "SIRIO_LAUNCH_ENV";`
  - `pub const LAUNCHER_ONLY: [&str; 5] = ["APPIMAGE", "APPDIR", "ARGV0", "OWD", LAUNCH_SNAPSHOT_VAR];`
  - `pub fn launch_environment(process: &BTreeMap<String, String>) -> BTreeMap<String, String>` — the decoded snapshot when `SIRIO_LAUNCH_ENV` is present and decodes, else `process`; `LAUNCHER_ONLY` removed in both cases.
  - `pub fn user_environment_from(launch: &BTreeMap<String, String>, login_path: Option<&OsStr>, cli_dir: &Path) -> BTreeMap<String, String>` — `PATH` = `cli_dir`, then the login entries, then the launch entries, deduplicated, empties dropped (the order `login_path::merge_paths` uses today, with `cli_dir` first and removed from later positions).
  - `pub const FOR_CHILDREN: [&str; 1] = ["SIRIO_PANE_HOST_HOME"];` — variables the app exports *for* its children (`session::export_pane_host_root`), copied from the process onto the user environment at `install` time. Every other `set_var`/`remove_var` in `sirio` is app-only and stays out: `GDK_BACKEND` and the `WAYLAND_DISPLAY` removal (`display_backend.rs:100,107`), `GPUI_DISABLE_DIRECT_COMPOSITION` (`main.rs:25704`); `PATH` is handled by `user_environment_from`.
  - `pub fn install(user: BTreeMap<String, String>)` — sets a `OnceLock`, after overlaying `FOR_CHILDREN` from the process; a second call is ignored.
  - `pub fn installed() -> Option<&'static BTreeMap<String, String>>`
  - `pub fn for_child() -> BTreeMap<String, String>` — the installed map, else `std::env::vars()`.
  - `pub fn command(program: impl AsRef<OsStr>) -> std::process::Command` — `Command::new(program)`, then, when installed, `env_clear()` + `envs(map)`. **The only way a site applies the user environment**, so a site's own `.env(..)` calls (git's `LC_ALL=C`, the remote flags) always come after the clear and survive it. There is deliberately no `apply(&mut Command)`.
  - `pub fn absent_from_user() -> Vec<String>` — names in `std::env::vars()` not in the installed map (empty when not installed).
- Produces (in `sirio`): `login_path::login_shell_path() -> Option<OsString>` becomes `pub(crate)`; `adopt_login_shell_path(login: Option<&OsStr>)` takes it instead of computing it.

- [ ] **Step 1: Write the failing tests** (in `sirio_env`, each named for the failure it rules out)
  - `snapshot_absent_falls_back_to_the_process` — no `SIRIO_LAUNCH_ENV` → equals `process` minus `LAUNCHER_ONLY`.
  - `snapshot_restores_values_the_launcher_changed` — process has `PATH=/m/usr/bin:/usr/bin`, `LD_LIBRARY_PATH=/m/lib`, `GTK_THEME=x`; snapshot (hex of `PATH=/usr/bin\0HOME=/h\0`) → result has `PATH=/usr/bin`, `HOME=/h`, no `LD_LIBRARY_PATH`, no `GTK_THEME`.
  - `snapshot_values_may_contain_equals_and_newlines` — `A=b=c\nd` round-trips.
  - `malformed_snapshot_falls_back_to_the_process` — odd-length hex, non-hex digit, non-UTF-8 bytes → process.
  - `entries_without_equals_are_skipped`.
  - `launcher_only_names_never_reach_children` — `APPIMAGE`, `APPDIR`, `ARGV0`, `OWD`, `SIRIO_LAUNCH_ENV` absent even when the snapshot has them.
  - `launch_environment_is_taken_before_the_app_changes_its_own` — a process map without `WAYLAND_DISPLAY` but a snapshot with it → kept (the reason `main` captures first).
  - `cli_dir_comes_first_and_only_once` — launch `PATH` already containing `cli_dir` in the middle → `cli_dir` first, no duplicate.
  - `login_entries_precede_launch_entries`, `empty_path_entries_are_dropped`.
  - `command_without_install_inherits_the_process` (spawn `/usr/bin/env` under `#[cfg(unix)]`, compare with the process).
  - `env_set_after_command_survives_the_clear` (install a map without `LC_ALL`; `command("/usr/bin/env").env("LC_ALL", "C")` → output has `LC_ALL=C` and nothing outside the map). nextest runs each test in its own process, so installing the global in a test is safe.
  - `for_children_variables_reach_the_user_environment` (process has `SIRIO_PANE_HOST_HOME=/h`, the map lacks it → installed map has it).
- [ ] **Step 2: Run** `cargo nextest run -p sirio_env` — expected: compile failure (crate empty).
- [ ] **Step 3: Implement** the functions above. Hex decode by hand; split on `\0`, then at the first `=`.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_env` — expected: all pass.
- [ ] **Step 5: AppRun as a file.** Move the heredoc body of `build-appimage.sh:228-247` verbatim into `Scripts/appimage/AppRun` and `cp` it into `$APPDIR/AppRun`. Add, **as the first statements after the shebang and comments**, the snapshot — taken **only when none is inherited**, because *Restart to update* (`main.rs:6746`, gpui `cx.restart()`) and any relaunch from the app start the new AppRun in the old app's environment, which already carries the old mount's prefixes and the original snapshot; re-snapshotting it would bring the dead mount back and nest the old snapshot inside the new one until `exec` fails with E2BIG. A snapshot above 100 KiB is not exported (a large nix or direnv environment must never stop the launch; the app then falls back to its process environment):
  ```sh
  if [ -z "${SIRIO_LAUNCH_ENV:-}" ]; then
    snapshot="$(env -0 | od -An -tx1 -v | tr -d ' \n')"
    if [ "${#snapshot}" -le 204800 ]; then export SIRIO_LAUNCH_ENV="$snapshot"; fi
    unset snapshot
  fi
  ```
  `sirio_apply` and the restart keep the app's environment (so the snapshot travels with them). In `Scripts/Tests/test-build-appimage.sh`, assert the built `AppRun` contains `SIRIO_LAUNCH_ENV` before the first `export PATH=`.
- [ ] **Step 6: Wire `main`.** At the very top of `fn main` (before `display_backend::prepare_environment()`), compute `let launch = sirio_env::launch_environment(&std::env::vars().collect())`. Keep the order of the existing mutations. Replace `adopt_login_shell_path()` with `let login = login_path::login_shell_path(); login_path::adopt_login_shell_path(login.as_deref());`, and **after** `session::export_pane_host_root()` (`main.rs:25723`, so `FOR_CHILDREN` is in the process) call `sirio_env::install(sirio_env::user_environment_from(&launch, login.as_deref(), &cli_dir))`, where `cli_dir` is the stable copy's directory for this build (Task 4's `sirioctl_install_dir`; until Task 4 lands, use the current `xdg_data_home_for(..).join("Sirio/bin")`). Add `sirio_env` to `sirio`'s `Cargo.toml`.
- [ ] **Step 7: Run** `cargo build -p sirio && cargo nextest run -p sirio login_path` and `bash Scripts/Tests/test-build-appimage.sh` — expected: pass.
- [ ] **Step 8: Commit** — `feat(env): capture the user environment before the app changes its own`

### Task 2: the host can build a session from the client's environment alone

**Files:**
- Modify: `rust/crates/sirio_host_protocol/src/term.rs:30-45` (`CreateParams`), `src/messages.rs:221-224` (capability), `src/version.rs` (`PROTOCOL_MINOR = 2`)
- Modify: `protocol/host-v1/capabilities.json`, `protocol/host-v1/schema.json` (regenerated), `protocol/host-v1/conformance/09-welcome-names-terminal.json`, Create: `protocol/host-v1/conformance/15-create-with-an-exact-environment.json`
- Modify: `rust/crates/sirio_host/src/router.rs:129-131,230-240`, `src/sessions.rs:115-160`
- Modify: `rust/crates/sirio_term_session/src/lib.rs:136-160,441-446` (`SHELL` from `spec.env`)
- Modify: `rust/crates/sirio_host_client/src/mux.rs:148,552`, `src/term.rs:25-35`
- Test: `rust/crates/sirio_host/src/sessions.rs` tests, `rust/crates/sirio_host_protocol/tests/schema.rs`

**Interfaces:**
- Produces: `CreateParams.env_exact: bool` with `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`; `capability::TERM_ENV_EXACT: &str = "term.env_exact"`; `TermClient::env_exact(&self) -> bool`.
- Debug-only knob: `SIRIO_HOST_NO_ENV_EXACT=1` makes a debug host omit the capability (Task 11 uses it to stand in for an older host).

- [ ] **Step 1: Write the failing tests**
  - `exact_env_excludes_the_hosts_own_variables` — host fallback has `LEAK=1`; `env_exact: true`, `env: {PATH, HOME}` → the session's env (read through the spawn spec the sessions table builds) has no `LEAK`.
  - `without_exact_env_the_fallback_is_extended_as_before` — same input with `env_exact: false` → has `LEAK` (today's behaviour, which `host_probe` relies on).
  - `exact_env_shell_comes_from_the_clients_map` — client `SHELL=/bin/zsh`, host `SHELL=/bin/bash`, shell session → program `/bin/zsh`.
  - `env_exact_false_is_not_serialized` — keeps every existing conformance case byte-identical.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_host -p sirio_host_protocol` — expected: FAIL.
- [ ] **Step 3: Implement.** Field, capability (advertised by `Router::capabilities` unless the debug knob is set), host branch in `sessions.create`, `login_shell` reading `spec.env["SHELL"]` first. Ledger: `term.env_exact` with `since_minor: 2`, method `term.create`. Regenerate the schema: `cd rust && cargo run -p sirio_host_protocol --example write_schema -- ../protocol/host-v1/schema.json`. Conformance 09 expects the third capability; case 15 sends `env_exact: true` and checks the reply shape like case 10's create.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_host -p sirio_host_protocol -p sirio_host_client` and `Scripts/Tests/test-host-e2e.sh` — expected: pass, `HOST E2E OK`.
- [ ] **Step 5: Commit** — `feat(host): let a client give a session its whole environment`

### Task 3: every child process runs in the user environment

**Files:**
- Modify (terminals): `rust/crates/sirio_terminal/src/link.rs:93` (`OpenRequest`), `:205-232` (`LocalLink::open`), `:478-537` (`attach_or_create_with_reconnect`)
- Modify: `rust/crates/sirio_control/src/host_panels.rs:90-116,291-300`
- Modify (host launch): `rust/crates/sirio_host_client/src/detach.rs` (`DetachedSpawn`, Linux `spawn` `:88-140`, macOS `plist_xml` call `:540`), `src/ensure.rs:27-33,299-320`, `rust/crates/sirio/src/main.rs:25432,25500-25509`
- Modify (chats): `rust/crates/sirio_acp/src/lib.rs:1298-1301`, `rust/crates/sirio_acp/src/claude/worker.rs:335-341`
- Modify (the audit: `Command::new(x)` → `sirio_env::command(x)`, keeping each site's own `.env(..)` calls after it): `sirio_git/src/git.rs:268-290,563-590`, `sirio_project/src/git.rs:166`, `sirio_forge/src/transport.rs:498`, `sirio_lsp/src/server.rs:36-41`, `sirio_registry/src/installer.rs:248`, `sirio_usage/src/claude_native.rs:96-103`, `sirio_usage/src/claude.rs:1069`, `sirio_usage/src/http.rs:55`, `sirio_ui/src/editor.rs:1154-1164`, `sirio/src/main.rs:3779-3837` (desktop notifications), `:3932` (summarizer), `sirio/src/claude_transport.rs:125`
- Test: `rust/crates/sirio_terminal/src/link.rs` tests, `rust/crates/sirio_acp` tests

**Interfaces:**
- Consumes: `sirio_env::{for_child, command, absent_from_user, installed}` (Task 1); `CreateParams.env_exact`, `TermClient::env_exact()` (Task 2).
- Produces: `OpenRequest.extra_env: BTreeMap<String, String>` (per-terminal variables layered over `for_child()`); `HostPanels::with_extra_env(self, BTreeMap<String, String>) -> Self`; `DetachedSpawn.env: Option<BTreeMap<String, String>>`; `EnsureOptions.host_environment: BTreeMap<String, String>`.
- Produces: the app sets `extra_env["SIRIO_WORKTREE_ID"]` to the worktree's id (`sirio_persistence::model`'s id, the value `worktree-set --worktree` accepts) for every terminal it opens in a worktree, and for `panel.create --worktree`.

**Kept on the app's environment, with the reason (the audit's outcome):** `gsettings` (`sirio_ui/src/titlebar.rs:162`) reads the desktop theme for the app's own GTK; `sirio_apply` (`hdiutil`, the Windows installer) replaces the app itself; `sirio_privacy`'s `osascript` probe tests the app's own TCC grant; `sirio_diagram`'s `plantuml` already builds its own scrubbed environment.

- [ ] **Step 1: Write the failing tests**
  - `host_create_is_exact_when_the_host_supports_it` — a fake `TermClient` advertising `term.env_exact` receives `env_exact: true` and `env` = user env + `extra_env`.
  - `old_host_gets_blanks_for_what_the_user_environment_lacks` — no capability → `env_exact: false` and every name of `absent_from_user()` present with `""`.
  - `local_link_session_gets_the_user_environment` (unix: the session runs `env`; its output has no name from `absent_from_user()` and has `SIRIO_WORKTREE_ID`).
  - `acp_agent_does_not_inherit_what_the_user_environment_lacks` (unix: a stub agent program prints its environment; a variable set in the test process and absent from the installed map is missing).
- [ ] **Step 2: Run** `cargo nextest run -p sirio_terminal -p sirio_acp` — expected: FAIL.
- [ ] **Step 3: Implement.**
  - Terminals: `LocalLink` and `HostLink` build their env from `for_child()` + `extra_env`; `HostPanels` likewise (in `sirioctl`, nothing is installed, so `for_child()` is its own pane environment — correct).
  - Host launch: `DetachedSpawn.env = Some(host_environment)`; Linux: `env_clear()` + `envs()` on the `systemd-run` and the direct `Command`; macOS: pass it to `plist_xml` instead of `std::env::vars_os()`; Windows: unchanged (no launcher there).
  - ACP (cannot clear env): on unix, when `absent_from_user()` is non-empty, launch `/usr/bin/env` with `-u NAME` for each, then `--`, then the program and its args; always add `.env(k, v)` for the user map. Claude native: `env_clear()` + `envs(for_child())` before the line's own env.
  - The audit list above: each `std::process::Command::new(x)` becomes `sirio_env::command(x)`; for `async_process` in `sirio_lsp`, `env_clear()` + `envs(installed map)` **on the line right after** `Command::new`, before anything else touches the command. Each touched crate gains the `sirio_env` dependency.
  - Add `git_output_stays_unlocalized_under_the_user_environment` in `sirio_git` (install a map with `LANG=de_DE.UTF-8`; a `git status` run through the crate's runner still sees `LC_ALL=C` — read it back with a `git -c alias.e='!env' e` call or an equivalent that prints the child's environment).
- [ ] **Step 4: Run** `cargo build --workspace --all-targets && cargo nextest run -p sirio_terminal -p sirio_acp -p sirio_control -p sirio_host_client` — expected: pass.
- [ ] **Step 5: Commit** — `fix: start every child process in the user's environment, not the launcher's`

### Task 4: one stable copy, current by content

**Files:**
- Modify: `rust/crates/sirio/src/main.rs:24095-24395` (resolver and installer; move them to a new `rust/crates/sirio/src/cli_install.rs`), its callers `:17089`, `:22783`, `:22914`
- Test: the moved tests (`main.rs:38080-38330`) into `cli_install.rs`

**Interfaces:**
- Produces (in `sirio::cli_install`):
  - `pub(crate) fn sirioctl_install_dir(environment: &BTreeMap<String, String>, debug: bool) -> PathBuf` — `<data>/Sirio/bin` or `<data>/Sirio/bin-debug`.
  - `pub(crate) fn release_sirioctl_path(environment: &BTreeMap<String, String>) -> PathBuf` — what user-global hooks name.
  - `pub(crate) enum CopyOutcome { Installed, Replaced, Current, SourceMissing }`
  - `pub(crate) fn ensure_stable_copy(current_exe: &Path, environment: &BTreeMap<String, String>, debug: bool) -> Result<(PathBuf, CopyOutcome), String>` — source = sibling of `current_exe`, else a PATH hit that is not the destination; compare bytes; replace via a temp file in the same directory + `rename`; on Windows rename the old file to `sirioctl.old-<pid>.exe` first and delete stale `.old-*` files on the next call.
  - `resolve_sirioctl_for_process()` keeps its name and returns the stable path after `ensure_stable_copy(.., cfg!(debug_assertions))`.
- Removed: `SirioctlInstallKind`, `sirioctl_install_kind`, `installed_copy_is_stale`, and their tests (the behaviour they pinned is gone).

- [ ] **Step 1: Write the failing tests**
  - `different_bytes_are_replaced_even_when_the_copy_is_newer` (set the copy's mtime after the source's).
  - `same_bytes_leave_the_copy_untouched` (mtime and inode unchanged).
  - `a_reader_of_the_old_copy_keeps_reading_it` (unix: open the old file, replace, read to the end → old bytes).
  - `two_concurrent_installs_leave_one_whole_file` (two threads, different sources of equal length → the file equals one of them, byte for byte).
  - `missing_source_keeps_a_present_copy` / `missing_source_and_copy_is_an_error`.
  - `debug_and_release_copies_live_apart`.
  - `a_foreign_file_at_the_stable_path_is_replaced` (spec §5.1: the stable path is Sirio's).
  - `user_global_hooks_name_the_release_path_from_a_debug_build` (calls `release_sirioctl_path`, compared with `sirioctl_install_dir(.., false)`).
- [ ] **Step 2: Run** `cargo nextest run -p sirio cli_install` — expected: FAIL.
- [ ] **Step 3: Implement**; `begin_global_hooks_install` passes `release_sirioctl_path(..)` to `install_global_hooks` (after ensuring the release copy only when this is a release build); worktree-local `prepare` keeps the resolved path of this build.
- [ ] **Step 4: Call it at startup** off the GPUI thread, next to `ensure_host_in_background` (`main.rs:26145`), so the copy is refreshed even when no agent is launched. Its outcome feeds Task 6.
- [ ] **Step 5: Run** `cargo nextest run -p sirio cli_install` — expected: pass; confirm the registered test names with `cargo nextest list -p sirio | grep cli_install`.
- [ ] **Step 6: Commit** — `fix: keep the stable sirioctl copy current by content`

### Task 5: `sirioctl` on the user's PATH

**Files:**
- Create: `rust/crates/sirio/src/cli_path.rs`
- Modify: `Scripts/build-inno.sh:100+` (`[Code]`: remove the PATH entry on uninstall), `Scripts/Tests/test-build-inno.sh`
- Test: `rust/crates/sirio/src/cli_path.rs` tests

**Interfaces:**
- Consumes: `release_sirioctl_path` (Task 4); `login_path::login_shell_path()` (Task 1).
- Produces:
  - `pub(crate) enum LinkOutcome { Linked(PathBuf), AlreadyLinked(PathBuf), Foreign { place: PathBuf, points_to: Option<PathBuf> }, NotOnLoginPath(PathBuf), NeedsAdmin, Skipped(&'static str), Failed(String) }`
  - `pub(crate) fn link_on_path(stable: &Path, home: &Path, login_path: Option<&OsStr>, admin: bool) -> LinkOutcome`
  - Platform rules exactly as spec §5.2's table. Linux: `~/.local/bin/sirioctl` → symlink; `NotOnLoginPath` when `~/.local/bin` is not a login PATH entry (the link is still made). macOS: `/usr/local/bin` when it exists and is writable; else `~/.local/bin` when on the login PATH; else `NeedsAdmin`; with `admin: true`, run `osascript -e 'do shell script "mkdir -p /usr/local/bin && ln -sfn <stable> /usr/local/bin/sirioctl" with administrator privileges'` (quote the path for AppleScript and shell). Windows: append the stable directory to `HKCU\Environment\Path` (`REG_EXPAND_SZ`, case-insensitive duplicate check), then broadcast `WM_SETTINGCHANGE` with `"Environment"`.
  - Debug builds: `Skipped("debug build")` — unless the debug-only knob `SIRIO_CLI_AS_RELEASE=1` is set (read only under `cfg!(debug_assertions)`), which makes the startup job install the release copy (`bin/`) and link it, so the E2E can prove the release path in an isolated `HOME`. A release build ignores the variable.

- [ ] **Step 1: Write the failing tests** (unix, a temp `home`)
  - `links_into_local_bin_and_reports_it`; `an_existing_link_to_the_stable_copy_is_already_linked`.
  - `a_foreign_file_is_left_alone_and_reported` (regular file, and a symlink elsewhere: `points_to` set).
  - `link_in_a_directory_off_the_login_path_is_reported` (login PATH without `~/.local/bin` → `NotOnLoginPath`, link present).
  - `a_dangling_sirio_link_is_replaced` (symlink to a removed old stable path inside `<data>/Sirio/` — Sirio's own).
  - `macos_never_asks_without_admin` (`cfg(target_os = "macos")`: unwritable `/usr/local/bin` stand-in → `NeedsAdmin`, no process started). Make the `/usr/local/bin` location a parameter of an inner function so the test can point it at a temp directory.
- [ ] **Step 2: Run** `cargo nextest run -p sirio cli_path` — expected: FAIL.
- [ ] **Step 3: Implement**; call `link_on_path(.., admin: false)` from the startup job of Task 4 (release builds only).
- [ ] **Step 4: Inno.** `[Code]` gets `CurUninstallStepChanged` removing `{localappdata}\Sirio\bin` from `HKCU\Environment\Path`; `test-build-inno.sh` asserts `CurUninstallStepChanged` and `Environment` appear in the generated `.iss`.
- [ ] **Step 5: Run** `cargo nextest run -p sirio cli_path && bash Scripts/Tests/test-build-inno.sh` — expected: pass. `cargo check -p sirio --target x86_64-pc-windows-msvc` if the target is installed; otherwise state in the report that the Windows arm is unchecked locally.
- [ ] **Step 6: Commit** — `feat: put sirioctl on the user's PATH at startup and from settings`

### Task 6: the Settings card shows the CLI's state

**Files:**
- Modify: `rust/crates/sirio_ui/src/settings.rs:748-861` (fields), `:1402-1427` (builders/setters), `:3651-3690` (the card)
- Modify: `rust/crates/sirio/src/main.rs:1526-1530` (`WorkspaceAction`), `:5551-5556`, `:26099-26114` (callbacks)
- Test: `rust/crates/sirio_ui/src/settings.rs` tests (gpui)

**Interfaces:**
- Consumes: `CopyOutcome`, `LinkOutcome` (Tasks 4–5).
- Produces: `pub struct CliStatus { pub stable_path: String, pub version_matches: bool, pub path_line: String }` in `sirio_ui::settings`; `Settings::set_cli_status(&mut self, Option<CliStatus>, cx)`; `Settings::on_install_cli(self, impl Fn() + 'static) -> Self`; `WorkspaceAction::InstallCli`; button id `general-install-cli`; selectors `settings-cli-path`, `settings-cli-link`.
- Copy, exact (`path_line` per `LinkOutcome`): `Linked`/`AlreadyLinked` → "On PATH at {place}"; `NotOnLoginPath` → "Linked at {place}, but your login shell does not search {dir}"; `Foreign` → "{place} is not Sirio's ({points_to})— left as it is"; `NeedsAdmin` → "Not on PATH — Install in PATH asks for your password"; `Skipped` → "Not installed: {reason}"; `Failed` → "Could not link: {error}". Button label: "Install in PATH".
- The host status line, when the host lacks `term.env_exact`, ends with " — terminals opened before it restarts keep Sirio's launcher environment".

- [ ] **Step 1: Write the failing test** `cli_card_reports_a_link_off_the_login_path` — set a `CliStatus` with the `NotOnLoginPath` line; the `settings-cli-link` element's text equals it; `general-install-cli` click calls the callback once.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_ui cli_card` — FAIL.
- [ ] **Step 3: Implement** the card rows with `selectable_text` (not inside the button); the button runs `link_on_path(.., admin: true)` off the GPUI thread and pushes the new status.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_ui cli_card && cargo build -p sirio` — pass.
- [ ] **Step 5: Commit** — `feat(settings): show where sirioctl is and install it in PATH`

### Task 7: the macOS bundle ships `sirioctl`

**Files:**
- Modify: `Scripts/build-app-bundle.sh` (5th argument `<sirioctl-binary>`, copy to `Contents/MacOS/sirioctl`, sign it like `sirio-host` at `:149-152`), `.github/workflows/build-release.yml:296-337` (pass the built `sirioctl`; fix the comment), `Scripts/Tests/test-build-app-bundle.sh:30-40`, `Scripts/Tests/test-release-workflow.sh`

- [ ] **Step 1: Failing test**: `test-build-app-bundle.sh` passes a fake `sirioctl` and asserts `Contents/MacOS/sirioctl` exists and is executable; `test-release-workflow.sh` asserts the bundle step's line names `sirioctl`.
- [ ] **Step 2: Run** both scripts — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** both scripts — pass.
- [ ] **Step 5: Commit** — `fix(macos): ship sirioctl inside Sirio.app`

### Task 8: the skill for all five agents, installed from the binary

**Files:**
- Modify: `rust/crates/sirio_agents/src/codex.rs`, `src/pi.rs` (`skill_markdown`, `prepare` calls `install_skill`), `src/lib.rs:326-360`
- Create: `rust/crates/sirio_agents/src/user_skill.rs`
- Modify: `rust/crates/sirio/src/main.rs:22783-22795` and `:22923` (after **both** `prepare` calls — `fresh_agent_shell` and `restored_agent_shell`: `sirio_git::exclude(worktree, ".agents/skills/sirio/")`, a failure logged, never fatal), `:17068-17077` and `:3753-3764` (Install Skill no longer opens a terminal), `rust/crates/sirio_ui/src/settings.rs:1851-1857,3696-3725`
- Delete: `rust/crates/sirio_project/src/skill.rs` and its re-export (`sirio_project/src/lib.rs:122`), the tests `skill_install_runs_the_path_resolved_program` (`main.rs:28056`) and `install_skill_opens_its_terminal_in_front_of_settings` (`main.rs:32851`)
- Test: `rust/crates/sirio_agents/src/user_skill.rs` tests

**Interfaces:**
- Produces:
  - `pub fn user_skill_paths(home: &Path, environment: &BTreeMap<String, String>) -> Vec<(&'static str, PathBuf)>` — exactly: Claude Code `$CLAUDE_CONFIG_DIR|~/.claude` + `skills/sirio/SKILL.md`; Codex, Pi and Oh-My-Pi share `~/.agents/skills/sirio/SKILL.md` (one entry, labelled "Codex, Pi, Oh-My-Pi"); OpenCode `$XDG_CONFIG_HOME|~/.config` + `opencode/skills/sirio/SKILL.md` (the `skills` CLI 1.7.2 table).
  - `pub fn install_user_skills(home, environment) -> Vec<UserSkillReport>` and `pub fn refresh_user_skills(home, environment) -> Vec<UserSkillReport>` (refresh writes only where a file with `SKILL_MANAGED_MARKER` already exists).
  - `pub struct UserSkillReport { pub label: &'static str, pub outcome: Result<PathBuf, String> }` with `summary_line()` in the shape of `GlobalHookReport::summary_line`.
  - `Settings::set_skill_install_report(Option<String>, cx)`; `on_install_skill` becomes `impl Fn() + 'static`.

- [ ] **Step 1: Write the failing tests**
  - `codex_and_pi_prepare_write_the_skill` (temp worktree → `.agents/skills/sirio/SKILL.md` equals the bundled text).
  - `user_skills_land_where_each_cli_reads_them` (paths above, with and without `CLAUDE_CONFIG_DIR`/`XDG_CONFIG_HOME`).
  - `user_skill_without_the_marker_is_left_alone` (install and refresh both report an error for it; bytes unchanged).
  - `refresh_never_creates_a_user_skill`.
  - `refresh_updates_an_outdated_managed_copy`.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_agents` — FAIL.
- [ ] **Step 3: Implement**; the startup job (Task 4) calls `refresh_user_skills`; the Settings button calls `install_user_skills` off the GPUI thread and shows the report lines.
- [ ] **Step 4: Run** `cargo build --workspace --all-targets && cargo nextest run -p sirio_agents -p sirio_ui` — pass.
- [ ] **Step 5: Commit** — `feat: install sirio's skill for every agent from the copy sirio ships`

### Task 9: the skill is checked against a release build

**Files:**
- Create: `rust/crates/sirio_control/src/served.rs` (`pub fn served_methods(debug: bool) -> Vec<&'static str>`, `pub const BROWSER_CAPABILITIES`, `pub const BROWSER_METHODS`), Modify: `rust/crates/sirio_control/src/lib.rs`
- Modify: `rust/crates/sirio/src/main.rs:710-745,2474-2569` (the `system.capabilities` arm and the browser arrays use `served`)
- Modify: `rust/crates/sirio_control/src/bin/sirioctl.rs:27-101` (global `--dry-run`), `:263,278,326,1153` (the four senders), `:1342-1366` (`not_supported:` for every action of the skill's table)
- Modify: `skills/sirio/SKILL.md` (the four corrections of spec §1, as decided in §5.7)
- Create: `rust/crates/sirio_control/tests/skill_is_served.rs`

**Interfaces:**
- Produces: `sirioctl --dry-run <command…>` prints the request it would send as one JSON line `{"method":…,"params":…}` and exits 0, contacting nothing; a command that sends several requests prints the first.
- Produces: `served_methods(false)` is the release list; `served_methods(true)` adds today's ten debug-only names. The `system.capabilities` arm returns `served_methods(cfg!(debug_assertions))` unchanged in content.

- [ ] **Step 1: Write the failing test** `every_command_the_skill_names_is_served_by_a_release_build`:
  - read `skills/sirio/SKILL.md` via `env!("CARGO_MANIFEST_DIR")`; collect lines inside ```` ``` ```` blocks whose first word is `sirioctl`;
  - cut each at the first unquoted `||`, `|`, `;`, `&&`; replace `"$NAME"` with `placeholder`; split on whitespace honouring single and double quotes;
  - run `env!("CARGO_BIN_EXE_sirioctl")` with `--dry-run` and those args, in a pane-like environment (`SIRIO_PANE_ID=pane-placeholder`, `SIRIO_WORKTREE_ID=worktree-placeholder`), so commands that read them (`identify`, `browser_workspace`, `require`) do not fail for the wrong reason; assert exit 0 and that the printed `method` is in `served_methods(false)`;
  - assert at least 20 lines were checked (a zero-match parse must not pass green).
  - plus `dry_run_contacts_nothing` (`SIRIO_SOCKET` pointing at a missing path → still exit 0).
- [ ] **Step 2: Run** `cargo nextest run -p sirio_control --test skill_is_served` — FAIL.
- [ ] **Step 3: Implement** `--dry-run`, `served`, the `not_supported:` answers; then fix `SKILL.md` until the test passes: drop `browser screenshot` from what works, make the `not_supported` table name exactly the actions `cmd_browser` answers that way, correct the `panel state`/`panel scrollback` field names to what the text and `--json` modes actually print (`sirioctl.rs:716-747`), keep `worktree-set --worktree "$SIRIO_WORKTREE_ID"` (now exported by Task 3).
- [ ] **Step 4: Run** `cargo nextest run -p sirio_control && cargo nextest run -p sirio browser_methods_are_explicit every_control_response_hides_verbatim_paths` — pass.
- [ ] **Step 5: Commit** — `fix(skill): check every command the skill names against a release build`

### Task 10: hooks are observable, and stub agents fire them

**Files:**
- Modify: `rust/crates/sirio/src/main.rs:17630-17637` (`control_terminal_state` adds `agentStatus` from the activity model for that pane; still debug-only)
- Create: `Scripts/Tests/stubs/hook-agents/{claude,codex,opencode,omp,pi}` (bash)
- Create: `rust/crates/sirio_agents/tests/hooks_live.rs`

**Interfaces:**
- Produces: `surface.terminal.state pane=<id>` → `sessionLost`, `agentStatus` ∈ `running|needs-input|done|error|none`.
- Produces: debug-only `surface.settings.act action=install-hooks|install-skill|install-cli` — runs exactly what the Settings button runs and replies with the report text; added to `served_methods(true)` only, with an arm in `request_for_control_method` (`main.rs:41265`).
- Produces: each stub, started in a pane, writes `$SIRIO_PANE_ID` to `$HOOK_STUB_OUT/<agent>.pane`, then fires its CLI's hooks **from the config `prepare` wrote**, exactly as written: `claude` reads `.claude/settings.local.json` and runs the `UserPromptSubmit` then `Stop` commands with `{"session_id":"stub-claude"}` on stdin; `codex` runs the argv after `-c notify=` from its own command line with a Codex notify JSON argument; `opencode` runs `node .opencode/plugin/sirio-session.js` against a minimal event stub if `node` is present, else records `SKIP`; `omp` likewise for `.sirio/omp-hook.ts` with `bun`; `pi` has no hooks and only prints a title (`π - <cwd>`). Each then waits on `$HOOK_STUB_OUT/<agent>.go` before firing the next hook, so the E2E can observe each status.
- `hooks_live.rs`: one real turn per installed CLI (`claude -p 'say ok' --max-turns 1`, `codex exec 'say ok'`, `opencode run 'say ok'`) in a scratch worktree prepared by the adapter, with `SIRIO_SOCKET` pointing at a recording socket the test serves; asserts a `notify` arrived. SKIPs (prints `SKIP` and returns) when the binary or its credentials are absent. Added to the release gate's skip list in `rust/.config/nextest.toml`, with the reason.

- [ ] **Step 1:** add `agentStatus`; extend `every_control_response_hides_verbatim_paths`'s request for the method only if it fails.
- [ ] **Step 2:** write the stubs; `shellcheck` them if available.
- [ ] **Step 3:** write `hooks_live.rs`; run `cargo nextest run -p sirio_agents --test hooks_live` — expected locally: pass or SKIP per CLI, stated in the report.
- [ ] **Step 4: Commit** — `test: let the e2e observe hook status and fire hooks like each cli`

### Task 11: the E2E — `SIRIOCTL E2E OK`

**Files:**
- Create: `Scripts/Tests/test-sirioctl-e2e.sh`
- Modify: `.github/workflows/pr.yml` (step after the host-sessions E2E), `Scripts/Tests/test-pr-workflow.sh` (anchored assertion), `.github/workflows/macos-check.yml`, `.github/workflows/windows-check.yml` (`--state-only`)

**Interfaces:**
- Consumes: everything above; the skeleton of `test-open-list-e2e.sh` (header, args `--out-dir`/`--state-only`, `tee` transcript, `RUN_DIR`, isolated `HOME`/`XDG_*`/`SIRIO_DB`/`SIRIO_SOCKET`, `launch_app`, `cleanup`). Builds `sirio`, `sirioctl`, `sirio-host`.
- Artifact under `--out-dir`: `transcript.log`, `app.log`, `env/<case>.env` (each pane's `env -0` dump), `copies.sha256`, `hooks/<agent>.status`.

Cases (spec §5.8), each `echo "step N: …"` / `echo "OK: …"`:
1. First launch with `SIRIO_CLI_AS_RELEASE=1`: `$XDG_DATA_HOME/Sirio/bin/sirioctl` equals the built one (`cmp`); `~/.local/bin/sirioctl` links to it; `env -i HOME=… PATH=$HOME/.local/bin:/usr/bin:/bin sirioctl version` names the app's version; with a login PATH lacking `~/.local/bin` (the isolated `SHELL` is a script printing a fixed PATH), `surface settings read` shows the "does not search" line.
2. Replace: rebuild-free swap — copy a different `sirioctl` (the built one with a byte appended) beside the app binary in `RUN_DIR/app/`, launch from there while `sleep 30 < old-copy` holds the old file open; after launch `cmp` shows the new bytes; the held descriptor still reads the old size.
3. A foreign `~/.local/bin/sirioctl` (a script) is unchanged after a `SIRIO_CLI_AS_RELEASE=1` launch, and the card's line names it.
4. Launcher: a fake mount `RUN_DIR/mnt` with `usr/bin/{sirio,sirioctl}` and `Scripts/appimage/AppRun` copied as `mnt/AppRun`, an `apprun-hooks/gtk.sh` exporting `GTK_THEME=leak`; launch through `mnt/AppRun` with `WAYLAND_DISPLAY=wayland-test` set. Open a terminal (`surface.terminal.open cmd="env -0 > $OUT/env/terminal.env; sleep 300"`), a `panel create --worktree` session (same command), and a chat opened through the debug `surface.open_list.open` Chat row (as `test-open-list-e2e.sh` does) with `SIRIO_ACP_PROGRAM` pointing at a stub that dumps its env before speaking ACP's `initialize`. Assert in each dump: `PATH` starts with the stable directory, no entry under `RUN_DIR/mnt`, no `LD_LIBRARY_PATH` entry under it, no `GTK_THEME`, no `APPIMAGE`/`SIRIO_LAUNCH_ENV`, `WAYLAND_DISPLAY=wayland-test`, `SIRIO_WORKTREE_ID` non-empty; the app's own `/proc/<pid>/environ` still has `GTK_THEME=leak`. Repeat the terminal half with `SIRIO_HOST_NO_ENV_EXACT=1` (a fresh host): `GTK_THEME` and `LD_LIBRARY_PATH` present but empty.
   **The shells users run:** the `cmd=` terminals run under `sh -c`; real tabs run the user's login shell and agents `$SHELL -lc`, whose profiles can reassign or reorder `PATH` (Debian's `/etc/profile`, macOS `path_helper`). So also open a default shell tab and `panel write` `env -0 > $OUT/env/login-shell.env; command -v sirioctl > $OUT/env/login-shell.which` into it, with the isolated `HOME` holding a `.profile` that assigns `PATH=/usr/bin:/bin`. Assert no mount entry anywhere and that `sirioctl` still resolves — to the stable directory or to the `~/.local/bin` link (run this half with `SIRIO_CLI_AS_RELEASE=1`). Record which; Task 13 writes the outcome into spec §5.3 (where the prefix does not survive a login profile, the PATH link is what keeps `sirioctl` in panes).
   **Restart into an update:** launch `mnt2/AppRun` (a second fake mount) from the environment the first app has (`tr '\0' '\n' < /proc/$APP_PID/environ`), as *Restart to update* does; a terminal opened by the relaunched app has no entry under `mnt` **or** `mnt2`, and the app's `SIRIO_LAUNCH_ENV` has the same length as before the relaunch (no nesting).
5. Quit, `rm -rf RUN_DIR/mnt`, relaunch without the launcher: in the terminal opened before the quit, `panel write` `sirioctl ping > $OUT/ping.out` and the file says `pong` (or the ping reply), proving the stable path survives the mount.
6. Hooks: put `Scripts/Tests/stubs/hook-agents` first on the app's `PATH`; for each agent `surface.terminal.open agent=<id>`; read its pane id from `$HOOK_STUB_OUT`; after each `go`, `surface.terminal.state pane=<id>` shows the expected `agentStatus` (`running` then `done` for Claude; `needs-input` for Codex; per stub for OpenCode/omp, or `SKIP` recorded when `node`/`bun` is missing; `none` for Pi). Then `surface.settings.act action=install-hooks` (Task 10) into the private `HOME` and run the global Claude `Stop` command from `~/.claude/settings.json` inside a pane (no `--session`) → that pane's status changes; run it outside a pane → exit 0, no change. Repeat the Claude worktree-local hook after case 2's replacement.
7. Skill: every adapter's worktree file equals `skills/sirio/SKILL.md` (the marker included); `cargo nextest run -p sirio_control --test skill_is_served` passes (run from the script, output in the transcript).
8. A debug build without the knob, in a fresh `HOME`: `$XDG_DATA_HOME/Sirio/bin` never created, no `~/.local/bin/sirioctl`; `~/.claude/settings.json` written by `surface.settings.act action=install-hooks` names `…/Sirio/bin/sirioctl`, not `bin-debug`. Then `action=install-skill` writes the three user-level copies of Task 8, each equal to the bundled skill.

- [ ] **Step 1:** write the script; run `Scripts/Tests/test-sirioctl-e2e.sh --out-dir /home/epalmisano/.cache/sirio-e2e/sirioctl` (never under `/tmp`'s quota-limited tmpfs; set `TMPDIR` to a disk path) — expected: `SIRIOCTL E2E OK`.
- [ ] **Step 2:** wire `pr.yml` (`run: Scripts/Tests/test-sirioctl-e2e.sh --out-dir sirioctl-e2e`, upload `sirioctl-e2e-linux`), `test-pr-workflow.sh` (anchored `run:` regex in the style of its lines 58-59), `macos-check.yml` and `windows-check.yml` with `--state-only` (cases 1–3, 7, 8; the script skips 4–6 there and says so). Run `bash Scripts/Tests/test-pr-workflow.sh` — pass.
- [ ] **Step 3: Commit** — `test: prove sirioctl, the pane environment and the hooks end to end`

### Task 12: the macOS admin prompt, a manual row

**Files:**
- Modify: `docs/testing/host-detach-probe.md`'s sibling — Create: `docs/testing/sirioctl-path-manual.md`

- [ ] **Step 1:** write the manual procedure (macOS: *Install in PATH* → one password prompt → `/usr/local/bin/sirioctl` → `sirioctl version` in a new Terminal; Windows: a new PowerShell finds `sirioctl` after first launch, and not after uninstall), with an empty "done on" column.
- [ ] **Step 2: Commit** — `docs: record the manual checks of sirioctl on PATH`

### Task 13: documentation

**Files:**
- Modify: `CLAUDE.md` (crate list: `sirio_env` below everything with `sirio_perf`; *Agent adapters*: the user-level skill paths and the startup refresh; *Control socket*: `--dry-run`, `served_methods`, the stable copy and PATH link; *Commands*: `test-sirioctl-e2e.sh` → `SIRIOCTL E2E OK`), `docs/superpowers/specs/2026-10-17-ade-programme-design.md` §5.3 (the refinements listed in this plan's Global Constraints), `rust/crates/sirio_host_protocol` README rule references if any name the capability list.

- [ ] **Step 1:** edit; **Step 2: Commit** — `docs: describe the user environment, the stable sirioctl and its e2e`
