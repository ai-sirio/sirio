# Drive slice E04-chat — 3 rows

Families: F-CHAT.

Exercise each row live and record what you saw. **Do not edit the ledger**;
a different agent adjudicates. For a `half-proven` row the evidence column
names the half already proven — drive only the missing half and say which.

| row | ledger line | current verdict | evidence recorded so far |
|---|---|---|---|
| `F-CHAT-26` | 175 | NOT EXERCISED | **the pass-8 "absent" was wrong.** `pending_question()` at `chat.rs:1490` returns the pending entry and its prompt. Exists — **unexercised live** |
| `F-CHAT-27` | 176 | NOT EXERCISED | **the pass-8 "absent" was wrong.** `PlanApproval.expired` plus test `a_question_whose_turn_ends_unanswered_expires_instead_of_waiting` (`chat.rs:4625`) — the state exists and the turn-end transition is tested. **Unexercised live** |
| `F-CHAT-33` | 182 | half-proven | turn-error half proven live (pass 17): killing the ACP subtree mid-stream produced a red error entry carrying the machine reason (`prompt failed: Incoming transport closed: {"reason": "incoming_transport_closed", "method": "session/prompt"}`) with a Retry button; partial streamed output is retained above it; banner position depends on how much streamed before death (top when… |
