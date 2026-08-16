# E-P4 critic verdicts

Critic pass, independent of the builder — the builder's own report (`E-P4-report.md`) already
flags this row as "not re-verified this pass (X11-only row)"; that live re-check is what this pass
supplies. Instrument: `Scripts/linux-drive.sh` against a fresh app process on the shared `DISPLAY=:1`
X11 lane (real persisted DB). HEAD confirmed clean (`git status --porcelain` empty) before driving.

## `F-BRW-01` — FAILED — defective (unchanged; live re-check finds byte-identical numbers to D-P1)

Drove the exact route the builder's own `howToExercise` names, twice independently. (1) A fresh
`linux-drive.sh` launch restored a persisted session with an already-open Browser tab on
`https://example.com` (`00-baseline.png`); pixel-scanned it exactly as D-P1 did. (2) In a second
fresh launch, opened tab-bar `+` (1218,54) → **New Browser** (1299,167) — confirmed genuinely new by
the sidebar gaining a third `Browser` row, now selected/highlighted (`03-fresh.png`) — and
pixel-scanned that tab too. Both scans, a content-row scan at y=400 and a chrome-row scan at y=850,
land on **exactly** D-P1's numbers: rendered content spans x=331..1060, the pane's true chrome
bounds span x=386..1236 (divider at 378-379) — the same ~55px left bleed and ~177px right gap,
reproduced pixel-for-pixel across two independent tabs and two independent process launches of the
current HEAD binary (`33fbb97` included). The builder's self-calibrating `SharedScaleCorrection`
mechanism is real code (confirmed by `Read` of `browser.rs:1752-1805`) and its unit test genuinely
passes (`cargo test -p tiller_ui --lib browser::` → 10/10, including
`scale_correction_recovers_the_exact_shrink_d_p1_measured_live`), but that test only feeds
hand-plugged D-P1 numbers through the correction math — it never calls the real `webview.bounds()`.
Live, the correction has zero observable effect: either `webview.bounds()` errors/never returns
`Ok`, or it echoes back the already-shrunk value instead of the window's true on-screen geometry, so
`self.scale_correction` never moves the rendered rect. This is the wave's own "token call site" trap
— new code with a passing test and no behavior change on the path a user actually takes. Not fixed.
Frames: `00-baseline.png`, `02a-menu.png` (New Browser menu open), `03-fresh.png` (new tab, same
shrink). Raw scans: `row400.txt`/`row850.txt` (restored tab), `row400b.txt`/`row850b.txt` (fresh tab).
