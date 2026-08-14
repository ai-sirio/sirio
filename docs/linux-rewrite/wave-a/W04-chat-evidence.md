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
