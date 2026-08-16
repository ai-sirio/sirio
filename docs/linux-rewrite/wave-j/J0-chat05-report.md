# J0-chat05 — `F-CHAT-05` offline composer, carried over from wave I

Row: `Disable composer input while waiting for permission or when the agent cannot interact`
(`docs/linux-rewrite/01-inventory-app.md:163`). Entering this wave the row was
`FAILED — defective`: the rewrite kept the composer **enabled** while offline (a distinct
placeholder plus a silent reconnect-and-resend on Send), and a previous builder had argued this was
a deliberate, better UX than the contract's literal wording (`docs/linux-rewrite/tasks/P132-f-chat-05-offline-composer-reword.md`).
A wave-I critic checked that argument against the Swift reference and found it did not hold — see
`docs/linux-rewrite/wave-i/I4-settle-verdicts.md`'s `F-CHAT-05` section.

## Reference check

Read both cited Swift files at `/home/enzopalmisano/Scrivania/Progetti/tiller` before touching any
code (read-only reference, never edited):

- `App/Chat/ChatComposerView.swift:25-28` — `canInteract` is true only for
  `controller.state ∈ {.ready, .prompting, .detached}` **and** no pending permission; line 92 gates
  the whole editor with `.disabled(!canInteract)`.
- `App/Chat/ChatController.swift:13-19` — `ChatState` includes `.disconnected(message:)`, which is
  **not** in that allow-list, and `start()`'s catch block (line ~360) sets exactly this state on a
  failed agent launch/connect.
- `App/Chat/ChatPaneView.swift:150-160` — `canAcceptDrop` mirrors `canInteract` verbatim (same three
  states, same pending-permission exclusion), so file drops onto the composer follow the same rule.

Confirmed: the reference disables the whole composer while disconnected, with no retry-and-resend
feature anywhere in `ChatController`. The rewrite's divergence was the defect, not the contract's
wording.

## What changed (`rust/crates/tiller_ui/src/chat.rs`)

- Added `Chat::is_offline()` (`self.client.is_none() && !self.connecting`) — the exact condition the
  composer's own offline placeholder already used, now named and reused as the single source of
  truth for "is the editor allowed to accept input".
- `can_send()` now also requires `!is_offline()`, so the Send control disables itself the same way
  it already does for a pending permission.
- `insert_text`, `backspace`, `delete` each gained an `is_offline()` early return, mirroring their
  existing `pending_question()` guard — the whole editor refuses text mutation while disconnected,
  not just Send. None of these guards touch `self.composer`, so a draft already present is never
  discarded by going offline.
- `can_accept_drop()` (F-CHAT-13's file-drop gate, shared with F-CHAT-05 by design) now also excludes
  `is_offline()`, matching `ChatPaneView.canAcceptDrop`'s reference behavior found above.
- `send()`'s old offline branch (`self.retry_pending_send = true; self.start_connection(cx, true);`)
  is removed. Reconnecting is now only ever the transcript error banner's explicit "Retry" control
  (`chat.retry`, unchanged) — never an implicit side effect of a disabled Send. Removed the now-dead
  `retry_pending_send` field and the `start_connection` parameter/`should_send` auto-resend machinery
  that existed solely to serve the removed branch (kept the signature honest rather than leaving an
  unreachable feature sitting in the code).

## Tests

Rewrote the three tests that encoded the old (now-wrong) behavior, in place, keeping their names and
their `F-CHAT-05` framing:

- `offline_composer_shows_its_own_placeholder` — kept its placeholder assertions, added the same
  "type + Enter is refused, placeholder survives" block `permission_wait_disables_...` already used,
  so both halves of the row now assert the *same* thing about the editor: distinct placeholder AND
  actually inert.
- `offline_enter_never_discards_the_typed_draft` — the guarantee itself (a typed draft survives a
  failed offline Return) still had to be locked in, but real keystrokes can no longer produce it: a
  genuinely offline composer now refuses `focus_and_type`. Rewrote it to seed the draft directly via
  `chat.composer.insert_text(...)` (bypassing the now-disabled UI path), modeling the case that
  actually matters — a draft that was already there *before* the connection dropped — then asserts
  the draft is unchanged after a real click + Enter, and that Enter neither sent anything nor started
  a spurious new connection attempt.
- `failed_launch_can_retry_and_complete` — rewrote to drive the explicit `chat.retry` path (what the
  transcript's own "Retry" button calls) instead of an offline Send, asserting Send is disabled while
  offline even with a draft present, that reconnecting via Retry doesn't touch the draft, and that an
  ordinary Send goes through once back online.
- `attach_control_accepts_one_image_and_rejects_the_rest` — unrelated to F-CHAT-05 on its face, but
  its fixture used the same permanently-missing-binary command as a cheap way to avoid needing a real
  agent process. Once offline disables typing, its final "type right after chip removal" assertion
  started exercising the disabled-editor path instead of the chip-removal-returns-focus behavior it
  was meant to prove. Switched its fixture to a real connected agent (`chat_view(cx, &["plain"])` +
  `pump_chat_until(client.is_some())`), which is attach/chip mechanics' actual dependency, not offline
  status.

`cargo test -p tiller_ui --lib chat::` — 63 passed, 0 failed.
`cargo test -p tiller_ui --lib` (whole crate) — 322 passed, 0 failed.
`cargo check --workspace` — clean (two pre-existing, unrelated dead-code warnings only).

## Live drive (pass bar)

`Scripts/wayland-drive.sh`, real binary (`cargo build -p tiller`), `TILLER_ACP_PROGRAM` pointed at
`/definitely/missing/mcp-nonexistent-binary`, fresh scratch git repo at `/tmp/f-chat-05-scratch`.

Route: `ctl project.add path=/tmp/f-chat-05-scratch` (a new project/worktree gets a default `Chat` +
`Terminal` tab pair automatically — the `Chat` tab's agent command is `default_chat_command()`, which
is the one path that honors `TILLER_ACP_PROGRAM`; the tab-bar "+" menu's per-agent items resolve
through `AgentAdapter::acp_program()` instead and do **not** honor the override — worth recording
since it cost real time to find the right open path) → click the `Chat` tab → confirmed offline state
(red `could not launch ACP agent: ACP transport error: Internal error: "No such file or directory (os
error 2)"` banner, status pill `● offline`, placeholder "Agent offline — reconnecting when you
send…").

1. **Refusing input**: clicked the composer, typed the marker
   `critic-owned draft cRiTiC-9f3q must not vanish`, pressed Return. Screenshots before/after are
   pixel-identical (same color count) to the pre-type frame — the placeholder never changed, no text
   landed, no second error banner appeared, `connecting` never flashed true. The editor is inert.
2. **Draft survival**: used `ctl surface.chat.compose surfaceId=default-chat
   text=predates-disconnect-cRiTiC-9f3q` to seed a draft (modeling "typed before the connection
   dropped", the case the guarantee is actually about), then clicked the composer and pressed Return
   for real. `ctl surface.chat.read` before and after: `composerText` is
   `"predates-disconnect-cRiTiC-9f3q"` byte-for-byte in both, `transcript` unchanged (still exactly
   one error entry — no new connection attempt fired). Screenshot confirms the text rendered in the
   (still orange-focus-ringed, still-offline-labeled) composer.
3. **Explicit Retry still works**: clicked the transcript's "Retry" button. `surface.chat.read`
   afterward shows a **second** identical error entry appended (the explicit reconnect attempt ran
   and failed again, as expected against the same missing binary) and `composerText` unchanged — the
   only path back online is intentional and still doesn't touch the draft.

All three match the pass bar: composer refuses input while offline, and a previously-typed draft
stays intact.

## The permission-wait half — checked, still separately unexercised, unchanged reasons

The critic's flag (`docs/linux-rewrite/wave-b/B2-chat-verdicts.md`,
`docs/linux-rewrite/wave-i/I4-settle-report.md`) is that `F-CHAT-05`'s other clause — disabling the
composer for a genuine unresolved permission/plan question — has never been *live*-reached by any
pass, on any installed agent, across many waves. Re-checked the two stated causes directly on this
box rather than taking the old finding on faith:

- `~/.claude/settings.json` still sets `"permissions": {"defaultMode": "auto"}` globally. Claude Code
  auto-approves and never emits an ACP `permission_request` while this is set — confirmed by reading
  the file, not re-derived from an old report. This is the live user's own global config; per
  `AgentAdapter::prepare`'s own contract ("never touches user-global config"), this task does not
  flip it just to force a reachable test, and doing so would affect every other agent session on this
  machine, not just a scratch drive.
- Codex's status is still "logged out" (visible in the app's own status bar in every screenshot this
  pass) — Codex's ACP bridge fails outright on auth before it could ever reach a permission prompt.
- `opencode.acp_program()` is still `None` (`tiller_agents/src/lib.rs`'s own test) and Pi/omp have no
  ACP path at all — none of the three can ever trigger this row's other half.

So the permission-wait half remains **code-verified only**, not live-verified, for the same
unchanged environmental reasons wave-B through wave-I already recorded — this pass adds a direct
recheck of those two facts rather than a new attempt at forcing the condition. The code path itself
(`pending_question()` gating `insert_text`/`backspace`/`delete`/`send`/`can_accept_drop`, plus its own
`permission-wait-placeholder`) is exercised by a real ACP `session/request_permission` event driven
over stdio JSON-RPC by the Python fixture agent in
`permission_wait_disables_the_composer_and_shows_its_own_placeholder` — the strongest evidence
obtainable without an installed agent that will actually prompt. If a future wave adds a way to
either (a) drive Claude Code with a scratch, non-global permission mode, or (b) get Codex
authenticated headlessly, that is the door back into a live drive for this half.

## Verdict

`F-CHAT-05` — offline half: fixed, live-confirmed, tests updated in place. Permission-wait half:
unchanged, code-verified only, environmental blocker rechecked and still present. Recommend the
combined row move from `FAILED — defective` to `half-proven` (offline half now matches the reference
literally and is live-proven; permission-wait half is still test-only) rather than `PASSED` — the
row's contract covers both clauses and only one has live evidence.
