# Finish-line critic: the ten uncovered rows (wave wf-rest4)

Lane: `wf-rest4`. Ten rows, each already half-proven (or NOT EXERCISED) with one named missing
half — see the brief. This report drives exactly that missing half per row; the already-proven
half is not re-litigated. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-rest4-tiller
export TILLER_WL_BIN=/tmp/wf-rest4-tiller TILLER_WL_LABEL=wf-rest4
```

Verdict vocabulary is `PASSED | FAILED - defective | FAILED - absent | half-proven | UNREACHABLE |
NOT EXERCISED | N/A - platform`, exactly as `EVIDENCE-STANDARD.md`/the brief specify.

This file is written incrementally, one row at a time, and committed after each row lands.

---

## Status table (filled in as driven)

| row | verdict | one-line reason |
|---|---|---|
| F-TAB-09 | (in progress) | |
| F-CORE-FILE-04 | | |
| F-CORE-FILE-03A | | |
| F-GIT-RUN-01 | | |
| F-TAB-20 | | |
| F-CHAT-25 | PASSED | AskUserQuestion's text/option/cancel arms all covered by named drawn tests, none of which existed at wave H's ledger writing |
| F-CHAT-33 | | |
| F-CORE-ACT-17 | | |
| F-CORE-ACT-24 | | |
| F-AGENT-CODEX-01 | | |

---

## F-CHAT-25 — PASSED

**Missing half named in the brief**: "the row's own current clause (AskUserQuestion, unrelated to
that fix) was never re-driven." The ledger's `wave H` evidence only exercised the "+" -> New Chat
-> Codex path; the question-card clause itself (VERIFY: trigger a question, enter text and click
Send; repeat with a listed option; repeat with Cancel, confirming answered/cancelled states) was
untouched.

Reading first (not accepted as verdict, just to find what to run): a prior pass
(`FINISH-sweep-tail.md`, not yet reflected in the ledger row I was given) added a `question-options`
mode to `chat_fixture.py` and a new named test for the listed-option arm, alongside two pre-existing
tests for the text-answer and cancel arms. I re-ran all three myself, fresh, today, rather than
trust that report:

```
$ cargo test --manifest-path rust/Cargo.toml -p tiller_ui leaves_the_surface_and_clears_the_pending_bar
test chat::tests::a_listed_option_leaves_the_surface_and_clears_the_pending_bar ... ok
test chat::tests::a_text_answer_leaves_the_surface_and_clears_the_pending_bar ... ok
test result: ok. 2 passed; 0 failed

$ cargo test --manifest-path rust/Cargo.toml -p tiller_ui cancel_on_a_question_closes_it_without_an_answer
test chat::tests::cancel_on_a_question_closes_it_without_an_answer ... ok
test result: ok. 1 passed; 0 failed
```

**Reachability check** (per the brief's "grep and confirm something outside the defining crate
reaches it" instruction, applied even though this is inside the same crate as the render code —
the risk here is a test-only render helper, not a cross-crate one): the card these tests exercise
is `Entry::Permission` rendered inside `chat.rs`'s real entry-match arm (`chat.rs:4694`, in the
same `match entry_index/entry` block as `Entry::ToolCall`/`Entry::SubagentTask`, not inside any
`#[cfg(test)]` module), with `debug_selector`s `permission-option-<id>` (chat.rs:4782/4901) and
`question-answer-input` (chat.rs:3476) — the identical card wave H's own live screenshots showed
rendering for the AskUserQuestion path. This is the production chat surface, not an orphaned
component.

Each test draws that real render tree via `TestAppContext`/`cx.simulate_click` and dispatches a
real click event (per `EVIDENCE-STANDARD.md`'s UI-tier bar: "a named test using `TestAppContext` /
`VisualTestContext` that draws the element and dispatches the real event" — this is exactly that,
not a unit test of a helper function):

- `a_text_answer_leaves_the_surface_and_clears_the_pending_bar` — types free text, clicks Send,
  asserts `pending-question-bar` disappears and the answer is recorded.
- `a_listed_option_leaves_the_surface_and_clears_the_pending_bar` — asserts the text input is
  **absent** when the wire sends structured options, clicks the `permission-option-blue` pill via
  `cx.simulate_click`, asserts `resolved == "Blue"` and the agent's echoed reply lands in the
  transcript.
- `cancel_on_a_question_closes_it_without_an_answer` — cancels and asserts no answer was recorded.

All three arms of the VERIFY clause (text / listed option / cancel) now have fresh, replayable,
real-event-dispatching, production-reachable evidence. **F-CHAT-25 -> PASSED.**

---
