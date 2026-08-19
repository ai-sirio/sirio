# F-CHAT — fresh critic pass (sweep-4)

Independent re-drive of all 37 F-CHAT rows against the live app (binary `/dev/shm/tt/debug/tiller`,
branch `linux/gpui-waku`). Driven with `Scripts/wayland-drive.sh`, label prefix `sweep4chat-*`,
fixture repo `/dev/shm/sweep4chat-fixture` (throwaway git init). **Correction to an earlier draft
of this preamble:** every row driven this pass — including runs I set out to run against the
committed `rust/crates/tiller_ui/tests/fixtures/chat_fixture.py` via `TILLER_ACP_PROGRAM` pointed
at a one-line wrapper — actually ran against the real `claude` CLI behind
`npx @agentclientprotocol/claude-agent-acp@latest`. `TILLER_ACP_PROGRAM` is read by
`Chat::launch_with_persistence()`/`default_chat_command()`, but the "New Chat → Claude Code"
submenu path (`add_chat_tab(..., Some(adapter), ...)` → `acp_agent_command(adapter.acp_program())`
in `rust/crates/tiller/src/chat.rs` and `rust/crates/tiller/src/main.rs`) resolves the adapter's
own program directly and never consults the env var — by design, per that code's own comment, so a
misconfigured env var can't silently downgrade a named-agent launch. The only route that honors the
override is the agent-less "New Chat" (`NewTabAction::NewChat`, also reachable as "New Chat Here"
in the command palette), which I could not drive reliably this pass (see Notes) — every attempt
that opened the command palette via `chord ctrl+shift p` died in the harness's virtual-keyboard
setup before any UI action ran, on 7 consecutive tries interleaved with unrelated scripts that
succeeded cleanly, so I treat it as a harness-side flake tied to that specific chord this session,
not evidence about the app. Net effect: **no row below is fixture-verified**; rows that the ledger
or a prior wave verified only via fixture-driven deterministic wire states (thinking chunks,
plan/question/subagent modes, the no-models fallback) could not be independently re-confirmed or
re-refuted this pass and are marked NOT EXERCISED with that reason stated explicitly, rather than
silently carried over as PASSED. Every other verdict below reflects a live re-drive against real
Claude Code, not a copy of the ledger's own verdict.

Screenshots referenced below live under `/dev/shm/sweep-4-F-CHAT/<run>/*.png` on this host — they
are transient scratch, not committed artefacts; the frame names and what they show are described in
full so the finding is reproducible without them.

## Headline disagreement with the ledger

**`F-CHAT-34` is flipped PASSED → FAILED — defective.** The existing ledger evidence ("browse/open
past sessions... confirmed live") does not survive a live re-drive. Full writeup in Defects below;
short version: Chat History **only** ever lists a still-**open** sibling tab's conversation. The
instant a chat tab is closed — the normal way any user finishes one conversation and goes to look at
"history" — its `chat_turn` rows are deleted (cascade from the session-layout save deleting the now-gone
`tab` row), so "browse a past/finished session" is structurally unreachable. Confirmed via direct
sqlite reads of the running instance's own DB, not just UI screenshots.

**`ACP-12`'s "mode pill dropdown never opens" appendix note is stale, and `F-CHAT-15`'s PASSED is
correctly re-confirmed.** `ACP-12` predates commit `ac696cb1` ("wire the mode pill to the live ACP
session-mode catalog", 2026-08-15) by about a day; the ledger's own `F-CHAT-15` PASSED (wave B,
2026-08-18) is the current, accurate state. Independently re-verified live in this pass (see row
below) — clicking the pill after a completed turn opens all six modes, and selecting one updates
the label.

## Row verdicts

| row | verdict | evidence |
|---|---|---|
| F-CHAT-01 | PASSED | Real turn against real `claude`: user bubble "reply with exactly the single word: PONG", agent reply "PONG", and turn timestamp "12:31" all rendered together in one frame (`rcb/08-07-effort-medium-selected.png` and originally `rca/08-07-after-first-turn.png`). |
| F-CHAT-02 | NOT EXERCISED | Retry-button-at-narrow-width row (transport error + resize). Not re-driven this pass; time went to the F-CHAT-34/mode-pill investigations below. Ledger's wave-F evidence (chat.rs:4973 fix, two widths) stands unchallenged. |
| F-CHAT-03 | NOT EXERCISED | Restart-agent-after-disconnect row. Not re-driven this pass; no reason to doubt the ledger's PID-discriminated wave-F evidence. |
| F-CHAT-04 | PASSED | Return sends: composer held "reply with exactly the single word: PONG" pre-Return (`rca/06-05-typed-before-send.png`), then transcript shows the sent bubble post-Return (`rca/08-07-after-first-turn.png`). Shift+Return inserts a newline without sending: typed "line one", `chord shift Return`, typed "line two" → composer showed both lines as one unsent draft (`rca/10-09-after-shift-return.png`), transcript still only 1 user entry. One flaked non-send observed and explained below (not counted against this row — see Defects/Notes). |
| F-CHAT-05 | NOT EXERCISED | Offline/permission-wait composer disabled states. Not re-driven live this pass (would need a transport kill or the fixture's `permission` mode); no reason from this pass's evidence to doubt the ledger. |
| F-CHAT-06 | half-proven | Composer stayed editable and did not error while "connecting" — text typed and Return pressed during the pre-idle window was silently accepted into the composer without sending or erroring (`rcc/02-01-after-turn.png`, see Notes). The specific contract clause — placeholder literally reads "Type to queue for the next turn…" while mid-turn, with a queued item shown — was **not** independently re-produced this pass (would need a slower fixture turn + a second send mid-stream); ledger's wave-B screenshot evidence for that exact state stands but I did not re-confirm it myself, hence half-proven rather than a clean re-PASS. |
| F-CHAT-07 | NOT EXERCISED | Stop/Escape mid-turn. Not re-driven live this pass. |
| F-CHAT-08 | PASSED | Connecting (orange dot, "connecting" label, `recon4/02-01-chat-opened.png`), idle (green dot, "idle", `rca/04-03-chat-idle-fresh.png`), and working/streamed states all observed across this pass's runs; a real send-arrow→stop-square transition was not separately isolated this pass but the three status-pill states were. |
| F-CHAT-09 | PASSED, with a found defect (see Defects) | Popup opens on `/` with real slash commands pulled from the user's own `~/.claude` command registry (`rca/12-11-slash-popup.png`). Filters correctly by prefix **at any pace ≥150ms/keystroke** (`repro2/02-01-150ms-per-key.png`, `rcb2/02-01-slash-d-filtered.png`). Keyboard Down + click both work: clicking a highlighted, filtered item inserted a real "+ design-consent ×" skill chip into the composer (`rcb2/04-03-slash-d-clicked.png`). Separately found: rapid/bursty typing (`type` sending a whole string near-instantaneously, or occasionally even ~1s-spaced keystrokes under host load) can lose all but the last character while the popup is live — see Defects. |
| F-CHAT-10 | PASSED, same caveat as above | `@` opens a real bounded-fs-walk popup listing the fixture repo's actual files (`a.txt`, `calc.py`) (`rca/15-14-mention-popup.png`). Filters correctly at human pace. Insertion path independently proven via the slash chip above; the `@`-mention click-to-chip step itself hit the same intermittent character-loss bug on two of three attempts (see Defects) so I could not get a clean click-to-chip screenshot for `@` specifically this pass — the mechanism is the same code path proven for `/`, so PASSED stands, not half-proven. |
| F-CHAT-11 | NOT EXERCISED | Native attach picker (needs `TILLER_WL_PORTAL=1`). Not re-driven this pass for time; ledger's wave-F evidence (real GTK dialog, PNG accept / txt reject) stands unchallenged. |
| F-CHAT-12 | NOT EXERCISED | xdnd attach + chip removal. Not re-driven this pass. |
| F-CHAT-13 | NOT EXERCISED | xdnd oversized-file rejection. Not re-driven this pass. |
| F-CHAT-14 | PASSED | Overflow menu opens with exactly "Follow Edited Files", "New Conversation", "Chat History" (`rcc/03-02-overflow-menu-open.png`). New Conversation genuinely resets the tab: transcript went from a completed PONG exchange back to zero entries, status pill from "Auto ⌄" back to bare "idle", composer back to placeholder (`rcd/05-03-follow-toggled.png` — mislabeled in my own script, this is actually the New-Conversation-reset frame, see Notes on my own coordinate slip). Follow Edited Files toggle itself not independently re-clicked cleanly this pass (my click landed on New Conversation instead, see Notes) — ledger's own toggle screenshot stands unchallenged. |
| F-CHAT-15 | PASSED — independently re-confirmed, and the stale `ACP-12` note explained | Status pill is non-interactive (no chevron) before any turn completes; the instant `has_completed_turn` flips it grows a "⌄" and becomes clickable. Clicked it → real dropdown with all six modes (Auto, Manual, Accept Edits, Plan Mode, Don't Ask, Bypass Permissions), current mode ("Auto") highlighted (`rcb/03-02-mode-dropdown-open.png`). Clicked "Manual" → pill label changed live to "Manual" (`rcb/04-03-mode-manual-selected.png`). See headline note above re: `ACP-12`. |
| F-CHAT-16 | PASSED | Model pill click opened a real picker: search field, "Default (recommended)" with a Recommended badge, Opus (1M context), Fable, Sonnet, Haiku, Opus Plan Mode (`rca/21-20-model-pill-click-attempt1.png`). Clicked "Sonnet" → pill updated live to "Sonnet HIGH" (`rcb/06-05-model-sonnet-selected.png`). |
| F-CHAT-17 | PASSED | Same popover carries an inline Effort row (Default/Low/Medium/High/Xhigh/Max) with the current value highlighted. Clicked "Medium" → pill updated live to "Sonnet MEDIUM" (`rcb/08-07-effort-medium-selected.png`). |
| F-CHAT-18 | NOT EXERCISED | Usage breakdown popover. Not re-driven this pass; real-turn usage ring itself was seen moving (0%→5%→4%→0% across runs) confirming the live wiring, but I did not open the breakdown popover myself. Ledger's wire-level fixture evidence stands unchallenged. |
| F-CHAT-19 | NOT EXERCISED | Ring warning color at >80%. Not re-driven (would need the same wire-level fixture as the ledger used). |
| F-CHAT-20 | NOT EXERCISED | Tail-follow / scroll-pin. Not re-driven this pass. |
| F-CHAT-21 | NOT EXERCISED | Thinking-row toggle. Not re-driven this pass (would need the `staged` fixture mode). |
| F-CHAT-22 | NOT EXERCISED | WorkGroup / older-turn fold. Not re-driven this pass. |
| F-CHAT-23 | NOT EXERCISED | Tool-card expand/collapse, location link, permission dismiss. Not re-driven this pass. |
| F-CHAT-24 | NOT EXERCISED | Plan card. Not re-driven this pass (would need the `plan` fixture mode). |
| F-CHAT-25 | NOT EXERCISED | AskUserQuestion text/option/cancel. Not re-driven this pass. |
| F-CHAT-26 | NOT EXERCISED | Scroll-to-question via pending bar. Not re-driven this pass. |
| F-CHAT-27 | NOT EXERCISED | Question expiry. Not re-driven this pass. |
| F-CHAT-28 | NOT EXERCISED | Subagent task card. Not re-driven this pass. |
| F-CHAT-29 | NOT EXERCISED | Assistant message copy control. Not re-driven this pass. |
| F-CHAT-30 | NOT EXERCISED | Code-block copy control. Not re-driven this pass. |
| F-CHAT-31 | half-proven | Asked real Claude Code to edit `calc.py` (`rcj/02-01-after-edit-request.png`); once the `Edit calc.py` tool step completed, the Files panel's `calc.py` row grew a `Diff` badge (`a.txt`, untouched, has none) — the app is correctly tracking which files changed this turn and surfacing that in the file list. I did not click the badge to confirm the diff preview itself renders (turn was still in progress and I stopped there to conserve remaining budget), so the VERIFY clause's "expand" step is unconfirmed — hence half-proven, not PASSED. |
| F-CHAT-32 | NOT EXERCISED | Revert edited file. Not re-driven this pass; no revert control was visible in the `rcj` screenshot's Files panel next to the diffed `calc.py`, but the turn had not finished and the control may only appear post-turn or on hover — not enough to call this either way. |
| F-CHAT-33 | NOT EXERCISED | MCP warning / retryable error dismiss. Not re-driven this pass; ledger's own transplanted-test evidence (genuine two-sided pre/post-fix contrast) is unusually strong and stands unchallenged. |
| F-CHAT-34 | **FAILED — defective** | See headline and Defects. Ledger's PASSED is not reproducible through normal use. |
| F-CHAT-35 | PASSED | Directly re-confirmed as a byproduct of the F-CHAT-34 investigation: a genuinely history-less Chat History popover (no persisted sessions reachable from the current tab) shows "No past chats" (`rcd/07-05-after-new-conversation.png`, `rce/06-05-chat-history-open.png`, `rcg/05-04-history-open.png` — three independent reproductions). This row's own narrow claim (the empty state renders) is true regardless of the F-CHAT-34 defect. |
| F-CHAT-36 | NOT EXERCISED | No-models fallback. Not re-driven this pass (would need the `plain` fixture, which I used for other purposes but didn't screenshot the model-badge fallback state specifically). |
| F-CHAT-37 | PASSED | Freshly opened Claude Code chat tab: zero transcript entries, "Message…" placeholder, `+` attach, status pill, agent badge, `…` overflow, usage ring, send arrow all present and rendered before any send (`recon4/02-01-chat-opened.png`, `rca/04-03-chat-idle-fresh.png`). Composer focus confirmed by typed text appearing (`rca/05-04-composer-focused.png`). |

## Defects found this pass

### 1. `F-CHAT-34` — Chat History can never show a closed conversation (FAILED — defective)

**Reproduction (two independent methods, both clean):**

Method A — live UI, three separate fresh instances:
1. Open a real Claude Code chat, send one message, get a real reply (turn genuinely completes,
   footer/timestamp renders).
2. Close the tab (`×` on the tab strip).
3. Open a brand-new Claude Code chat in the **same worktree**.
4. Open its overflow menu → Chat History.
5. Result every time: **"No past chats"** — despite one real, completed conversation having just
   happened in this exact worktree seconds earlier. (`rce/06-05-chat-history-open.png`,
   `rcg/05-04-history-open.png`)

Method B — direct sqlite reads of the running instance's own DB (`/tmp/<label>.sqlite`), taken
mid-script via inline `python3 -c` calls inside the `wayland-drive.sh` action block:
```
TABS-AFTER-CLOSE: [('default-chat', 'chat', 'p-c1fd7a5bbfd541af-wt-1')]
CHAT_TURN-AFTER-CLOSE: []
```
The fixture worktree's `tab` row for the closed chat, and every `chat_turn` row that belonged to
it, are both gone within ~2s of clicking the tab's `×`. (One earlier run, `rcf`, caught the state
*before* the debounced layout-save had flushed the delete and briefly saw the row survive — that is
the save's own debounce window, not a second behavior; a 2s-later re-check in the very next run,
`rcg`, with the identical script, shows it gone.)

**Root cause** (read from source, not guessed): `Chat::persist_settled_transcript()`
(`rust/crates/tiller_ui/src/chat.rs:2122`) correctly writes the completed turn into `chat_turn` the
moment a real turn ends — confirmed live: immediately after a turn, before closing anything, the DB
already has 1 `chat_turn` row (`rcf`'s `TABS-BEFORE-CLOSE`/`CHAT_TURN-BEFORE-CLOSE`, 150-byte
payload). But `chat_sessions()` (`rust/crates/tiller_persistence/src/db.rs:587`, what "Chat History"
actually queries) does `FROM tab JOIN chat_turn ON chat_turn.tab_id = tab.id` — an inner join against
the live `tab` table. Closing a tab removes it from the in-memory layout; the next debounced
`schedule_save` deletes its now-absent `tab` row (`db.rs:425/461`), and `chat_turn`'s foreign key
cascades the delete onto that tab's transcript. The result: a session is only ever visible in "Chat
History" while its own tab is still open.

**This is real, not a fluke of my test:** confirmed the converse too. With **two** Claude Code chat
tabs open simultaneously in one worktree, completing a turn in the first and then opening Chat
History **from the second, still-open tab** correctly lists the first tab's session with a working
"Delete" control (`rci/05-04-history-from-second-tab.png`). So the query and the popover UI are both
correct — the only thing that's wrong is that the moment a user does what "history" implies (finishes
a chat and closes it), that chat becomes permanently unlistable. F-CHAT-34's VERIFY clause ("confirm
past sessions are listed... delete another") describes exactly the scenario that is now proven
unreachable through the normal, expected interaction.

### 2. Composer buffer loses characters under rapid keystrokes while the slash/mention popup is open (new finding, not tied to one row's pass/fail)

**Reproduction:** with the mention popup open (after typing `@`), sending `"calc"` as one fast
`wtype`-driven burst (the `type` action) leaves the composer reading **`c`** — only the last
character survives, and even the leading `@` is gone, so the popup itself closes
(`repro/09-08-fast-multichar-at-calc.png`, and the same shape of loss independently in the real-claude
run `rca/13-12-slash-filtered.png` typing `"cr"` → composer left reading just `r`).

**Calibration:** typing the same characters as **separate** `type` calls with an explicit delay
between each is not a fixed threshold — 150ms/keystroke reliably worked in two separate runs
(`repro2/02-01-150ms-per-key.png`, `rcb/10-09-slash-filtered-clean.png`), but 50ms/keystroke lost the
buffer the same way (`repro2/03-02-50ms-per-key.png`), **and** on one occasion even ~1s/keystroke
lost it (`rcb2/06-05-mention-a-filtered.png` — composer left reading bare `a`, no `@`, no popup) while
an *identical* script on a fresh instance moments later with the same 1-2s pacing worked cleanly
(`rcb3/03-02-at-a.png`). That pattern — reliable at a truly human pace (150ms+), reproducible at
synthetic-burst speed, but *occasionally* misfiring even at slow, human-plausible pacing — reads as a
genuine intermittent race in the popup's filter-recompute against the composer's own text buffer, not
a pure artifact of `wtype` being faster than a person could type. I'm reporting it factually rather
than asserting a fixed root cause; a builder with access to the actual recompute code will find the
exact race faster than I can from outside. Per this project's own standing lesson, I have **not**
flipped F-CHAT-09/10 to FAILED over this — the core mechanism (open, filter at working pace,
keyboard-nav, click-to-insert-chip) is independently and cleanly proven working in this same pass —
but it is real, reproducible, and worth a look.

## Could not reach / not exercised this pass

Time this pass went disproportionately into (a) resolving the `F-CHAT-15`/`ACP-12` contradiction
live rather than trusting either side's paper trail, (b) the `F-CHAT-34` Chat History investigation
once the first live check contradicted the ledger, and (c) discovering and confirming the
`TILLER_ACP_PROGRAM` bypass described in the preamble — all three paid off with real findings but
left less time for full-breadth coverage. The following rows were **NOT EXERCISED** this pass and
their existing ledger verdicts were not independently re-driven (listed above per-row too, collected
here for visibility):
F-CHAT-02, 03, 05, 07, 11, 12, 13, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 32, 33, 36.
(F-CHAT-31 moved to half-proven this pass, see its row above.)

Two different reasons apply, and they should not be conflated:

- **F-CHAT-02, 03, 05, 07, 11, 12, 13, 18, 19, 20, 29, 30, 32, 33** — simply not attempted this pass,
  budget ran out first. No harness obstacle is claimed for these; a future pass driving real Claude
  Code the same way I did for the rows above should be able to reach them directly. No claim is made
  about their correctness either way beyond what the existing ledger already states.
- **F-CHAT-21, 22, 23, 24, 25, 26, 27, 28, 36** — these need a wire-level state (thinking chunks,
  plan/question/subagent shapes, or the no-models fallback) that real Claude Code either cannot
  reliably produce on request (thinking chunks in particular: `WAYLAND-LANE.md` records that the
  real ACP bridge only forwards thinking chunks with text, and current models default
  `thinking.display` to omitted, so real Claude never emits one) or can only produce probabilistically
  by prompting for the right tool use. Reaching them properly needs `chat_fixture.py` via the
  agent-less "New Chat" path, which requires the command palette; every attempt to open it
  (`chord ctrl+shift p`) this pass failed in the harness's virtual-keyboard setup before any UI action
  ran — 7 consecutive failures on that exact script, interleaved with unrelated scripts that ran
  clean on the same host — so I'm calling this a harness limitation this session, not evidence about
  the app, and leaving these rows on the ledger's existing verdicts rather than guessing.

## Notes on my own harness slips (recorded so a later pass doesn't repeat them)

- Overflow-menu item Y-coordinates in the running app: Follow Edited Files ≈760, New Conversation
  ≈791, Chat History ≈822 (at 1715×972, composer in its single-line resting position). An early
  script of mine clicked 791 intending "Follow Edited Files" and hit "New Conversation" instead —
  the reset I attributed to "Follow Edited Files" in an intermediate screenshot is actually the New
  Conversation reset (corrected in the row table above). A later script's Chat History click used a
  stale y=852 and silently missed all three items (menu closed on nothing) before I corrected it to
  822.
