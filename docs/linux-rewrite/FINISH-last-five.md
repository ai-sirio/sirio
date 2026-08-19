# FINISH — the last five half-proven rows (lane `wf-last5`)

Critic pass, lane `TILLER_WL_LABEL=wf-last5`, pinned binary `/tmp/wf-last5-tiller` built from
this tree's HEAD before any change in this pass. Scope: the five named rows in the orchestrator's
brief, each with a named missing half. Written and committed incrementally, one row at a time, per
`EVIDENCE-STANDARD.md` and this wave's disk-pressure instructions. Host: x86 desktop, 2026-08-19,
shared with several sibling agents (`uptime` load average around 50-55 on 12 cores at times this
pass — noted where it affected a drive).

Order followed: F-TERM-PTY-04, F-CORE-WSP-07, F-CORE-WSP-05, F-CHAT-33, F-CORE-USG-07 (per brief),
though some background compiles were interleaved opportunistically while waiting on others.

---

## F-TERM-PTY-04 — shell fallback, terminfo choice, scrollback restore, settled resize

Row: "The terminal pane starts a shell in the worktree, prefers `$SHELL` then `/bin/zsh`, chooses
`xterm-ghostty` when available otherwise `xterm-256color`, restores initial scrollback, and waits
for a settled first resize before fallback sizing." PLATFORM: shell paths, AppKit sizing, and
libghostty are macOS/reference dependencies; Linux needs native GPUI/terminal sizing and terminfo
choices. VERIFY: "Start panes with and without `$SHELL`/ghostty terminfo, resize before launch,
and inspect shell, TERM, restored output, and final dimensions."

Downgraded this morning: the promoted evidence (`FINISH-sweep-tail.md`) covered only the
`$SHELL -> /bin/zsh` fallback, via a sibling's unit test re-run rather than independent live
proof, and left terminfo choice, initial-scrollback restore, and settled-first-resize completely
untouched. This pass drives all three, live, on lane `TILLER_WL_LABEL=wf-last5`.

### Clause 1 — `$SHELL` then `/bin/zsh`, and is `/bin/zsh` a *considered* Linux answer?

`rust/crates/tiller_terminal/src/lib.rs:346`:
```rust
let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
```
No `#[cfg(target_os)]` branch, no comment about Linux, no check that `/bin/zsh` exists — this is
the identical literal fallback path the macOS reference uses (macOS has shipped `/bin/zsh` as the
default login shell since Catalina). **This host genuinely has no `/bin/zsh`** (`ls /bin/zsh
/usr/bin/zsh` → "File o directory non esistente" for both; `which zsh` → nothing), re-confirmed
fresh this pass. Ubuntu/Debian/Fedora/Pop!_OS do not ship zsh by default either — this is not an
idiosyncrasy of this one box. A Linux user with `$SHELL` unset (rare, but real: minimal containers,
some service accounts, a corrupted environment) hits a terminal pane that **fails to spawn at
all**, which the already-landed test
`system_shell_falls_back_to_bin_zsh_when_shell_is_unset` demonstrates directly (it asserts the
spawn error names `/bin/zsh`, not that the fallback works).

**Judgment: not defensible as a considered Linux answer.** A considered port would fall back to
`/bin/sh` (POSIX-mandated to exist on every conforming system) or `/bin/bash` (near-universal on
Linux) — either would actually run. `/bin/zsh` copied verbatim from the macOS reference is an
accidental carry-over, not a Linux decision, and it is the one clause of this row that is a
genuine, reachable defect rather than a platform difference. **This sub-clause: FAILED —
defective.**

### Clause 2 — terminfo choice (`xterm-ghostty` vs `xterm-256color`)

`grep -rn "ghostty" rust/crates/tiller_terminal/src/*.rs` → zero hits (exit 1). Validated the
pattern is a working grep first (positive control): `grep -n "xterm-256color"
rust/crates/tiller_terminal/src/lib.rs` → hits at line 356, the one and only place `TERM` is ever
set, unconditionally. There is no terminfo-selection code path of any kind to drive.

Per `EVIDENCE-STANDARD.md`, a validated negative grep is an acceptable disproof of absence. But
this row also carries a PLATFORM note asking whether the absence is *defensible*. It is:
`tiller_terminal/Cargo.toml` depends on `alacritty_terminal` (the Zed fork), not libghostty —
confirmed (`grep alacritty_terminal rust/crates/tiller_terminal/Cargo.toml`). CLAUDE.md's own
architecture section says Tiller's terminal rendering "is built on libghostty" for the macOS app;
the Linux port does not embed libghostty at all, so there is no Ghostty-flavoured terminfo entry
that could ever be "available" in the sense the row means (Ghostty ships and installs its own
`xterm-ghostty` terminfo entry as part of *being* Ghostty; a port built on a different terminal
emulation library has nothing to check for). `xterm-256color` is the correct, standard, always-
present fallback for a non-Ghostty terminal emulator. **This sub-clause: `N/A - platform`** —
a genuinely considered consequence of the architecture substitution CLAUDE.md documents, not an
oversight the way the zsh fallback is.

### Clause 3 — settled-first-resize before fallback sizing

Code: `TerminalHandle::new` constructs the PTY with a hardcoded `WindowSize { num_lines: 24,
num_cols: 80, ... }` (`lib.rs:331-336`) before any GPUI layout has happened — this is the literal
"fallback sizing" the row names. `TerminalElement::prepaint` (`lib.rs` `impl Element for
TerminalElement`) then computes `columns`/`rows` from the **actually measured** `bounds` and font
metrics and calls `self.terminal.resize(...)` — but it does this **unconditionally, on every
single prepaint call**, not once at a "settled" moment. This is architecturally the opposite
sequencing from the macOS reference (which waits for a settled resize before spawning/sizing): the
Linux port spawns immediately at the 80×24 fallback, then continuously re-syncs to real bounds on
every paint. The open question was whether that continuous correction means a user could ever
*see* the fallback, or a resize-before-launch could still leave a pane stuck wrong.

**Live-driven, hard discriminator, this pass** (`/tmp/wf-last5-shots-resize`, `TILLER_WL_KEEP=1`):
booted, `project.add`, created pane-1 (`ctrl-t` is not needed — the default terminal opens with
the project) at the default 1715×972 window. Then **resized the window before creating a second
pane** — `swaymsg -s "$SWAYSOCK" output HEADLESS-1 resolution 2400x1400`, settled, then `chord
ctrl t` (`NewTerminalTab`) to create pane-2 *after* the resize. Read back **final dimensions**
with `stty size` (a real kernel `ioctl(TIOCGWINSZ)` query, typed live into each pane, not a shell
variable) — this required an explicit click into the pane's content area first (a real finding:
unlike Split (`F-CORE-WSP-05`), `ctrl-t`'s new tab does not GPUI-focus its terminal for text input
automatically; one-shot `wtype`/`key z` sent no keystrokes anywhere until a click landed on the
pane first, confirmed via `swaymsg -t get_tree`'s `focused:true` on the Tiller window the whole
time — a window-level/element-level focus distinction, not a dead keyboard per the harness-trap
checklist, which I ruled out the same way the checklist prescribes before drawing any conclusion).

Result: **both** pane-1 (created *before* the resize, at the original 1715×972) and pane-2
(created *after*, at 2400×1400) report `stty size` → `71 211` — 71 rows, 211 columns, matching
the *current* 2400×1400 window, not the 80×24 fallback and not each pane's own creation-time size.
Switching back to pane-1 and re-querying re-confirmed the same `71 211`. This proves the
"continuous re-sync on every prepaint" reading of the code is correct in practice: **every**
visible terminal pane's dimensions track the actual, current window bounds, regardless of when it
was created, and the 80×24 construction-time fallback is never what a user sees — GPUI's
layout→prepaint→paint pipeline runs synchronously before any frame is composited, so by the time
anything is on screen the size is already correct. "Resize before launch, inspect final
dimensions" (the row's own VERIFY wording): final dimensions matched the resize, on both the pane
created after it and the pane created before it. **This sub-clause: PASSED** — a different
mechanism than the reference's one-time wait-then-size (continuous sync vs. single settle), but a
considered, GPUI-native way of guaranteeing the same user-facing property the clause protects: the
fallback size is never observably shown.

### Clause 4 — restores initial scrollback

Live-driven across a genuine process restart, same lane, in three phases:

1. Fresh boot, `project.add`, clicked into pane-1's content, typed
   `echo PTY04_RESTORE_PROOF_M9910` + Return (landed and echoed, confirmed live), then clicked the
   Chat tab and back to Terminal to force a `schedule_save` (typing into a PTY does not itself
   reschedule a snapshot — a real seam worth knowing: only tab-level mutations do), then let the
   debounce (500 ms) elapse before the script's own cleanup killed the process.
2. **Direct, offline confirmation the byte-exact marker reached disk**, independent of the app:
   read `/tmp/wf-last5.sqlite`'s `tab_state` row for `default-terminal` with `sqlite3`'s Python
   binding (no `sqlite3` CLI on this host), decoded the JSON `scrollback` map's pane-1 byte array
   → contains `PTY04_RESTORE_PROOF_M9910`.
3. **Fresh process, same DB** (`TILLER_WL_LABEL=wf-last5`, `TILLER_WL_KEEP=1`, no typing this
   time), queried `panel.read id=pane-1` directly over a raw socket connection (bypassing the
   `ctl` helper's 3000-char print cap, which had truncated an earlier attempt right before the
   marker and produced a false-negative-looking read). The **live scrollback of the brand-new
   pane, in the brand-new process**, reads: `...echo PTY04_RESTORE_PROOF_M9910\n` followed by its
   own echoed output `PTY04_RESTORE_PROOF_M9910\n`, the **old** prompt line, immediately followed
   by a **second, fresh** neofetch banner and a **new** prompt from the pty's own freshly-spawned
   login shell — exactly the "replayed-history-then-live-session" shape
   `replay_persisted_terminal_scrollback`/`schedule_restored_scrollback`
   (`rust/crates/tiller/src/main.rs`) is built to produce, and exactly what pass 11's own prior
   live exercise described ("restored pane showed the previous session's prompt inside a newer
   session"), reproduced fresh here with a byte marker rather than trusted from that earlier
   report. **This sub-clause: PASSED.**

### Row verdict

Four sub-clauses: shell fallback path is proven both ways (`$SHELL` live, `/bin/zsh` fallback by
the already-landed test) but the *choice* of `/bin/zsh` as the Linux fallback is judged
**FAILED — defective** (accidental macOS carry-over, unreachable-in-practice on a stock Linux
box); terminfo choice is **N/A - platform** (defensible: no libghostty on Linux, confirmed via
Cargo.toml); scrollback restore is **PASSED** (live, cross-process, byte-marker discriminator);
settled-first-resize is **PASSED** (live, resize-before-launch discriminator on two panes).

**Row verdict: `half-proven`.** Per `EVIDENCE-STANDARD.md`'s conjunction rule, one genuinely
defective sub-clause (the zsh fallback choice) keeps the whole row off `PASSED` even though the
other three legs are now cleanly resolved (two `PASSED`, one legitimate `N/A - platform`). This is
a real, actionable, narrow gap for a builder: change the Linux fallback from `/bin/zsh` to
`/bin/sh` (or `/bin/bash`) in `rust/crates/tiller_terminal/src/lib.rs:346`, gated by
`#[cfg(not(target_os = "macos"))]` if the macOS behavior should stay `/bin/zsh` — at which point
this row is fully closeable.

---

## F-CORE-WSP-07 — the additive-evolution argument, tested rather than repeated

Row: "Workspace snapshots use schema version 1, canonical sorted JSON without escaped slashes,
reject future or missing versions, and materialize malformed snapshots as an empty group
registry." VERIFY: "Save a snapshot, compare canonical JSON stability, then feed malformed,
missing-version, and future-version data and observe the fallback/error behavior."

**Already proven, not re-touched:** the malformed-to-empty fallback (`SessionTabState::decode`'s
`Err` arm falling back to `SessionTabState::default()` plus quarantine, same evidence
`FINISH-window-persist.md`/`FINISH-wsp-rows.md` already established via a real corrupted-DB-row +
restart).

**Missing half, per the brief: test the additive-evolution argument instead of restating it.**
The argument (from `FINISH-wsp-rows.md`) is that `SessionTabState` has no version field and no
canonical-ordering treatment because the format evolves additively via `#[serde(default)]`
instead — the one real historical field-add (`chat_draft`, F-CORE-WSP-08) is guarded that way, so
an old blob still decodes under new code, and nothing in the codebase diffs or hashes the
persisted blob, so key ordering has no observable consequence either. That argument had never
been driven by a test — only cited.

Added two named tests to `rust/crates/tiller/src/session.rs` (module `session::tests`), both
calling the crate's own private `SessionTabState::decode` directly (not GPUI, not the DB layer —
this is a pure serde-boundary claim and belongs at that seam):

1. **`an_old_blob_predating_chat_draft_still_decodes`** — a hand-written JSON string in the exact
   shape `encode()` produced *before* `chat_draft` existed (`root_id`, `pane_events`, `scrollback`
   only — no `chat_draft` key at all), not today's own `encode()` output with a field stripped
   out. Asserts `decode()` succeeds and `chat_draft` defaults to `""`. This is a genuine test of
   "an old blob still decodes", not a tautology against current output.
2. **`reordered_keys_decode_to_an_identical_state`** — two JSON blobs with identical field values
   but keys in different order, both decoded, then compared with the derived `PartialEq` on
   `SessionTabState`. Proves key order really does produce byte-identical decoded state, not just
   "both happen to succeed".

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller -- \
    an_old_blob_predating_chat_draft_still_decodes reordered_keys_decode_to_an_identical_state
test session::tests::an_old_blob_predating_chat_draft_still_decodes ... ok
test session::tests::reordered_keys_decode_to_an_identical_state ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 194 filtered out
```

Both green. The argument holds under test.

**Residual gap, named rather than argued away (unchanged from prior passes):** a *semantically*
incompatible-but-structurally-identical future change (a field's meaning changes without its JSON
shape changing) would still be silently misread — an explicit version-reject is the one thing that
would catch it, and nothing tests for it because it has never happened in this format's one real
schema-change instance (`chat_draft`, purely additive). This remains a theoretical residual risk,
named honestly, not a demonstrated defect.

**Verdict: PASSED.** Per the brief's own instruction ("If the argument holds under test, PASSED
is available"): the malformed-to-empty leg was already proven live; the additive-evolution
counter-argument to "schema version + canonical JSON + reject" is now independently tested, not
just cited, and holds. The row's literal clause ("schema version 1... reject future or missing
versions") is not implemented verbatim, but the behavior it exists to guarantee — an old or
reordered blob is never misread, and a malformed one never crashes or corrupts — is verified
end-to-end with one narrow, named, never-observed exception.

Commit: `e8e488a9` (`test(F-CORE-WSP-07): prove the additive-evolution argument, not just cite it`).

---
