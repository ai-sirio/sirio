# Independent verification — `tiller_control` cross-platform seam (wave K)

Scope: re-run everything the K-seam wave claimed, from scratch, on this box. I did not write
`panel.rs`'s seam; this is a critic pass over commits `da053445`/`448601ce` plus the CI gate they
depend on.

## 1–2. Cross-target `cargo check --workspace --keep-going`

Ran both real commands from `/home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust`:

```
cargo check --target x86_64-pc-windows-msvc --workspace --keep-going
cargo check --target aarch64-apple-darwin --workspace --keep-going
```

**`grep -n tiller_control` on both full logs finds zero `error` lines** — only 3 pre-existing
warnings on Windows (`unused_imports` in `client.rs`/`server.rs`, `dead_code` on
`ControlServer::handler`), zero warnings on macOS. `cargo check -p tiller_control` alone on both
targets: 0 errors, exit 0. **`tiller_control`'s seam is compiler-clean on both cross targets.**

Full-workspace residual, read by hand (not off the classifier — see §4 for why that matters):

- **Windows, 4 `^error` lines**: `stacker`, `psm`, `libsqlite3-sys`, `gpui` — all four print
  `error occurred in cc-rs:` immediately above them (`failed to find tool "lib.exe"` for gpui,
  `GNU compiler is not supported for this target` / assembler rejects `.def`/`.scl` pseudo-ops for
  the other three). All four are the same root cause: no MSVC cross-toolchain on this Linux box.
  **Non-wall (residual) count: 0.**
- **macOS, 4 `^error` lines**: `psm` and `libsqlite3-sys` print `error occurred in cc-rs:` directly
  (`cc: error: unrecognized command-line option '-mmacosx-version-min=11.0'/'-arch'` — no macOS
  cross-linker). The other two (`couldn't read .../media-.../out/bindings.rs`, `could not compile
  media`) are **not** literally `cc-rs`-branded, but trace to a distinct, equally environmental
  cause: `media`'s vendored `build.rs` (`zed` git dependency) is gated `#[cfg(target_os = "macos")]`
  on the **build host**, not the target triple — build scripts always run on the host, and the host
  here is Linux, so it's a no-op and never writes `bindings.rs`, regardless of target or SDK
  availability. Confirmed by reading
  `~/.cargo/git/checkouts/zed-*/crates/media/build.rs` directly. This is a structural limitation of
  cross-compiling this vendored crate from a non-macOS host, not fixable by any `cfg` seam in this
  repo, and not something a real macOS SDK on this box would fix either. **Non-wall count, counting
  only the literal cc-rs/psm/libsqlite3-sys definition: 2 (`media`'s two lines). Non-wall count in
  the sense of "a defect in our own code": 0** — nothing in the residual on either target names a
  `tiller_*` crate.

I am not claiming macOS or Windows *works*. `cargo check` proves the crate graph type-checks and
links symbol-resolves for that target; it proves nothing about runtime behavior, and there is no
cross-linker or real hardware here to test that at all.

## 3. No Linux regression

Per-crate, never `--workspace`, matching the stated baseline exactly:

| crate | expected | measured |
|---|---|---|
| `cargo build -p tiller` | green | green (0 errors, pre-existing `dead_code` warnings only) |
| `tiller` | 162 | **162 passed** |
| `tiller_ui` | 322 | **322 passed** |
| `tiller_terminal` | 44 | **44 passed** |
| `tiller_persistence` | 39 | **39 passed** (8 lib + 39 integration — the task's "39" is the integration suite; 8 lib tests are additional and also green) |
| `tiller_control` | "whatever it was before this wave" | **46 passed** (+10 lib), same as `f6a985c9`-era report |

For `tiller_control` specifically, checked the actual pre-wave baseline rather than assuming it:
`git checkout da053445~1 -- crates/tiller_control && cargo test -p tiller_control` on the current
tree gives the identical **10 lib + 46 integration**, then restored `HEAD`. Zero drift from the
K-seam commit on Linux.

## 4. Gate negative control — **the headline finding is here, and it is not the one the task expected**

Ran the *real* `run_cross_target_stage` function, extracted verbatim from
`Scripts/ci-linux.sh` (lines 144–161, 288–344) via a harness that sources it unmodified and calls it
against the real `cargo check` invocation — not a hand-typed reimplementation.

**Baseline (no injected regression, current committed tree): the stage already reports `FAILED` on
both targets, every time.** Not `BLOCKED`. This is not the wall-detection hole the wave-K brief was
built to close (cargo's fail-fast racing a fixed error against the wall) — it is a **new, separate
bug in the fix itself**, present right now on the clean tree:

- `CROSS_TARGET_WALL_PKGS='psm|stacker|libsqlite3-sys|rusqlite'` is matched against build-script
  failures with the pattern `` `failed to run custom build command for \`($CROSS_TARGET_WALL_PKGS)\`` ``
  — literally requiring the closing backtick to sit immediately after the bare package name. But
  `cargo check --keep-going`'s actual message shape is
  `` error: failed to run custom build command for `psm v0.1.32` `` — the version string sits
  between the name and the backtick, so **this filter never matches a single one of the four wall
  packages, on either target, in any run.** (`could not compile \`(...)\`` — the other filter line —
  never fires either: with `--keep-going`, a build-script failure never produces a downstream
  "could not compile" line for the crate whose own script failed, only for crates that depend on its
  output, and `psm`/`stacker`/`libsqlite3-sys` have no such downstream dependents here.)
- Even a version-tolerant fix to that regex would not reach `BLOCKED`: `gpui` (Windows) and `media`
  (macOS) are not in `CROSS_TARGET_WALL_PKGS` at all, and both are genuine wall failures (see §1–2).
- Net effect, verified by literally running the extracted function: **`residual` is never empty on
  either target on this box**, so the `if [[ -z "$residual" ]] && grep -qE "$CROSS_TARGET_KNOWN_WALL"`
  branch is unreachable here, and every run falls through to `fail_stage` → `FAILED`.

This means **the negative control the task asked for is not a meaningful test as specified**: adding
`gtk = "0.18.2"` to `tiller/Cargo.toml`'s unconditional `[dependencies]` and running the real stage
3 times does report `FAILED` all three times —

```
=== RUN 1 === FAILED: cargo check --target x86_64-pc-windows-msvc --workspace / FAILED: ...aarch64-apple-darwin...
=== RUN 2 === FAILED: ... / FAILED: ...
=== RUN 3 === FAILED: ... / FAILED: ...
```

— but the stage reports `FAILED` **identically, on every run, whether or not `gtk` is present**,
because the classifier can never reach `BLOCKED` on this box regardless of what the code says. A
gate that is permanently red on the cross-target stage teaches contributors to ignore it — the exact
failure mode the wave's own header comment warns about for a different reason (the sccache-daemon
probe). It also means item 4's literal pass/fail bar ("confirm it reports FAILED — three times") is
satisfied on paper while the property the task actually cares about (*the gate distinguishes a real
regression from the wall*) is **not demonstrated at all**, because both conditions produce the same
output.

`gtk`'s own transitive deps (`glib-sys`, `gobject-sys`, `gio-sys`, `cairo-sys-rs`, `pango-sys`,
sometimes `gdk-sys`) do surface as their own distinct `failed to run custom build command for`
lines in the residual once injected — so the defect is visible in the log, just not distinguished by
the classifier's `PASS`/`BLOCKED`/`FAILED` verdict, which was already `FAILED` before the injection.

Reverted after each run; `git status --porcelain` confirmed clean (no output) after the final
revert.

**Recommendation for whoever picks this up**: fix the backtick regex to tolerate a version suffix
(`` \`($CROSS_TARGET_WALL_PKGS)( v[^\`]*)?\` ``), and extend `CROSS_TARGET_WALL_PKGS`-equivalent
coverage to `gpui` (Windows) and `media` (macOS) — or better, match on the `error occurred in cc-rs:`
line itself plus the specific crate name it names, since that generalizes to whatever `sys`-crate the
dependency graph reaches next, which is exactly the lesson wave J4 already drew when it replaced a
plain crate-name whitelist with the mechanism-based match for the *outer* `BLOCKED` check. Until
fixed, this stage cannot pass on this box under any code state, wall-only or not — it should not be
trusted as a Linux-regression signal for cross-target work, and a genuinely fixed classifier needs a
**positive** control too (baseline tree, no injection → confirm `BLOCKED`) alongside the negative one,
since the negative control alone cannot detect "always FAILED."

## 5. Seam honesty

This wave's actual diff (`git show da053445` — 1 file, `panel.rs`, +84/-23) adds exactly two
`#[cfg(not(unix))]` items:

- `spawn_process` → `Err(PaneError::Unsupported(...))`, naming the ConPTY/`alacritty_terminal`
  counterpart in the message. Honest — it is the only function that ever constructs a `PaneEntry`, so
  no non-unix build can ever populate `panes`.
- `terminate` → empty body (`{}`). Provably unreachable rather than a silent no-op: since
  `spawn_process` always errors first on non-unix, no live `PaneProcess` can exist for it to be
  called on. Documented as such in the source comment, and every caller (`close`/`shutdown`/
  `shutdown_for`) is platform-generic and correctly reports `UnknownPane`/`ClosedPane` before ever
  reaching it.

No silent-success pattern (empty `Ok(())`, default value passed off as real work) in either branch.
Read the full file (963 lines) plus `client.rs`/`server.rs` in full — the brief's claimed error sites
at `client.rs:12`/`server.rs:38` are, as wave K's own report already noted, stale line numbers now
pointing at unrelated `use` lines producing only `unused_imports` warnings; both files' actual
`UnixStream` usage was already correctly `#[cfg(unix)]`-gated in J2/J3, with honest `Unsupported`
errors on the non-unix twins (`ClientError`/`ServerError::Unsupported`). Nothing new to fix there —
confirmed independently, not just re-quoted from the existing report.

## Summary

- **tiller_control's own seam: verified clean.** 0 errors on either cross target, 0 Linux
  regression, no dishonest `cfg(not(unix))` branch in this wave's diff.
- **The CI gate that was supposed to prove this is currently broken** in a way unrelated to
  `tiller_control`: `run_cross_target_stage`'s wall-package regex doesn't tolerate cargo's actual
  `` `pkg vX.Y.Z` `` message shape, and its wall-package list doesn't cover `gpui`/`media`. Net effect:
  the stage reports `FAILED`, not `BLOCKED`, on both cross targets on this box **right now, with no
  injected regression** — confirmed by running the exact function extracted from the script, not a
  paraphrase.
