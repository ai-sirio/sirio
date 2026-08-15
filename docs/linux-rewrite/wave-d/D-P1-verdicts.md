# D-P1 critic verdicts

Critic pass, independent of the builder — everything in `D-P1-report.md` was treated as a claim to
verify, not evidence. HEAD at verify time matches the tree the builder/integrator left (F-CHAT-14's
fix already landed in `tiller/src/main.rs`, confirmed by `grep`). Instruments: `Scripts/linux-drive.sh`
against the shared X11 (`DISPLAY=:1`) default instance (real persisted DB, real configured `claude`
CLI) for every row that needed a real ACP agent turn or pixel measurement; `Scripts/wayland-drive.sh`
(label `critic-dp1*`) for the sidebar project-form rows. `reference/linux-progress/sweep-D*/` frames
were not cited; every frame below is freshly captured this pass. Real side effects (files written to
disk, a real `git clone` subprocess, a real project folder) were checked directly with `ls`/`cat`,
not just read off a screenshot.

## F-CHAT-05 — half-proven (unchanged; two new live attempts, still could not force the condition)

Confirmed the guard is real and unchanged: `pending_question()` (chat.rs:2863) requires an
unresolved `Entry::Permission` or `Entry::Plan` approval, and `insert_text` (chat.rs:2897) returns
early when it's `Some` — this is the code the builder cites, still present. Made two genuine new
live attempts against the real `claude` CLI (not the builder's zero attempts): asked it to Edit a
file outside the worktree, once under "Auto" permission mode and once after switching to "Manual"
mode via the composer's permission-mode picker. Both times the tool call showed a transient
`Pending` label in the transcript card but resolved to `Completed` within a few seconds with no
`Entry::Permission`/accept-reject card ever appearing — this CLI's Manual mode did not produce a
human-in-the-loop ACP `PermissionRequest` for this action in this sandbox, matching the builder's
and Wave-C's "no adapter on this box reliably triggers it" finding. Not the target condition, so
this doesn't move the verdict either way. Instrument: real `claude` CLI turns, X11 lane,
`x05-05b`/`x05-07b`/`x05-07c` frames.

## F-CHAT-14 — PASSED (new evidence: real restart + real agent turn)

The integrator's landed fix (`main.rs:4292-4299`, `restore_launch_snapshot` now loops
`Self::bind_chat` over restored chat tabs) is real — confirmed by direct `grep`/`Read`, not the
integrator's word. Live-drove the exact route: `Scripts/linux-drive.sh` inherently kills and
relaunches the app each invocation against the shared persisted DB, so every fresh call is a real
`restore_launch_snapshot`. Confirmed via screenshot that a fresh launch shows the "Claude Code" Chat
tab already selected with status `connecting` (i.e., restored, not freshly created). In one
continuous drive: waited for reconnect, turned "Follow Edited Files" on via the overflow menu
(label flips `Follow Edited Files` -> `Stop Following`, confirmed by re-reading it), sent a real
message asking the agent to Write `/tmp/critic-dp1-edit-target2.txt`, and watched a new file tab
(`critic-dp1-edit-target2.txt`) open in the tab strip automatically right after the write completed
— `ls`/`cat` on disk confirms the file has the exact requested content. This is the previously-silent
no-op now working, on a genuinely restored session, with a real agent turn. Frames:
`x14-10-post-restore.png` (restore, `connecting`), `x14-11b-follow-toggled.png` (`Stop Following`),
`x14-11c-after-edit.png` (new file tab open).

## F-CHAT-18 — PASSED (new evidence: real `claude` CLI reports end-of-turn usage)

The builder's own caveat was "the real claude CLI's exact support for this ACP extension was not
independently confirmed live." Closed that gap: in one continuous X11 drive, sent a real one-line
turn to the real `claude` CLI, then clicked the context ring (composer's percent badge). The
popover reads `5% of context used`, `53477 / 1000000 tokens`, `Cost: 0.15 USD`, then below a
divider: `Input: 2 tokens`, `Output: 4 tokens`, `Cache read: 30418 tokens` — exactly the
Input/Output/Cache-read breakdown the row asks for, sourced from a real `PromptResponse.usage`
event, not a synthetic one. A control click on the same ring before any turn had run showed "The
agent has not reported context usage yet." (`x18-04-ring-click.png`), proving the popover
distinguishes real absence from real presence rather than always rendering the same thing. Frame:
`x18-05b-ring-popover.png`.

## F-CHAT-33 — half-proven (unchanged; new independent live attempt, same negative result)

Placed a genuinely broken `.mcp.json` (`command` pointing at a nonexistent binary) at the project
root, started a fresh conversation so the agent would load it, and ran a real turn against the real
`claude` CLI. No `mcp-warning` transcript card appeared — only the user message and the assistant's
reply. This is a third independent live attempt (two by the builder/prior critic, one by me) that
all fail to observe the card despite a genuinely broken MCP config; `looks_like_mcp_warning` and
the `mcp_warnings()` wiring remain real, unit-tested code (chat.rs/lib.rs unchanged), so this is not
a code regression — the live trigger condition (a specific stderr shape from the real CLI) still
isn't reachable in this sandbox. `.mcp.json` removed after the test; repo left clean.

## F-PRJ-06 — FAILED — defective (new, decisive: reproducible text-entry drop, positive control included)

Re-drove the real sidebar overlay (not a direct-mount harness), correctly targeting the popover's
actual sidebar-confined position (P123's finding, itself re-confirmed live: the "Clone repository"
card renders flush in the ~300px sidebar column, not window-centered). **Button clicks are not the
live blocker** — Cancel closed the popover on the first click every time. **Text entry into the URL
field is**: across 5 independent live attempts (varying settle from 0.3s to 2.0s before typing),
the full 44-character URL landed exactly zero times — results were `"h"`, `""`, `"ht"`, `""`,
`"https://githu"` (13/44 chars, the best case). A same-session positive control types
`FILTERCONTROLTEST` (18 chars) into the sidebar's own Projects filter field, using the identical
`wtype` mechanism, and it lands character-for-character every time — ruling out a lane/input-
injection artifact and pointing at the Clone form's own field (its `on_key_down` recomputes a
derived `Destination` path on every keystroke, a plausible source of dropped input under load).
Because the URL never lands, `CloneFormState::can_submit()`'s real, correct guard (empty url ->
no submit) keeps the button inert — so "type a URL, click Clone; status must leave Ready to clone"
fails every time, not from a blocked click but from a live, reproducible text-entry defect this
drive discriminates from a working control. Frames: `05-04-url-typed.png` ("h"),
`06-04-url-typed.png` ("ht"), `05-04-url-typed-longsleep.png` ("https://githu"),
`03-02-filter-control.png` (control: full 18 chars land).

## F-PRJ-09 — PASSED (new, decisive: real project folder created on disk)

Same overlay family, opposite result. Opened the real sidebar "Create Project…" popover, clicked
the Project name field, typed `mytestproject` (13 chars) with a 1.5s settle — it landed in full on
the first attempt, `Destination` line updated live to
`/home/enzopalmisano/mytestproject`. Clicked "Create project" (two clicks queued, need not have
been two); the popover closed, the sidebar gained a new `mytestproject` project row marked
`Primary`, and — checked directly with `ls`, not just the UI — `/home/enzopalmisano/mytestproject`
exists on disk as a real directory. This is the full click -> handler -> filesystem round trip
completing live. Test artifact removed after verification (`rm -rf`), not left in the user's home
directory. Frames: `05-04-name-typed.png`, `07-06-after-second-submit-click.png`.

## F-BRW-01 — FAILED — defective (new: reproduced live on the X11 lane, exact numbers match)

The builder could not reach a live X11 display this pass; I could. Opened a real Browser tab
(`New Browser` from the tab-bar `+` menu) pointed at `https://example.com`; real page content
rendered ("Example Domain" text, confirmed visually). Pixel-scanned two independent horizontal
rows with `convert ... txt:-`: a content-row scan (y=400) finds the rendered page spanning
x=331..1059; a chrome-row scan (y=850, address-bar-free) finds the pane's own background spanning
x=386..1236 (bordered by black divider pixels at 378-379 and 1236 exactly). That reproduces the
prior critic's exact numbers (`55px` sidebar bleed on the left, `177px` unpainted gap before the
Files panel on the right) on a freshly-opened tab today — not a stale/orphan frame. Confirms the
builder's own diagnosis (GPUI-vs-GTK/GDK scale mismatch, ~0.857x ≈ 1/1.1667) is still live and
unfixed; no patch was attempted (D-P1 owns `browser.rs`'s `native_webview_rect`, which the builder
already showed is a correct pass-through — the bug is on the wry/GTK side of `set_bounds`, exactly
as diagnosed). Frame: `x11-03-browser-open.png`; raw pixel scans recorded above.
