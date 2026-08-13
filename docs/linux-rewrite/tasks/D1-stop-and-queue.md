# D1 — Stop and queue: the two behaviours that make a turn yours

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
It is the first brief in the design series (`D1`–`D6`), written by `fable` under the design mandate
in `05-the-design-of-the-program.md`. If anything here contradicts the current code or ledger, the
code and ledger win — read them first, then this.

## The test that earns this piece

`escape_cancels_the_stream_and_the_transcript_states_it` in `chat.rs` is the best test in the
crate: it drives a real streamed turn from a scripted agent, cancels it mid-stream through the real
key path, and asserts the transcript **states the cancellation instead of impersonating a normal
completion** — the exact failure this codebase has produced five times, refused in advance. This
piece extends the machinery that test already proves.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh   # strict; a warning you introduce is a red gate for everyone
```

**`tiller_ui/**` and `tiller_theme/**` are yours.** codex12 is in `tiller/src/main.rs` and
`tiller_control/**`; codex11 owns the rest. Your notes: `PI-HANDOFF.md`.

## The piece: `D-CHAT-02` + `D-CHAT-03` — F-CHAT-07 and F-CHAT-06

These are J1's spine (`DESIGN-LEDGER.md`) and the first two behaviours
`00-ui-observed-from-screenshots.md` lists. Neither exists. Today the send control
(`debug_selector "send"`, the 26px circle) always draws `↑`; while `streaming` it greys out and
**loses its click handler entirely** — `.when(can_send, on_click(send))` and nothing else. Escape
cancels (`cancel_turn` → `client.cancel()`), but the visible control is inert exactly when a user
most wants it.

**1. The send control becomes stop while a turn runs (`D-CHAT-02`, F-CHAT-07's click half).**
While `streaming`, the same control draws a stop state (a filled square glyph; use theme tokens —
the colour is appearance-debt, the *state and behaviour* are the piece) and its click dispatches
the same path Escape uses. When the turn ends it is the send control again. Keep the `"send"`
selector stable; tests target the control, not the glyph.

**2. Typing during a run queues (`D-CHAT-03`, F-CHAT-06).** While `streaming`:

- the composer stays editable and its placeholder becomes `Type to queue for the next turn…`;
- **Enter commits the text as the queued item** — one slot, latest commit wins — shown as a visible
  queued row/chip with its text and a ✕ that removes it (F-CHAT-06's VERIFY: "shows the queued
  item");
- when the turn ends — **completed or cancelled alike, one rule** — a committed queued item sends
  exactly once as a normal turn. Queue-then-stop is the redirect gesture; it must work.
- Uncommitted composer text at turn end stays in the composer and does **not** auto-send: a user
  mid-sentence when the turn ends must not have half a message fired for them.

**Opportunistic, only if the same state machinery makes it nearly free:** F-CHAT-05, the composer
disabled with its own placeholder during a permission-wait. If it is not nearly free, leave it and
say so.

## Evidence

Drawn tests, using the machinery already in `chat.rs` — `chat_view(cx, &[…])` scripted agent,
`pump_chat_until`, `simulate_keystrokes`, real clicks at debug bounds, **every test hardened with
the full `run_until_parked()` pump** (you established that an unhardened one is not evidence):

- pump until `streaming`, then a **real click on `"send"`** → `!streaming` and the turn footer
  states the cancellation;
- type + Enter during a stream → queued item visible, transcript unchanged; turn ends → the queued
  text arrives as the next user turn, exactly once;
- ✕ on the queued item → nothing sends when the turn ends;
- stop via click, with a queued item present → the queued item still sends.

**A ledger correction to report:** `F-CHAT-08` reads `PASSED — drawn connecting/send/stop states`,
but no stop state exists in `chat.rs` — the glyph is hardcoded `↑`. Say this in your report so the
critic re-judges that row; do not edit its verdict yourself.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku is the visual reference; take the ideas, write the lines.
- Update `INVENTORY-LEDGER.md` rows F-CHAT-06/07 and the `D-CHAT-02`/`D-CHAT-03` rows in
  `DESIGN-LEDGER.md` as **`builder-claimed, unverified` — never `PASSED`**. Only the critic closes.
- Keep the gate green. Proceed without asking for design approval; this brief is the design.

## Reporting

Reply in **12 lines or fewer**: what the control does in each state, what happens to a queued item
on stop / on completion / on ✕, the tests by name, the F-CHAT-08 discrepancy restated, the gate
result, and the honest remainder.

## Addendum — after the audit landed (fable, FABLE-04, 2026-08-13 ~19:20)

Written after this brief was executed; nothing above changes. Two records:

- **F-CHAT-08 — the correction is applied; do not re-report it.** Pass 12 re-marked the row
  `FAILED — absent`: the UI control states remain unbuilt, and P53's socket transcript proves stop
  at the door, which is a different claim. The D1 builder claim on F-CHAT-07 is the build that
  closes it; the critic verifies.
- **F-CHAT-02 (auth banner) — out of D1's scope; a separate row.** The audit overturned it
  (pass 12): no auth state exists anywhere — `tiller_acp` surfaces none (case-insensitive `auth`,
  zero matches) and the ACP `initialize` `authMethods` response is ignored. The UI half is now
  genuinely small: `Entry::Error` carries a `kind` (`ErrorKind`, chat.rs:117 — today `Connection`
  only), so the banner is one `Auth` variant, a CLI-guidance line, and the existing Retry. But the
  trigger must originate in `tiller_acp`'s session lifecycle, which is not this brief's machinery
  or pi's crate. **Decision: an acp-side piece surfaces auth-required first; the banner rides
  whichever D-piece then owns that ground.** Until then the generic connection banner is not auth
  cover — do not cite it as such.
