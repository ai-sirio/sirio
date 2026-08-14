# Verdicts — E04-chat (F-CHAT)

Adjudicated against `docs/linux-rewrite/sweep/E04-chat-evidence.md` and its captures under
`reference/linux-progress/drive-E04-chat/`. I did not drive this slice and did not build any of
the surfaces it touches — no source read outside `grep`/targeted line reads, no compile, no edit
under `rust/`.

All thirteen captures cited by the driver were opened and inspected directly (not taken on the
driver's word), plus tab-bar crops (`convert -crop`) of every one of them. Two of three rows
overclaim, for the same underlying reason: the driver ran "measurement-only, no vision" and never
noticed that **every single capture in this drive — `02-chat-open.png` through
`04-after-click-1190-895.png` — shows the `Terminal` tab foregrounded (bold, orange top-border),
not `Chat`** (which sits in the background the whole time, showing only an unread-activity dot,
later a checkmark). `tab.select index=2` did not land on the Chat surface. The
ImageMagick stddev band the driver measured is real pixel data, but it is Terminal scrollback
noise (garbled, overlapping shell-prompt segments — itself a rendering artifact, out of scope for
this slice), not the chat pending-question bar. This is caught here, not upstream, because the
driver had no vision this pass and could not have caught it themselves.

The `surface.chat.read`/`surface.chat.stop` control-socket JSON quoted in the evidence doc is a
different kind of evidence — not screenshot-dependent — and does hold up: I independently read
`chat.rs` and confirmed `control_entry_row` (chat.rs:5448-5449) mirrors the exact `expired`/
`resolved`/`dismissed` fields the render arms at chat.rs:3839-3846 and `pending_question()`
(chat.rs:1490) branch on, and that `expire_unanswered()` is genuinely called from the
`TurnEnded`/`TransportError`/`Timeout` arms (chat.rs:~1479-1512) before the `TurnFooter` push. So
the backend state machine was genuinely driven live through the row's trigger conditions — that
part of the evidence is real and worth keeping. What it does not do is show the UI actually
painted the text/bar on screen, which is what both rows' VERIFY clauses ask for.

---

## F-CHAT-26 — pending-question bar + Show jump

**Verdict: half-proven** (unchanged from NOT EXERCISED → half-proven, but the proven half is
narrower than the driver's "partially-exercised" claim).

The driver's own split was "state half confirmed live (screenshot stddev), gesture half not." On
inspection the state half is not what the screenshots show. Tab-bar crops of `02-chat-open.png`
(negative control), `02-pending-state.png`, and `02-pending2.png` all show `Terminal` bold/active
and `Chat` merely carrying an unread dot in the background — the 900×180+400+750 band measured
sits inside the Terminal pane's garbled scrollback, not any chat composer. The stddev delta (0 vs
60.9/49.2) is real but does not discriminate for "the pending bar renders": it tracks how much
Terminal noise had accumulated by capture time, which happens to correlate with elapsed session
time, not with `pending_question()`'s state. No capture in this drive ever shows the `Chat` tab
foregrounded, so there is zero visual evidence the pending bar or its "Show" control exist on
screen, or that the two blind clicks (`03-after-click-1150-850.png`, `04-after-click-1190-895.png`
— also both Terminal-foregrounded) landed on it.

What does hold up: `surface.chat.read` returning a genuine `{"id":"1","kind":"permission",
"status":"pending"}` row from a live ACP turn is real, live evidence — `control_entry_row`
(chat.rs:5448-5449) only emits `"pending"` when `resolved.is_none() && !expired && !dismissed`,
which is exactly the `Entry::Permission{resolved:None,expired:false}` condition `pending_question()`
(chat.rs:1490) matches. That is more than source-reading: the driver's real ACP prompt genuinely
produced that data-model state twice, independently, in the running app. This is the half that's
proven — the backend trigger fires live. The bar's visual rendering and the Show-jump gesture are
both still owed, and prior X11 evidence (ledger F-CHAT-23) already found Show/body-click/Stop all
no-op on this same bar in a real click test, so the missing half is not a formality.

Captures used: `02-chat-open.png`, `02-pending-state.png`, `02-pending2.png` (all
non-discriminating for the visual-bar claim — wrong tab foregrounded in every one, confirmed by
tab-bar crop). No capture is discriminating for the Show-jump gesture either.

---

## F-CHAT-27 — expired unanswered-question state ("No answer — the turn ended")

**Verdict: half-proven** (downgraded from the driver's claimed "exercised-working").

The driver's `surface.chat.stop` → `surface.chat.read` sequence is real, live evidence for the
backend transition: the permission entry's `status` field flipped `"pending"` → `"expired"` and a
`TurnFooter` "cancelled" row was appended. Cross-checked directly in source: `control_entry_row`
sets `status: "expired"` exactly when the `Entry::Permission.expired` flag is true, which is the
same flag the render arm at chat.rs:3839-3846 branches on for the "No answer — the turn ended"
text, and `expire_unanswered()` is genuinely invoked from the turn-ending event arms before the
footer push. That data-level mechanism is real and independently confirmed against the current
HEAD's source, not just repeated from the driver's prose.

The claimed screenshot corroboration does not hold, though. `03-after-stop.png` is described as
"the post-expiry frame" — its tab-bar crop shows `Terminal` bold/active and `Chat` carrying a
checkmark in the background, the same pattern as every other capture in this drive. It is a
screenshot of the Terminal pane's garbled scrollback, not of the transcript card. No capture in
this drive shows the words "No answer — the turn ended" on screen, or any chat surface content at
all. "13366 colours, non-blank" is true of the image but is not evidence about the chat UI it was
presented as evidence for.

Net: the state transition (and its exact mirror in the render condition, confirmed via source)
is proven live; the visual half the row's VERIFY clause actually asks for — the card's text
actually appearing on screen — was never observed. That is short of "exercised-working."

Captures used: `03-after-stop.png` (non-discriminating for the visual claim — wrong tab
foregrounded, confirmed by tab-bar crop).

---

## F-CHAT-33 — MCP-configuration-warning half

**Verdict: half-proven** (unchanged — turn-error half still stands from pass 17; MCP-warning half
still owed, now reconfirmed as code-absent rather than merely untriggered).

Independently reconfirmed against `4073297` rather than taken on the driver's word:
`enum ErrorKind { Connection, }` (chat.rs:510-512) has exactly one variant; the only `mcp` hit
(case-insensitive) in `chat.rs` is an unrelated doc comment about tool-output truncation
(`grep -in mcp rust/crates/tiller_ui/src/chat.rs`); and `grep -rln mcp rust/crates --include=*.rs
-i | grep -v test` matches only that same file, that same line — no other crate mentions MCP
outside tests. All twelve `Entry::Error` construction sites in the file were enumerated and every
one that specifies a kind uses `ErrorKind::Connection`. This is a read-only, no-compile
confirmation that the row's MCP-configuration-warning half has no render arm to reach in the
current binary — not a coverage gap a better drive could close, an absent affordance. The
turn-error half's live proof (pass 17, killing the ACP subtree mid-stream) is untouched by this
pass and stands as recorded.

No captures for this row (read-only investigation, as the row itself calls for when no MCP server
is configured to misconfigure).
