# P132 — `F-CHAT-05`'s offline-composer clause: reword, don't implement literally

## The row as written

`docs/linux-rewrite/01-inventory-app.md:163`:

> F-CHAT-05 | Disable composer input while waiting for permission or when the agent cannot interact |
> VERIFY: Put the chat into a permission-wait/offline state and confirm the editor is disabled with
> the corresponding placeholder | SRC: App/Chat/ChatComposerView.swift:72

Two prior sweeps (`wave-h/H5-drive-report.md`, `wave-h/H6-instruments-verdicts.md`) already converged
on the same finding and recommendation. This task is the live re-confirmation the `I4-settle` brief
asked for, plus the write-up for a human to accept, rather than a third pass re-deriving the same
result.

## What the code actually does, confirmed live this pass

Fresh scratch project (`/tmp/i4chat05/proj`, plain `git init` + one commit), a genuinely-missing ACP
binary via `TILLER_ACP_PROGRAM=/definitely/missing/mcp-nonexistent-binary` (not a mock — the same env
seam `F-USE-03` used for its own live timeout drive), driven end to end in one `wayland-drive.sh`
invocation, no code changed beforehand:

1. `project.add` + selecting the worktree's Chat tab shows a red banner —
   `could not launch ACP agent: ACP transport error: Internal error: "No such file or directory (os
   error 2)"` — and the composer pill reads `● offline · Claude Code`. The composer itself renders its
   own placeholder, **"Agent offline — reconnecting when you send…"**, matching `chat.rs:6299`
   verbatim. Frame: `/tmp/i4chat05/out/02-01-chat.png.png`.
2. Clicking into the composer and typing lands real text — the field is not disabled, greyed, or
   read-only; the cursor and typed characters render normally. Frame:
   `/tmp/i4chat05/out2/02-01-typed.png.png` shows `my important draft that must not vanish` typed in
   full.
3. Pressing Return while offline does **not** send (there is nothing to send to) and does **not**
   clear the field — it retries the connection, which fails again the same way, adding a **second**
   identical red banner above the transcript. The typed text is still sitting in the composer,
   unchanged, byte for byte. Frame: `/tmp/i4chat05/out2/03-02-after-return.png.png`.

This is a first-hand, live reproduction of exactly what the deterministic test
(`offline_enter_never_discards_the_typed_draft`, added in `wave-h/H5-drive-report.md`) already proved
at the model level: `Chat::send`'s offline branch (`self.client.is_none()`) returns before ever
touching `self.composer`, and none of the four `self.composer = …` write sites in `chat.rs` run on
this path. The live drive adds nothing new to that half — it confirms the same conclusion through the
actual rendered UI instead of only through the model.

## The decision

**Not implementing the literal contract.** Disabling the editor while offline would mean *removing* a
feature the code deliberately has: type now, and a successful `Send` (once the agent reconnects)
retries and auto-resubmits (`start_connection`'s `should_send` path). That is a reasonable, arguably
better UX than blocking input outright, and the one thing that tradeoff must not cost — the user's
typed text — is proven, twice now (once by a deterministic test, once live in this pass), not to be
lost.

**Proposing a wording change instead.** The clause's VERIFY line should read:

> VERIFY: Put the chat into an offline state and confirm a failed offline Send never discards what the
> user typed; separately, confirm a pending-permission state disables the composer with its own
> placeholder (this half is unexercised by any pass so far — the permission-wait state has never been
> live-reached; see `wave-b/B2-chat-verdicts.md`'s F-CHAT-05 row for why).

This keeps both real halves of the row honest: the offline half is proven and passes under the
reworded clause; the permission-wait half (a genuinely different code path from the offline one) stays
open on its own terms rather than being credited by association.

## Why this needs a human, not another builder pass

This is a product-scope call — "should the offline composer stay usable" — not a code defect. A third
builder pass would either (a) implement the literal clause and delete a working, deliberate feature to
satisfy a VERIFY line that was written before this UX existed, or (b) re-confirm the same finding a
third time without authority to change the contract text itself. Recording it here, with fresh live
evidence attached, is the correct terminal state until a human accepts or rejects the reword.

## Files

No `rust/` changes in this pass — the code was already correct and already tested
(`wave-h/H5-drive-report.md`'s commit). This task is the live-confirmation + written case for the
reword the `I4-settle` brief asked for. Screenshots referenced above are scratch (`/tmp/i4chat05/`),
not committed, per the project's own convention for disposable drive captures (see `P131`'s own
`/tmp/winch-trap.sh` precedent) — the frames are reproducible in under a minute from the recipe above
if a human wants to re-look before deciding.
