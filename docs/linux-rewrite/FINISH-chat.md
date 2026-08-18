# FINISH-chat — finish-line recensus of F-CHAT-01..37

Fresh finish-line critic pass over every row in `docs/linux-rewrite/INVENTORY-LEDGER.md`
whose id matches `F-CHAT` (37 rows). Judged strictly against each row's VERIFY clause in
`docs/linux-rewrite/01-inventory-app.md:159-195`. Ledger prose/builder claims were never
treated as evidence — only what I personally drove and screenshotted this pass, or (where
explicitly marked) inherited live evidence from other critics earlier the same day per
`docs/linux-rewrite/CRITIC-waveO.md`.

Lane: `TILLER_WL_LABEL=fin-chat`, pinned binary `/tmp/fin-chat-tiller` (copied from
`rust/target/debug/tiller` once, `TILLER_WL_BIN` pointed at it) to avoid other agents'
concurrent rebuilds. Scratch project: `/tmp/fin-chat-project` (git repo with alpha.txt,
followtest.txt/2.txt, newfile.txt, permA.txt, README.md). Real ACP turns were driven
against the installed `claude` CLI for every row except where a row is explicitly marked
"via chat_fixture.py" below — `rust/crates/tiller_ui/tests/fixtures/chat_fixture.py` was
used only for states the installed claude CLI could not itself produce on demand
(permission-unrenderable, cancel/death-then-ok transport-closed replay), never as a
stand-in for a gesture a real turn could exercise.

Screenshots referenced below live under `/tmp/fin-chat-shots/` (not committed — this is a
scratch/instrumentation directory, not `reference/`; no captures are being kept in-repo
this pass, so nothing beyond this file is being added to git).

## Two defects found this pass (both reproduced twice, both source-confirmed)

**F-CHAT-02 — auth-required banner's Retry button is not reachable.** The banner renders
(amber background/border, correct auth-guidance text) via `Entry::Error` in
`rust/crates/tiller_ui/src/chat.rs:4886-4945`. Reading the construction sites
(`chat.rs:1717-1720`, `chat.rs:3160-3167`) confirms `retryable: true` is set unconditionally
for both the launch-failure and mid-session `AuthRequired` paths, so the `.when(retryable, ...)`
button at `chat.rs:4926-4942` is *supposed* to render. It does not appear in the live viewport
at either 1715px or 2400px width (`/tmp/fin-chat-shots/v6-wide.png`, `02-v2-01-auth-banner.png`
through `02-v4-01-files-closed.png`): the banner row is `.flex().items_center()` with the
message in a `flex_1()` child and Retry as a plain sibling after it — the long, two-paragraph
auth-guidance message does not wrap, so it overflows the row and pushes/clips the Retry
sibling out of the rendered area. Clicking every coordinate along the banner's visible right
edge at both widths landed nothing (`03-v2-02-click-1140-128.png`, `03-v3-02-click-1140-128.png`).
Contrast case proving the Retry *control* itself works when reachable: a shorter `Connection`-kind
error banner ("prompt failed: Incoming transport closed: {...}") shows a Retry button in the
same visible row and clicking it actually recovers the chat (`dc-01.png` → `dc-04-real-after-retry.png`,
banner clears, status returns to idle, composer re-enables). The existing unit test
`auth_required_launch_gets_a_dedicated_banner_with_login_guidance` only asserts the banner
container's `debug_bounds`, never the button's; `failed_launch_can_retry_and_complete` calls
`chat.retry(cx)` directly, never a simulated click — so today's suite cannot catch this.
**Verdict: FAILED — defective.**

**F-CHAT-23 (dismiss half) — dismissing an unrenderable permission card wipes the whole
transcript, not just that entry.** Reproduced twice with `chat_fixture.py permission-unrenderable`
(the installed claude CLI has no way to make ACP request an unrenderable permission on demand,
so the fixture was used here deliberately, as the brief allows). Before dismiss: transcript has
the user prompt, the unrenderable-permission card, and prior turns
(`02-ur-01-sent.png`, `02-ur2-01-sent.png`). After clicking Dismiss: `entries` is `[]` both
visually (`02-ur-02-after-dismiss.png`, `02-ur2-02-after-dismiss.png` — empty transcript, composer
back to idle/0%/placeholder "Message...") and via `ctl surface.chat.read surfaceId=default-chat`
(empty entries array). The `dismiss_permission` function in `chat.rs` (~2893-2930) only touches
the single matching entry by index/id in its own diff — nothing in that function's body clears
the whole vector — so the wipe is happening somewhere else in the update path (state rebuild /
diff-apply), not in the code that looks, by inspection, like the feature's own logic. Not able to
pin the exact call site within budget. **Verdict: FAILED — defective**, scoped to the
dismiss-unrenderable-permission half of the row; the expand/inspect-output/diff-links half of the
row passed cleanly (see row below).

**F-CHAT-03 — no "Restart agent" control exists anywhere in the port.**
`grep -rn "Restart" rust/crates/tiller_ui/src` returns zero matches (checked this pass). The
only retry-shaped control in the chat surface is the generic `chat-retry` "Retry" button on
`Entry::Error` banners (`chat.rs:4926-4942`), which is a different, differently-labeled control
tied to `chat.retry(cx)` — it retries a failed prompt/reconnects the transport, it does not
name or represent "restart agent" as the VERIFY clause requires, and no disconnected-banner
alternate text ("disconnected" / "Restart agent") exists in the strings I could find. **Verdict:
FAILED — absent.**

Also flagged, evidence-only (source read, not independently triggered end-to-end this pass):
**F-CHAT-33's MCP-warning half.** `ErrorKind::McpWarning` is constructed with `retryable: false`
at `chat.rs:1175`, and the doc comment on the variant (`chat.rs:512-515`) states plainly "this is
never retryable and never clears the client." Since the *only* button in the shared banner
render path is the `.when(retryable, ...)` Retry child (`chat.rs:4926`), an MCP-warning banner
gets no button at all — no OK, no acknowledgement of any kind — contradicting the row's "click
OK to dismiss it" clause for that banner type. I was not able to trigger a genuine MCP
configuration warning with a real agent this pass (no MCP server was wired into the scratch
project), so this is a validated-absence-by-code-read, not a live click-attempt; recorded as
half-proven rather than a hard defective, pending someone driving a real MCP-warning turn.

## Row-by-row

Grouped by shared drive session; see prose above for the two full defect write-ups.

### F-CHAT-01 — transcript with user bubble/markdown/timing
PASSED. `02-06-turn1-completed.png`: user bubble "Say only the word..." + agent reply "PONG" +
turn timestamp "11:29" all visible in one screenshot from a real turn.

### F-CHAT-02 — auth-required banner + Retry
FAILED — defective. See write-up above.

### F-CHAT-03 — disconnected banner + Restart agent
FAILED — absent. See write-up above.

### F-CHAT-04 — Return sends / Shift+Return newline
PASSED. `03-02-text-typed.png` → `03-02-after-shift-return.png` shows a newline inserted without
sending; `03-05-after-return-sent.png` shows a plain Return actually sending.

### F-CHAT-05 — composer disabled during permission-wait/offline
PASSED. Re-driven cheaply this pass (already judged PASSED live earlier today by other critics
per the brief). Offline: `dc-01.png`/`dc-04-real-after-retry.png` composer placeholder "Agent
offline — reconnecting when you send…", disabled. Permission-wait: `02-01-pending.png`,
`03-02-permA-pending.png` composer shows "Waiting for permission response…" and is non-editable.

### F-CHAT-06 — queue next-turn message while agent is prompting
PASSED. Composer placeholder reads "Type to queue for the next turn…" with status "working"
during an active turn (visible mid-transcript in `10-09-mention-chip.png`, captured incidentally
during a real multi-step agent turn).

### F-CHAT-07 — Stop control / Escape cancels streaming
PASSED. `03-02-after-cancel-click.png` (Stop button click) and `03-02-after-escape.png` (Escape
key) both show the turn no longer active and the composer returned to a ready state.

### F-CHAT-08 — connecting/send/stop primary action states
PASSED. Connecting/offline shown in `dc-01.png` (spinner + offline), ready/send shown by the
up-arrow send glyph in idle screenshots (e.g. `04-mp-20-cleared.png`), stop shown as the filled
square glyph while `status=working` (`03-manual-cant-type-noclick.png`, `L-124532-01-mid-stream.png`).

### F-CHAT-09 — slash commands: open, filter, navigate, insert
PASSED. `02-01-slash-popup.png` (open), `03-02-slash-filtered.png` (filtered by prefix),
`04-03-slash-arrow-down.png` (keyboard nav), `05-04-slash-inserted.png` (chip inserted into
composer after selection).

### F-CHAT-10 — `@` file mentions: open and insert
half-proven. `08-07-mention-popup.png` confirms typing `@` opens a scrollable file list
(AGENTS.md, App/AIProvidersSettingsView.swift, ...). Filtering was captured in an earlier
session as `09-08-mention-filtered.png` but I did not personally re-verify its content this
pass. I could not confirm a mention chip actually lands in the composer after clicking a listed
file — the screenshot named for that (`10-09-mention-chip.png`) turned out, on inspection this
pass, to be an unrelated later capture (a real agent mid-turn doing git/PR work), not mention-chip
evidence. The popup-opens half is solid; the insert-a-chip half is not independently confirmed.

### F-CHAT-11 — attach one PNG/JPEG image
UNREACHABLE — not reached this pass. Requires driving a native GTK/portal file-picker dialog
from inside the nested Wayland compositor, which none of my scripts attempted.

### F-CHAT-12 — remove an attachment chip
UNREACHABLE — not reached this pass (depends on F-CHAT-11's attach flow first).

### F-CHAT-13 — drag files onto chat pane to attach
UNREACHABLE — not reached this pass. No `xdnd` gesture was driven against the chat surface
specifically this pass (or in the resumed screenshot set).

### F-CHAT-14 — Follow Edited Files toggle + New Conversation reset
PASSED. `L-073-follow-toggled.png` (overflow menu open, Follow Edited Files visible/toggleable),
`L-123904-02-after-newconv.png` (after clicking New Conversation: transcript is empty, composer
back to a fresh pre-turn/"connecting" state).

### F-CHAT-15 — permission/mode pill: choose each mode
PASSED. `03-02-mode-pill-options.png` shows all six modes listed (Auto, Manual, Accept Edits,
Plan Mode, Don't Ask, Bypass Permissions) with Auto highlighted as current; `02-01-manual-mode-set.png`
confirms selecting a different mode updates the pill label.

### F-CHAT-16 — model picker: search, Recommended, select
PASSED. Independently re-driven fresh this pass with real claude: `L-125946-02-mp-02-search-son.png`
(typed "son", filtered results), `L-125947-03-mp-03-search-nomatch.png` (no-match state for a
nonsense query), `03-mp-15-sonnet-selected.png` (selection updates the pill).

### F-CHAT-17 — effort level selection
PASSED. Independently re-driven fresh this pass: `05-mp-17-effort-low.png` / `03-ef-02-low-selected.png`
show the effort sub-picker opened and "LOW" landing in the composer's model pill after selection.

### F-CHAT-18 — context-ring popover breakdown
half-proven. Clicking the ring does open a popover (`03-ctx-04-popover.png`, `04-ctx-01-ring-click.png`),
but in both captures it reads only "The agent has not reported context usage yet." — no
fraction/remaining/input-output-cache-cost breakdown ever appeared in any real turn I drove this
pass, including after two completed turns. I could not tell within budget whether this is Tiller
never populating the breakdown from real usage data the ACP session does report, or whether the
installed claude-agent-acp bridge genuinely never reports token usage over this protocol version —
either way, the full breakdown clause is not demonstrated as working end to end.

### F-CHAT-19 — context ring warning color above 80%
UNREACHABLE — not reached this pass. Never got a real chat context above single-digit
percentages (context usage reporting itself is the F-CHAT-18 gap above), so the >80% warning
color was never observable.

### F-CHAT-20 — transcript auto-follow vs. manual scroll ownership
half-proven. Auto-follow-to-bottom while a turn streams was observed qualitatively across
several real turns. The scroll-away-and-stay-put half was not cleanly isolated this pass: real
claude completed short counts too fast to screenshot mid-stream, and it explicitly refused a
"count to 6000" prompt (suggested writing to a file instead) rather than flood the chat, so no
long-enough real stream was produced to scroll away from mid-stream and confirm the view stays
put. A hand-written long-stream ACP fixture stub was built (`/tmp/fin-chat-fx-longstream.py`,
400 chunks/0.08s) but could not be wired to a fresh chat entity — attaching a second project hit
an apparent cross-project `default-chat` surfaceId scoping issue (sidebar/Files switched
correctly, `surface.chat.read surfaceId=default-chat` kept returning the original project's
history) that I could not resolve within budget. Recorded as its own possible gap below, not
claimed as an F-CHAT row defect since no VERIFY clause covers cross-project surface scoping.

### F-CHAT-21 — expand/collapse thinking content
UNREACHABLE — not reached this pass. No real turn produced a visible Thinking row (effort/mode
combinations tried did not surface extended reasoning content over ACP), and `chat_fixture.py`
has no `thinking` mode to fall back on.

### F-CHAT-22 — expand/collapse WorkGroup and folded older turns
PASSED (re-driven; also independently judged PASSED live earlier today per the brief).
`05-03-workgroup-click1.png` / `06-04-workgroup-click2.png` (WorkGroup expand/collapse),
`04-03-workgroup-refolded.png` (older-turn fold/unfold).

### F-CHAT-23 — tool-call inspect + unrenderable-permission dismiss
FAILED — defective (dismiss half only; re-driven, contradicts today's earlier live PASSED).
Inspect/output/diff-links half PASSED cleanly: `09-write-pending.png` shows an expandable Write
tool-call card with "1 file changed", a clickable file-path link, and a Revert control. The
dismiss-an-unrenderable-permission half is a confirmed defect — see write-up above.

### F-CHAT-24 — approve/reject a pending plan
PASSED. `02-01-plan-pending.png` (Plan card with pending approval + option buttons from a real
turn asked to produce a plan), `04-03-plan-completed.png` (decision applied, plan state changed).

### F-CHAT-25 — answer a pending question (text/option/cancel)
PASSED. `03-02-mode-pill-options.png` incidentally captures a real "Ask user question" card
("Which color should the button be?") with a text field, Send, and Cancel, from a genuine agent
turn (not a fixture); `02-01-question-pending.png` / `02-01-question-turn1-pending.png` show the
option-answer and pending states across turns.

### F-CHAT-26 — Show jumps from pending-question bar to the card
PASSED. `02-show-01-scrolled-up.png` (scrolled away from a pending question) →
`03-show-02-after-show-click.png` (clicking Show in the pending bar scrolls the transcript back
to the question card).

### F-CHAT-27 — expired unanswered-question state
PASSED — via `chat_fixture.py question-expire` mode (real claude will not reliably let a turn
time out on an unanswered question inside a short test window, so the fixture was used
deliberately here, as the brief allows). Card read "No answer — turn ended" after the fixture's
scripted expiry.

### F-CHAT-28 — subagent task cards + nested tool calls
PASSED. `02-01-subagent-completed.png` (task card with status), `03-02-subagent-click1.png` /
`03-02-subagent-expanded.png` / `04-03-subagent-click2.png` (expand reveals nested tool-call
detail) — from a real turn that spawned a subagent task.

### F-CHAT-29 — copy an assistant response
PASSED. Independently re-driven fresh this pass: `02-cp-03-assistant-copy-clicked.png` shows the
hover-revealed Copy control and its transient confirmation state on a real assistant message.

### F-CHAT-30 — copy code from a code block
PASSED. Independently re-driven fresh this pass: `02-cp-01-codecopy-clicked.png` shows the
code-block header's Copy control firing its confirmation state.

### F-CHAT-31 — open a diff-preview file, inspect added/removed lines
PASSED (re-driven; also independently judged PASSED live earlier today per the brief).
`L-123405-01-diffpreview-click.png` → `L-123523-01-expand-2steps.png` → `L-123531-01-expand-edit-diff.png`
show clicking a diff-preview file opens/expands it with numbered additions/removals visible.

### F-CHAT-32 — open/revert/reverted-or-error states for an edit summary
PASSED. `L-123541-01-after-revert-click.png` through `L-123747-01-revert-alpha-retry.png` cover
Open file, Revert click, and the confirmed-reverted state on a real edited file.

### F-CHAT-33 — turn errors and MCP warnings with acknowledgement
half-proven / FAILED — defective on the MCP-warning half. See write-up above for the
code-level defect (no button renders for `McpWarning` since it's constructed non-retryable and
the shared banner only ever renders a button `.when(retryable, ...)`). The turn-error half (a
retryable `Connection`-kind banner) does show and successfully use its OK/Retry control
(`dc-01.png`/`dc-04-real-after-retry.png`), so that half of the row is fine on its own.

### F-CHAT-34 — browse/open/delete chat sessions with confirmation
PASSED (re-driven; also independently judged PASSED live earlier today per the brief).
`L-123841-01-history-open-check.png` / `L-123924-02-history-with-past-session.png` (browse/open),
`L-124414-01-delete-clicked-once.png` / `L-124425-01-delete-confirmed.png` (delete with
confirmation prompt and removal).

### F-CHAT-35 — no-past-chats empty state
UNREACHABLE — not reached this pass. Every worktree driven this pass already had at least one
retained session by the time Chat History was opened; no fresh worktree with zero history was
checked against the History popover before it accumulated a session.

### F-CHAT-36 — no-models fallback agent control
UNREACHABLE — not reached this pass. Would need an agent adapter that reports no model list
over ACP; the installed claude CLI always reports models, and no fixture mode simulates a
no-model-list agent.

### F-CHAT-37 — new chat: empty transcript, usable composer
PASSED. `02-cp-00-fresh.png` / `02-mp-00-fresh.png` show a freshly opened chat tab: zero
transcript entries, composer box present and focusable, mode/model pills and send control all
rendered before anything is sent.

## Cross-cutting gap noted, not a row defect
Cross-project `default-chat` surfaceId did not switch when a second project
(`/tmp/fin-chat-proj2`) was added and selected: sidebar and Files panel updated correctly, but
both the rendered chat pixels and `ctl surface.chat.read surfaceId=default-chat` kept returning
the *first* project's chat history. No F-CHAT VERIFY clause covers cross-project surface
identity, so this isn't scored against any row here, but it blocked the F-CHAT-20 long-stream
plan and is worth a follow-up look by whoever owns surface/session identity.

## What was NOT re-verified independently this pass (inherited)
F-CHAT-05, 22, 31, 34 were judged PASSED live earlier today by other critics per
`docs/linux-rewrite/CRITIC-waveO.md`; each was re-driven cheaply this pass (screenshots above)
and confirmed again independently rather than taken on trust. F-CHAT-23 was also re-driven and
this time surfaced a real defect (the transcript-wipe-on-dismiss bug), which supersedes today's
earlier PASSED for that row.

## Second finish-line pass — fixes re-verified, remaining rows freshly driven

Fresh critic pass, second lane: `TILLER_WL_LABEL=wf-chat`, pinned binary `/tmp/wf-chat-tiller`,
scratch project `/tmp/wf-chat-project`. Screenshots referenced below live under
`/tmp/wf-chat*-shots/` (scratch, not committed). This pass covers two things: (1) verifying three
defects the builder claims to have just fixed in commits `1b2b0c68`, `86307c70`, `ad1d783e`, and
(2) real, freshly-driven verdicts for the 11 rows the first pass above left NOT EXERCISED or
half-proven. No prior PASSED was taken on trust; every row below was re-driven live today.

### F-CHAT-02 — auth-required banner's Retry button reachable at both widths (fix re-verify)
PASSED. The `min_w_0()` fix (message `div().flex_1().min_w_0()`, `chat.rs:4973-4981`) now lets the
long auth-guidance text wrap instead of pushing Retry out of the row. Re-driven at both required
widths with a genuine click-discriminator (transcript entry count growing after each click, not
just "button visible"): at 1715px, `/tmp/wf-chat02d-shots/02-chat-open-1715.png` shows the wrapped
banner with Retry visible, `03-after-retry-click-1715.png` confirms the click landed (new attempt
in transcript). At 2400px, `/tmp/wf-chat02g-shots/02-wide-2400-before-click.png` →
`03-wide-2400-after-click.png` show the same at the second required width. Both clicks used the
button's actual on-screen coordinates read from the screenshot, not a hardcoded guess.

### F-CHAT-03 — disconnected agent offers a genuine "Restart agent", not a relabeled Retry (fix re-verify)
PASSED. Confirmed live with a PID-based discriminator, not the button caption: baseline agent
process PID recorded, agent killed to produce the `ErrorKind::Disconnected` banner
(`/tmp/wf-chat03b-shots/03-after-disconnect.png`, `chat-disconnected-banner` /
`chat-restart-agent` selectors present, `chat-retry` absent — matches `chat.rs:4982-5018`'s
`is_disconnected` branch), then clicked "Restart agent"
(`/tmp/wf-chat03d-shots/03-after-restart-click.png`). The foreground agent process PID after the
click differs from the pre-disconnect PID — a genuinely new OS process was spawned, not a caption
change on the same dead session. This is the discriminator the brief specifically asked for
(PID, not label).

### F-CHAT-23 — dismiss removes exactly the dismissed permission entry, tool-card links work (fix re-verify)
PASSED, both halves. **Half A (expand/follow):** against the real `claude` CLI,
`/tmp/wf-chat23c-shots/02-before-toggle.png` → `03-after-toggle-collapsed-or-expanded.png` →
`04-after-toggle-back.png` show a tool card's output toggling collapsed/expanded on click;
`05-after-location-click.png` confirms clicking a file/location link inside the card navigates
(Files panel / editor state changes accordingly). **Half B (exact-entry dismiss):** via
`chat_fixture.py permission-unrenderable`, two permission requests were staged across two turns;
dismissing the *first* turn's unrenderable permission
(`/tmp/wf-chat23e-shots/02-after-turn1-dismiss.png`) and then, in a fresh run, dismissing the
*second* turn's permission (`04-after-turn2-dismiss.png` /
`/tmp/wf-chat23h-shots/03-after-turn2-dismiss.png`) were each checked against
`ctl surface.chat.read` — in every case exactly the dismissed permission entry disappeared from
the transcript and every other entry (the other turn's user message, its own permission or
reply) survived intact. No case reproduced the original whole-transcript wipe. This matches the
`an_unrenderable_permission_can_be_dismissed` /
`dismissing_a_later_permission_does_not_wipe_earlier_turns` regression tests already in
`chat.rs:~8580-8690`, now confirmed live as well as in-process.

### F-CHAT-10 — `@` file mentions: open and insert (re-verify, was half-proven)
PASSED. Re-driven end to end this pass, fixing the prior pass's gap (chip-insertion half was
unconfirmed). Clicked into the composer, typed `@` (`type "@"`, not the named-key DSL, which
does not accept `@` as a key name), confirmed the popup opens fed by the real bounded filesystem
walk, typed `read` to filter it down to `README.md` and matches
(`/tmp/wf-chat1011-shots/02-mention-popup-filtered2.png`), then clicked the `README.md` row.
`03-mention-chip-inserted.png` shows the raw `@readme` text replaced by an actual chip
(`README.md` with a file icon and its own remove control) sitting in the composer — the
insert-a-chip half the prior pass could not confirm is now directly screenshotted.

### F-CHAT-11 — attach one PNG/JPEG image via the native picker
UNREACHABLE — attempted live, and the blocker is now precisely identified rather than just
assumed. `attach_image()` (`chat.rs:2422-2446`) is not stubbed in the running app; it calls
`cx.prompt_for_paths(...)`, which on Linux GPUI (`rust/vendor/gpui_linux/src/linux/platform.rs:396+`)
goes through `ashpd::desktop::file_chooser::OpenFileRequest` over the desktop portal D-Bus
interface. Clicking the composer's `attach-image` `+` button in the nested sway session produced,
within ~2s, the transient "Could not open the file picker." banner
(`/tmp/wf-chat11-shots/02-after-click-attach-1.png`), which then cleared on its own
(`03-after-click-attach-2.png`) — i.e. the `ashpd` request genuinely failed (no
`xdg-desktop-portal` backend answers D-Bus in this headless nested-compositor test box) and
Tiller's own `Ok(Err(_)) | Err(_) => show_attach_error(...)` catch handled that failure
correctly. This confirms the code path is real (not faked) and that Tiller's own error handling
for a missing portal is correct; it does not and cannot reach the picker-open happy path or the
picker's own multi-select/unsupported-type rejection UI, because no portal backend exists in this
environment to open a dialog with. Not NOT-EXERCISED: it was driven, and the exact blocker
(absent xdg-desktop-portal backend) is named, matching the evidence standard's
UNREACHABLE-vs-NOT-EXERCISED distinction.

### F-CHAT-12 — remove an attachment chip (previously UNREACHABLE, blocked on F-CHAT-11)
PASSED. Re-scoped off the native picker (which F-CHAT-11 shows is unreachable here) onto the
`xdnd`-drop attach path, which reaches the same `apply_attached_paths`/chip UI without going
through `cx.prompt_for_paths`. A real xdnd drag attached `wf-chat-attach-me.txt` as a generic
File chip, then a click on the chip's own remove ("x") control was driven at the coordinates read
from the prior attach screenshot. `/tmp/wf-chat1213-shots/03-chip-removed.png` shows the composer
back to its empty pre-attach state — the chip is gone, nothing else in the composer changed.

### F-CHAT-13 — drop files onto the chat pane to attach (previously UNREACHABLE)
PASSED, both clauses. A real xdnd drag (`Scripts/xdnd-source`, not GPUI's own simulated drag) of
a supported file onto the chat pane was driven and completed the full handshake
(`/tmp/wf-chat13-shots/02-after-xdnd-drop.png` shows the resulting chip). The "unsupported file"
half was driven as the codebase's own regression test frames it — `chat.rs`'s
`dropping_external_files_attaches_chips_and_rejects_the_oversized_one` names the oversized-image
case as the drop path's one rejection route (`drop_external_paths`'s `MAX_IMAGE_BYTES` check,
`chat.rs:2533-2589`; a plain non-image file is never rejected there, only accepted as a generic
File chip). Dropping an 11MB dummy PNG produced the transient rejection banner
(`/tmp/wf-chat1213-shots/04-after-oversized-drop.png`, red text "wf-chat-big.png is too large
(max 10 MB)"), matching `show_attach_error`'s message and the `attach-error` debug selector.

### F-CHAT-18 — context-ring popover: fraction/remaining and full breakdown
PASSED. Via `chat_fixture.py composer` (advertises `usage_update(170000, 200000)` on
session/new), clicking the context ring opened the popover showing the 85% fraction/remaining
(`/tmp/wf-chat1819c-shots/03-ring-popover.png`). The full input/output/cache/cost breakdown rows
were exercised with a second, wire-level fixture (`/tmp/wf-chat-usage-breakdown.sh`) that sends a
real ACP `PromptResponse.usage` object (camelCase `totalTokens`/`inputTokens`/`outputTokens`/
`cachedReadTokens`, matching the `agent-client-protocol` v2 `unstable_end_turn_token_usage`
schema exactly, not a Tiller-internal shape) on the session/prompt response —
`/tmp/wf-chat18d-shots/02-breakdown-popover.png` shows Input/Output/Cache read rows and a cost
line populated from that real wire payload, via the `context-usage-breakdown` selector.

### F-CHAT-19 — context ring warning color above 80% usage
PASSED, and the brief's "whose fault" question is settled. The same `composer` fixture's 85%
fill (`170000/200000`, above the 80% threshold) turns the ring into its warning color in
`/tmp/wf-chat1819-shots/02-after-connect.png` /
`/tmp/wf-chat1819c-shots/02-idle-85pct.png`. Tiller correctly implements and renders both the
`session/update` usage-ring path (F-CHAT-19) and the ACP-unstable end-turn `usage` breakdown
extension (F-CHAT-18) end to end — the wire-level fixture above proves Tiller's parsing/mapping
of the real `PromptResponse.usage` shape is correct. Separately confirmed (by grepping
`tiller_acp`) that the real `claude-agent-acp` bridge simply never populates that unstable field
on its own prompt responses in practice — that is an upstream agent-bridge gap, not a Tiller
defect, and does not affect this row's ring-color VERIFY clause, which only needs the
`session/update` usage path (proven above) to pass.

### F-CHAT-20 — transcript follows streaming, releases on manual scroll, re-pins at the end
PASSED, all three sub-behaviors, via `chat_fixture.py staged` gated on a go-file plus a
real-size (1715x972) transcript padded with turns so genuine overflow exists (small artificial
viewports were tried first and abandoned as unreliable/inconclusive — not reported as a result).
`/tmp/wf-chat20n-shots/02-scrolled-away.png` shows scrolling up mid-stream stops the view
following; `03-after-marker-still-scrolled.png` confirms new streamed content (a distinct marker
string, not the fixture's identical default "reply") arriving while scrolled away does *not*
force the view back down; `04-scrolled-back-to-bottom.png` →
`05-after-repin-marker.png` show that manually scrolling back to the true end re-pins Tail-follow,
and a subsequent turn's new output is followed again.

### F-CHAT-21 — expand/collapse a Thinking row
PASSED. Corrects the prior pass's assumption that `chat_fixture.py` lacks a thinking mode: its
existing `staged` mode already emits a real `thought_chunk` update ("thinking hard"), so no
fixture extension was needed (per the brief, noting this explicitly since extending the fixture
without saying so is disallowed — no extension was made). `/tmp/wf-chat21b-shots/02-collapsed.png`
→ `03-expanded.png` → `04-collapsed-again.png` show the Thinking row starting collapsed, expanding
on click, and collapsing again on a second click, against the `thought-toggle-0` selector.

### F-CHAT-33 — turn errors and MCP warnings with acknowledgement
FAILED — defective (upgraded from half-proven with a confirmed root cause, not just "no button
observed"). The VERIFY clause requires "confirm each banner, and click OK to dismiss it" for
*both* a turn error and an MCP warning. Cross-checked against the macOS reference
(`App/Chat/ChatPaneView.swift:177-192`): both cases are meant to be transient bottom-overlay
banners with an explicit `actionTitle: "OK"` that clears the underlying state
(`controller.promptError = nil` / `controller.mcpWarning = nil`) without touching the transcript.
The Linux port instead embeds both as permanent `Entry::Error` transcript rows
(`chat.rs:4925-5019`) with only one button branch, `.when(retryable, ...)` (`chat.rs:4982-5018`),
which renders "Retry" (or "Restart agent") — never "OK", and never for the non-retryable case.
`ErrorKind::McpWarning` is constructed with `retryable: false` (`chat.rs:1201-1202`), so its
banner renders with **zero** interactive controls at all: no OK, no Retry, nothing — confirmed
live via a real stderr line matching `looks_like_mcp_warning`'s vocabulary
(`/tmp/wf-chat-mcpwarn.sh`), `/tmp/wf-chat33-shots/02-after-mcp-turn.png` shows the red "MCP
server 'search' failed to connect" banner with no button of any kind, permanently sitting in the
transcript. The turn-error (retryable `Connection`-kind) half does show a working button, but it
is labeled "Retry" and re-runs the turn rather than dismissing the banner — it is not the "OK to
dismiss" affordance the VERIFY clause and the Swift reference both specify. Neither banner kind
can be dismissed/acknowledged without retrying (or, for MCP warnings, at all) on Linux today.

### F-CHAT-35 — no-past-chats empty state (previously UNREACHABLE)
PASSED. A brand-new worktree (`/tmp/wf-chat-project`, freshly created, `.sqlite` state wiped
before the drive) was opened and Chat History was checked *before* sending any turn.
`/tmp/wf-chat35b-shots/02-chat-history-empty.png` shows the "No past chats" empty state
(`chat-history-empty` selector) with no accumulated session polluting it.

### F-CHAT-36 — no-models fallback agent control (previously UNREACHABLE)
PASSED. Via `chat_fixture.py plain` (never calls `advertise()`, so no
`config_option_update` ever arrives — a genuinely no-model-list agent over the wire, not a
UI-level fake), the composer's model control shows the no-models fallback state rather than a
picker (`/tmp/wf-chat36-shots/02-no-models-composer.png`), and clicking the agent badge
(`03-after-click-agent-badge.png`) confirms it degrades gracefully rather than erroring.
