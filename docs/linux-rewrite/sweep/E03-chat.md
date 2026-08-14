# Drive slice E03-chat — 5 rows

Families: F-CHAT.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-CHAT-05` | 154 | half-proven | offline half exercised (pass 17): from `● offline` the exact click+type+Return sequence that sent three earlier messages produces nothing — the composer is effectively inert — but NO placeholder or visual communicates the disabled state (frame p17-ak4); the clause's "corresponding placeholder" does not appear. Permission-wait half unexercised — manual mode never raised a per… |
| `F-CHAT-13` | 162 | NOT EXERCISED | instrument-unreachable for this critic (pass 17): xdotool cannot synthesize an XDND drag (no source window to negotiate the protocol), and the alternative "+" attach control opens the Wayland portal picker, which is invisible to X captures (ENVIRONMENT.md) — a human CAN drop a file, so this is not UNREACHABLE, it is unexercisable by the current harness. Needs either a real h… |
| `F-CHAT-20` | 169 | half-proven | follow half proven live (pass 17): during two real streamed replies the viewport stayed pinned to the tail as content grew — frames p17-ai2 (rows 36–60 visible mid-stream) and p17-aj3 (stream advanced to 90, tail still in view). The manual-scroll-ownership half (scroll away mid-stream → follow stops until re-pinned) was not exercised |
| `F-CHAT-24` | 173 | NOT EXERCISED | **the pass-8 "absent" was wrong.** `PlanApproval` in `chat.rs` is commented `(F-CHAT-24)` and carries `request_id`/`options`/`resolved`/`expired`; `PlanEntryRow` holds each plan row; test `a_plan_renders_approval_attaches_and_the_plan_advances` (`chat.rs:4719`) is named for it. Code exists and is tested — **no critic has exercised it live**, so it is not PASSED |
| `F-CHAT-25` | 174 | NOT EXERCISED | **the pass-8 "absent" was wrong.** `AnswerTextInput` (placeholder + prefill), `actions!(chat_question_answer, [SendAnswer, CancelAnswer])`, `answer_question_text` (`chat.rs:1381`), `cancel_question` (`:1412`), `render_question_answer_row` (`:1853`), test `cancel_on_a_question_closes_it_without_an_answer` (`:4551`). Both the text answer and the cancel exist — **unexercised li… |
