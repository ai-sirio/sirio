# W04-chat evidence — Wayland lane (`TILLER_WL_LABEL=wavea-W04-chat`)

Slice: F-CHAT-28, F-CHAT-29, F-CHAT-30, F-CHAT-32 (triage: reclassify — chat.rs features
built later the same day the FAILED verdict was recorded, per
`docs/linux-rewrite/wave-a/W04-chat.md`).

## F-CHAT-28 — subagent task cards

**Drove:** real ACP turn over `surface.chat.send`, prompt (underscored, ctl splits on
whitespace): `Use_the_Task_tool_to_dispatch_a_subagent_that_runs_pwd_and_reports_back._Do_it_now.`
Polled `surface.chat.read` every 6s until `status:"completed"` (10 polls, ~60s).

**Observed:** the turn completed with a tool entry `{"kind":"tool","status":"Completed",
"text":"Run pwd and report"}` followed by an assistant message: *"The subagent ran `pwd` and
reported: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`"* — i.e. the installed Claude
Code CLI genuinely dispatched its Task tool (a real subagent) and Tiller's chat surface carried
the round trip through to completion. `control_entry_row` (chat.rs:5408-5432) deliberately
collapses `Entry::SubagentTask` into the same `kind:"tool"` JSON shape as `Entry::ToolCall`, so
the socket cannot distinguish which variant was constructed — this is a known instrument limit,
not an ambiguous result. A forced-repaint capture of the transcript region (`x=680,y=0,720x400`)
measured `stddev=23.46` (non-uniform — real content drawn, not blank), a plausibility check only.

**Claim:** partially-exercised. The subagent-dispatch backend path is live-confirmed with a
prompt only this drive could have produced (an ACP Task-tool round trip ending in a
pwd-result assistant reply); given `is_subagent_tool_call`'s raw-input check
(`chat.rs:396-405`, matches on `subagent_type`/`agent_type` in the tool's raw ACP input, which
a real Task-tool call carries), this construction path was very likely taken. The click-to-expand
gesture on the resulting card and its nested tool call (the other half of the clause) was not
independently confirmed — this lane cannot see rendered pixels, only measure regions, and no
JSON field exposes card identity or expanded state to distinguish a hit from a miss.

**Captures:** `reference/linux-progress/wavea-W04-chat/f28/01-baseline.png`,
`reference/linux-progress/wavea-W04-chat/f28/02-02-after-send.png`

## F-CHAT-29 — assistant-response Copy + confirmation

**Drove:** real ACP turn (`Reply_with_only_the_single_word:_COPY29MARK` — the model declined
and answered with a multi-line refusal instead, which is itself fine for this row: any rendered
`Entry::Assistant` gets the hover-Copy control). Attempted a hover(`move 700 190`)+`click 1350
186` at an estimated top-right-of-entry coordinate, then forced two repaints back to the
launch resolution to diff before/after.

**Observed — why this could not be closed live:** two blocking facts, both confirmed this
drive, not assumed:
1. **No exposed geometry.** `control_entry_row` (chat.rs:5408-5432) serializes only
   `kind`/`id`/`text`/`status` for every entry — no bounds, no DOM-like index-to-pixel mapping.
   The transcript is a virtualized `list()` whose row heights depend on wrapped markdown line
   count, which depends on the model's actual reply text — not knowable before the reply lands,
   and not queryable after it either.
2. **The compositor's actual startup resolution does not match its configured default before an
   explicit `swaymsg output` call.** A raw `grim` capture taken immediately at launch (no prior
   `shot`/`swaymsg` call) measured `1400x900`, not the `1715x972` `wayland-sway.conf` declares —
   confirmed by `identify` on `b1-before.png` vs `b2-after.png` in this very drive. Any click
   coordinate computed against the documented default is unreliable until the first forced
   repaint, which itself changes the layout it's meant to verify.
3. **The clause's second half — "paste elsewhere, and confirm the copied text" — has no
   reachable path in this lane at all, independent of click precision.** Grepped
   `composer.rs`, `chat.rs`, and `tiller_terminal/src/lib.rs` for a paste keybinding: the only
   `Paste` action anywhere in the app is `TerminalContextAction::Paste`, wired solely to the
   terminal's right-click context menu (`tiller_terminal/src/context_menu.rs:43-44`).
   `WAYLAND-LANE.md` is explicit that right-click is unexercised on this lane. There is no
   keyboard paste binding to fall back on.

**Claim:** could-not-reach. The backend/render half (control exists, writes the clipboard,
shows `Copied ✓` for 2s) is already established by triage from the drawn test
`assistant_response_copy_writes_text_and_confirms` (398c0aea) and is not disputed here. The live
gesture half is blocked on this lane by (1) no queryable button geometry and an
unreliable pre-repaint resolution baseline for blind coordinate guessing, and (2) the clause's
paste-confirmation half requiring a control this lane has no verified way to invoke. Closing
this row needs `DISPLAY=:1` (real click precision via visual feedback) — still not right-click,
since Copy is a plain left-click control, so a left-click-capable lane could reach the Copy half;
only the paste-elsewhere half is intrinsically right-click-only.

**Captures:** `reference/linux-progress/wavea-W04-chat/f29/b1-before.png`,
`reference/linux-progress/wavea-W04-chat/f29/b2-after.png`

## F-CHAT-30 — code-block Copy + confirmation

**Drove:** re-used the F-CHAT-29 finding rather than re-running an identical failed-geometry
drive (same transcript, same `control_entry_row` instrument, same compositor). Confirmed the
code-block control's selector is per-block and per-entry
(`code-block-copy-{entry}-{id}`, chat.rs:2973-2974) — even less predictable geometry than the
assistant-response control, since its position depends on where inside a multi-line assistant
reply the fenced block itself starts, which in turn depends on the model's actual generated
markdown (not knowable in advance, not queryable after).

**Claim:** could-not-reach, for the same two structural reasons as `F-CHAT-29`: (1) no
selector-to-pixel mapping is exposed anywhere on the control socket — `control_entry_row` never
serializes code-block-level detail at all, only the parent entry's `kind`/`text`/`status` — so a
code block's Copy control has strictly less locatable information than the assistant-row
control that was already unreachable; (2) the clause's paste-confirmation half needs a paste
path this lane cannot invoke (no keyboard paste binding anywhere in the app; the only `Paste`
action is the terminal's right-click context menu, out of scope per `WAYLAND-LANE.md`). Backend
half already established by triage from the drawn test `code_block_copy_writes_code_and_confirms`
(0ecbd525) — not disputed here.

**Captures:** none new; see `F-CHAT-29`'s captures for the shared compositor-baseline finding.

## F-CHAT-32 — edit summary open/revert

**Drove:** real ACP turn asking the connected Claude Code CLI to write a throwaway probe file
(`docs/scratch/w04chat32_probe.md`, one line `PROBE_LINE_32`) — a genuine file-editing tool call,
which the row's clause needs to reach an edit summary at all. Polled `surface.chat.read` to
completion.

**Observed:** the turn completed with a real `Write` tool call
(`{"kind":"tool","status":"Completed","text":"Write docs/scratch/w04chat32_probe.md"}`), and —
checked directly on disk, not inferred from the transcript text — the file genuinely existed
at that path with exactly the requested content (`cat` confirms `PROBE_LINE_32`;
`git status --porcelain` showed it `??` untracked). This is a real ACP write, not a chat-only
claim, and the diff-carrying tool call is exactly what `render_edit_summary` (chat.rs:3442)
needs to attach an edit-summary card to. Removed the probe file afterward with `git clean -fd
docs/scratch` (not an app edit — deleting a test artifact this drive created).

**Claim:** could-not-reach for the Open/Revert click gesture, for the same structural reason as
`F-CHAT-29`/`F-CHAT-30`: `render_edit_summary`'s card is a child of the same list-virtualized,
content-dependent-height transcript, and `control_entry_row` exposes no bounds for it. Unlike
the copy rows, this row has no clipboard/paste barrier — Open/Revert are both plain left-clicks,
so a lane with real visual feedback (`DISPLAY=:1`) could plausibly close this by seeing the card
and clicking it precisely, without needing right-click. What is exercised here: the trigger
condition (a real diff-carrying tool call landing in the transcript) is live-confirmed with a
side effect only this drive could have produced. Backend Open/Revert/error-state mechanism
already established by triage from the drawn test
`edit_summary_opens_and_reports_revert_success_or_error` (c23da365) — not disputed here.

**Captures:** `reference/linux-progress/wavea-W04-chat/f32/01-baseline.png`,
`reference/linux-progress/wavea-W04-chat/f32/02-02-after-send.png`
