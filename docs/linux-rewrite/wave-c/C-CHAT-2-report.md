# C-CHAT-2 report

## F-CHAT-32 — Open + Revert -> Confirm Revert gesture (DONE, no code change needed)

Prior evidence: trigger half proven (real ACP Write tool call produced a `render_edit_summary`
card), but the Open (file-path text) and Revert -> Confirm Revert gesture had never been clicked.

This pass drove it live end to end, no code changes required — the wiring in
`rust/crates/tiller_ui/src/chat.rs` (`render_edit_summary`, ~line 3756) was already correct. The
first attempt's click missed because `wayland-drive.sh`'s `shot()` alternates the output
resolution (1715x972 / 1400x900) on every call, and a `click` issued between two `shot` calls
uses whatever resolution is currently active — not the resolution of the *next* screenshot. Two
clicks issued back-to-back right after the same `shot` call share that call's resolution, so
batching the Open+Revert clicks together (both at 1715-width coordinates, right after the
`04-turn75s` shot) worked; a later attempt at the Confirm Revert button needed its own
resolution-matched coordinates, found by trying both plausible x positions in one drive.

Live proof (this session, label `f32c4`):
- `07-05-revert-clicked-1715.png` — clicking Revert (1138,260 @1715px, right after Open at
  700,260 same resolution) flips the row to "Confirm Revert" / "Cancel".
- `09-07-confirm-attempt-1715.png` — clicking Confirm Revert (1050,260 @1715px) flips the row to
  "reverted", and `git status`/`ls` on the worktree confirmed the probe file
  (`docs/scratch/f32chat2c_probe.md`) was actually deleted from disk — the revert is real, not
  just a chat-transcript label.

**howToExercise**: open the Chat tab (`surface.chat.open` + `tab.select index=1`), send a message
that makes the agent write a new file (e.g. "Write a new file at docs/scratch/probe.md containing
X"), wait for the Write tool card (`render_edit_summary`) to render with "1 file changed", click
the file path text (Open — emits `ChatEvent::OpenFile`), then click "Revert" (turns into "Confirm
Revert"/"Cancel"), then click "Confirm Revert" (row shows "reverted", file removed from disk).
Coordinates are resolution-dependent under the wayland-drive lane — batch same-resolution clicks
between `shot` calls, or read pixel positions off the just-captured frame before clicking.

No commit needed for this row — nothing in `chat.rs` required a change, only the drive gesture was
missing. Row was already `half-proven`; the render/trigger half was already landed by a prior pass.

## F-CHAT-28 — subagent card click-to-expand (DONE, no code change needed)

Prior evidence: render half proven live (purple "Subagent" card with rail border and collapsed
chevron appears after a real Task-tool round trip); click-to-expand + nested-tool-call visibility
never attempted.

Drove it live this pass (label `f28c3`): sent "Use the Task tool to dispatch a subagent that runs
pwd and reports back. Do it now." to a real Claude Code ACP session, waited for the Subagent card
(`render_subagent_task_card`, `chat.rs:4412`), then clicked its header row. Two earlier attempts
in this same pass (`f28c1`, `f28c2`) missed the header because the assistant's preamble text
before the card varies in length turn to turn, shifting the card's y-position — the fix was to
read the card's exact y off the just-captured `04-turn75s` screenshot before clicking, rather than
reuse a coordinate from a previous turn's layout.

Live proof: `07-05-expand-clicked.png` shows the chevron flipped from collapsed (`>`) to expanded
(`v`) and a nested `Execute pwd — Completed` tool-call row now rendered inside the card — both the
click-to-expand gesture and the nested-tool-call render are proven. No code defect found on
inspection or in the drive; `on_click` -> `toggle_subagent_task_expanded` (`chat.rs:4463-4467`)
was already correct.

**howToExercise**: open the Chat tab, send a message that makes the agent use the Task tool (e.g.
"Use the Task tool to dispatch a subagent that runs pwd and reports back. Do it now."), wait for
the purple "Subagent" card, click anywhere on its full-width header row — the chevron flips and
child tool-call rows render inside. Read the header's y-coordinate off a screenshot taken
immediately before clicking; it moves with the length of the assistant's preceding text.

No commit needed for this row either — nothing in `chat.rs` required a change.

## F-CHAT-29 — assistant-message hover Copy control (DONE, no code change needed)

Prior evidence: control confirmed live in source (`chat.rs:3628-3670`, since renumbered — the
hover-only Copy button now sits inside `Entry::Assistant` rendering around `chat.rs:3944-3980`),
but the only live-click attempt used a miscalibrated guess and landed in the Files panel.

Drove it live this pass (label `f29c1`): sent a prompt engineered to produce a plain-text
`Entry::Assistant` response ("The answer is Four.") followed by a fenced code block in the same
turn, then — keeping the instance alive with `TILLER_WL_KEEP=1` rather than guessing coordinates
blind — read the exact button position off the just-captured frame before clicking.

Live proof: `02-assistantcopy-clicked.png` shows the button reading "Copied" after the click. The
hover-reveal itself needed no separate `move`: a synthetic `click`'s underlying pointer motion
passes through the target first, and — non-obviously — the earlier click on the code-block Copy
button (inside the same `Entry::Assistant`'s markdown document, same `group()` container) had
already left the assistant-copy button visible, because the hover group wraps the whole entry
(text *and* any embedded code blocks), not just the plain-text line.

**howToExercise**: get any plain-text assistant reply into the transcript, hover the response
text (or simply click near its top-right corner — the whole entry, including any code block
inside it, shares one hover group), a "Copy" pill appears at the top-right of the block; clicking
it copies the text and the pill reads "Copied" for a few seconds.

## F-CHAT-30 — code-block Copy control (DONE, no code change needed)

Prior evidence: control confirmed live in source (`code-block-copy-{entry}-{id}`,
`chat.rs:2968-2974` at the time, now `render_markdown_block`'s `Block::CodeBlock` arm around
`chat.rs:3278-3310`), never live-clicked.

Same drive as F-CHAT-29 (label `f29c1`): the code block's Copy button is **not** hover-gated
(unlike the assistant-message one) — it renders unconditionally in the code block's header row
next to the language label, so no hover step was needed, just a correctly-aimed click.

Live proof: `01-codecopy-clicked.png` shows the button's label mid-transition to "Copied ✓" with
the cursor still over it, directly after the click.

**howToExercise**: get any fenced code block into the transcript (assistant markdown, or a tool
result), the code block's header row always shows "Copy" on the right — click it, the label
changes to "Copied ✓".
