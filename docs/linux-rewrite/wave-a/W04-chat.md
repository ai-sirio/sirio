# Wave A slice W04-chat — 4 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CHAT-28` — ledger line 177, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Entry::SubagentTask is constructed in production (is_subagent_tool_call), rendered (render_subagent_task_card), and fully drawn-tested (a_subagent_task_card_expands_nested_tool_calls) -- built in commit 9c286975 (2026-08-14 14:19), after the stale 'pass 8' evidence the verdict cites. Not absent; needs a live drive with a subagent-dispatching prompt to close.
- **Shared cause:** One of four rows (28/29/30/32) marked FAILED before their chat.rs features were built later the same day, 2026-08-14; ledger text unchanged since the 2026-08-13 20:32 checkpoint.
- **Evidence on record:** no subagent task cards

## `F-CHAT-29` — ledger line 178, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** CopyTarget::Assistant + the hover-revealed assistant-copy control (invisible by default, .group_hover reveals it) + copy_local_text's 2s 'Copied ✓' confirmation all exist -- built in 398c0aea (17:25), 14.75h after the pass-17 evidence (02:40 same day) the verdict cites as finding no per-message control. Re-drive: hover, click Copy, paste, confirm.
- **Shared cause:** One of four rows (28/29/30/32) marked FAILED before their chat.rs features were built later the same day, 2026-08-14; ledger text unchanged since the 2026-08-13 20:32 checkpoint.
- **Evidence on record:** the clause's hover-Copy control on a response does not exist: live hover over an assistant message produced no affordance (p17-an1), and `grep -in copy chat.rs` shows the ONLY copy path is the transcript-wide `CopyTranscript` action — no per-message control anywhere. That chord path is itself live-defective: `ctrl-a` produced no visible selection and `ctrl-c` + paste-check came back empty twice with the composer focu

## `F-CHAT-30` — ledger line 179, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** CopyTarget::CodeBlock + a code-block-copy-{entry}-{id} button wired to the same copy_local_text/confirmation exist -- built in 0ecbd525 (17:26), after the same pass-17 (02:40) evidence the verdict cites as finding no block-copy control anywhere. Re-drive: click the code block's Copy control, paste, confirm.
- **Shared cause:** One of four rows (28/29/30/32) marked FAILED before their chat.rs features were built later the same day, 2026-08-14; ledger text unchanged since the 2026-08-13 20:32 checkpoint.
- **Evidence on record:** the code block itself renders correctly — `bash` language label, monospace body (p17-am1-codeblock) — but no Copy control exists on it: hover produced nothing (p17-an1-hover), a blind click at the conventional top-right corner + paste-check into the terminal came back empty (p17-an3-pastecheck), and neither chat.rs nor tiller_markdown contains a block-copy control (`grep -i copy` — only the transcript-wide CopyTransc

## `F-CHAT-32` — ledger line 181, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** EditSummaryState, render_edit_summary, and the full Open/Revert flow (request/cancel/confirm_edit_revert, discard-via-git) all exist and are drawn-tested (edit_summary_opens_and_reports_revert_success_or_error, including a failed-discard error state) -- built in c23da365 (17:33), after the stale 'pass 8' evidence. Not absent; needs a live drive.
- **Shared cause:** One of four rows (28/29/30/32) marked FAILED before their chat.rs features were built later the same day, 2026-08-14; ledger text unchanged since the 2026-08-13 20:32 checkpoint.
- **Evidence on record:** no edit summary

