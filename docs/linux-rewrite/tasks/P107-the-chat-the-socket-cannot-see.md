# P107 — the chat the socket drives is not the chat on screen

**Owner: `codex11`.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`. Files: `rust/crates/tiller/src/main.rs`, `rust/crates/tiller/src/session.rs`,
`rust/crates/tiller_ui/src/chat.rs`.

## The finding

`surface.chat.*` drives a chat session that **is never rendered**. Verified live on the Wayland
lane on 2026-08-14, four independent ways:

| drove | socket said | screen showed |
|---|---|---|
| `surface.chat.send text=pwd` | `status: completed`, transcript of 4 entries ending `EndTurn`, assistant answer `` `/home/…/tiller-linux` `` | transcript area **empty**, composer `idle` |
| `surface.chat.compose text=MARKER_ZZ9` | `composerText: "MARKER_ZZ9"` | composer still shows the `Message…` placeholder |
| `surface.chat.compose surfaceId=pane-0` | `error: chat surface is not open: pane-0` | — |
| `panel.list` | the visible tabs are `pane-0` (Chat, active) and `pane-1` (Terminal) | — |

Captures: `/tmp/wl-chatverify/03-chat-after-send.png`, `/tmp/wl-chatdisc/02-composer-marker.png`.

The ACP session is **real** — a live agent genuinely answered `pwd` with this worktree's path. It
is simply answering into a surface no one can see.

## The cause

`main.rs:650`:

```rust
chat_sessions: Arc<Mutex<BTreeMap<String, ChatSession>>>,
```

initialised empty at `:699`, with its own `chat_database_path` and `chat_command`. Every
`surface.chat.*` handler (`:889, :909, :951, :975, :1005, :1025`) opens, composes, sends and reads
against **that** map. Nothing in the path touches the rendered chat view.

The id makes it look correct and is the trap: `surface.chat.open` returns `surfaceId:
"default-chat"`, and `default-chat` is genuinely the persisted id of the visible Chat tab
(`session.rs:322`, `SessionLayout::default_in`). So the socket answers with the right name for the
wrong object. Meanwhile `panel.list` reports the same tab as `pane-0` — a second id space that the
chat API rejects outright.

There are two chat implementations in this binary and the socket owns the one with no pixels.

## Why this is urgent rather than tidy

**It has already produced evidence that looks green and is not.** `P106` routed chat rows to the
control socket on my instruction, and every `F-CHAT` row driven that way proves the control API
works — not the UI. Those rows are corrected in `P106-report.md`; do not let the pattern repeat.

Second, `surface.chat.send` **spawns an agent process** against `chat_command`. Anything that
drives chat over the socket is silently starting a second agent beside the one the user is talking
to, with its own database.

## What to decide, and say out loud in the commit message

Two defensible designs. Pick one and argue it in one paragraph — **do not pick silently.**

1. **The socket addresses the rendered surface.** `surface.chat.*` resolves `surfaceId` to the live
   chat view and drives it, so `compose` fills the visible composer and `send` appears in the
   visible transcript. This is what every caller already believes happens. It is the larger change:
   the handlers run off the UI thread and the view is `@MainActor`-equivalent in GPUI terms, so it
   needs whatever the codebase's existing cross-thread route into the view is — find it, do not
   invent one.
2. **The two are deliberately separate, and the API says so.** Keep the headless session, but stop
   it impersonating the visible tab: give it its own id space (not `default-chat`), and make
   `surface.chat.*` reject or explicitly document the visible-tab ids. Cheaper and honest, but it
   leaves the socket unable to drive the real chat, which forfeits the whole `F-CHAT` family to the
   X drive lock forever.

**I recommend (1)** — the reason the socket exists is to drive the app, and an API that drives an
invisible replica of the app is worse than no API, because it manufactures false evidence. Take (2)
only if you find a concrete reason (1) cannot work, and say what it is.

## How to prove it

Not with a unit test. The bug is precisely that the tested layer works.

- `Scripts/wayland-drive.sh` — `ctl surface.chat.compose surfaceId=<id> text=MARKER`, then `shot`,
  and the marker must be legible in the composer in the capture.
- Then `ctl surface.chat.send`, and the user message, the tool call and the assistant reply must
  appear in the rendered transcript.
- Read `WAYLAND-LANE.md` first. `open` does not focus — `tab.select index=1` (1-based, string
  params) brings the Chat tab forward, and without it you will photograph the Terminal and
  conclude nothing rendered.

Attach both captures to the commit.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.**
- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done.
- If the fix turns out to be large, land it in reviewable pieces rather than one commit, and say in
  each what is proven and what is still owed.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
