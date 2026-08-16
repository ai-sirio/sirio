# Wave J critic verdict

Independent critic pass over wave J (cross-platform macOS/Windows portability). Everything below was
run by hand from this worktree (`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`, cargo root `rust/`), not taken from the wave's own reports. Environment: single
Linux box (`x86_64-unknown-linux-gnu`), no macOS or Windows machine, no cross-linker. Everything
below marked "compiles" is `cargo check` evidence — it proves the code type-checks for a target, and
**nothing here proves the app starts, renders, or behaves correctly on macOS or Windows.** No such
claim is made anywhere in this document.

## 1–2. Cross-target `cargo check --workspace`

Both commands run directly, twice each (once alone, once with `--keep-going` for diagnosis):

```
cargo check --target x86_64-pc-windows-msvc --workspace   → exit 101 (fails)
cargo check --target aarch64-apple-darwin --workspace     → exit 101 (fails)
```

Both fail with the exact same signature: `error occurred in cc-rs:` from `psm`/`stacker` (gpui's
stack-probe crates, needing a real cross-assembler/SDK this box does not have) — matches
`Scripts/ci-linux.sh`'s `CROSS_TARGET_KNOWN_WALL` pattern, so the gate reports these as `BLOCKED`
(non-fatal), not `FAILED`. This matches PORTABILITY.md's claim.

Per-crate reality check (since `--workspace` without `--keep-going` stops at the first error and
never proves anything about crates later in the build graph): individually checked all 6 crates
PORTABILITY.md calls "compiler-clean on both targets" —

```
tiller_project, tiller_git, tiller_agents, tiller_activity, tiller_markdown, tiller_usage
```

— all 6 pass `cargo check --target x86_64-pc-windows-msvc -p <crate>` and
`cargo check --target aarch64-apple-darwin -p <crate>` individually. Confirmed true, not just
claimed.

**Caveat found and worth flagging**: `cargo check --workspace` (no `--keep-going`) reaches these 6
crates on the macOS run (`Checking tiller_project`/`tiller_usage`/`tiller_agents`/`tiller_git`/
`tiller_markdown` all appear in that log before the fatal psm/libsqlite3-sys errors), but on the
Windows run it does **not** — the Windows log only shows dependency-graph output, gpui/psm/stacker
failing, and nothing from any `tiller_*` crate. Cargo's default fail-fast aborts before the 6 clean
crates are ever reached on that target. Verified those 6 individually instead (above) since the
workspace-wide run alone cannot support the "6 clean" claim on Windows.

## 3. No Linux regression

`cargo build -p tiller`: **green**, exit 0.

Per-crate `cargo test`, run individually (never `--workspace`):

| crate | result | note |
|---|---|---|
| `tiller` | **162 passed**, 0 failed | task's baseline says 161; audited every `#[test]`/`#[gpui::test]` function name against the pre-wave commit (`184ae5f4^`) — **zero removed, one added** (`tray_jump_lands_on_the_target_worktrees_worst_status_tab`, tray-seam coverage). 162 is the correct post-wave count, not a regression. |
| `tiller_ui` | **322 passed**, 0 failed | matches exactly |
| `tiller_terminal` | **44 passed**, 0 failed | matches exactly |
| `tiller_persistence` | **39 passed**, 0 failed on isolated reruns | flaked once in 4 runs on `concurrent_writers_save_disjoint_records_and_exit` (38/39) — confirmed pre-existing (documented in wave-e's `E-C-1-report.md` as a known parallel-load flake, unrelated to this wave), reproduces clean every time run alone |
| `tiller_usage` | 41 (lib) + 3 + 1 + 10 (integration files) = 55 total, 0 failed | task's baseline "10" matches exactly one file, `tests/usage_tests.rs` (10 tests) — full-crate audit against pre-wave commit shows 55 before, 55 after, **zero added/removed** |
| `tiller_acp` | 28 (lib, 2 ignored) + 11 + 6 + 0(1 ignored) = 48 total, 0 failed | task's baseline "12" doesn't match any single file exactly (`acp_integration.rs` is 11) — full-crate audit against pre-wave commit shows 47 before, 48 after, **one added** (`real_agent_stays_silent_on_stderr_for_an_unapproved_broken_mcp_json`), zero removed |

No test was removed anywhere. The two count mismatches against the task's stated baselines
(`tiller` 161→162, `tiller_usage`/`tiller_acp` not matching a whole-crate total) are accounted for by
audited additions, not silent losses — **no Linux regression found.**

## 4. Negative control on the CI gate

This is the headline finding of the whole pass, and it is not simply "the gate passed" or "the gate
failed" — it is conditional on *where in the dependency graph the injected defect gets built relative
to the pre-existing psm/stacker wall*, and that ordering is not fully stable.

Reintroduced the exact defect twice, reverting cleanly between attempts (`git checkout --` after
each, `git status --porcelain` empty both times, confirmed clean at the end):

- **`ksni` moved to unconditional `[dependencies]`**: does not actually break cross-compilation —
  `ksni` itself is portable enough to compile cleanly for both foreign targets (confirmed via
  `--keep-going`: `Checking ksni v0.3.6` with no error). This specific defect is invisible to
  `cargo check` regardless of gate logic, because it is not in fact a compile-time regression on this
  evidence.
- **`gtk` moved to unconditional `[dependencies]`** (the other option the task offered): this *does*
  break cross-compilation — `gobject-sys`/`glib-sys`/`gdk-pixbuf-sys`/`gdk-sys`/`gtk-sys` all fail
  with `pkg-config has not been configured to support cross-compilation`, a real, distinct, correctly
  diagnosable error with nothing to do with psm/stacker.
  - Running the **full gate** (`bash Scripts/ci-linux.sh`) with this defect present: **`FAILED:
    cargo check --target x86_64-pc-windows-msvc --workspace`** — the gate correctly aborted with a
    nonzero exit and did not print `CI OK`. Confirmed 6 more times via the bare `cargo check`
    command (matching the gate's own invocation) with a warm `target/x86_64-pc-windows-msvc/`: all 6
    correctly showed no `error occurred in cc-rs:` match, i.e. correctly classified as a real failure.
  - **But the very first invocation of that same command, immediately after introducing the `gtk`
    defect (against the same warm target dir), showed the opposite**: the log contained only the
    `psm` `error occurred in cc-rs:` line and none of the `gtk`-sys pkg-config errors — the gate's
    `grep -qE "$CROSS_TARGET_KNOWN_WALL"` classifier would have called this `BLOCKED` (non-fatal,
    "known SDK wall, not a code regression") and the gate would have printed `CI OK` with the real
    `gtk` regression sitting undetected inside a workspace-wide log cargo aborted early on. This log
    is saved-equivalent evidence, not a guess — it was a real run, not a hypothetical.
  - Root cause: `cargo check --workspace` without `--keep-going` is fail-fast — it stops at the
    *first* build-script failure encountered, and which failure (psm's slow real `cc` subprocess
    invocation vs. gtk's near-instant pkg-config check) wins the race is a function of build
    parallelism/scheduling, not something the gate controls. `run_cross_target_stage`'s
    `grep -qE "$CROSS_TARGET_KNOWN_WALL" "$log"` then classifies purely on **presence of the wall
    string anywhere in whatever partial log cargo produced**, not on absence of any other error.
    That means the gate's BLOCKED/FAILED distinction is **not deterministic** for a real regression
    that happens to compile alongside a dependency that hits the known wall — it depends on which
    failure cargo's parallel scheduler reports first, which in turn depends on build-cache state
    (this pass could not reliably reproduce the masked outcome on request; it fired once, spontaneously, out of 7 total attempts).
  - This is a real, demonstrated hole in the gate's classification logic, not a hypothetical: on 1 of
    7 attempts, with a real compile-breaking regression present, the gate would have gone green. The
    gate is not decorative — most runs (6/7) caught the defect correctly — but it is not reliable
    either, and "usually catches it" is a materially weaker claim than PORTABILITY.md/J4's "this is a
    real gate" framing implies. Recommend `run_cross_target_stage` use `cargo check --keep-going`
    (collect every error before classifying) or check that the *only* errors present are cc-rs
    failures from known non-workspace crates, rather than grepping for wall-presence alone.

Reverted both attempts: `git diff --stat rust/crates/tiller/Cargo.toml` and
`git status --porcelain` both empty after the final revert.

## 5. `F-CHAT-05` — driven live, independently

Built `cargo build -p tiller`, drove the real binary under `Scripts/wayland-drive.sh` (nested headless
Wayland/sway), `TILLER_ACP_PROGRAM=/definitely/missing/mcp-nonexistent-binary-critic`, fresh scratch
git repo, own unique marker strings (not reused from any prior report).

- Added the scratch project, selected its worktree, opened the `Chat` tab: confirmed offline —
  red banner `could not launch ACP agent: ACP transport error: Internal error: "No such file or
  directory (os error 2)"`, status pill `offline`, placeholder `Agent offline — reconnecting when
  you send…`.
- **Refusal of input, via the real keyboard path** (click composer + `wtype`, not a control-socket
  shortcut): typed `ZQX-CRITIC-MARKER-8271-unsent`, pressed Return. `surface.chat.read` afterward:
  `composerText` empty, `transcript` still exactly the one original error entry. Screenshot confirms
  the placeholder is unchanged and no text landed.
- **Draft survival, byte-for-byte**: seeded a draft via `surface.chat.compose` with
  `ZQX-CRITIC-PREDATES-OFFLINE-3f9d` (modeling text typed before the connection dropped, since the
  UI itself now correctly refuses to let new text in while offline). Real click + real Return via
  `wtype`: `composerText` unchanged, byte-for-byte, and no second transcript entry. Then clicked the
  transcript's real "Retry" button: a second identical error entry appended (confirming Retry is the
  only path that reconnects) and `composerText` **still** unchanged byte-for-byte.
- Independently rechecked the stated permission-wait blocker rather than trusting the prior report:
  `~/.claude/settings.json` still has `"permissions": {"defaultMode": "auto"}` (read directly), and
  `~/.codex/` has no `auth.json` (matches "Codex logged out" shown live in the app's own status bar
  in every screenshot this pass). Both environmental blockers for the permission-wait clause are
  confirmed still present.

**Verdict: half-proven.** The offline-disable clause is live-proven, independently, with fresh
evidence and my own markers — composer refuses input while offline, and a pre-existing draft survives
untouched. The permission-wait clause remains code-verified only; no installed agent on this box can
be driven into emitting a real ACP permission request, for the same reasons already documented. Since
the row's contract covers both clauses and only one has live evidence, `half-proven` (not `PASSED`)
is the correct verdict for the combined row, matching J0's own recommendation — now independently
confirmed rather than taken on faith.

## 6. Seam honesty

Read every non-Linux branch PORTABILITY.md's cfg-seam table lists. Most are honest exactly as
claimed: `tray.rs::spawn`/`nudge` (logs + `None`, same shape Linux already uses for "no SNI host"),
`main.rs`'s `browser.wait` (still polls `surface.state()` on the same deadline off-Linux — a real
fallback, not a no-op), `tiller_activity/src/process.rs::inspect_process_names` (honest
`Err(Unsupported)`, correctly gated `cfg(target_os = "linux")`), `tiller_markdown/src/file_events.rs`
(honest `Err(Unsupported)`, same gate), `tiller_control/src/{server,client}.rs` (genuinely portable —
`UnixListener`/`UnixStream` work on macOS too, so `cfg(unix)` is correct there, not a stub gap), and
`tiller_usage/src/claude.rs`'s PTY fetch (also genuinely portable — `openpty`/`fork` are real macOS
syscalls, so `cfg(unix)` correctly gives macOS a *working* fetch, not a degraded one).

**One dishonest seam found, in two places, both introduced by this wave's own J2-windows slice**
(commit `182d2d87a`, "seam tiller_terminal's process-group teardown behind cfg(unix)"):

- `rust/crates/tiller_terminal/src/lib.rs:542,577,614,632` — `terminate_process_group`,
  `descendant_pids`, `descendant_process_groups`, `terminate_descendant_process_groups`
- `rust/crates/tiller_ui/src/settings.rs:664,703` — `descendant_pids`, `terminate_login_process_group`

Both pairs are gated `#[cfg(unix)]`, not `#[cfg(target_os = "linux")]`. `cfg(unix)` is **true on
macOS**, so macOS compiles into the *real* implementation, not the honest `#[cfg(not(unix))]` no-op
stub next to it — but the real implementation walks `/proc/<pid>/task/<tid>/children`, a Linux-only
procfs path that does not exist on macOS. At runtime on macOS, `std::fs::read_dir("/proc/...")`
returns `NotFound`, which the walk's own error handling silently treats as "no children here" and
skips — so `descendant_pids` silently returns an empty list, and the process-group teardown quietly
degrades to killing only the shell's own top-level pgid, with **no log line, no error, nothing**
distinguishing this from the full, correct descendant walk. This is precisely the failure mode the
task named: a macOS build where real-looking, wired code silently does less than it appears to.

This is not merely uncaught — it is actively **misdocumented** in two places:
- `J3-macos-report.md` states outright: "the `/proc` walks in
  `tiller_activity`/`tiller_terminal`/`tiller_ui`... the `cfg(not(target_os = "linux"))` twin already
  exists... and is already an honest, correctly-typed stub" — false for `tiller_terminal` and
  `tiller_ui/settings.rs`: there is no `cfg(not(target_os = "linux"))` twin for either, only a
  `cfg(not(unix))` twin, which never fires on macOS.
- `PORTABILITY.md`'s cfg-seam table lists `tiller_terminal/src/lib.rs` under "Process enumeration"
  with the shape "real `/proc` walk → `Err(Unsupported)` / no-op, same signature" as if it behaved
  identically to `tiller_activity/src/process.rs` (which *is* honest, correctly gated). It does not:
  macOS gets neither an `Err` nor an acknowledged no-op, it gets the silently-degraded real path.

Functionally this is a narrow blast radius (the PTY's own immediate child is still torn down via the
existing `Msg::Shutdown` send regardless; only escaped/detached descendants that changed process
group are unreached on macOS — the same "narrow gap" J2-windows correctly describes for the Windows
side), and the underlying `/proc`-only behavior likely predates this wave's `cfg` additions (the code
would have hit the same wall on macOS before J2 too, if it had ever compiled there). But wave J's own
documentation asserts this exact case was checked and is honest, when it demonstrably is not — that
claim, not the underlying narrow functional gap, is the finding.

## Summary

1. Windows `cargo check --workspace`: fails (exit 101), classified `BLOCKED` by the gate on the known
   psm/stacker wall — confirmed correct on this evidence, with the caveat in §1–2 about which crates
   the workspace-wide run actually reaches.
2. macOS `cargo check --workspace`: fails (exit 101), same `BLOCKED` classification, same caveat.
3. No Linux regression: `tiller` 162 (161 baseline + 1 legitimately added test, audited, zero
   removed), `tiller_ui` 322 (exact match), `tiller_terminal` 44 (exact match), `tiller_persistence`
   39 (exact match, one pre-existing documented flake under parallel load), `tiller_usage`/
   `tiller_acp` per-file counts audited zero-removed against the pre-wave commit. `cargo build -p
   tiller` green.
4. Negative control: **not simply green-or-red**. `gtk` reintroduced unconditionally is caught
   (`FAILED:`) on 6 of 7 real attempts, but was masked as `BLOCKED:` (gate would have printed `CI OK`)
   on 1 of 7, because the gate's wall-classifier greps for wall-string presence rather than collecting
   all errors — a real, demonstrated, non-deterministic hole, not a hypothetical one. `ksni`
   reintroduced unconditionally does not break compilation at all on this evidence, so it is not a
   useful negative control by itself. Both attempts reverted cleanly; `git status --porcelain` empty.
5. `F-CHAT-05`: **half-proven** (independently re-driven live with fresh markers — offline-disable
   clause confirmed both halves of the pass bar; permission-wait clause remains environment-blocked,
   confirmed by re-reading the blocking config directly rather than trusting the prior report).
6. Seam honesty: one real, wave-J-introduced dishonest seam, in two files
   (`tiller_terminal/src/lib.rs`, `tiller_ui/src/settings.rs`) — `cfg(unix)` around Linux-only `/proc`
   code silently degrades on macOS with no log/error, and this was actively misdocumented as already
   honest in `J3-macos-report.md` and `PORTABILITY.md`. All other seams checked are honest as claimed.

No claim is made anywhere above that the app works, starts, or renders on macOS or Windows — every
finding above is `cargo check` (type-check only) or Linux-native runtime evidence.
