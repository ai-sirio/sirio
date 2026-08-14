# Critic verdicts — W11-ctrl+brw+sid

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/wave-a/W11-ctrl+brw+sid-evidence.md`, captures under
`reference/linux-progress/wavea-W11-ctrl+brw+sid/`. HEAD under test: `4073297` (current HEAD
`05d31a3f` differs only by docs-only commits — `git log --oneline 4073297..HEAD -- rust/` is
empty, so the prebuilt binary matches the source read below).

All source citations below were read-only (`Read`/`grep`); nothing under `rust/` was touched.
Two rows (`F-CTRL-BROWSER-02`, `F-SID-18`) were independently re-driven live on an isolated
`Scripts/wayland-drive.sh` instance (own compositor, own socket, own DB, `TILLER_WL_LABEL` set)
to settle a discrepancy the screenshots exposed; new captures/transcripts from that re-drive are
committed under `reference/linux-progress/wavea-W11-ctrl+brw+sid/sid18-critic-verify/` and
`.../brw02-critic-verify/`.

## F-CTRL-NOTIFY-03 (ledger line 442) — verdict: `PASSED` (reclassified from `FAILED — defective`)

Driver's dbus-monitor capture is credible and source-confirmed. `record_notification`
(`rust/crates/tiller/src/main.rs:717-744`) now calls `(self.notification_poster)(&payload)`
directly after pushing to the in-memory store — this is the exact wiring commit `a5d09d7`
was supposed to add, and it is genuinely present at HEAD. `notification_poster` defaults to
`post_desktop_notification` (line 693), which shells out to `notify-send` (line 1604-1611).
The driver's dbus-monitor transcript shows a real `Notify` method call carrying the caller's own
`W11NOTIFYTEST`/`W11NOTIFYBODY` strings — a positive discriminator, not a coincidence. The
previously-cited defect ("the posting conjunct never happens") no longer reproduces. Screenshots
(`notify03/01-baseline.png`, `02-after-create.png`) are non-discriminating as the driver states
(headless compositor shows no desktop-notification chrome); the dbus capture plus the source
wiring is what carries this verdict.

## F-CTRL-BROWSER-02 (ledger line 445) — verdict: `FAILED — defective` (unchanged bucket, narrower defect)

Both previously-named defects are genuinely fixed, confirmed independently:
- **No surface/url/title in the reply** — fixed. `handle_browser_action` (main.rs:4476-4490)
  returns `surface`/`url`/`title` explicitly. `brw02/02-after-open.png` shows a real Browser tab
  opened with the sent URL in the address bar.
- **Missing url silently defaulted** — fixed. Line 4481:
  `.ok_or_else(|| "browser.open requires a non-empty url".to_string())?`. Confirmed live by my
  own re-drive (see below) — the exact message reproduces.

**The driver's third-call claim does not hold up.** The evidence log frames the third call as
"the very first request of a fresh instance with no prior `project.add`," but
`brw02b/01-baseline.png` — the driver's own screenshot, taken immediately before that call —
shows the **same running instance already has the `tiller` project, the `linux/gpui-waku`
worktree, and three prior tabs open (Chat/Terminal/Browser)**. That is not a no-workspace
probe; it is `browser.open` in a perfectly normal, populated workspace, which is expected to
succeed and proves nothing about the clause's missing-workspace requirement.

I redid the test properly: a genuinely fresh `Scripts/wayland-drive.sh` instance, `browser.open`
called with a URL **before any `project.add` was ever issued**:
```
$ ctl browser.open url=https://w11-critic-verify.example
{"id":"drive","ok":true,"result":{"surface":"surface:2","title":"","url":"https://w11-critic-verify.example"}}
```
transcript in `reference/linux-progress/wavea-W11-ctrl+brw+sid/brw02-critic-verify/transcript.txt`.
It still returns `ok:true` with a real surface — zero workspace context, and the call succeeds
anyway. Source confirms why: `handle_browser_action`'s `browser.open` arm (main.rs:4476-4491)
performs no context/adapter check of any kind before calling `add_browser_tab`. The clause
(`02-inventory-packages.md:123`) explicitly requires rejecting "missing context or unavailable
adapter" — genuinely still absent, now proven by a test that actually removes the context rather
than one that only claimed to.

**Disagreement with driver:** the "fresh instance" framing was false (contradicted by the
driver's own baseline screenshot); the conclusion it was used to support (gap still real) happens
to be correct, reconfirmed here with a valid experiment.

## F-CTRL-CLI-02 (ledger line 451) — verdict: `half-proven`

**Installed-symlink half — confirmed, independently re-verified.** Ran
`ls -la ~/.local/share/TillerRust/bin/tillerctl` myself (the real, shared `$HOME`):
```
lrwxrwxrwx ... tillerctl -> /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust/target/debug/tillerctl
```
Matches `resolve_tillerctl_path`/`TILLERCTL_INSTALL_SUBPATH` (main.rs:7858-7872) exactly.

**Real-agent-hook half — not exercised, but not for the reason the driver gives.** The evidence
log claims "the agent-tab launch path ... is only reachable from the command palette, which
opens on `ctrl-shift-p`." This is false. The tab bar's own `+` button (`new-tab-button`,
visible in every capture in this slice, top-right of the tab strip) opens a menu via a plain
`on_click` (`rust/crates/tiller_ui/src/tab_bar.rs:566-573`, `.on_click` at line 588 — no chord)
containing "Claude Code" / "Codex" / "OpenCode" / "Pi" / "Oh-My-Pi" as ordinary menu items
(`tab_bar.rs:518-556`), each wired to `NewTabAction::ClaudeCode` etc. via `render_menu_item`'s
own `on_click` (line 314). `open_action` (main.rs:4639-4658) routes those actions straight into
`add_agent_tab`, which calls `adapter.prepare(...)` (main.rs:4615) — the exact hook-wiring code
path `VERIFY` asks for. No modifier chord is involved anywhere in this path. `panel.create` is
correctly excluded by the driver's own reasoning (it bypasses `AgentAdapter::prepare`).

I did not drive a real `claude`/`codex` process myself — spawning a live external agent CLI
(network/auth, indeterminate runtime) is a materially different risk than the internal
UI-state clicks used elsewhere in this pass, and settling the false "could not reach" premise
did not require it. The half remains genuinely un-exercised, but it is reachable with one more
plain click on an affordance already visible in this slice's own screenshots
(e.g. `brw02/01-baseline.png`, top-right `+`).

## F-BRW-07 (ledger line 256) — verdict: `half-proven` (unchanged)

Re-confirmed independently: `grep -rn request_permission rust/crates/tiller_ui/src/browser.rs`
reproduces exactly what the driver reports — definitions at lines 588 and 880, and the only two
call sites anywhere in the crate are inside `#[test]` functions (1666, 1677). Zero production
callers. This matches `F-BRW-06`'s existing `UNREACHABLE` finding for the same function. The
persisted-reload half (`main.rs:8161`/`:8234`) is unaffected and stays proven. The owed half
("trigger access, confirm no new prompt") has no seam to drive — not a lane limitation, a
missing caller — so `half-proven` is the correct bucket until `F-BRW-06` gets a real caller;
this is not currently blocked by anything this slice can fix.

## F-BRW-08 (ledger line 257) — verdict: `PASSED` (reclassified from `half-proven`)

Seeded-state and end-state both independently checkable:
- `brw08/02-before-revoke-all.png` shows exactly the two seeded origins
  (`w11-origin-a.example`, `w11-origin-b.example`) each with their own per-row `Revoke` button,
  plus a `Revoke all` pill in the card header.
- `brw08/03-after-click-158.png` shows `No browser origins have been granted.` — both gone.
- Source confirms the mechanism: `render_browser_grants` (`tiller_ui/src/settings.rs:3035-3058`)
  wires `Revoke all`'s `on_click` to `revoke_all_browser_origins` (settings.rs:1016, calling
  `Db::revoke_all_browser_origins`), while each per-origin `Revoke` (settings.rs:3089-3099+) has
  its own per-index `on_click` — structurally it can only ever remove the one row it belongs to,
  so a single click clearing both rows cannot be the per-origin control.

I initially flagged a resolution mismatch between the three captures (1400x900 / 1715x972 /
1400x900) as a possible discontinuity between "before" and "after." It is not: `Scripts/wayland-drive.sh`'s
`shot()` helper deliberately alternates the output between two sizes on every call to force a
compositor repaint (`Scripts/wayland-drive.sh:310-323`, documented inline) — expected, not a
break in the sequence. Combined with the driver's independent DB re-query (2 rows -> 0 rows) and
the structural one-click-can't-be-per-origin argument above, this is solid: `Revoke all` works.
Both the single-origin path (proven at E06-brw) and this pass's multi-origin path are now closed.

## F-SID-12 (ledger line 81) — verdict: `half-proven` (unchanged)

Re-confirmed independently. `grep -n "BTN_LEFT\|button" Scripts/wayland-virtual-pointer.c`
reproduces the driver's finding exactly: `0x110` hardcoded at both call sites, no button
parameter. Checked for an alternate chord-free route myself: `command_palette.rs:440/455` does
list a "Set Primary Worktree" palette action, but the palette itself only opens via
`ctrl-shift-p` (main.rs:2600-2610, "ctrl-shift-p is universal"), which is the same
out-of-scope chord the driver already excludes. `sidebar.rs`'s `context_menu_items` has no
plain-click/kebab affordance for `SetPrimary` — right-click is genuinely the only route. No new
gesture available on this lane; matches the existing record exactly.

## F-SID-18 (ledger line 87) — verdict: `PASSED` (reclassified from `FAILED — absent`; driver's `could-not-reach` also overturned)

The driver's own account states the blocker precisely: "no OCR available," three blind clicks at
guessed coordinates, none landed. I did the sighted pass the driver's own evidence says was
needed — zoomed the driver's own `sid18/02-00-two-tabs.png` with `convert -crop ... -resize` and
read the close-button glyph directly: its center sits at **(556, 50)** at 1715x972, not any of
the three coordinates the driver tried ((572,48), (570,33)x2 — all clean misses of a 14x20px
hitbox, `main.rs:5639-5652`).

I then re-drove the full row live on an isolated `wayland-drive.sh` instance
(`TILLER_WL_LABEL=critic-w11-sid18d`, no collision with any other agent's instance):
1. `project.add` -> two default tabs (Chat, Terminal). `sid18-critic-verify/01-two-tabs.png`.
2. `click 556 50` on the Terminal tab's close X -> a `Close dirty tab?` confirmation dialog
   appeared (`sid18-critic-verify/02-dirty-dialog.png`) — the terminal's live shell makes it
   "dirty," which is *why* the driver's `panel.list` never moved: the click was landing near the
   target but the app was correctly waiting on a second confirmation, not silently no-op'ing.
3. `click 699 463` (the dialog's `Close` button) -> `panel.list` now returns one panel (Chat
   only). `sid18-critic-verify/03-after-terminal-close.png`.
4. `click 427 50` on the Chat tab's own close X (a different x than Terminal's, since it's now
   the only tab) -> `panel.list` returns `[]`. The empty state renders exactly as the clause
   specifies: "No Terminals", "Open a new terminal to get started.", and a `New Terminal` button.
   `sid18-critic-verify/04-empty-state.png`.
5. `click 659 520` on `New Terminal` -> `panel.list` returns a **new** pane (`pane-2`, distinct
   id from the original `pane-1`) replacing the empty state.
   `sid18-critic-verify/05-after-new-terminal.png`.

This is a full, live, gesture-level pass of `01-inventory-app.md:35`'s VERIFY line ("Select a
worktree with no tabs and confirm 'No Terminals' and a New Terminal action are shown") plus the
bonus step of actually clicking through it. Both the ledger's `FAILED — absent` (the empty state
demonstrably exists and renders correctly) and the driver's `could-not-reach` (the close button
is reachable with the correct two-click sequence — one dismiss-dialog step the driver's approach
note didn't anticipate) are wrong. The drawn GPUI test the driver cited as corroboration-only is
now also backed by live pixels.

## Notes / disagreements with the driver

- **F-CTRL-BROWSER-02**: the driver's "fresh instance, no prior `project.add`" framing for its
  third call is contradicted by its own screenshot (see above) — an overclaim on the experimental
  setup, though the conclusion it reached happens to be right for an unrelated (source-level)
  reason, reconfirmed with a valid live test.
- **F-CTRL-CLI-02**: the driver's stated blocker ("only reachable via `ctrl-shift-p`") is false —
  a chord-free path exists (tab-bar `+` menu) and is visible in this slice's own captures. The
  row stays `half-proven` because that path was not actually driven (by either the driver or this
  critic), not because it is blocked.
- **F-SID-18**: the driver's `could-not-reach` significantly undersells what its own evidence
  supported — the three tried coordinates were off by single-digit-to-low-double-digit pixels
  against a genuinely tiny (14px-wide) hitbox, not a wrong region. A sighted re-read of the exact
  same screenshot the driver already had (no new gesture technique, just correct pixel-reading)
  resolved it completely, and a live re-drive confirms the full clause, PASSED.
- All seven rows were reached; none needed a `NOT EXERCISED` fallback for lack of driver progress.
