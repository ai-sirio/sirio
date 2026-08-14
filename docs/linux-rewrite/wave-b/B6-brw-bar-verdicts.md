# B6-brw-bar — verdicts

Critic pass, independent of the builder. Driven live on `DISPLAY=:1` with `Scripts/linux-drive.sh`
(the Wayland lane was not used for `F-BRW-*`: `WAYLAND-LANE.md` explicitly routes every `F-BRW` row
to the X lane because the embedded webview renders no page content under Wayland). All screenshots
and logs are under `reference/linux-progress/verify-b6-brw-bar/`.

## F-BRW-01 — native webview rect — **FAILED — defective**

Opened a fresh Browser tab (tab-bar `+` → New Browser) twice, independently, with 4s and 10s
settle respectively. Pixel-scanned the native child's rect both times with `convert … txt:-` at
four edges:

```
left edge (y=400):  #181818 -> #EEEEEE at x=331
top edge  (x=700):  #1A1A1A -> #EEEEEE at y=114
right edge(y=400):  #EEEEEE -> #1A1A1A at x=1060  (width 729)
```

Rect = **(331,114) 729×679** — bit-identical to the pre-fix FAILED evidence on record (same
left/top/width to the pixel). `reference/linux-progress/verify-b6-brw-bar/crop-sidebar-primary.png`
shows the same tell as the original evidence: the sidebar's `Primary` badge is still clipped to
`Pri` by the child bleeding into the sidebar column, and the pane's right edge (1060→1236, ~176px)
is still unpainted background before the Files panel.

The code change is real and present at HEAD (`bcad730`, verified via `git log`/`git show` and by
reading `native_webview_rect` directly — it now builds the `Rect` from `LogicalPosition`/
`LogicalSize` with no scale-factor math, exactly as the report describes), but it produced **zero**
observable change on screen across two independent captures. Either this function isn't the one
actually driving the live child's geometry (a different call site or a cached initial-creation
bound not re-issued on prepaint), or wry's own `to_logical` does not behave the way the report's
reading of its source predicted. Root cause aside: the row's own acceptance bar (`howToExercise`)
is not met — the child still renders outside its pane, identically to before.

## F-BRW-02 — Back button repaint — **PASSED**

Live-drove the same method the original FAILED evidence used: clicked the in-page `Learn more`
link inside the real WebKitGTK content (not a typed URL, to avoid confounding with F-BRW-03's
defect below). Address bar and title flipped to `https://www.iana.org/help/example-domains` /
`Example Domains` (`back1-01-after-learnmore-click.png`). Clicked `‹` (417,104); within 2s both the
address bar and the rendered page content reverted cleanly to `https://example.com/` / `Example
Domain` (`back1-02-after-back-click.png`). This is the exact contradiction the ledger recorded
before (hover/focus proved the click landed but nothing moved) — now it moves. `cx.notify()` fix
confirmed working live.

## F-BRW-03 — address field editing — **FAILED — defective**

The specific pre-fix symptom (silent-append, caret stuck at end) is gone: clicking mid-text
correctly visually repositions the caret (confirmed: click at the URL's second character placed
the blinking caret between `h` and `t`), and named keys (`End`, `Home`) reach `on_address_key` and
move the caret correctly. But **typing plain characters into the focused field is unreliable and
corrupts the text**, tested three independent ways after confirming the caret was correctly
positioned by a click:

- `type Z` / `type QQQQQ` (single call, `xdotool type --delay 25`): **zero** characters landed —
  text unchanged (`caret-01-after-click-and-Z.png`, `caret2-01-after-QQQQQ.png`).
- `key q`, `key w`, `key e` as three separate calls, 300ms apart: all three landed but **out of
  order** and **one position off** from the click — `https://example.com/` became
  `htweqtps://example.com/` (typed q,w,e rendered as w,e,q, inserted after `ht` not at the actual
  caret) (`caret3-01-after-qwe-keys.png`).
- `key a`,`b`,`c`,`d`, 600ms apart: only `a` landed (again one position off) — `b`,`c`,`d` were
  silently dropped (`caret4-01-after-abcd.png`).

This is reproducible, not a one-off flake — three separate app relaunches, three different
patterns of corruption, all broken. The `howToExercise`'s own bar ("type a character — it should
insert at the click position") fails outright: no run produced a clean, correctly-ordered
insertion at the clicked position. Whether this is the same `pump_web_events` race under a new
guise or a second, independent defect in the same input path, the field is not usable for reliable
text entry today. The `ctrl+a`-then-replace half of the row was not cleanly separable from this
corruption (any attempt to type a replacement URL after `ctrl+a` hit the same character-loss/
reordering bug — see `s2-after-first-nav.png` / `s3-after-second-nav.png`, both garbled).

## F-USE-01 — status bar refresh control — **PASSED**

Confirmed present first (`crop-statusbar-left.png`): a second icon sits immediately right of the
settings gear. Functional test, three shots in sequence on one running instance:

1. Settled state: `Claude 25% 5h · 84% wk` / `Codex logged out` (`use2-00-settled.png`).
2. Clicked the refresh icon (67,901) and captured **immediately** (no sleep): both segments flip
   to `Claude …` / `Codex …` (loading) in the very next frame, button shows its pressed/hover
   background confirming the click landed (`use2-01-immediately-after-click.png`).
3. After 10s: both segments repopulate with the same real values as step 1
   (`use3-01-after-10s.png`).

Because step 1 confirmed the bar was already idle/settled before the click, the loading flash in
step 2 is caused by the click, not a coincidental periodic refresh. End-to-end round trip verified
live.

## F-USE-02 — status bar tooltip — **half-proven** (unchanged status, evidence flipped)

Hovered the loaded `Claude` segment (140,901) for 1.6s: a tooltip bubble appeared reading
`Claude 25% 5h · 84% wk`, exactly matching the segment's own text (`use-03-hover-claude-tooltip.png`).
This is the half the ledger previously live-confirmed **absent**; it is now live-confirmed
**present**. The unavailable-segment half of the row's own acceptance bar ("try both an
enabled/loaded segment and — once the PATH-strip precondition unblocks it — an unavailable one")
remains unreached: forcing a provider into the unavailable state depends on F-USE-03, a different
row not owned by this slice, and was out of scope to fix here. Since only one of the two states the
row explicitly names was exercisable, the row stays half-proven rather than PASSED — the reachable
half flipped from broken to working, the other half is still just unread.

## Summary

| Row | Verdict |
|---|---|
| F-BRW-01 | FAILED — defective |
| F-BRW-02 | PASSED |
| F-BRW-03 | FAILED — defective |
| F-USE-01 | PASSED |
| F-USE-02 | half-proven |
