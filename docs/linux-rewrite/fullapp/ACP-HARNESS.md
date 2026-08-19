# ACP harness — real `claude` CLI, live streaming, and Restart agent

Verdict: **the goal's own sentence is now satisfied** — "connette un harness reale via
ACP, manda messaggi, verifica streaming e risposte" — driven against the **real**
`claude` CLI (via `npx -y @agentclientprotocol/claude-agent-acp@latest`, Tiller's
actual default `TILLER_ACP_PROGRAM`), not the `chat_fixture.py` stand-in. The
fixture was never needed: the real CLI worked on the first attempt once the disk
had space.

## Environment for this pass

- The user freed the root filesystem: `df -h /` → `452G 49G 381G 12% /` at the
  start of this pass (was 0 bytes free during the earlier full-app critic pass
  this session).
- Driven with `Scripts/wayland-drive.sh`, own dedicated instance, label
  `acpharness1` (never touched the shared/other agents' instances):
  `TILLER_WL_LABEL=acpharness1 TILLER_WL_BIN=/dev/shm/tt/debug/tiller
  TILLER_WL_KEEP=1`, nested compositor on `wayland-16`, socket
  `/tmp/acpharness1.sock`. This is a pre-existing debug snapshot binary, not a
  fresh build from this branch's current source — the finding is about the ACP
  runtime path (agent process launch, transport, streaming rendering), which
  that snapshot exercises identically.
- The box was under heavy load the whole session (`uptime` showed `load average:
  29.43, 22.40, 18.22` on a 12-thread machine — roughly a dozen other critic
  sway+tiller pairs left running from earlier waves) — the first cold boot of
  this instance took ~53s to present its first frame (normally a few seconds)
  and one `wayland-drive.sh` invocation's 30s control-socket wait timed out
  before the app actually finished starting. This is a harness/host load
  artifact, not an app defect; once up, the app was fully responsive for the
  rest of the pass.
- Project under test: this same repo (`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`),
  worktree `linux/gpui-waku`, its default Chat tab, agent = Claude Code, mode =
  "Opus Plan Mode" / HIGH (the tab's default on this box).

## 1. Does the real `claude` CLI now launch as an ACP agent?

**Yes — confirmed, and the earlier `incoming_transport_closed`-on-`initialize`
diagnosis is confirmed correct** (it was the full disk, not an app bug):

- With the disk fixed, opening the Chat tab and sending `Say exactly: ciao`
  produced a clean reply `ciao` within about a second, no error banner, no
  fallback needed.
- The live process tree under the Tiller process (pid 662063) was the real
  chain: `npm exec @agentclientprotocol/claude-agent-acp@latest` → `sh -c
  'claude-agent-acp'` → a genuine `claude` binary (pid 673666 in the first
  session, later 876649, 922216 in subsequent ones — confirmed via `pstree -p
  662063`), not a stub or mock.
- The composer's Model control populated with real live metadata from the
  agent handshake: `Model  Opus Plan Mode  HIGH`, matching the real CLI's own
  config, not a placeholder.

## 2. Streaming — tokens arriving incrementally, not one final blob

Proven two ways, from weakest to strongest:

**a. Turn lifecycle matches the reference shots.** Right after Return, the
composer flips to `working` status (orange dot) with a stop-square button and
`Type to queue for the next turn…` placeholder — the same state
`reference/shots/16-chat-streaming.png` shows (there captured as "Thinking").
On completion the composer returns to `Auto`/idle with a plain up-arrow send
button and the reply is timestamped — the same state
`reference/shots/17-chat-response.png` shows.

**b. Word-level mid-stream capture — the decisive evidence.** A short reply
("ciao") streams too fast between 1-second-spaced screenshots to show partial
growth, so a slower request was used: `Write the numbers one to two hundred /
List the first 150 positive integers as English words, one per line.` Screenshots
were taken every 0.3s starting immediately after Return. The sequence over one
run (`fine-*.png`, `/tmp/…/scratchpad/acp-harness/`):

- t≈1.8s: transcript ends mid-word — **`...one hundred eleven` / `one hund`**
  (the line is cut off inside the word "hundred", not at a word or line
  boundary).
- t≈2.4s: the same transcript, now continued to **`...one hundred twenty-two`**,
  composer still `working`.
- t≈4.8s: complete, ending exactly at **`one hundred fifty`** (the requested
  count), composer back to `Auto`/idle.

A response rendered from a single final blob cannot end mid-word between two
polls — GPUI is repainting content as ACP `session/update` chunks arrive over
the wire, which is what "streaming" means. This is stronger evidence than the
reference screenshots alone, which only show the before/after state, not the
motion in between.

## 3. Fallback to `chat_fixture.py`?

**Not needed and not used.** The real `claude` CLI worked end-to-end for every
turn sent this pass (short replies, the two long-list requests, and the
post-restart confirmation below). Every quoted transcript excerpt above and
below is from the **real agent**, not the fixture.

## 4. F-CHAT-03 — Restart agent, exercised on a genuinely disconnected agent

Two distinct disconnect shapes were produced and are worth recording
separately, because Tiller's own error classification (`chat.rs`'s
`classify_connection_error`) treats them differently by design — only a
transport-level close (the message contains "transport closed") gets the
`ErrorKind::Disconnected` treatment and the **"Restart agent"** label; a
same-shaped JSON-RPC business error from a bridge that is still technically
alive keeps the generic **"Retry"** label. Both call the identical
`Chat::retry` → `AcpClient::launch` underneath.

- **Killing only the inner `claude` process** (`kill -9` on the leaf pid, npm/sh
  wrapper left alive) produced `prompt failed: Internal error: {"details":
  "Session not found"}` with **Retry / OK** — the wrapper was still holding the
  pipe open and answered with a JSON-RPC error rather than closing the
  transport. Correctly classified as `ErrorKind::Connection`, not
  `Disconnected` — this is the generic-business-error path, working as
  designed, not the row under test.
- **Killing the agent process tree at the moment `New Conversation` spawns it**
  (a tight poll loop killed the fresh `npm exec …` pid — 916211 — 6 iterations
  after the click, i.e. within the `initialize` handshake window) reproduced
  the exact banner this row is about, verbatim:

  ```
  could not launch ACP agent: ACP transport error: Incoming transport closed: {
    "reason": "incoming_transport_closed",
    "method": "initialize"
  }
  ```

  with **Restart agent** / **OK** buttons, `debug_selector`
  `chat-disconnected-banner` / `chat-restart-agent` per `chat.rs:4997-5030`, and
  the composer's status dot showed `offline` with placeholder `Agent offline —
  reconnecting when you send…` (F-CHAT-05's documented state, re-confirmed as a
  side effect).

- **Clicking "Restart agent"**: the error banner disappeared, the transcript
  reset to empty (fresh session), composer returned to `idle`/`Auto`. A PID
  discriminator confirms a genuinely new OS process was spawned, not the same
  one recovering: `pstree -p 662063` before showed no agent children at all
  (killed); afterward showed a brand-new chain `npm exec … (921938)` → `sh
  (922130)` → `claude (922216)` — entirely new PIDs, unrelated to the
  916211 that was killed.
- **Confirmed the reconnected agent actually works**, not just that a process
  exists: sent `Say exactly: restarted-and-working` through the composer;
  status went `working` → the reply arrived verbatim: **`restarted-and-working`**,
  timestamped, composer back to `Auto`/idle.

This closes the loop the brief asked for: the same failure mode that made
F-CHAT-03 necessary (a `claude` process dying before or during ACP `initialize`)
still produces the documented banner and control, and clicking it genuinely
recovers a working agent now that the environment isn't disk-starved.

## Summary table

| Question | Answer | Evidence |
|---|---|---|
| Real `claude` CLI launches via ACP now? | **Yes** | Real `claude` PID under the npm/sh bridge chain; live Model metadata (`Opus Plan Mode HIGH`) populated from its handshake; multiple successful turns |
| Was `incoming_transport_closed` a disk-write failure? | **Confirmed** | No transport errors at all once disk had 381G free, across ~6 fresh agent launches in this pass |
| Streaming is real (incremental), not a final blob? | **Yes** | Mid-poll capture caught a line cut off *inside a word* ("one hund…" → "…one hundred twenty-two" one poll later) |
| Fallback to `chat_fixture.py` needed? | **No** | Real CLI used throughout; fixture never invoked |
| F-CHAT-03 Restart agent control reconnects? | **Yes** | Genuine `incoming_transport_closed`/`initialize` banner reproduced live; Restart agent clicked; new PID chain confirmed; new session answered a real prompt correctly |

## What this does not cover

- This pass used the pre-existing `/dev/shm/tt/debug/tiller` snapshot, not a
  fresh build of the current `linux/gpui-waku` HEAD — if source has changed
  since that snapshot was built, a rebuild-and-repeat would be needed to claim
  this against the exact current tree.
- Only the Claude Code adapter was exercised (the row's own scope). Codex's ACP
  path was not touched this pass.
- The `ErrorKind::Connection`/"Retry" path (dead inner process, wrapper still
  answering) was observed as a side effect but not itself a target of this
  pass; it is recorded above only to explain why it is *not* the same row.
