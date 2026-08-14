# Critic verdicts — W13-git+per+persist+win (F-GIT-RUN-02, F-PER-08, F-PERSIST-DB-11, F-WIN-06)

Adjudicated by a critic that neither built nor drove this slice. Driver return:
`docs/linux-rewrite/wave-a/W13-git+per+persist+win-evidence.md`, captures under
`reference/linux-progress/wavea-W13-git+per+persist+win/`. HEAD under test: `4073297`.

## F-GIT-RUN-02 (ledger line 484) — verdict: `PASSED` (changed from `NOT EXERCISED`)

Opened every capture in the slice's directory in mtime order (the filename prefixes are not
monotonic with time — `03-07-clone-form.png` and `06-10-clone-after.png` are *earlier* in wall
time than `02-11-clone-form.png`). Confirmed the discriminating claim directly:

- `03-02-form-open.png` -> `04-03-typed.png` -> `05-04-after-return.png`: a real "New worktree in
  w13-testrepo" prompt, `w13-newbranch` typed into it, and a genuine new worktree row appearing
  after `key Return` — the fastest, cleanest proof Return delivery works on this lane.
- The Clone-repository dialog sequence took several failed rounds first, not one clean pass as the
  evidence's prose implies (see disagreement below) — but the *final* round is unambiguous:
  `04-25-url-typed.png` shows `/tmp/w13-testrepo` actually typed into the Repository URL field
  (Destination correctly derived as `/home/enzopalmisano/w13-testrepo`), and the very next capture,
  `05-26-clone-a.png`, already shows the dialog closed and a second `w13-testrepo` project with a
  `main` (`Primary`) worktree at `/home/enzopalmisano/w13-testrepo` in the sidebar —
  `06-27-clone-b.png` and `07-28-clone-c.png` hold that state stable. On disk today,
  `/home/enzopalmisano/w13-testrepo` no longer exists (consistent with the evidence's claimed
  scratch cleanup) and `/tmp/w13-testrepo` (the clone *source*) is still present with a `.git`
  worktree-link file, matching the evidence's description exactly. Independently confirmed
  `run_streaming`'s caller (`clone.rs:33`) and that a same-machine source path takes the
  `--no-local` branch, per the evidence's reasoning.

**Disagreement with the evidence's framing, not with its conclusion.** The write-up says inserting
a `shot` before clicking the field made `key Return` "work every time." The raw captures show
otherwise: `02-12-url-typed.png` through `05-15-after-clone.png` (one full round) and
`02-16-form.png` through `02-19-typed-test2.png` (a second round) both show the Repository-URL
field's placeholder text (`https://github.com/owner/repository.git`) completely unchanged after a
claimed `type` action — i.e. text entry itself failed to register multiple times, not just Return.
Only the third round succeeded, first with a `HELLO` test string (`04-22-after-type.png`) and then
with the real path. The underlying claim — Return delivery works and a real clone ran — is
genuinely proven by the final round; "worked every time" overstates what the capture history shows.
This does not change the verdict.

## F-PER-08 (ledger line 244) — verdict: `half-proven` (unchanged)

Own `discriminating: false`; agree, and no new captures were offered. Independently re-read
`main.rs` around the cited call site: `newly_allowed` (populated at `main.rs:4560`-ish, immediately
above the `save_browser_origin_grant` call the evidence cites) is filled *exclusively* from
`surface.allowed_origins()` on a live `BrowserSurface`, and the `browser.*` socket dispatch
(`main.rs:4476-4524`) only implements `open`/`navigate`/`act` — no method that could synthesize a
permission grant. This confirms the evidence's reasoning, not just its prose. Combined with
`F-WIN-06`'s own capture (`04-37-after-new-browser.png`, this pass) showing the embedded browser
still renders chrome only (`Direct XCB build failed... Wayland(...)`), the browser-origin half
remains genuinely out of reach on this lane. General-settings half stays proven from prior
evidence, untouched this pass. Verdict carries forward unchanged.

## F-PERSIST-DB-11 (ledger line 514) — verdict: `half-proven` (unchanged, half owed narrowed)

Opened all four new captures. `02-29-migrated-boot.png` shows the documented first-attempt
self-heal (project vanished because its worktree path didn't exist on disk yet); `02-30-migrated-boot-b.png`
shows a real, populated sidebar (`w13db11proj` / `main` / a `Chat` tab) after a genuine v1->v12
migrated boot — schema half is solidly reconfirmed. `02-31-chat-restored.png` (valid-payload retry)
and `02-32-control-current-schema.png` (fresh-v12, no-migration control) are visually identical to
`02-30`: an idle Chat pane with an empty transcript and no restored message, in both the migrated
case and the no-migration control. This matches the evidence's claim precisely.

Independently traced the code the evidence cites: `restore_persisted_transcript`
(`tiller_ui/src/chat.rs:1657`) reads from the DB and pushes entries only if
`load_chat_transcript` returns rows; `clear_persisted_transcript` (`chat.rs:1704`) is real but is
only called from `new_conversation` (`chat.rs:2066`), the explicit "New Chat" user action — the
evidence hedges correctly ("exists on a path that can run after that") rather than claiming this
is what fires on boot. `quarantine_record` exists (`db.rs:610`, insert at `db.rs:1275` — close to,
not exactly, the evidence's cited line numbers, consistent with a slightly different working-tree
state) and the evidence's claim that it stays empty in both conditions is plausible and unrefuted.

Net: the schema half is proven stronger than before. The data half is proven for every table this
row's manifest named except one: `session_ref` and `tab.agent_id` verifiably survived migration
verbatim; `chat_turn` content did not come back in any tested condition (migrated or fresh-schema
control), so the manifest's specific ask — old-version `chat_turn` data surviving forward migration
— is still not demonstrated working end to end, even though the *reason* is now shown to be
unrelated to the migration functions themselves. `half-proven` remains the correct verdict; the
owed half narrows from "data preservation, unstudied" to "chat_turn transcript restoration
specifically, reproducibly empty, cause not yet isolated."

## F-WIN-06 (ledger line 58) — verdict: `PASSED` (changed from `FAILED — defective`)

Independently re-verified the ledger's own cited defect is stale: `grep -n "NewBrowser"
main.rs` shows the match arm at `main.rs:4663` is real and non-empty (calls `add_browser_tab`),
not the empty arm the current ledger cell describes at `main.rs:4215`. Opened the captures:
`03-36-tabmenu2.png` shows the tab-strip's own "+" menu with `New Terminal`, `Changes`,
**`New Browser`**, the five agent entries, `Split Claude Code`, `New Chat` — exactly the menu the
ledger's VERIFY line names. `04-37-after-new-browser.png` shows a real `Browser` tab added to the
sidebar's tab list under the worktree, with full chrome: back/forward/stop controls and a URL bar
reading `https://example.com`. The content pane shows `Direct XCB build failed: the window handle
kind is not supported; XCB->Xlib adapter failed: GPUI returned unsupported handle:
Wayland(WaylandWindowHandle {...})` — grepped `docs/linux-rewrite/WAYLAND-LANE.md:104-105` and
confirmed this exact error string is that document's own pre-existing, catalogued Wayland-lane
webview limitation (also cited independently in `W06-set-evidence.md`), not a new or
row-specific defect. The row's actual concern — does the "New Browser" menu entry do something
when clicked — is now affirmatively yes: a real tab opens with correct chrome and address state.
The blank content is a known platform/lane constraint that applies to every webview surface on
this lane, not evidence the `NewBrowser` action itself is broken. Triage's reclassify is correct.

## Notes for the orchestrator

- **`F-GIT-RUN-02` and `F-WIN-06` both move off stale `NOT EXERCISED`/`FAILED — defective` bases
  onto `PASSED`**, independently confirmed by opening the raw captures myself (not trusting the
  driver's prose) and, for `F-WIN-06`, by re-grepping the cited source line and the referenced
  `WAYLAND-LANE.md` error string.
- **Overclaim caught and not fatal:** `F-GIT-RUN-02`'s evidence says the settle-then-click fix made
  `key Return` "work every time." The capture history (mtime-sorted, since the filename prefixes are
  out of chronological order) shows two full failed rounds of text entry before the third round
  succeeded. The final round is genuinely discriminating and the clone genuinely happened and was
  verified on disk, so the verdict is unaffected — but the reliability claim in the prose is
  stronger than the evidence it's citing.
- **`F-PERSIST-DB-11`'s new captures are real and strengthen the schema half**, but the specific
  table the manifest called out by name (`chat_turn`) still doesn't come back after boot in any
  tested condition. The driver's own filing as "a separate, schema-independent observation" is a
  fair and appropriately hedged read of the code, but it does not convert this row's own
  data-preservation ask into a pass — `half-proven` stays, with a narrower, better-characterized
  owed half.
- `F-PER-08` is an honest, non-discriminating re-confirmation reusing `F-WIN-06`'s own capture as
  corroboration; the code trace checks out and the verdict carries forward unchanged.
