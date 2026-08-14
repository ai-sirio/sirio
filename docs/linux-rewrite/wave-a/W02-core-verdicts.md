# W02-core critic verdicts

Adjudicated against HEAD `4073297` on `linux/gpui-waku`. Captures inspected directly
(`Read`, `identify`, `compare`) under `reference/linux-progress/wavea-W02-core/`, not
taken from the driver's prose. Source claims re-checked with `grep`/`sed` against the
current tree; no code edited, nothing compiled.

## `F-CORE-AUTH-01` — half-proven (unchanged)

Driver reviewed code only, no new capture. Verified independently:
`AgentAccountIdentity::parse_claude_json` is at `tiller_usage/src/account.rs:52`, and
`tiller_ui/src/settings.rs:548` (`discover_claude_identity`) genuinely feeds it the real
stdout of `claude auth status` — no mock in between. This confirms the code-path half of
the driver's claim, but it adds no new observation: the parser still has never seen a real
completed-login JSON, and the row's existing half-proof (PKCE URL spawn) is unchanged.

**Evidence discriminates:** no — pure code reconfirmation, same conclusion as before.

## `F-CORE-DOM-03` — NOT EXERCISED (unchanged)

Viewed all 9 captures in this row's sequence, across both the 23:07–23:08 attempt
(`02-01…04-05`) and the cited 23:09 attempt (`02-06-before-open.png`,
`03-07-after-open-click.png`, `02-08-tree-check.png`). In **neither** attempt does any
frame show the `add-project-menu` popover (`sidebar.rs:1746`, item `add-project-open` /
"Open Project…") actually rendered — every frame shows either the empty Projects panel or
the Terminal, never the menu itself. This means the pass cannot positively confirm the
click sequence even reached `start_open_project` → `cx.prompt_for_paths`
(`sidebar.rs:974,980`), on top of the already-known problem that a real portal dialog
would be invisible to this lane's own `swaymsg get_tree`/`grim` capture even if it fired.
The `swaymsg -t get_tree` check is real and correctly shows a single `con` (this lane's
own Tiller toplevel) both before and after — but that only rules out a portal window
appearing *inside this lane's own nested compositor*, which was already the documented
limitation (`ENVIRONMENT.md:77`). Net: reconfirms the existing ambiguity on current HEAD,
adds no resolution, and surfaces one more layer of doubt (menu-click success itself is
unconfirmed) rather than less.

**Evidence discriminates:** no — verdict and its rationale are unchanged from the ledger.

## `F-CORE-FILE-03` — NOT EXERCISED (unchanged)

Independently read `Scripts/wayland-drive.sh`'s header comment: the action vocabulary is
exactly `ctl`, `click`, `move`, `type`, `key`, `title`, `shot` — an instantaneous left
`click`, no press-hold/motion/release sequence. Confirms the driver's claim precisely; this
is a tooling-vocabulary fact, not a code question, and matches `ENVIRONMENT.md:76-78`'s
documented XDND limitation. No capture to check (none taken, correctly — nothing to
photograph).

**Evidence discriminates:** no — no new capture, confirms an existing, already-recorded gap.

## `F-CORE-FILE-06` — half-proven (unchanged verdict, evidence corrected)

Checked every capture in the row's sequence (`02-01` through `05-29`), not just the pair
the driver's evidence log cited, because the cited pairs turned out to be the wrong
frames:

- **Silent-reload / clean-buffer half — genuinely proven, but by a different pair than
  cited.** The driver's evidence cites `02-20-editor-open-a.png` /
  `04-22-after-edit-a.png` as the "pixel-identical, md5-equal" clean-buffer proof. Both of
  those frames actually show the **Terminal tab active** (an idle bash prompt) — no
  `testfile.txt` editor tab is even open in either shot, so that specific pixel-diff is
  vacuous, not evidence of anything. The real proof is elsewhere in the same capture set:
  `02-17-editor-open.png` shows `testfile.txt` genuinely open in FileView with content
  `line one`; `03-18-after-external-edit.png` (same tab, same file, different capture
  resolution per `wayland-drive.sh`'s alternating 1715×972/1400×900) shows the buffer now
  reading `line one` + `external edit at ven 14 ago 2026, 23:14:51, CEST`, with no banner
  — a real, live, out-of-band inotify-driven reload of a clean buffer, and visibly a
  distinct component from the Changes panel (correcting the ledger's stale overclaim, as
  the row intended). This half is real; the driver just cited the wrong screenshots for
  it.
- **Conflict-banner / dirty-buffer half — still unproven, and now known why.** The
  driver's evidence cites `02-26-dirty-b.png` / `04-28-conflict-b.png` as "inconclusive."
  Both of those frames actually show the **git Changes panel** (diff `+3`/`+4` lines on
  `testfile.txt`), not the FileView editor at all — the exact same Changes-panel-vs-FileView
  confusion the row's own triage note was written to correct. Worse, the intermediate
  frame `03-24-dirty-b.png` shows the literal text `LOCALEDIT_MARKER` sitting unsent in the
  **Terminal's** bash prompt — direct visual proof that the "click into the editor" step
  landed on the Terminal pane instead, so the buffer was never dirtied through the UI at
  all. This isn't merely inconclusive; it's a demonstrated miss. The conflict-banner branch
  (`Conflict::ChangedOnDisk`, `file_view.rs:743`, unit-tested but never UI-driven) remains
  unexercised.

Net: the row's headline verdict (half-proven, one half real, one half owed) is correct,
but the driver's own evidence citations for both halves point at the wrong frames — the
first half is provable from other frames in the same directory; the second half's frames
show something worse than "inconclusive."

**Evidence discriminates:** yes — real, verified positive evidence for the silent-reload
half (via the corrected frame pair), and a concrete (not merely suspected) miss for the
conflict-banner half.

## `F-CORE-TERM-02` — half-proven (unchanged)

Independently confirmed in the current tree: `tiller_terminal/src/lib.rs:1446,1504` wire
`open_context_menu` only to `MouseButton::Right`, with no other call site in the crate.
`WAYLAND-LANE.md:24` documents right-click as explicitly not yet exercised on this lane
("Pointer drags, right-click, modifiers/chords... are not yet exercised"). No new capture
taken this pass (correctly — nothing this lane can produce). Reconfirms the existing
half-proof and its owed half unchanged.

**Evidence discriminates:** no — pure code/doc reconfirmation, same conclusion as before.

## `F-CORE-USG-07` — half-proven (unchanged)

Independently confirmed `codex.rs:319` is exactly `Err(_) => return
UsageFetchOutcome::Unavailable(UsageReason::LoggedOut)` inside `load_credentials()`'s
caller. Viewed `02-30-normal-statusbar.png` (status bar reads "Codex 100% 5h") against
`02-32-missing-creds-statusbar.png` (same layout, empty `$CODEX_HOME`, status bar reads
"Codex logged out") — a real, legible, human-readable label change in the same on-screen
location, not a subpixel/noise difference. This is solid, discriminating, positive
evidence that the `LoggedOut` gesture the row's `Approach` names is genuinely live. The
refresh-needed / merge-save-on-success half (shared with `F-CORE-USG-05`) was correctly
not attempted this pass and remains owed.

**Evidence discriminates:** yes — real, legible, positive-control-backed visual proof of
the missing-credentials state.

## Notes for the orchestrator

- **F-CORE-FILE-06's evidence citations are wrong for both halves**, even though the
  row's headline half-proven verdict happens to still be correct once the *right* frames
  are checked. This is exactly the kind of overclaim-by-mislabeling that a driver scoring
  its own frames would be least likely to catch. Whoever next touches this row should cite
  `02-17-editor-open.png` / `03-18-after-external-edit.png`, not `02-20`/`04-22`.
- **F-CORE-DOM-03**: across all 9 frames captured for this row (both the 23:07 and 23:09
  attempts), the `add-project-menu` popover is never actually seen open. The pass cannot
  positively confirm `cx.prompt_for_paths` fired at all, independent of the already-known
  portal-visibility ambiguity. Still NOT EXERCISED, but for a slightly more pessimistic
  reason than "the portal might be invisible" — the click sequence's own success is
  unconfirmed too.
- All six rows reached; none skipped for time/scope reasons.
