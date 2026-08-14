# E05-core verdicts — F-CORE (adjudicated)

Adjudicator note: driven by a separate agent (report at
`docs/linux-rewrite/sweep/E05-core-evidence.md`, captures under
`reference/linux-progress/drive-E05-core/`). This agent did not drive or build any part of
this slice, and per the fleet's extension of the critic rule, is not the driver either. All
9 listed captures were opened and inspected directly with the `Read` tool (not taken on the
driver's prose); several were cropped/zoomed with local ImageMagick for pixel-level checks.
Source claims (`next_pane_id`, `post_activity_notification`, `tab_status`, the terminal
context menu's mouse binding, `WAYLAND-LANE.md`) were independently re-grepped/re-read
against `rust/` at HEAD `4073297` (read-only, no compile, no edits under `rust/`).

| row | ledger line | verdict | evidence |
|---|---|---|---|
| `F-CORE-ACT-02` | 338 | half-proven | Re-verified live, with a correction to the driver's own reading of their second frame. `03-05-notify-needs-input-pane1.png`: after `ctl notify session=pane-1 status=needs-input`, the Terminal tab's icon changes from a hollow "○" to "?" (confirmed by direct pixel-level crop/zoom against the `02-01-initial.png` baseline) while Chat is untouched — this is genuine, discriminating, live confirmation of the sidebar half for a Terminal-kind pane. **But the driver's second claim is wrong**: they assert `04-06-notify-needs-input-pane0.png` shows "Chat's tab icon also changes (small filled dot next to Chat), leaving both tabs marked" after `ctl notify session=pane-0 status=needs-input`. Side-by-side zoomed crops of the tab bar in `02-01-initial.png` (baseline) and `04-06-notify-needs-input-pane0.png` (post-call) show the Chat tab's icon is **pixel-identical**: a plain hollow "○" in both, no dot, no badge, no color change — confirmed again in `03-08-chat-active.png` and `04-09-after-notify-hidden.png`, where Chat stays "○" throughout while Terminal's "?" persists. Source read explains why: `tab_status` (`main.rs:3257-3300`) branches on `TabContent`; the `TabContent::Terminal` arm reads `self.activity.status(&format!("pane-{pane_id}"))`, but the `TabContent::Chat` arm (`:3261-3270`) derives status *only* from `chat.is_streaming()`/`chat.has_completed_turn()` and never consults `self.activity` at all. A Chat-kind tab structurally cannot reflect a `ctl notify` call regardless of which pane id is targeted — the driver's "per-pane-id, not focused tab" interpretation is not what's being demonstrated by that frame. Notification (D-Bus) half: still unproven, as the driver reports — the `printf`-based OSC-title injection did not execute as a single command (`02-07-title-set-claude.png` shows it split mid-string), so no pane ever acquired Layer-B identity and `post_activity_notification`'s `agent_id` gate (`main.rs:3738`) was never satisfiable in this session. Net: the ledger's existing "sidebar half live" clause is reconfirmed (for Terminal panes, which is what it was ever actually resting on) with a fresh discriminating gesture; the notification half stays unproven; verdict unchanged at half-proven. |
| `F-CORE-ACT-06` | 342 | NOT EXERCISED | Confirmed could-not-reach. `02-07-title-set-claude.png` shows the typed `printf \033]0;\xe2\x9c\xb3 working\007\n` landed unquoted in the live shell and visibly split into two lines/commands rather than executing as one — no pane acquired title-owned identity, so the owed gesture (title-owned vs process-owned clearing on separate panes) was never attempted. Note for the next drive pass: the failure is very likely plain shell quoting, not a wtype timing race as the driver's blocker note speculates — the bare `;` inside an unquoted `printf` argument terminates the command in bash on its own; wrapping the whole argument in single quotes (or the driver's own suggested script-file workaround) should fix it without needing a "more deliberate" timing recipe. Verdict unchanged: NOT EXERCISED. |
| `F-CORE-ACT-07` | 343 | NOT EXERCISED | Confirmed could-not-reach, same blocker as ACT-06 (`02-07-title-set-claude.png`, shared). The `notify` half of the debounce race is independently reachable (reconfirmed under ACT-02 above), but the "contradictory recognized title" half needs the same broken title-injection gesture, so no timing result in either direction was produced. Verdict unchanged: NOT EXERCISED. |
| `F-CORE-ACT-11` | 347 | NOT EXERCISED | Confirmed could-not-reach. Same title-injection blocker as ACT-06/07, plus the process-owned pane's precondition (a real child process matching an `AgentCatalog` comm name) was never attempted once the title half failed. Driver correctly declined to credit the spawn-owned class alone, since the row's actual claim is about the three ownership classes not clobbering each other, which needs all three live at once. Verdict unchanged: NOT EXERCISED. |
| `F-CORE-TERM-02` | 395 | half-proven | Confirmed could-not-reach from the Wayland lane specifically; the ledger's already-recorded half-proof stands unchanged since no new capture was taken (driver reports none, and none exist beyond the pre-existing `p17-rclick-term.png`). Independently verified both of the driver's supporting claims: `docs/linux-rewrite/WAYLAND-LANE.md` does state right-click/drag/chords "still require `DISPLAY=:1`"; and a direct source read of `rust/crates/tiller_terminal/src/lib.rs` shows `open_context_menu` (the handler that populates the Copy/Paste/Set Title/Split/Clear/Close menu) is wired only via two `.on_mouse_down(MouseButton::Right, ...)` calls (`:1446`, `:1504`) with no `.on_action` or keybinding path anywhere in the crate — so "no keyboard-only path exists" is a fact, not just an unsuccessful grep of `main.rs` (the driver looked in the wrong file but reached the right conclusion). Verdict unchanged: half-proven, missing half (per-item effects) still owed and still only reachable on the `DISPLAY=:1` lane, out of this agent's scope. |

## Disagreements with the driver's report

- **F-CORE-ACT-02**: the driver's `04-06-notify-needs-input-pane0.png` reading is an
  overclaim. They report "Chat's tab icon also changes (small filled dot next to Chat),
  leaving both tabs marked" and call this "a discriminating result... confirms the mapping
  is per-pane-id, not whichever tab is focused." Direct pixel/zoom inspection of that frame
  (and `03-08`/`04-09`, which also show an untouched Chat tab) contradicts this: Chat's icon
  never changes in any of the four captures that include it. This is not a subtle
  misreading — a side-by-side zoomed crop of the exact same tab-bar region in the baseline
  and post-call frames is pixel-identical for Chat. The actual explanation, confirmed by
  reading `tab_status` in `main.rs`, is more interesting than "id targeting worked": Chat-kind
  tabs never read `AgentActivityModel` status at all, by construction, so no `ctl notify`
  call — correctly targeted or not — could ever show up there. This doesn't flip the row's
  verdict (the ledger's "sidebar half live" claim was always about the Terminal-tab path,
  which the driver's *first* frame, `03-05`, genuinely and correctly reconfirms), but the
  driver's own narrative about what their second gesture proved is wrong and is not carried
  into the ledger evidence above.
- **F-CORE-ACT-06/07/11 blocker root cause**: the driver's blockers note speculates about a
  "timing race between wtype's per-character delivery and Return" as a possible cause. The
  captured frame is more simply explained by a missing shell quote (the OSC sequence
  contains a bare `;`, which bash treats as a command separator when unquoted) — worth
  fixing before the next attempt rather than chasing a timing issue that may not exist.
  Does not change today's verdicts, which are already the conservative NOT EXERCISED the
  ledger already carried.
- **F-CORE-TERM-02**: no disagreement on the verdict; the driver's citation ("main.rs's
  keybinding wiring") pointed at the wrong file (the relevant handler lives in
  `tiller_terminal/src/lib.rs`, not `main.rs`), but independent verification confirms their
  conclusion (no keyboard path exists) is correct regardless.
