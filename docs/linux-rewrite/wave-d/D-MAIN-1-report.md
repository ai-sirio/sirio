# D-MAIN-1 report — link 1 of 8 in the D-MAIN chain

Owned files: `rust/crates/tiller/src/main.rs` (plus `tiller_ui/src/browser.rs` and
`tiller_ui/src/chat.rs`, which two of these rows also list). `db.rs` was read but not
edited — the persistence functions it already had (`chat_sessions`, `delete_chat_session`)
were complete; the gap was that nothing called them.

Re-read `main.rs` from disk before every edit, per the brief (it had moved since the brief
was written). Built `cargo build -p tiller` green after every row, committed each row alone
with explicit paths.

## F-AGENT-OMP-03 — blocked, upstream

Reran `oh-my-pi --version` myself: `SyntaxError: Unexpected token ':' at
.../oh-my-pi/bin/oh-my-pi.js:176` — the installed distribution ships a `.js` file containing
TypeScript type annotations (`function checkFile(path: string, label: string)`) and runs it
directly under plain `node`, which cannot parse the type annotation. This is the exact
upstream defect `F-AGENT-OMP-01`/`F-AGENT-OMP-02` are already graded `UNREACHABLE` for.

`omp.rs`'s `summarizer_command` (`oh-my-pi --print --no-tools '<prompt>'`) is unit-tested and
correct — the row's own comment explains the `omp`→`oh-my-pi` executable-name substitution.
There is nothing to fix in `main.rs` or `tiller_agents`: no caller anywhere invokes any
adapter's `summarizer_command` (same gap noted for opencode's F-SET-05, out of this slice), and
even if one did, the upstream binary cannot run to produce inspectable stdout. Not attempted
further — this is a blocked-upstream row, not a code gap.

**howToExercise:** `oh-my-pi --version` on this machine to reproduce the SyntaxError directly;
`cargo test -p tiller_agents omp_summarizer_command_uses_the_distribution_binary_name` proves
the command string itself is correct.

## F-BRW-04 — fixed

`browser.open`'s control-socket handler now calls `normalize_address` on the requested URL
*before* creating the tab, returning `Err("browser.open failed: ...")` instead of silently
falling back to `https://example.com` and reporting `ok:true` (the exact defect P96 caught).

`browser.navigate` now runs a bounded reachability probe (`probe_host_reachable`, new free fn
in `main.rs`) after syntax validation passes: a background thread does DNS resolution +
`TcpStream::connect_timeout` (1.2s) against the target, joined with a 1.5s `recv_timeout` so a
single navigation can never hang the control socket. A dead host now returns
`Err("browser.navigate failed: Could not reach/resolve ...")` and calls
`surface.record_navigation_error` (new `BrowserSurface` method forwarding to
`BrowserState::did_fail_navigation`) so the same error also renders in the visible banner, not
just the socket reply.

**howToExercise:** `ctl browser.open url=https://` → `ok:false`, error mentions "Enter a valid
HTTP or HTTPS address". `ctl browser.navigate url=http://127.0.0.1:9/dead` on an open browser
tab → `ok:false` within ~1.5s, error mentions "Could not reach"; the browser pane's red error
banner shows the same text.

## F-BRW-05 — already-correct, verified live

No code defect found. `browser.act driving=true/false` is (still) the only call site of
`set_agent_driving`, which is correct given this Linux port's architecture: browser automation
is agent-driven entirely through the control socket (no in-process ACP browser tool exists to
call it a second way — confirmed via `grep -rl browser crates/tiller_acp/src/*.rs`, zero
matches).

Live-verified via wayland-drive (see `/tmp/brw05b/04-02-driving.png`, not preserved past this
session): `ctl browser.open` then `ctl browser.act driving=true` renders an "Agent driving"
badge in the browser toolbar next to the Stop control; `driving=false` clears it. Both
directions confirmed by screenshot, not just the socket's `ok:true`.

**howToExercise:** `ctl project.add path=<repo>`, `ctl browser.open url=https://example.com`,
`ctl browser.act driving=true`, screenshot the Browser tab's toolbar — "Agent driving" pill
visible next to Stop; `ctl browser.act driving=false` and re-shot — pill gone.

## F-BRW-07 — fixed (and unblocks F-BRW-06, not in this slice)

`request_permission` had zero production callers (only its own wrapper and two `#[test]`
sites). `browser.navigate`'s handler now checks `surface.state().agent_driving()`: if true and
the target's origin (`scheme://host[:port]`, via new `browser_origin_of` helper) isn't already
in `allowed_origins`, it calls `surface.request_permission(origin)` and returns
`{"permission":"requested","origin":...}` *without* navigating, instead of navigating straight
through. The existing (already fully wired) Allow/Deny doorhanger UI takes it from there — a
click on Allow adds the origin to `allowed_origins`, and a repeat `browser.navigate` call then
proceeds normally.

Live-verified via wayland-drive: `ctl browser.act driving=true` then
`ctl browser.navigate url=https://agent-target.example` returned `permission:requested`; the
doorhanger rendered "Allow agent browser access to https://agent-target.example?" with
Allow/Deny. This closes F-BRW-07's own trigger gap and, as a side effect, gives F-BRW-06's
doorhanger (a separate ledger row, not in this slice) its first production caller too.

**howToExercise:** `ctl browser.open url=https://example.com`, `ctl browser.act driving=true`,
`ctl browser.navigate url=https://some-new-origin.example` → response is
`permission:requested`, not a navigation; screenshot shows the Allow/Deny doorhanger. Click
Allow (its debug selector renders at the doorhanger's Allow button), then the same
`browser.navigate` call proceeds and returns `url`/`title`.

## F-BRW-09 — fixed

A plain click on an HTTP(S) link in the chat transcript previously called `cx.open_url`
unconditionally — always the system browser, no modifier distinction, no internal-tab route at
all. The transcript's `TranscriptSelectableText` mouse-up handler now checks
`event.modifiers.platform && event.modifiers.shift`: that combination still bypasses to the
system browser via `cx.open_url` (this is the "human Cmd+Shift gesture" the row's VERIFY
names); a plain click instead emits `ChatEvent::OpenLink(url)`. `main.rs`'s `bind_chat`
subscription queues that as a new `WorkspaceAction::OpenBrowserLink`, drained by the existing
`update_in` action loop (the same one `NewTab`/`OpenSettings` already use to reach a `Window`
from a `cx.subscribe` callback that doesn't have one) and opened via `add_browser_tab` — Tiller's
own internal browser tab.

`chat.rs`'s render path for non-transcript markdown (`interaction: None`, used outside the chat
transcript) was left untouched — no chat entity is reachable from there to route through, and
the row's VERIFY is specifically about chat.

**howToExercise:** open a chat tab, get an assistant message containing a bare `https://` link
into the transcript (e.g. via a fixture ACP response), click it — a new Browser tab opens
in-app; open another such link with the platform+shift modifier held — no internal tab,
`cx.open_url` fires instead (observable as the browser control's `xdg-open`/portal call, not a
new Tiller tab).

## F-CHAT-34 / F-CHAT-35 — implemented together

Both rows trace to the same root cause the critic found: `chat_sessions()` and
`delete_chat_session()` (already correct in `tiller_persistence::db`) had zero non-test
callers, because no chat tab was ever launched with `ChatPersistence` set —
`launch_with_command_and_persistence` itself had zero callers anywhere in `main.rs`; every
chat tab used the non-persisting `launch`/`launch_with_command`. Turns were never saved, so
there was never anything to browse (this is the same gap `F-PER-01` tracks from the write
side; not this slice, but wiring the launch call fixes both).

Changes:
- `main.rs`'s `add_chat_tab` (single function, both call sites — `NewChatAgent` and
  `NewTabAction::NewChat`) now always launches with persistence:
  `session::database_path()` and `session::persisted_worktree_id(&self.working_directory)`
  (both pre-existing public free functions, already used elsewhere) are threaded through
  `Chat::launch_with_command_and_persistence` (adapter case) or the new
  `Chat::launch_with_persistence` (bare/no-adapter case — mirrors `Chat::launch`'s default
  command but also sets persistence, since `launch()` itself takes no persistence params).
- `ChatPersistence` gained a `worktree_id` field so the history query can be scoped correctly.
- `chat.rs` adds a "Chat History" entry to the existing composer overflow menu
  (`overflow-chat-history`), opening a new popover (`chat-history-menu`) that lists
  `chat_sessions(worktree_id)`: each row has an Open action (`open_chat_history_session` —
  loads the transcript, replaces the tab's live entries, repoints `persistence.tab_id` at the
  opened session so new turns append there) and a Delete action requiring a second confirming
  click (`request_delete_chat_session` → `confirm_delete_chat_session`/
  `cancel_delete_chat_session`, calling `delete_chat_session`). An empty list renders exactly
  "No past chats" (F-CHAT-35), matching the Swift original's string.

Live-verified via wayland-drive: Chat tab → overflow (`...`) → "Chat History" → popover shows
"No past chats" (no turns persisted yet in the fixture worktree). `cargo test -p tiller_ui
chat::` — 59/59 green, including a pre-existing test fixed for the new `ChatEvent::OpenLink`
variant (an irrefutable-pattern match became non-exhaustive).

**howToExercise:** open a chat, complete at least one real turn (so `chat_turn` rows exist —
`chat_sessions`'s SQL inner-joins `chat_turn`), close the tab or open a second chat tab in the
same worktree, then overflow → Chat History: the completed session should be listed with a
title/turn count; click it to reopen its transcript; click Delete then Confirm on a row to
remove it and watch it drop from the list. With zero persisted turns, Chat History shows "No
past chats" instead (this half is exerciseable immediately, no fixture turn needed).

## Foreign files wanted

None. Every fix landed inside the three owned files (`main.rs`, `browser.rs`, `chat.rs`); the
persistence layer (`db.rs`) already had everything F-CHAT-34/35 needed.

## Build/test status

`cargo build -p tiller` green after every commit. `cargo test -p tiller_ui -p
tiller_persistence`: 294 + persistence suite green, no regressions from any row in this slice.
