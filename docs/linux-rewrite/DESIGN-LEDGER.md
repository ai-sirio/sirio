# Design ledger — the contract's missing half

`INVENTORY-LEDGER.md` holds 388 rows derived from what the Swift app **did**. The goal's *first*
requirement was never in it: **a UI redone taking inspiration from waku, with Tiller's old macOS UI
no longer the visual reference.** "Done = 388 ticks" operationalised the features and silently
dropped the design, so design had no rows, no owner, and no way to be judged. `fable` named this in
FABLE-01; these rows are the fix.

**They count toward done exactly as the 388 do.** A row is closed only by the critic, and only by
exercising it. `builder-claimed, unverified` is not `PASSED`.

Decisions and their rationale: `05-the-design-of-the-program.md` (D1–D8, J1/J2). Measured visual
bar: `03-visual-bar-and-gpui-patterns.md`. Briefs cite rows by id. The J1 brief series covering
these rows is written: `tasks/D1`–`D6` (dispatch order; D2 is codex12's, the rest pi's). Pixel
halves and display-blocked rows are scheduled in `SHOT-LIST.md`.

Kept in its own file, not appended to `INVENTORY-LEDGER.md`, because the critic rewrites that file
wholesale each pass; merge the two when a pass is not in flight.

| id | decision | VERIFY | owner | verdict | judged |
|---|---|---|---|---|---|
| `D-SID-01` | D1 | a sidebar row draws branch (13.5px) + project chip + activity state + relative time, in the 51px two-line card anatomy | pi | NOT EXERCISED | — |
| `D-SID-02` | D1 | **no sidebar row contains `/` path text**; paths are status-bar and tooltip material only | pi | NOT EXERCISED | — |
| `D-SID-03` | D1 | projects are group headers, not disclosure parents; tabs do not nest under worktrees; one primary CTA (`+ New Worktree`) above the fold; selection is the 6% neutral fill | pi | NOT EXERCISED | — |
| `D-CMD-01` | D2 | a palette opens on `ctrl-shift-p` / `ctrl-k`, filters, and dispatches one sidebar action and one tab action through the real dispatch path | codex12 + pi | builder-claimed, unverified | `drawn_palette_filters_and_dispatches_sidebar_action_through_shell_route`; `drawn_palette_filters_and_dispatches_tab_action_through_dirty_close_route`; `ctrl_k_in_a_focused_terminal_does_not_open_the_palette`; `drawn_disabled_save_row_shows_reason_and_does_not_dispatch` |
| `D-CMD-02` | D2 | right-click context menus exist on sidebar rows, tabs and panes; **no in-window menu-bar strip exists** | codex12 + pi | builder-claimed, unverified | `resting_frame_has_context_menu_surfaces_but_no_in_window_menu_bar`; P43 `right_click_context_menu_dispatches_a_typed_worktree_action`; shell contract selectors in `P43-tab-command-layer-contract.md` |
| `D-CHAT-01` | D3 | composer draws as one card: radius 13, `composer` fill, max-w 720, roomy placeholder, one labelled chip row (agent · model · mode · context ring) | pi | NOT EXERCISED | — |
| `D-CHAT-02` | D3 | the circular send control **becomes stop while a turn runs**, and stopping ends the turn | pi | builder-claimed, unverified | D1: `stop_click_cancels_the_stream_and_the_transcript_states_it` — while streaming the `"send"` selector stays stable, a `stop-glyph` fills the control, a real click cancels through the Escape path (`cancel_turn`), the footer states the cancellation, and the control returns to send |
| `D-CHAT-03` | D3 | typing during a running turn shows the queued state, and the queued text sends when the turn ends | pi | builder-claimed, unverified | D1: `enter_during_a_stream_queues_and_the_turn_end_sends_it_exactly_once` (queue placeholder, commit, completion drain, mid-sentence rule), `removing_the_queued_item_means_nothing_sends_when_the_turn_ends`, `stopping_via_click_with_a_queued_item_still_sends_it` (cancelled turn drains the queue too) |
| `D-EMPTY-01` | D4 | app with no project: icon, 20px headline, 12.5px description, primary CTA that dispatches Add Project | pi | NOT EXERCISED | — |
| `D-EMPTY-02` | D4 | worktree with no tab: same anatomy, CTAs that really start an agent and open a terminal | pi | NOT EXERCISED | — |
| `D-EMPTY-03` | D4 | chat with no turns: same anatomy | pi | NOT EXERCISED | — |
| `D-CHROME-01` | D5 | the resting chat frame draws **≤ 12 interactive controls window-wide** (waku shows ~8) | pi | NOT EXERCISED | — |
| `D-CHROME-02` | D5 | the right panel is **absent at rest**; Files / Activity / Changes are one summon away | pi | NOT EXERCISED | — |
| `D-TAB-01` | D6 | one 11.5px chip row; hover-revealed ✕; overflow menu at the strip's end; activity by glyph never by colour fill; no second row; no per-tab paths | pi | NOT EXERCISED | — |
| `D-J1` | J1 | **the spine, end to end in one run:** launch clean → empty state → add project → sidebar → open worktree → empty state → start a chat → composer → streamed turn with tool and permission cards → **stop it** → type mid-turn and see it queue → done state on the sidebar row → relaunch and the session restores. P53 now provides an executable non-drawing chat/control readback seam; main.rs wiring, queue UI, and the complete drawn journey remain. | critic | builder-claimed, unverified | `chat_door_streams_stops_and_restores_transcript_over_a_real_socket` plus P52 relaunch proof; independent end-to-end verification required |
| `D-J2` | J2 | **trust the diff, end to end:** an agent edits → Changes tab → collapsed bands read and expand → stage / unstage / discard live → status propagates to the sidebar | critic | NOT EXERCISED | — |

## What is testable now and what is display debt

**Behaviour halves are drawn-testable today** — `TestAppContext` + `VisualTestContext`,
`.debug_selector(id)`, `simulate_keystrokes`, real mouse events, every drawn test hardened with the
full `run_until_parked()` pump.

`D-SID-02`, `D-CHROME-01` and `D-CHROME-02` are **absence** claims — no path text, ≤ 12 controls, no
right panel — and absence is exactly what a drawn frame's debug-bounds map can assert. They do not
need a display.

**Appearance halves join the display debt** beside the existing seven blocked entries, and must be
judged against waku's frames **by a participant who can see**. Every builder and the critic are
text-only; only the orchestrator and `fable` have ever read a frame. That is a structural fact about
this project and it is why design was never judged.

## The queue rule

**A builder takes the next absent row on the active journey, not the largest coherent block it
owns.** J1 is active. Breadth resumes when J1 is critic-green.

`INVENTORY-LEDGER.md` remains the truth of verification. It stops being the work queue.
