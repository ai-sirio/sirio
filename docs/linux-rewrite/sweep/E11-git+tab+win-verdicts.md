# Verdicts — E11-git+tab+win (F-GIT, F-TAB, F-WIN)

Adjudicated against `docs/linux-rewrite/sweep/E11-git+tab+win-evidence.md` and the captures under
`reference/linux-progress/drive-E11-git+tab+win/`. I did not drive this slice and did not build
any of the surfaces it touches — no compile, no edit under `rust/`; only `grep`, targeted line
reads, and one already-cached `cargo test` invocation (0.25s, nothing to rebuild).

All captures cited by the driver were opened and inspected directly, plus pixel-level crops
(`convert -crop`, `convert txt:-` colour sampling) of the two F-TAB-08 frames that matter. All
three rows keep the verdict they already had (`half-proven`), because all three are genuine
split-halves — one half independently reconfirmed working/reachable, the other independently
reconfirmed absent — matching the same pattern the ledger itself uses for `F-GIT-REMOTE-01`
("genuinely half, unlike `F-CORE-ACT-22`/`-USG-05` whose hedges failed"). No row's evidence was
strong enough, or weak enough, to move it off that resting point.

---

## F-GIT-REMOTE-01 — `GitRemote::project_name` vs `github_owner`

**Verdict: half-proven (unchanged).**

Independently re-ran the driver's grep rather than take it on trust:
`grep -rn "github_owner" rust/ --include=*.rs` hits only `tiller_git/src/lib.rs:62` (re-export),
`remote.rs:13,15,20,76,77` (definition + its own instance-method wrapping the free function), and
`tiller_git/tests/p41_git_behaviors.rs` (unit tests). Zero hits in `tiller_ui`, `tiller`, or
`TillerControl`. A parallel grep for `project_name` confirms the other half's app-reachability is
untouched: `tiller_ui/src/project_forms.rs:704` still calls `GitRemote::project_name(url)`. Also
reran the cited test directly (already built, no compile triggered):
`remote_parsing_supports_github_ssh_https_and_project_suffixes ... ok`.

This is exactly what the ledger note already says, now confirmed independently rather than
copied from the driver's prose. `evidenceDiscriminates: true` — the grep is a clean, unambiguous
zero-vs-eight-callers split.

Captures: none (source/test only, matching the row's own nature — there is no UI control to
photograph for a function with no caller).

---

## F-TAB-08 — New Chat submenu's no-agent fallback

**Verdict: half-proven (unchanged), but the live-click evidence for the missing half is now
genuinely trustworthy rather than merely asserted.**

Source read independently (`tiller_ui/src/tab_bar.rs:406-454`): `render_chat_agent_item` chains
`.hover(...)` and `.on_click(move |_, _, cx| ... this.emit_chat_agent(id, cx))`;
`render_chat_empty` — the fallback shown when `available_chat_agents.is_empty()` — has neither.
It is inert markup, not a broken button.

The live-click evidence needed a harder look than the driver's own writeup got. `04-picker-open.png`
(1715×972) and `05-after-fallback-click.png` (1400×900) are captured at **different output
resolutions**, and pixel-sampling `05` at the claimed click point (1345,400) landed on the `#262626`
Files-panel background, not the `#232323` menu background — i.e., by the time `05` was captured,
the "Other agents…" row had visually moved out from under that absolute pixel coordinate. That
looked at first like the click missing its target entirely, which would make the "no-op" result
uninformative rather than damning.

Traced it to source, not guesswork: `Scripts/wayland-drive.sh`'s `shot()` (lines 285-312)
deliberately alternates the compositor's output resolution between two fixed sizes on **every**
capture, purely to force a `configure` event past GPUI's lazy repaint (the script's own comment:
"a socket call that demonstrably succeeded can produce an identical PNG… this alternates between
two sizes to force a configure"). Resolution changes happen only inside `shot()`, and hold steady
for every `click`/`type`/`key` issued between two `shot()` calls. Replaying the actual action order
— `shot picker-open` (sets 1715×972) → `click 1345 400` → `shot after-fallback-click` (sets
1400×900, *then* captures) — the click was dispatched while the output was still 1715×972,
exactly the resolution the driver read the fallback row's bounds from. It landed inside the row.
The apparent "miss" in frame `05` is the virtual pointer's absolute pixel position not migrating
when the output later shrinks underneath it — an artifact of the repaint-forcing resize, not
evidence the click went astray.

With that resolved, the decisive signal is the one that doesn't depend on cursor position at all:
frame `05`'s menu content is byte-for-byte the same list (New Terminal … Split Claude Code … New
Chat expanded … "Other agents…" / "No supported agent found on PATH") as frame `04`. A click that
opened Agents settings would have replaced this whole popover; it did not. Combined with the
source read, the missing half — "select it and confirm Agents settings opens" — is now confirmed
absent by a correctly-targeted live click, not merely inferred from the absence of `.on_click` in
source (which was already the state of the ledger note going into this pass).

`evidenceDiscriminates: true`. Verdict stays `half-proven` per the same standard the ledger
already applies to `F-GIT-REMOTE-01`: two independently-confirmed halves, one working
(fallback renders exactly right under bare PATH) and one definitively not (selecting it does
nothing) — not a placeholder for "still need to check," a settled split.

Captures used: `04-picker-open.png`, `05-after-fallback-click.png` (both re-inspected via pixel
crop/colour-sample, not taken on the driver's coordinate description).

---

## F-WIN-07 — session.restore vs a History menu

**Verdict: half-proven (unchanged).**

Independently reran the source grep rather than trust the driver's summary:
`grep -rniE "previous launch|history" rust/crates/tiller/src rust/crates/tiller_ui/src --include=*.rs`
— every hit is unrelated (browser back/forward `history`/`history_index` in `browser.rs`, the
`limit_chat_history` retention setting, `pane_event_history` for the persisted-pane replay
mechanism, or a captured-glyph `history` test variable in `project_identity.rs`). No "History"
menu, no "previous launch" string, no restore-picker route anywhere in the app or UI crates —
matches the driver's claim and the pre-existing P106/P108 finding exactly.

Did **not** re-hit the live control socket: a `tiller` process was already running at review time
under a sibling agent's own `wayland-drive.sh` invocation (`TILLER_WL_LABEL=drive-E09-prj+edit`),
and issuing a fresh `session.restore` against a shared, concurrently-driven instance risked
disturbing that agent's state rather than adding independent signal. The driver's live
`restoredCount:0` reproduction stands un-recontested; it is also independently a re-confirmation
of the already-established P106/P108 result, not a novel claim resting on this pass alone.
`01-baseline.png` shows the app's own chrome (window controls, sidebar toggle, nav arrows) with no
menu bar at all — consistent with, though not solely dispositive of, "no History menu."

`evidenceDiscriminates: true` for the source half (clean, exhaustive grep); the live half is
carried over from the driver's own re-reproduction plus prior established evidence rather than
re-driven here.

Captures used: `01-baseline.png`.

---

## Notes

No verdict changed. The one real disagreement-in-progress was with myself: F-TAB-08's live-click
evidence looked, on first pixel inspection, like a coordinate-space mismatch that would have made
it non-discriminating — that suspicion did not survive tracing `wayland-drive.sh`'s deliberate
resize-to-force-repaint mechanism, and the click evidence holds up. Worth flagging for future
critics on this fleet: any two adjacent captures in a `wayland-drive.sh` session will show
different output resolutions (1715×972 / 1400×900 alternating) by design — that alone is not a red
flag, but it does mean an absolute pixel coordinate read off one frame is not safe to compare
against cursor position in the *next* frame without checking which `shot()` call the intervening
action actually ran under.
