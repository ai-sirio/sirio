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

## F-CORE-WSP-05 — isolating the divider/nonstructural leg, live-isolated (not code-proof)

Row (per the orchestrator's own summary of the evidence cell): Split and Close were already
live-driven this wave with a zero-click scrollback discriminator (`panel.scrollback` marker text,
negative control on the un-clicked pane). Owed: the divider/nonstructural leg, previously
**code-proof only** — `update_divider` read in full, takes no `Window` param and calls no focus
function — but never independently isolated live, because a drag attempt could not rule out the
mousedown simply landing on the already-focused pane. Also owed: settle whether Insert/Move have a
real-app analog.

### Isolating the divider, with a drawn test (not a live Wayland drive)

Per `EVIDENCE-STANDARD.md`'s UI tier ("a **named** test using `TestAppContext` /
`VisualTestContext` that draws the element and dispatches the real event"), a drawn test is valid
UI-tier proof, and it is the only instrument that can truly **isolate** the divider from the
"mousedown lands on the already-focused pane" confound the brief names: it lets the divider's own
drag-handle hitbox be located by its own drawn bounds (`pane-divider-h-handle-{path:?}`) and driven
independently of either pane's leaf area (`pane-leaf-{id}`), something a screen-coordinate live
drive cannot pin down as precisely.

**Instrumentation added, harmless in release builds:** `render_pane_tree`'s divider handle divs
(`rust/crates/tiller/src/main.rs`, both the horizontal and vertical branches) and its leaf wrapper
div had no `debug_selector` (or only a single, non-unique static `"pane-leaf"` one shared by every
leaf, colliding across panes) — so `VisualTestContext::debug_bounds` could not locate either
element by a distinguishing key. Added a per-pane `debug_selector(move || format!("pane-leaf-{pane_id}"))`
and matching `debug_selector`s on both divider-handle divs (mirroring their existing `.id(...)`
strings). `debug_selector` is `#[cfg(any(test, feature = "test-support"))]` in GPUI — a compiled-out
no-op in a release build, confirmed by reading `div.rs`'s own two cfg-gated definitions — so this
is pure test instrumentation, not a behavior change.

**The new test:** `drawn_divider_drag_blurs_focus_to_workspace_root`
(`rust/crates/tiller/src/main.rs`, `cargo test --manifest-path rust/Cargo.toml -p tiller --bin
tiller -- drawn_divider_drag_blurs_focus_to_workspace_root`: **1 passed**). It draws a real 2-pane
split via the same `ctrl-alt-shift-right` keychord a live drive uses, captures a live
`FocusHandle` for each pane's real `TerminalView` (not a stand-in), asserts the known-true baseline
(split leaves the new pane focused), locates the divider handle's own drawn bounds, then drags it
by a real `MouseDownEvent`/`MouseMoveEvent`×2/`MouseUpEvent` sequence confined to the handle's own
9px hit strip (so the final mouse-up cannot land over a neighbouring pane — ruling out the exact
confound the brief names) and confirms the drag genuinely resized the split (a `SetRatio` pane
event is recorded — otherwise "focus didn't move" would be vacuously true because nothing happened).

### What it found: the isolation contradicts the hoped-for result

`update_divider` itself is exactly as previously read — no `Window` parameter, no focus call. But
the isolated drag still blurs focus, through a different mechanism:

- Immediately after the `MouseDownEvent` alone (before any move, before any resize), **both**
  captured `FocusHandle`s read `is_focused == false`.
- `tab.focused_pane` (the data-model field `select_pane` would update) is confirmed **unchanged**
  (still the new pane, id 1) at this same point — ruling out the exact confound named in the brief:
  this is not `select_pane`'s own `on_mouse_down` firing from a stray hit on a pane.
- A same-handle identity check (`focus_new == focus_new_after`, `focus_old == focus_old_after`,
  read fresh from the model after the drag) confirms neither `TerminalView` entity was dropped and
  recreated — the loss is a real focus transfer, not a stale-handle artifact from entity churn.
- After the drag settles, the actually-focused element is **the workspace's own `root_focus`**
  (`workspace.read(cx).root_focus`, `is_focused == true`) — the exact fallback handle
  `TillerWorkspace::render`'s F-SID-19 comment describes a few lines above
  `render_pane_tree`: "if nothing at all holds keyboard focus this frame ... reclaim focus onto
  `root_focus`". Something about pressing the divider handle empties `window.focus` outright (not
  merely re-targets it), and this pre-existing safety net — built for an unrelated case, an
  unmounted previously-focused surface — is what actually catches the fall and keeps the app from
  being left with *no* keyboard target at all.
- Reproduced with a real preceding hover-`MouseMoveEvent` before the down (rules out a hit-test
  staleness artifact specific to jumping straight to `MouseDownEvent` with no prior hover), and
  with the whole drag confined to under 5px of travel, well inside the handle's own strip (rules
  out the up-event landing over a neighbouring pane).

**This is a genuine, reproducible defect, not a code-reading assumption and not a test artifact of
skipping hover-before-down.** A user who drags a pane divider to resize a split loses keyboard focus
entirely for one frame, silently recovered onto the workspace root rather than the pane they were
just typing into — meaning their very next keystroke goes nowhere useful (`root_focus` has no
visible input target) until they click a pane again. **Divider/nonstructural sub-clause: `FAILED —
defective`**, not the row's hoped-for `PASSED`. (One further, secondary oddity surfaced while
building the negative control for this test: a raw synthetic click directly on a pane's own leaf
area updated the data model's `focused_pane` but did not reliably move `window.focus` there in this
specific fixture, even *before* any divider interaction — a second, distinct harness/app question
not chased down further here since it is outside this row's scope; named so a future pass does not
have to rediscover it from scratch.)

### Insert/Move: still no real-app analog — confirmed absent, not merely unreachable

Re-checked directly at the source of truth rather than re-citing the prior pass: `session.rs`'s
`PaneEvent` enum (the pane mutation model everything else in this row replays through) has exactly
three variants — `Split`, `SetRatio`, `Close` — confirmed by reading the full `enum PaneEvent`
definition. There is no `Insert` or `Move` variant at all, so this is not "a variant exists but
nothing in the UI constructs it" (which would leave open the possibility of a hidden or
keyboard-only path); the pane data model itself has no representation for either operation to
replay, persist, or drive. A broader grep for `insert_terminal_tab`/`"Move"`-adjacent pane
functions in `main.rs` and `panes.rs` turns up only tab-level operations (inserting a new *tab*,
unrelated to pane-tree structure) — nothing pane-structural. This settles the brief's open question
as a genuine, confirmed absence rather than an unreachable-but-present feature: **Insert/Move have
no real-app analog in this port at all.**

### Row verdict

Split: **PASSED** (prior wave, live, zero-click discriminator, not re-touched here). Close:
**PASSED** (prior wave, live, twice-reproduced discriminator, not re-touched here).
Divider/nonstructural: **FAILED — defective**, newly isolated live via a drawn test this pass (was
code-proof-only). Insert/Move: confirmed no real-app analog exists (not a missing search, a settled
absence).

**Row verdict: `half-proven`** (unchanged) — but the divider leg has moved from "code-proof only,
ambiguous" to a **positively identified defect**, which is a materially stronger and more actionable
state than before: a future pass has an exact, named repro (`drawn_divider_drag_blurs_focus_to_workspace_root`)
and an exact mechanism (mousedown on the divider handle empties `window.focus`; F-SID-19's
`root_focus` reclaim is what prevents total focus loss) rather than an unresolved ambiguity.

Commit: `c4ec95ce` (adds `debug_selector`s to `render_pane_tree`'s divider/leaf divs and the new
drawn test to `rust/crates/tiller/src/main.rs`).

---
