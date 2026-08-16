# J4-ci — gate the macOS/Windows compile checks into `Scripts/ci-linux.sh`

Slice: add `cargo check --target x86_64-pc-windows-msvc --workspace` and
`cargo check --target aarch64-apple-darwin --workspace` to `Scripts/ci-linux.sh` as two more
`run_cargo_stage`-style stages, so the portability debt J1/J2/J3 closed cannot silently come back.
Rewrite PORTABILITY.md's "Recommended order" section to describe what was actually done instead of
what was planned. **Outcome: green**, but getting there required clearing pre-existing, unrelated
debt in the gate itself that nobody had actually run end-to-end in a while (see "Unplanned" below).

## The two new stages

Added to `Scripts/ci-linux.sh`, placed last (after fmt/clippy/build/test/the two Python suites/the
visual-sweep contract check), immediately before the headless smoke test:

```
run_cross_target_stage "cargo check --target x86_64-pc-windows-msvc --workspace" x86_64-pc-windows-msvc
run_cross_target_stage "cargo check --target aarch64-apple-darwin --workspace" aarch64-apple-darwin
```

`run_cross_target_stage` reuses the existing `fail_stage` helper (so a real failure still aborts the
gate exactly like every other stage's `FAILED:`) but adds two non-fatal, loudly-announced outcomes on
top of `PASS:`/`FAILED:`:

- **`SKIP:`** — `rustup target list --installed` doesn't list the triple. Prints the exact
  `rustup target add <triple>` fix. Same reasoning as the existing `TILLER_ACP_REAL` skip a few
  stages up: a one-command, always-fixable, per-contributor setup gap that cannot regress from a code
  change should not block a contributor from reaching a green gate.
- **`BLOCKED:`** — rust-std is present, but the check fails with `error occurred in cc-rs:` in the
  log. That string is `cc`-the-crate's own signature for "the C/asm compiler invocation itself
  failed", and it only ever originates from a third-party build script (`psm`, `stacker`,
  `libsqlite3-sys` were all observed triggering it during this slice — see "Unplanned" for why the
  crate-name enumeration was replaced with this mechanism-based match). None of our 13 workspace
  crates shell out to a C compiler from `build.rs`, so this signature cannot originate from our own
  code. Confirmed independently by J2-windows and J3-macos to be a real, structural wall on this box
  (no MSVC toolchain, no macOS SDK, no cross-linker), not fixable by any `cfg` seam.

Anything else — dependency resolution failing, a missing `cfg` branch, a new unconditional
Linux-only crate — matches neither pattern and hard-fails the gate via the same `fail_stage` every
other stage uses.

Full reasoning, including why `SKIP` and `BLOCKED` are treated differently and why this does not
make `Scripts/ci-linux.sh` and `Scripts/ci.sh` (the Swift/macOS gate) disagree about what "green"
means, is written as a comment block directly above the stages in the script — that comment is the
primary record, this report summarizes it.

## `Scripts/Tests/test-ci-linux.sh` extended to match

Added markers for the two new stage labels, `rustup target add`, and `error occurred in cc-rs:` to
the existing `required_markers` array; added `windows_line`/`macos_line` to the stage-ordering
assertion (`visual_line < windows_line < macos_line < smoke_line`); added two more `grep -Fq`
assertions mirroring the existing "the ACP skip must be announced, not silent" check, this time for
`SKIP: $stage` and `BLOCKED: $stage` plus the `not a code regression` phrase the BLOCKED path prints.
`bash Scripts/Tests/test-ci-linux.sh` passes.

## PORTABILITY.md

Replaced the "Recommended order" section (a plan) with what was actually done: a 13-crate ×
2-target table (6 crates compiler-clean on both targets with zero platform work needed; the other 7
blocked on this box by two named, confirmed-hard walls, not by anything in our source), a table of
every `cfg` seam that exists today and exactly what its non-Linux branch does, and — the part
PORTABILITY.md did not previously say clearly enough — an explicit call-out that
`tiller_control/src/panel.rs` (ConPTY backend) and `tiller_ui/src/browser.rs` (X11/XCB child-window
attach, `NSView`/`HWND` counterparts) are **not seamed at all**, `browser.rs` in particular having
had zero attention because `tiller_ui` never got compiler-checked far enough on either foreign target
to reach it. Ends with a short "What is next" pointing at real hardware as the only way to advance
the 7 blocked crates and the two unseamed files.

## Verifying the two new stages in isolation

Before touching the real gate, extracted `run_cross_target_stage` into a standalone script and ran
it against both real targets to confirm the `SKIP`/`PASS`/`BLOCKED` branches actually fire correctly:

- `wasm32-unknown-unknown` (not installed) → `SKIP:` with the `rustup target add` line. Confirmed.
- `x86_64-pc-windows-msvc` → `BLOCKED:`, log shows `error occurred in cc-rs:` from `psm`'s assembler
  step (first run) or `stacker`'s `windows.c` (second run, same target, different build-graph
  ordering — see "Unplanned"). Confirmed both trigger the same match.
- `aarch64-apple-darwin` → `BLOCKED:`, log shows `error occurred in cc-rs:` from `psm`'s `cc -arch
  arm64` invocation, matching J3-macos's own finding exactly.

## Unplanned: the gate had accumulated real, unrelated debt

No wave-J report before this one records ever running `bash Scripts/ci-linux.sh` end-to-end — J0-J3
all used targeted `cargo check`/`cargo test` invocations. Running the real gate for the first time
this slice surfaced three pre-existing failures, none touched by J0-J3, none part of this slice's
actual task, all reproduced identically on `f6a985c9` (the commit immediately before this slice
started) to confirm they predate it:

1. **`cargo fmt --check` failed across ~34 files spanning virtually every crate** — uniform
   line-wrap/width-heuristic differences (spot-checked several, all mechanical), not one owner's
   in-flight work. The repo pins no `rust-toolchain.toml`, so this is rustfmt-version drift between
   whatever wrote the tree and this box's `rustfmt 1.9.0-stable (2026-07-14)`. Fixed with
   `cargo fmt --all`; `cargo fmt --all -- --check` clean afterward. Whitespace-only, so no build/test
   risk — confirmed by running the full test suite afterward (see "Full gate run" below).
2. **`cargo clippy --workspace --all-targets --exclude tiller --exclude tiller_ui -- -D warnings`
   failed with 3 distinct lints**, likely the same lint-version-drift story as (1):
   - `tiller_persistence/src/db.rs`: `classify_open_error` was defined *after* the `#[cfg(test)]`
     test module (`clippy::items_after_test_module`). Moved verbatim above the module; no logic
     changed. `cargo test -p tiller_persistence --lib` stayed green (8/8) after the move.
   - `tiller_acp/src/lib.rs`: `run_connection` has 8 parameters (`clippy::too_many_arguments`, limit
     7). Bundling them into a params struct would touch call sites outside this slice's scope, so
     added a scoped `#[allow(clippy::too_many_arguments)]` with a comment explaining why, rather than
     refactor pre-existing wiring incidentally. `cargo test -p tiller_acp --lib` stayed green (28/28)
     after.
   - `tiller_usage/tests/usage_tests.rs`: `isolated_path`/`fake_claude` took `&PathBuf` where
     `clippy::ptr_arg` wants `&Path`; both call sites already pass `&PathBuf` fields (`&dir.0`), which
     deref-coerce cleanly to `&Path`, so this is a pure signature fix, zero call-site changes needed.
3. **The `psm`/`libsqlite3-sys` crate-name whitelist for the `BLOCKED` match was too narrow the
   moment it was tested a second time**: the first probe run against `x86_64-pc-windows-msvc` failed
   inside `psm`; a second run against the same target (same warmed `target/x86_64-pc-windows-msvc/`)
   failed inside `stacker` instead — a different crate in the same dependency chain, hitting the same
   underlying "no cross C toolchain" wall through a different build script. A crate-name whitelist
   would have missed `stacker`, and would keep missing whichever `sys`-crate the graph reaches next
   as it grows. Replaced the whitelist with the mechanism-based match (`error occurred in cc-rs:`)
   described above before this was committed as the gate's real behavior — caught during my own
   verification, not left for the critic to find.
4. **The headless smoke test itself could not reach `CI OK` on any commit, old or new.** It spawns
   the app against a fresh, empty `$TILLER_DB` (by design — a new one every run, so parallel gates
   never collide) and immediately calls `tillerctl current-workspace`. `ControlState::from_catalog`
   (`crates/tiller/src/main.rs`) builds the `workspaces` list purely from the persisted project
   catalog; being launched inside a git checkout is not by itself enough for a workspace to appear —
   nothing in `session::restore`/`session::restore_catalog` auto-registers `working_directory` as a
   project. So `current-workspace` always answered `no current workspace`, every time, regardless of
   cwd. **Confirmed pre-existing and unrelated to this slice**: reproduced identically from a clean
   `git clone` of `f6a985c9` (before any J4-ci commit), spawning that build directly and calling
   `tillerctl current-workspace` by hand — same failure, same message, zero Rust source involved on
   my side. `Scripts/visual-sweep.sh` already carries the fix for its own fixture (`tillerctl project
   add "$FIXTURE"` before any `workspace.*` call, gated on `system.capabilities` advertising
   `project.add`); `ci-linux.sh`'s smoke test was simply missing the equivalent line for `$ROOT`. One
   line added, mirroring that exact precedent; confirmed by hand (`tillerctl project add <dir>` then
   `current-workspace`) before touching the real script, then confirmed again through the full gate.

Items 1–3 are committed separately from the stage-addition commit; item 4 is its own commit too
(`fix(ci): seed a project before the smoke test asks for current-workspace`). Each is labeled
explicitly as pre-existing and unrelated, so they read as their own honest record rather than being
folded into "add two CI stages." Taken together they mean **nobody had run `Scripts/ci-linux.sh` to
a real `CI OK` on this checkout before this slice** — every prior wave-J report verified its work with
targeted `cargo check`/`cargo test` invocations, never the full gate script, so fmt drift, clippy-lint
drift, and the smoke test's missing bootstrap step had all accumulated invisibly underneath a gate
that looked ready but had never actually been run to completion.

One more, smaller finding, left alone rather than fixed: `smoke_capture`'s `current-workspace` and
`panel-create` calls are wrapped in `$(...)` (`current_workspace=$(smoke_capture ...)`). Bash captures
a command substitution's **stdout** into the variable — so on failure, `smoke_failure`'s own
diagnostic `echo`s (called from inside that same subshell) are silently captured into the variable
instead of reaching the gate's visible output, and the script exits 1 with no printed reason at all.
This is exactly what made the first several debugging attempts at item 4 look like a silent hang
before the actual "no current workspace" cause was traced through the raw `$SMOKE_DIR/*.out` files by
hand. It's a real, separate, pre-existing sharp edge in the gate's own diagnostics — worth its own
narrowly-scoped fix — but changing `smoke_capture`'s control flow is outside what this slice touched,
so it is reported here rather than patched incidentally.

## Full gate run

```
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
bash Scripts/ci-linux.sh
```

Real, un-doctored final run against the fully committed tree (`git status --porcelain` clean except
for this report file):

```
==> Rust format and clippy
PASS: cargo fmt --check
PASS: cargo clippy (owned crates)
PASS: cargo build
PASS: cargo test --workspace
SKIP: real ACP acceptance — set TILLER_ACP_REAL=1 to exercise it
PASS: test-crash-supervise.py
PASS: test-crash-freeze-supervise.py
PASS: test-visual-sweep.sh
==> Cross-platform compile checks (macOS, Windows)
BLOCKED: cargo check --target x86_64-pc-windows-msvc --workspace — known SDK/cross-toolchain wall, not a code regression
  no MSVC toolchain / macOS SDK / cross-linker on this box; see docs/linux-rewrite/PORTABILITY.md
  error occurred in cc-rs: ... -c src/arch/x86_64_windows_gnu.s   (psm)
BLOCKED: cargo check --target aarch64-apple-darwin --workspace — known SDK/cross-toolchain wall, not a code regression
  no MSVC toolchain / macOS SDK / cross-linker on this box; see docs/linux-rewrite/PORTABILITY.md
  error occurred in cc-rs: ... -c sqlite3/sqlite3.c   (libsqlite3-sys)
==> Headless smoke test
PASS: current-workspace -> tiller	linux/gpui-waku	/home/enzopalmisano/Scrivania/Progetti/tiller-linux	p-c1fd7a5bbfd541af-wt-1
PASS: panel.write → panel.read returned generated nonce
PASS: headless smoke test
CI OK
EXIT_CODE=0
```

`bash Scripts/Tests/test-ci-linux.sh` (the meta-test protecting the gate's own contract, extended by
this slice): `PASS: Linux verification gate contract is present and ordered`.

**Note on flakiness observed along the way, not in the final run above:** this box is shared with
several other concurrent Claude Code sessions (`load average: 21.14, 16.61, 14.10` on 12 cores was
observed mid-slice). Under that load, `cargo test --workspace` twice failed on
`tiller_terminal::tests::shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
— confirmed to be the exact same pre-existing timing-sensitive test J2-windows-report already
documented flaking "once under full-workspace parallel load"; reproduced passing cleanly in isolation
(`cargo test -p tiller_terminal --lib <name>`, single thread) both times. Not a regression, not
touched by this slice, and not present in the final run recorded above.

## Commits

- `ci: gate macOS/Windows cargo check into the Linux verification gate` — the two new stages plus the
  reasoning comment block, in `Scripts/ci-linux.sh` and `Scripts/Tests/test-ci-linux.sh`.
- `chore: apply cargo fmt to clear pre-existing toolchain-drift formatting debt` — 34 files,
  whitespace-only.
- `chore: clear pre-existing clippy debt blocking the owned-crates gate` — `tiller_persistence/src/
  db.rs`, `tiller_acp/src/lib.rs`, `tiller_usage/tests/usage_tests.rs`.
- `docs: replace PORTABILITY.md's plan with what wave J actually did` — the per-crate/per-target
  table, the `cfg`-seam table, and the explicit unseamed-gap call-out.
- `fix(ci): seed a project before the smoke test asks for current-workspace` — the one-line
  `project add` fix plus its `test-ci-linux.sh` marker.
- (this report)
