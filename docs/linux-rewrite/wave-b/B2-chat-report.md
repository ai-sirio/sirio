# B2-chat build report

Slice: B2-chat (6 rows). Owned file: `rust/crates/tiller_ui/src/chat.rs`. All work committed to
`linux/gpui-waku` in three commits, path-scoped to `chat.rs`:

- `82d4ec5` — F-CHAT-05
- `7c8c787` — F-CHAT-12, F-CHAT-13, F-CHAT-14
- `ce3c13e` — F-CHAT-16

`cargo check -p tiller_ui` is clean; `cargo test -p tiller_ui --lib` is green: 284 passed, 0
failed (up from 264 baseline — 20 new/extended assertions across the four built rows).

## Index-contention note for the orchestrator

Commit `7c8c787` briefly and accidentally swept in two files staged by sibling agents at the
same moment (`docs/linux-rewrite/wave-b/B3-sidebar-report.md`, new; `rust/crates/tiller_terminal/src/lib.rs`,
modified) — `git commit -m` with no pathspec commits the *entire* index, not just what I had
just `git add`ed. No content was lost (working trees were untouched, nothing was reset), but
those two files now carry my commit message/authorship instead of B3's own. I caught this
immediately after and switched to `git commit -m "..." -- <path>` (pathspec form, which commits
only that path's staged changes and leaves everything else staged) for the remaining two
commits, both of which are clean single-file commits. Flagging so B3's owner isn't confused by
where their file landed.

## Rows

### F-CHAT-05 — composer disable during permission-wait — **built**

`can_send()` returned false during a pending permission only incidentally, because
`streaming` happened to also be true — it never named `pending_question()` as its own reason.
The composer placeholder was identical during permission-wait and ordinary mid-turn queueing
("Type to queue for the next turn…"), so the UI never actually distinguished the two states the
clause asks for, and — unlike the Swift original's `.disabled(!canInteract)` — typing still
silently queued characters instead of being refused outright.

Built to match the Swift original exactly:
- `can_send()` now explicitly checks `pending_question().is_none()`.
- `send()`, `insert_text()`, `backspace()`, `delete()` all return early while
  `pending_question().is_some()` — the whole editor is out of service, not just Send.
- A new placeholder, `permission-wait-placeholder` / "Waiting for permission response…",
  shown instead of `queue-placeholder` while a question is pending.

**Test**: `permission_wait_disables_the_composer_and_shows_its_own_placeholder` (new) — drives a
real unresolved `Entry::Permission` via the existing `"permission"` Python fixture agent,
asserts the new placeholder is drawn instead of `queue-placeholder`, then attempts to type and
press Enter and asserts the composer stays empty and no new entry is sent.

**How to exercise live**: open a chat pane on an agent whose CLI raises a genuine ACP
`permission_request` mid-turn (this repo's own dev environment's installed Claude Code CLI
auto-applies edits instead of raising one — a non-allow-listed Bash call or a different agent
CLI is more likely to surface it). While the permission card is unresolved: the composer shows
"Waiting for permission response…", typing does nothing, and the Send/queue control stays
disabled. Answering the permission card (Allow/Deny) should immediately restore normal typing.

### F-CHAT-12 — chip removal leaves the composer keyboard-dead — **built**

`remove_composer_chip` mutated the model correctly but never re-requested focus, so the
composer's own `on_mouse_down`-based focus-grab (which only fires on a *new* mouse-down into the
composer container) never ran again after a chip's `×` was clicked — the field went keyboard-dead
until the user clicked back into it.

Built: the chip-remove `on_click` handler now calls `chat.composer_focus.focus(window, cx)`
immediately after `remove_composer_chip`.

**Test**: extended the existing `attach_control_accepts_one_image_and_rejects_the_rest` (already
tagged F-CHAT-12) with a post-removal assertion — `cx.simulate_input("still here")` with *no*
intervening click back into the composer, then asserts the text landed in the draft.

**How to exercise live**: attach an image (the `+` control), then click the chip's `×` to remove
it. Immediately — without clicking the composer field again — type a character. It must appear
in the composer; before this fix it silently went nowhere.

### F-CHAT-13 — no file-drop target existed — **built**

`grep -rn ExternalPaths crates/` was genuinely empty repo-wide; `chat.rs` had zero `.on_drop` of
any kind (structural absence, not an unreached instrument wall, per the ledger's own
reclassification).

Built, following the Swift original's `ChatPaneView.onDrop` + `FileDrop.classify` +
`ComposerDropApplier.apply` exactly:
- `chat-root` now chains `.on_drop(cx.listener(Self::drop_external_paths))`, gated behind a new
  `can_accept_drop()` (identical rule to F-CHAT-05's guard — a drop must not silently
  accumulate chips while the composer itself refuses input).
- An invisible `"Drop files to attach"` overlay (`chat-drop-overlay`), revealed by gpui's own
  `drag_over::<ExternalPaths>` style refinement while an external drag is present over the pane
  — same mechanism Zed's own `agent_panel.rs::render_drag_target` uses.
- `drop_external_paths`: recognized image extensions (png/jpg/jpeg/gif/webp — the Swift drop
  path's *wider* list, deliberately different from the "+" picker's stricter png/jpeg-only) become
  attachment chips; anything else becomes a `@`-style file chip with a path relative to
  `agent_cwd` when inside it, absolute otherwise; an oversized image (>10MB, matching Swift's
  `FileDrop.maxImageBytes`) is rejected through the existing `attach_error` transient-message
  path, one message per rejected item in a multi-file drop.

**Tests**: `dropping_external_files_attaches_chips_and_rejects_the_oversized_one` (new) — drops
a valid PNG, a valid non-image file, and an 11MB PNG in one batch via gpui's real
`FileDropEvent::Entered`/`Submit` test events; asserts both valid chips land, the oversized one
is rejected with a visible `attach-error`, and the file chip's path is worktree-relative.
`dropping_external_files_is_refused_during_permission_wait` (new) — same drop sequence during an
unresolved permission; asserts no chip and no draft change (chat-root never chained `.on_drop`
that render, so gpui has nothing registered to call).

**How to exercise live**: drag a PNG and a `.txt` file from the desktop file manager onto the
chat pane and drop them together. Expect: a dashed/tinted "Drop files to attach" overlay while
hovering, then one image chip + one file chip in the composer after the drop. Drag a >10MB image
alone: expect a transient "… is too large (max 10 MB)" message and no chip. Attempt the same
drag while a permission card is pending: nothing should happen.

### F-CHAT-14 — Follow Edited Files does nothing — **built**

Confirmed the ledger's finding that the bool was read by nothing — but the fix it points at
(`FileSystemEventMonitor`, the inotify watcher) is the **wrong mechanism**: `DEAD-MODULES.md`
already corrected this — that watcher is the built half of a different row, `F-CORE-FILE-06`
("editor auto-reloads external changes"), unrelated to chat. The Swift original's "Follow"
doesn't watch the filesystem at all; it watches ACP tool-call *locations*
(`ChatController.handle`: `if isFollowing, let location = update.toolCallLocations.last, … {
onFollowLocation?(location.path) }`), throttled 500ms, and the callback is externally wired by
the host to open a file tab.

Built the Rust equivalent entirely inside `chat.rs`, with no foreign-file change needed at all:
`chat.rs` already has `ChatEvent::OpenFile(PathBuf)`, already emitted by F-CHAT-32's edit-summary
"open" link, and already consumed live by the host (`Workspace::bind_chat` in
`rust/crates/tiller/src/main.rs`, which calls `workspace.add_file_tab(path, cx)` — read to
confirm the seam is real and live, not edited). `maybe_follow_location` is called from all three
tool-call event handlers (`ToolCallStarted`/`Updated`/`Completed`) and, when
`following_edited_files` is on, emits `ChatEvent::OpenFile` for the most recent reported
location, throttled 500ms via a new `last_follow_at: Option<Instant>` field — reusing the
already-live seam rather than inventing new host wiring.

**Test**: `following_edited_files_opens_the_tool_calls_reported_location` (new) — drives
`handle_event(AcpEvent::ToolCallStarted/Updated { locations, .. })` directly, subscribes to the
chat entity's `ChatEvent`s, and asserts: no event while the toggle is off; exactly one
`ChatEvent::OpenFile` with the reported path once it's on; a second location inside the 500ms
throttle window does not re-fire.

**How to exercise live**: open the composer overflow menu, toggle Follow Edited Files on, then
ask the agent to edit a file. The edited file should open in a tab shortly after the tool call
reports its location — the same way clicking a path in an edit-summary card already does.

### F-CHAT-16 — model picker missing search + Recommended — **built**

Re-read the whole render arm as the manifest asked: the model list and the effort section
(F-CHAT-17, already PASSED) both already existed in the shared `model-picker` popover — the
ledger's "opens an Effort selector, not a model picker — no search, no list" evidence was stale,
predating that list. The real, current gap was exactly what the approach named: no search field,
no "Recommended" designation, no no-match state.

Checked `tiller_acp::ModelOption`/`ModelCatalog` first, per the approach's instruction — there is
no recommended flag, and per the Swift original (`ModelPickerPopover.swift`) there never needs to
be one: `"Recommended"` is purely `models.first?.modelId`, the driver's own list order. No ACP
change needed.

Built entirely in `chat.rs`:
- `model_query_matches` ports `ModelPickerFilter.filter`'s exact semantics (trimmed, lowercased,
  substring match against name/id/description, order-preserving, empty query keeps everything).
- A new `model_search: String` field, reset each time the picker opens; a `model-search-input`
  row rendered above the model list with a placeholder when empty.
- The model list now iterates the *filtered* set; the row matching `available_models.first()`'s
  id gets a `model-option-recommended` "Recommended" badge — gone from the list the moment its
  model is filtered out, matching Swift.
- A `model-picker-no-match` "No models match" state when the catalog is non-empty but the filter
  yields nothing (kept the existing, more specific "did not report any models" message for a
  genuinely empty catalog).
- Typing routes to `model_search` via a new `model_picker_open` branch in `on_composer_key`
  (mirroring the existing `question_answer.for_request` convention right above it). Backspace,
  Send (Enter), and `insert_text`'s Shift+Enter path needed their own explicit
  `model_picker_open` guards too, since those three are bound GPUI *actions* on `chat-root`'s
  "ChatComposer" key context — the model picker inherits that context (it has no override of its
  own for Backspace/Send) — dispatched *before* `on_composer_key`'s raw fallback ever sees the
  keystroke. Without the guards, Backspace while correcting a search typo would have silently
  eaten a character from the composer's draft instead, and Enter would have sent the composer's
  message outright.

**Test**: `model_picker_search_filters_and_badges_the_recommended_model` (new) — three
distinctly-named models; opens the picker; asserts the search input and the Recommended badge on
the first model are drawn; types `"son"` via real keystrokes and asserts only the matching model
row (and its own badge, now correctly absent since the recommended model got filtered out) stays
visible; types a non-matching query and asserts the "No models match" state; presses Backspace
repeatedly and asserts every model reappears *and* that the composer's own draft (a completely
separate piece of state) was never touched.

**How to exercise live**: complete one turn with an agent that reports multiple models, click the
model chip, and confirm a search field is drawn above the list with the first model badged
"Recommended". Type a substring of one model's name — the list should narrow to just that model
(losing its badge if it was the recommended one); type gibberish — "No models match" should
appear instead of an empty list; Backspace should correct the query without ever touching the
message you were composing underneath.

### F-CHAT-18 — context popover usage breakdown — **not-built, needs a foreign file**

Re-verified the manifest's own residual-gap framing (not the stale "click opens nothing" half,
which is genuinely fixed): read the whole `context_popover` render arm in `chat.rs` — it already
shows percent, `used / size` tokens, and cost when present. The clause's "full input/output/
cache/cost breakdown" is not renderable because `tiller_acp::ContextUsage` (line ~210,
`rust/crates/tiller_acp/src/lib.rs`) carries only `used`, `size`, `cost` — no
input/output/cache-token fields exist anywhere in the ACP layer to render, confirming the
ledger's "structurally impossible" verdict rather than contradicting it.

Traced this further than the ledger did, for whoever picks it up: the underlying wire type
Tiller's Rust ACP layer parses, `agent_client_protocol::schema::v1::client::UsageUpdate` (from
the `agent-client-protocol-schema` crate, v1.5.0, a published dependency — not this repo's own
code), itself only has `used: u64`, `size: u64`, `cost: Option<Cost>`, and an untyped `_meta:
Option<Meta>` bag — genuinely no typed token-breakdown fields at this protocol version. Read the
Swift original's equivalent for exact parity and found the breakdown does **not** come through
the generic ACP bridge at all: `Packages/TillerACP/Sources/TillerACP/Drivers/
ClaudeStreamJSONDriver.swift` taps the Claude Code CLI's own `--output-format stream-json` raw
output directly — a Claude-specific side channel entirely separate from the generic
`agent-client-protocol` JSON-RPC subprocess bridge — to read `usage.input_tokens`,
`usage.output_tokens`, `usage.cache_creation_input_tokens`, `usage.cache_read_input_tokens`, and
`result.total_cost_usd`. Nothing resembling that side channel exists anywhere in
`tiller_acp` today. Building it is a materially bigger undertaking than the row's own "size S"
suggests — a new Claude-specific raw-stream parser and a second data path alongside the existing
ACP bridge, not a field addition — and it lives entirely outside `chat.rs`.

**wantedForeignFiles**: `rust/crates/tiller_acp/src/lib.rs` (plus, realistically, a new sibling
module for the Claude stream-json side channel, mirroring
`ClaudeStreamJSONDriver.swift`/`ClaudeWire.swift`'s split). The `chat.rs` half — a breakdown line
in the popover, `"Input: N · Output: M · Cache write: W · Cache read: R"` when
`input_tokens`/`output_tokens` are `Some`, matching `ComposerControlBar.contextUsageDetail`'s
Swift logic exactly — is a five-minute follow-up once `ContextUsage` actually carries the fields,
and I did not touch `chat.rs` for this row since there is nothing in it to build against
non-existent struct fields.
