# D-P2 report

## F-SET-11 — fixed

**Files:** `rust/crates/tiller_usage/src/claude.rs`, `rust/crates/tiller_usage/tests/usage_tests.rs`

Root cause matched the critic's evidence exactly: `ClaudeUsageFetcher::fetch_with` always
launched `claude` through a **login** shell (`-lc`), which re-sources the user's login/interactive
dotfiles (`.bash_profile`, `.zprofile`, `.zshrc`, …). Any `PATH`/env override a synthetic test set
before calling the fetcher could be silently reset by those dotfiles before `claude` was ever
resolved, which is why no instrument could reach `NotInstalled`/`LoggedOut`/`Error` — the fourth
reason, `TimedOut`, was already provable because it just needs the bounded timeout to expire.

Added `ClaudeUsageFetcher::fetch_with_env(settle, poll, timeout, envs)` — `fetch_with` now
delegates to it with an empty `envs`. When `envs` carries the key `TILLER_USAGE_NO_DOTFILES`, the
shell is launched with `--noprofile --norc -c` (bash) or `-f -c` (zsh) instead of `-lc`, so nothing
re-sources dotfiles and a `PATH` override in `envs` is the only thing deciding what `claude`
resolves to. `Pty::spawn` was replaced by `Pty::spawn_with_env`, which threads `envs` onto the
spawned `Command`. Production `fetch()` never sets the skip key, so real users still get a genuine
login shell — behavior is unchanged for them.

Added three integration tests in `usage_tests.rs`, each driving the **real** PTY/login-shell path
(not the `transcript_outcome` pure stand-in already used for other rows):

- `not_installed_is_reachable_through_the_real_shell_when_claude_is_absent_from_path` — `PATH`
  pointed at an empty temp dir; the shell's own "command not found" is what's captured. (Had to
  pin the child to `LC_ALL=C`/`LANG=C` too — this machine's locale prints "comando non trovato",
  which `classify_failure`'s English-only marker doesn't match; that's a pre-existing localization
  gap in `classify_failure` itself, out of this row's scope, worth a follow-up row.)
- `logged_out_is_reachable_through_the_real_shell_with_a_fake_claude_on_path` — a fake executable
  `claude` script on the isolated `PATH` prints `Please run /login to continue`.
- `error_is_reachable_through_the_real_shell_with_a_fake_claude_on_path` — fake script prints
  `failed to load usage`.

All 10 tests in `usage_tests.rs` pass (`cargo test -p tiller_usage --test usage_tests`).
`cargo build -p tiller` is green.

**howToExercise:** not live-drivable (this is a background PTY fetch with no UI surface of its
own until the status bar polls it) — the proof is
`cargo test -p tiller_usage --test usage_tests -- --test-threads=1`, which now shows all four
`Unavailable` reasons reached through the real fetch path plus the pre-existing `Loaded`/`Stale`
coverage.

## F-SET-20 — fixed (root cause is in a foreign file: `rust/crates/tiller/src/main.rs`)

**Files I own that were checked:** `rust/crates/tiller_ui/src/settings.rs` — already correct.
`SettingsSnapshot::translucency` exists, `SettingsPanel::report()`/`snapshot()` already copy the
live `self.translucency` field into it (`settings.rs:1197` and around), and the existing test
`appearance_controls_drive_theme_translucency_and_font_size` proves the toggle flips the field and
the emitted `snapshot()` carries it. Nothing to change on my side.

**wantedForeignFiles:** `rust/crates/tiller/src/main.rs`

The bug is entirely in `settings_report_pairs` (`main.rs:1913`), which hand-builds the `values`
`BTreeMap` sent over the control socket (`surface.settings.select`) field-by-field from
`report.snapshot` and simply never copies `snapshot.translucency` into it — every other
`SettingsSnapshot` field (`theme`, `interfaceFontSize`, `terminalFontSize`, `fileIcons`,
`controlSocketEnabled`, `socketPath`, …) has an entry; `translucency` has none. Confirmed live:
toggling Translucency ON changes `settings.translucency` (proven by the unit test above) but the
ctl payload carries no `translucency` key before or after.

**Exact patch for the integrator**, inserted into the `values` `BTreeMap::from([...])` literal in
`settings_report_pairs`, right after the `"theme"` entry (currently `main.rs:1944-1948` — line
numbers will drift as other slices touch this file concurrently; anchor on the `"theme"` entry
instead):

```rust
        (
            "translucency".to_string(),
            snapshot.translucency.to_string(),
        ),
```

**howToExercise (once applied):** `ctl surface.settings.select` on the Appearance section, toggle
the Translucency row, then `ctl surface.settings.select` again — the `values.translucency` key
should flip from `"false"` to `"true"`.

## F-USE-03 — already-correct on my side, live-proof still open

**Files:** `rust/crates/tiller_usage/src/claude.rs`, `rust/crates/tiller_usage/src/model.rs`

The critic's remaining gap ("stale-after-timeout half not attempted") is about live-driving the
transition on the real bar, not a code defect. The reducer logic is already correct and unit-tested
in both files I own: `model.rs`'s `reduce()` maps `TimedOut` onto `Stale(last_usage)` when a
previous `Loaded` value exists (`success_replaces_the_previous_state` test), and
`claude.rs::fetch_with`'s bounded-timeout path (`the_fetch_is_bounded_and_single_attempts_do_not_hang`)
already drives this through a real PTY. The consumer that would show it live —
`tiller_ui/src/status_bar.rs` — is not in this slice's owned files, so I made no changes there.
Reported `already-correct` rather than `fixed`: nothing needed changing in the files I own.

**howToExercise:** not exercisable through this slice's files alone; the live transition needs
`status_bar.rs` (foreign to D-P2) to actually re-poll and hit a timeout after a prior success —
out of scope here.

## F-EDIT-12, F-CORE-TERM-02, F-TERM-PTY-06, F-TERM-UI-02 — already-correct, harness-blocked only

**Files:** `rust/crates/tiller_terminal/src/lib.rs`, `rust/crates/tiller_ui/src/changes.rs`

Verified each test the critic named still exists and still compiles/passes; no code gap found in
the owned files for any of these four rows:

- `F-EDIT-12`: `a_drawn_change_row_drags_its_diff_payload_to_a_drop_target` — `changes.rs:2634`,
  drags onto `DiffDropTargetFixture` (`changes.rs:2599`).
- `F-CORE-TERM-02`: `shift_f10_opens_the_context_menu_from_the_keyboard` — `lib.rs:2973`.
- `F-TERM-PTY-06`: `a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files` —
  `lib.rs:3112`; `on_drop::<ExternalPaths>` wired at `lib.rs:2376`.
- `F-TERM-UI-02`: `platform_modifier_click_opens_a_terminal_link` — `lib.rs:2728`.

All four are blocked purely by `wayland-drive.sh` missing primitives (drag, modifier-composed
click, chord key combos) per the brief's own note that a separate agent is extending the lane this
wave. Nothing to implement here; reported `already-correct`.

**howToExercise (once the lane gains the primitive):**
- `F-EDIT-12`: drag a changed-file row in the Changes tab onto a diff drop target; the diff payload
  should land at the drop target.
- `F-CORE-TERM-02`: focus a terminal pane, press Shift+F10; the context menu should open with
  `terminal-context-item-0` drawn.
- `F-TERM-PTY-06`: drop several external file paths onto a running terminal pane (XDND — likely
  out of reach on any lane per `ENVIRONMENT.md`, not just this one).
- `F-TERM-UI-02`: Cmd/Ctrl-click a URL printed in a terminal pane; it should open with no
  ordinary-click side effect.

## Build

`cargo build -p tiller` — green (only a pre-existing unrelated warning in `tiller_ui/src/browser.rs`).
