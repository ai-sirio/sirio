# E03-chat evidence — drive-E03-chat (Wayland lane)

Worktree: tiller-linux, branch linux/gpui-waku, HEAD 4073297. Lane: `Scripts/wayland-drive.sh`
with `TILLER_WL_LABEL=drive-E03-chat`. Captures under
`reference/linux-progress/drive-E03-chat/`.

## F-CHAT-05 (ledger line 154, half-proven going in)

Offline half already proven by pass 17 (composer inert, no placeholder). Drove the
**permission-wait half** — the piece that was unexercised.

Setup: `project.add`, `surface.chat.open`, `tab.select index=1`, clicked the composer, sent a
real prompt to a live Claude Code ACP agent ("Opus Plan Mode", mode pill "Ask") asking it to
run `pwd` via the Bash tool — completed with no permission gate shown at all (screenshot
`02-f05-mid.png`: tool card reads `Execute pwd … Completed`, composer re-enabled with plain
"Message…" placeholder immediately after).

Escalated to a file-write prompt ("Use your Write tool to create /tmp/f05_perm_probe.txt…").
This produced a tool card with status **`Pending`** and a red `Revert` affordance
(`02-f05-write-mid.png`) — but this is not the ACP `permission_request`/option-button UI the
code implements (`respond_permission`, `permission-option-*` in `chat.rs`); no Allow/Deny
buttons ever appeared. While that card showed `Pending` and the top pill read `working`, the
composer placeholder was **"Type to queue for the next turn…"** — i.e. still accepting input,
not disabled — same as the ordinary mid-stream busy state.

**Claim: could-not-reach the permission-option UI live.** The agent installed in this
environment (Claude Code CLI via ACP) auto-applies file edits with a revert affordance rather
than raising the ACP `permission_request` this code path renders; no combination of prompts
tried in this pass produced the `permission-option-allow`/`permission-option-deny` card. What
*was* observed and is discriminating: the composer never disables during any live busy state
reached (streaming, tool-pending-with-revert) — it always stays typeable and switches its
placeholder to "Type to queue for the next turn…", queuing rather than rejecting. This does
not confirm or refute the row's specific "cannot send while a permission is pending" clause,
since a true permission-pending state was never reached. Reason it could not be forced: no
control was found (mode selector shows "Ask" already) to make this particular agent build stop
and request an explicit permission option card instead of auto-editing-with-revert.

Captures: `reference/linux-progress/drive-E03-chat/02-f05-connecting-send.png`,
`02-f05-mid.png`, `02-f05-write-mid.png`.

## F-CHAT-20 (ledger line 169, half-proven going in)

Follow half already proven live (pass 17). Attempted the missing half: manual-scroll-ownership
(scroll away mid-stream stops follow until re-pinned).

Reproduced a long stream live: sent "Write a numbered list from 1 to 80…" to the same live
Claude Code agent and captured mid-stream (`02-f20-mid-stream.png`, transcript visibly filling
with numbered lines, viewport pinned to the growing tail — consistent with the already-proven
follow half).

**Claim: could-not-reach the manual-scroll half on this lane.** The follow implementation is
GPUI `ListState`/`FollowMode::Tail`, which only leaves tail-follow on a real scroll-wheel/axis
event. `Scripts/wayland-drive.sh`'s DSL exposes only `click`/`move`/`type`/`key` — its virtual
pointer (`Scripts/wayland-virtual-pointer.c`) implements `motion_absolute`/`button`/`frame` only,
no `axis`/scroll event, confirmed by reading the source (not run — no compiling, per lane
rules). `key`-based Page Up/Down goes through `wtype` as keyboard input, not a wheel event, and
does not exercise the same code path the row's clause describes ("scroll up mid-stream"). No
gesture available on this lane reaches the discriminating input; this half needs the
`DISPLAY=:1` lane's real pointer-wheel support or a scroll-capable virtual pointer, neither of
which this slice may invoke/build.

Capture: `reference/linux-progress/drive-E03-chat/02-f20-mid-stream.png`.

## F-CHAT-13 (ledger line 162, NOT EXERCISED going in)

Prior verdict: instrument-unreachable on the X lane (no XDND source window; the "+" attach
control opens an invisible-to-X portal picker).

Tried the "+" attach control on the Wayland lane instead, since this lane's `click` is a real
synthetic left-click (unlike the X lane's xdotool). Set up project + chat as usual, located the
"+" control precisely (confirmed by a capture with the cursor sitting on it,
`02-f13-plus-click4.png`), then clicked it. No popup, file picker, or portal dialog appeared in
the next forced-repaint capture in any of four attempts (`02-f13-plus-click.png` through
`...click4.png`) — composer stayed "idle", unfocused, same as before the click.

**Claim: could-not-reach.** Two candidate explanations, neither provable without more
instrumentation: (a) `shot`'s forced repaint requires an output-resolution change, which may
itself dismiss an open popup as an outside interaction before the capture lands — this lane's
only way to force a fresh frame is also destructive to transient popups; (b) this headless sway
session runs no `xdg-desktop-portal`, so a portal-backed file picker may legitimately produce
nothing to capture, matching what the X-lane note already predicted. Drag-and-drop itself
remains separately unreachable here too: `Scripts/wayland-virtual-pointer.c` implements only
`motion_absolute`/`button`/`frame`, no `wl_data_device`/drag-offer protocol, so a synthetic XDND
or Wayland DnD drag cannot be issued from this lane's tooling without extending it — out of
scope for this pass (no compiling). Same bottom line as the prior pass: a human can reach this
control, neither current harness lane can drive it to a photographable result.

Captures: `reference/linux-progress/drive-E03-chat/02-f13-plus-click.png` … `04-f13-plus-click4.png`.

## F-CHAT-24 (ledger line 173, NOT EXERCISED going in — code+tests existed, never driven live)

Drove it live end to end with a real Claude Code ACP agent, entirely inside one
`wayland-drive.sh` invocation (state does not survive between invocations — each restarts the
compositor and the agent subprocess, losing ACP session context; this took two attempts to
learn).

Prompted the agent to enter plan mode, write a 3-step plan, then call its exit-plan-mode tool.
After ~16s: `02-f24-a-plan.png` / `chat.read` show the transcript growing `ToolSearch` →
`EnterPlanMode` → `Write .../plans/....md` → a `Pending` "Ready to code?" tool paired with a
`kind:"permission"` entry — and the rendered frame shows exactly the code path this row
describes: a **Plan card** ("2 steps", both completed) with a **"Ready to code?" approval
attached to the bottom of that same card** (not a separate card) offering four options ("Yes,
and bypass permissions" / "…auto mode" / "…auto-accept edits" / "…manually approve edits") plus
a pending-question bar below reading "Question waiting · Ready to code?".

Clicked **"Yes, and manually approve edits"** (`click 924 420`). `03-f24-b-resolved.png` and the
paired `chat.read` show the resolution live: the option buttons are gone, replaced by
**"Answered: Yes, and auto-accept edits"** directly on the Plan card; `permission id:"1"` flipped
from `pending` to `selected`; the assistant then said "Plan approved. Executing it now." and
**the plan advanced** — a new `Write /tmp/f24_probe.txt` tool card appeared with its own fresh
pending permission (id `"2"`, Allow Once / Always Allow / Deny), i.e. the turn genuinely
continued past the approval rather than stalling.

**Claim: exercised-working.** Both mechanics the row's test name promises were observed live,
not just in the unit test: the approval renders attached to the Plan card (not a standalone
permission card), and clicking an option both resolves that card in place and lets the plan's
next step run.

Captures: `reference/linux-progress/drive-E03-chat/02-f24-a-plan.png`,
`03-f24-b-resolved.png`.
