# P114 report — chat-local copy and edit-summary rows

## Warning cleanup

`cargo test -p tiller` no longer emits `private_interfaces` for
`Chat::transcript_from_entries`: the helper is now private, matching its
private `Entry` argument. The only remaining `tiller_ui` warning is the
pre-existing unused `BrowserSurface::pump_task` field in `browser.rs`.

## F-CHAT-29 — assistant-response Copy

Built in `398c0ae` (`feat(chat): copy assistant responses`). Assistant rows
form a hover group with a local Copy control. Clicking writes only that
response to the clipboard and changes that same control to `Copied ✓` for two
seconds.

- Drawn test: `assistant_response_copy_writes_text_and_confirms` moves over
  the real assistant row, clicks its control, asserts clipboard text, then
  asserts `assistant-copy-confirmed-0`.
- Capture: `reference/linux-progress/p114-chat/02-composer.png` confirms the
  socket drives the visible chat composer in this build; it does **not** show
  an assistant row with the new control.
- Owed: gesture — live hover and Copy click against an ACP-rendered assistant
  response, including an external paste check and captured `Copied ✓`.

The copy and acknowledgement conjuncts are separately covered by the drawn
test, but neither is live-exercised in the capture above.

## F-CHAT-30 — code-block Copy

Built in `0ecbd52` (`feat(chat): copy code blocks`). Every assistant-rendered
fenced code block has its own Copy control. It writes raw code (no fence or
language label) and changes only that block's control to `Copied ✓` for two
seconds.

- Drawn test: `code_block_copy_writes_code_and_confirms` clicks the rendered
  `code-block-copy-0-assistant-block-0`, asserts the exact clipboard value,
  then asserts its confirmation selector.
- Capture: no code-block control was captured. The available socket capture
  only reached the visible composer (`reference/linux-progress/p114-chat/02-composer.png`).
- Owed: gesture — live code-block Copy click, external paste check, and a
  capture showing the code-block confirmation state.

The raw-code copy and confirmation conjuncts are separately covered by the
drawn test; both remain live-unexercised.

## F-CHAT-32 — edit summary

Built in `c23da36` (`feat(chat): add edit summaries`). A tool call carrying
ACP diffs renders an `N files changed` card. File-name actions route Open to
the workspace's existing editor-tab path. Revert first becomes Confirm Revert;
the existing git discard operation then marks the path `reverted` or renders
the actual error.

- Drawn test: `edit_summary_opens_and_reports_revert_success_or_error` uses a
  temporary git repo, clicks Open and observes `ChatEvent::OpenFile`, confirms
  a real git revert and observes `reverted`, then confirms a stale file and
  observes the rendered error state.
- Capture: none. A live ACP output containing a diff/edit summary was not
  obtained in this pass.
- Owed: agent-output trigger plus visible Open and confirmed Revert gestures,
  photographed against a real ACP diff.

## F-CHAT-35 — no past chats

Not built. The required empty state depends on a user-openable Chat History
surface, which is absent: `F-CHAT-34`'s browser/open/delete UI has no
production surface. The nearest existing behavior hides Resume Chat when no
retained chat exists; attaching `No past chats` to that hidden path would not
make the state reachable. Dependency: **F-CHAT-34 Chat History surface**.

## Verification

Ran:

```text
cargo test -p tiller
cargo test -p tiller_ui assistant_response_copy_writes_text_and_confirms -- --nocapture
cargo test -p tiller_ui code_block_copy_writes_code_and_confirms -- --nocapture
cargo test -p tiller_ui edit_summary_opens_and_reports_revert_success_or_error -- --nocapture
```

All focused chat tests passed. `cargo test -p tiller` completed without the
new private-interface warning.
