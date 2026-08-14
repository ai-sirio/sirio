# Wave A slice W04-chat — critic verdicts

Adjudicated independently — did not drive or build this slice, and did not drive any other
slice in this fleet either. HEAD `4073297`, worktree `tiller-linux`, branch `linux/gpui-waku`.
All 4 captures directories opened with `Read`; source cross-checked in
`rust/crates/tiller_ui/src/chat.rs`. No source touched.

## `F-CHAT-28` — ledger line 177, was **FAILED — absent** → **half-proven**

**Render/trigger half: PROVEN LIVE, and stronger than the driver's own write-up claims.**
Opened `f28/02-02-after-send.png`. It shows a card reading `Subagent  Run pwd and report
Completed`, purple-accented `"Subagent"` label, left rail border in `colors.rail_task`, and a
right-pointing chevron — this is unmistakably `render_subagent_task_card` (chat.rs:4072),
**collapsed** (`Icon::ChevronRight`, matching `expanded:false`). Below it, an assistant message
reads *"The subagent ran `pwd` and reported: `/home/.../tiller-linux`"* — a genuine ACP
Task-tool round trip only a live subagent dispatch could produce.

The driver's own evidence over-hedges here: it argues `control_entry_row` collapses
`Entry::SubagentTask` into the same socket `kind:"tool"` as a plain `ToolCall`, so "the socket
cannot distinguish which variant was constructed" and falls back to an inference
(`is_subagent_tool_call`'s raw-input check, "very likely taken, but not independently
confirmed"). That hedge is unnecessary — the **screen**, not the socket, is the row's real
oracle here, and on screen the purple `"Subagent"` label and rail-colored border make this
visually indistinguishable from nothing else in the app; it is definitively the SubagentTask
render path, independently confirmed against source, not inferred.

**Gesture half: NOT attempted, and — contrary to the driver's "no button geometry available on
this lane" — plausibly reachable, not structurally blocked.** `render_subagent_task_card`'s
whole header row is one `on_click` target spanning the full card width (chat.rs:4083-4121), not
a small hover-revealed icon; it is the only card in an otherwise short, fixed transcript (one
user pill, one three-line hook notice, this card, one assistant line), so its on-screen bounds
were computable directly from `02-02-after-send.png` itself, the exact frame the driver already
had in hand. A `click <x> <y>` + `shot` follow-up (the documented pattern in
`WAYLAND-LANE.md`) was never tried. Unlike `F-CHAT-29`/`F-CHAT-30`'s copy controls (small,
`invisible()` until hover, position dependent on unknowable reply length), this target is
large, always-rendered, and sits in a short, predictable transcript — closer to `F-CHAT-32`'s
missed opportunity (below) than to a genuine blocker.

**Verdict: half-proven.** Owed half: click the task-card header to expand it, then click the
nested `render_subagent_tool_call_card` header (chat.rs:4151, same full-width-click pattern) to
expand its content/diff, and confirm current action/status/details render.

## `F-CHAT-29` — ledger line 178, was **FAILED — absent** → **NOT EXERCISED**

**Not absent.** `grep -n "CopyTarget::Assistant\|group_hover" rust/crates/tiller_ui/src/chat.rs`
confirms the hover-revealed per-message Copy control exists (chat.rs:3628-3670): `.invisible()`
by default, `.group_hover(hover_group, |style| style.visible())`, `on_click` → `copy_local_text`
→ 2s `"Copied ✓"`. The ledger's current evidence text (pass 17, 02:40) predates this control's
build commit `398c0aea` (17:25) by 14.75h, exactly as triage says — it is stale, not a live
re-confirmation of absence.

**Not proven live either.** Opened `f29/b1-before.png` and `f29/b2-after.png`. The attempted
click landed at `(1350, 186)` — outside the Chat panel entirely (the transcript column's right
edge sits at roughly `x=1176` in every other capture in this slice; `x=1350` falls inside the
**Files** panel, which shows `"Loading files…"` in `b2-after.png`, consistent with a stray click
there, not a miss inside a mis-measured Chat column). No `"Copied ✓"` ever appears. This wasn't
a proven structural impossibility so much as a poorly calibrated blind guess — the copy control
is `absolute() top(0) right(0)` of its own message container (chat.rs:3634-3636), so its target
column is the Chat panel's own right edge, not the window's.

That said, the harder parts of the driver's blocker are real and independently confirmed:
(1) the control is hover-**only** visible, so no baseline screenshot shows where to click before
guessing, unlike `F-CHAT-32`'s always-rendered card; (2) `grep -rn "Paste" rust/crates/tiller_terminal/src/ rust/crates/tiller_ui/src/composer.rs rust/crates/tiller_ui/src/chat.rs` confirms the
only `Paste` action anywhere is `TerminalContextAction::Paste`, right-click-only, out of scope
per `WAYLAND-LANE.md` — so even a perfect click could not close the clause's
paste-and-confirm half on this lane.

**Verdict: NOT EXERCISED.** Blocked environmentally for this lane (hover-gated control with no
queryable geometry pre-click, no keyboard paste path), not absent and not disproven live.

## `F-CHAT-30` — ledger line 179, was **FAILED — absent** → **NOT EXERCISED**

**Not absent.** `grep -n "CopyTarget::CodeBlock\|code-block-copy" rust/crates/tiller_ui/src/chat.rs`
confirms the control exists (chat.rs:2973-2974 selector, same `copy_local_text`/confirmation
plumbing as `F-CHAT-29`), built in `0ecbd525`, after the stale pass-17 evidence the ledger still
carries.

**No new drive this pass** — the driver correctly reused `F-CHAT-29`'s structural finding
rather than re-running an identical failed-geometry attempt (`captures: []`, self-reported
`discriminating: false`). Confirmed the reasoning holds: the code-block control's selector is
per-block and per-entry, position dependent on where inside the model's actual generated
markdown a fenced block starts — strictly less locatable than `F-CHAT-29`'s already-hard,
hover-gated, always-only-one-per-message control. No live attempt exists to grade as a miss or
a hit.

**Verdict: NOT EXERCISED.** Same reasoning as `F-CHAT-29`; ledger's "absent" evidence is stale,
control exists, nothing was live-driven.

## `F-CHAT-32` — ledger line 181, was **FAILED — absent** → **half-proven**

**Trigger half: PROVEN LIVE.** Opened `f32/02-02-after-send.png`. It shows a real, fresh
`render_edit_summary` card (chat.rs:3442): header `Edit  Write docs/scratch/w04chat32_probe.md
Completed`, `"1 file changed"`, the file's absolute path in accent color, and a red-colored
`"Revert"` control on the same row. This followed a genuine ACP `Write` tool call the driver
verified independently of the chat transcript — `cat`/`git status --porcelain` on the actual
file on disk, cleaned up afterward with `git clean -fd`. Solid, disk-independent proof the edit
summary path is real and reachable from a live turn — not "absent."

**Gesture half: NOT attempted, and the driver's "could-not-reach" framing does not hold up.**
Read `chat.rs:3479-3568`: the file-path text itself is `id("edit-summary-open-{entry}-{index}")`
(the row's "Open" control — `flex_1`, accent-colored, `on_click` → `ChatEvent::OpenFile`), and
`"Revert"` is a separate, always-rendered `on_click` sibling on the same row (no hover-gating on
either). Both are clearly visible, at a fixed, non-content-dependent vertical position in
`02-02-after-send.png` — this card's layout does not vary with reply length the way `F-CHAT-29`/
`30`'s controls do; there is exactly one diff, so exactly one row, right below the fixed
`"1 file changed"` header. The driver had this exact frame in hand and argued a
`DISPLAY=:1`-only lane "could plausibly close this" — but `WAYLAND-LANE.md` documents this
Wayland lane's own `click <x> <y>` + `shot` gesture loop as the intended way to close exactly
this class of row, and the driver used that same mechanism (imperfectly) on `F-CHAT-29`. It was
not tried here at all, on a target that was easier to locate than `F-CHAT-29`'s.

**Verdict: half-proven.** Owed half: click the file path (Open), click `"Revert"` → `"Confirm
Revert"`/`"Cancel"` (chat.rs:3517-3548, a second click required — not a single Revert-and-done),
click Confirm, and observe the `"reverted"` label or a `revert_error` string.

## Summary

| Row | Old | New | Half proven | Half owed |
|---|---|---|---|---|
| F-CHAT-28 | FAILED — absent | half-proven | Subagent card renders, visually distinct, real ACP round trip | Click-to-expand card + nested tool call |
| F-CHAT-29 | FAILED — absent | NOT EXERCISED | (control exists in source; nothing live) | Hover+click Copy, paste, confirm — click missed the panel |
| F-CHAT-30 | FAILED — absent | NOT EXERCISED | (control exists in source; nothing live) | Click code-block Copy, paste, confirm — not attempted |
| F-CHAT-32 | FAILED — absent | half-proven | Edit-summary card renders from a real Write tool call | Open + Revert→Confirm click flow |

**Disagreement with the driver, for the record:** the driver labeled `F-CHAT-28` and `F-CHAT-32`
"partially-exercised" and `F-CHAT-29`/`F-CHAT-30` "could-not-reach" — none of those are ledger
vocabulary, so this mapped them to `half-proven` / `NOT EXERCISED` respectively. The more
substantive disagreement is with the driver's own stated reason for not attempting the
`F-CHAT-28` and `F-CHAT-32` click gestures ("no button geometry available on this lane" /
"could-not-reach... a lane with real visual feedback could plausibly close this"): both drives
already held a capture with the target control clearly, statically visible at a computable
position, and `WAYLAND-LANE.md` documents this exact lane as supporting a `click`-then-`shot`
loop for precisely this class of gesture. The driver reached the harder half of both rows (a
real live ACP round trip) and then declined the easier half (a plain click on a control already
sitting in a screenshot in front of them). That is a real, avoidable gap, not a structural one —
flagging it because a `half-proven` row closed on a future pass should attempt the click, not
repeat the "no geometry" framing.
