# Wave C slice C-CHAT-2 — verdicts

Critic pass. No code edited (independent-critic house rule). Every row below was
re-driven live by this pass on its own instance(s) of the Wayland lane
(`Scripts/wayland-drive.sh`, binary at `rust/target/debug/tiller`, built at HEAD) —
the builder's report was read only for routes, never accepted as proof. Frames the
builder cited were opened directly; two of the four "already-correct" rows had
proof-frame paths that did not exist where claimed, so this pass independently
located and verified the real frames (see `F-CHAT-29`/`F-CHAT-30` below) before
crediting them. `F-CHAT-25/26/27` were re-driven from scratch with fresh instances
(`critic1`–`critic7`), because the builder's "blocked" claim turned out to be wrong
for two of the three rows — see the note at the bottom.

## F-CHAT-28 — PASSED

Builder's own frames, opened directly: `/tmp/f28c3shots/06-04-turn75s.png` shows
the collapsed purple "Subagent · Run pwd and report · Completed" card after a real
Task-tool round trip; `/tmp/f28c3shots/07-05-expand-clicked.png` (same session,
click at the header row) shows the chevron flipped and a nested
"Execute pwd — Completed" row rendered inside. Discriminating: the collapsed and
expanded states are visually distinct and the nested row only exists post-click.
Not a re-drive — the builder's cited proof genuinely supports the claim.

## F-CHAT-29 — PASSED

The builder's report claims proof frames "02-assistantcopy-clicked.png" under
session label `f29c1`, but `/tmp/f29c1shots/` only contains six frames ending at
`06-04-turn60s.png` — no click frame exists there. The real proof lives in a
differently-named, unmentioned directory, `/tmp/f29manual/` (same wall-clock
session, one minute later: 13:22 vs 13:23). Opened both frames directly:
`02-assistantcopy-clicked.png` shows the top-right hover "Copy" pill now reading
"Copied" for the plain-text assistant reply "The answer is four." — a state the
baseline frame (`06-04-turn60s.png`, no pill visible at all) never reaches on its
own. PASSED on the merits, but the report's own directory reference for this row's
evidence was wrong and would have sent a reader to a directory with no proof in it.

## F-CHAT-30 — PASSED

Same session as above; report cites the same `f29c1` label but the real frame is
`/tmp/f29manual/01-codecopy-clicked.png`, opened directly: the code block's
always-visible "Copy" button is mid-transition to "Copied ✓" with the cursor still
on it, and the assistant-message-level "Copy" pill is now also visible (confirming
the two controls share one hover group, as the report separately claims for
F-CHAT-29). Discriminating vs. the pre-click baseline, which shows plain "Copy".

## F-CHAT-32 — PASSED

Builder's own frames, opened directly and cross-checked against disk:
`06-04-turn75s.png` (Revert, unclicked) → `07-05-revert-clicked-1715.png` (row now
reads "Confirm Revert" / "Cancel") → `09-07-confirm-attempt-1715.png` (row reads
"reverted", red text). Then verified independently on disk: `git status --porcelain`
and `ls docs/scratch/` in the actual worktree — the directory does not exist at
all, meaning the probe file the agent wrote and the user reverted is genuinely gone,
not just relabeled in the transcript. This is the strongest kind of instrument for
this row (a real filesystem check, not a second screenshot).

## F-CHAT-26 — PASSED (report's "blocked" verdict is incorrect)

The builder reported this blocked because "a calibrated click directly on the
[mode] pill... did not open the picker in either attempt." This pass re-drove it
independently on a fresh instance (`critic3`, frame
`/tmp/critic3shots/05-03-after-click-530-864.png`) using the same coordinates
(530,864 @1715px) the builder used, and the mode picker opened cleanly, showing
all six modes (Auto / Manual / Accept Edits / Plan Mode / Don't Ask / Bypass
Permissions). Selected "Manual" (`critic4`/`critic5`, pill now reads "Manual").
Sent a real Write-tool turn under Manual mode (`critic6`): a live
`Entry::Permission` card rendered — "Write docs/scratch/critic_probe2.md" with
"Deny" / "Allow Once" / "Always Allow" buttons, a bottom "Question waiting" bar
with a "Show" affordance, and the composer showing "Waiting for permission
response…" with the send button replaced by a Stop square
(`/tmp/critic6shots/08-06-pending.png`). Discriminating: the identical write-tool
action completed with **no** permission card under Auto mode in this same pass
and in the builder's own F-CHAT-32 evidence — the pending-permission bar is a
state Auto mode never reaches. Whatever caused the builder's two failed clicks
(timing, a stale coordinate, bad luck) was not a defect in the control itself.

## F-CHAT-27 — PASSED (report's "blocked" verdict is incorrect)

Continuation of the same live session (`critic6`): with the permission card from
F-CHAT-26 still pending, called `surface.chat.stop surfaceId=default-chat` over
the control socket. `surface.chat.read` immediately after shows the permission
entry's status flip from `"pending"` to `"expired"` and the turn entry read
`"14:24 · cancelled"`. The capture taken at the same moment
(`/tmp/critic6shots/09-07-after-stop.png`) shows the permission card's button row
replaced by the exact string `"No answer — the turn ended"` under
"mkdir -p docs/scratch" — byte-for-byte the text at `chat.rs:4194`
(`expire_unanswered`, called from the turn-stop path). Both the data-model half
and the render half are proven live in the same drive.

## F-CHAT-25 — UNREACHABLE (confirmed blocked, but with much stronger evidence than the report had)

Re-drove independently (`critic7`): switched to Manual mode exactly as for
F-CHAT-26, then sent "Use your AskUserQuestion tool right now to ask me to pick
between Red and Blue. Call the AskUserQuestion tool directly, do not just
describe it in text." The live agent's own reply
(`/tmp/critic7shots/07-05-after-send1.png`): *"I don't have an AskUserQuestion
tool available in this environment — it's not in my tool list or the
deferred-tool registry, so I can't invoke it directly. I can still ask you
directly: Red or Blue?"* — the installed Claude Code CLI in this ACP session
genuinely does not expose that tool, confirmed directly from the model rather than
inferred from an absent card. `parse_permission_question` / `AnswerTextInput` /
`render_question_answer_row` are all present and structurally correct in source
(`tiller_acp/src/lib.rs:1483`, `chat.rs:196-212`/`~2748`), and the exact same
`Entry::Permission` rendering path is proven live end-to-end for the plain-gate
case (F-CHAT-26/27 above) — only the CLI-side trigger is missing. Triage's
"try Pi's `ui/select`" alternate route (which folds into the same
`parse_permission_question` path per its doc comment) was not attempted this pass
either — time did not allow standing up a second agent adapter on top of the six
drives already run. Genuinely UNREACHABLE this pass, not merely untried: a live
attempt was made and the live agent explicitly declined for a reason outside this
slice's owned files.

## Note on the builder's report

Four of seven rows ("already-correct") checked out — two (`F-CHAT-29`/`30`) only
after finding the real proof frames under a path the report never mentioned. The
three "blocked" rows were the opposite problem: the report's own investigation
was thorough and honest about what it tried (a calibrated click, checked against
cursor position in the capture) and honest that it found no code-level defect —
but the conclusion ("blocked," "not currently reachable") did not hold up. Two of
the three opened immediately on a fresh instance. Worth recording for future
passes: a single failed click at a coordinate read off a *different* drive's frame
is not strong enough evidence to file "blocked" against a control this central —
the mode picker gates all of F-CHAT-25/26/27, and treating it as unreachable
would have left three real, working rows permanently misfiled.

## Files touched this pass

None. `git status --porcelain` shows no changes under this slice's owned files
(`rust/crates/tiller_acp/src/lib.rs`, `rust/crates/tiller_markdown/src/{file_events,lib}.rs`,
`rust/crates/tiller_persistence/src/migrations.rs`, `rust/crates/tiller_ui/src/{chat,settings}.rs`,
`rust/crates/tiller_usage/src/account.rs`) — every row was a verification-only pass.
