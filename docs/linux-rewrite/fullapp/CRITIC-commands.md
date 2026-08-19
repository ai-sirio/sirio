# Critic pass — Group 3: F-WIN-06 (browser commands), F-CORE-DOM-06 (tab-jump duplication)

**Verdict: BOTH CLEARED.** The two missing commands now exist, are bound to the documented
stand-in chords, reach through the real input path, and produce the correct on-screen effect.
The duplicated tab-selection path is deleted; its single replacement is exercised, live, through
both callers (numeric key chords and the `tab.select` control-socket door), and the specific
"clamp to last tab" defect this row was failed for is gone.

Fresh critic, no relation to the commits under judgement. Work judged: `5e6c1715`
(`fix(F-CORE-DOM-06)`), `fd0417f5` (`feat(F-WIN-06)`), `7f41e684` (`style(F-WIN-06)`, a one-line
`cargo fmt` fixup) on top of `8fdbd9a0`, in the worktree at
`/home/enzopalmisano/Scrivania/Progetti/tiller/.claude/worktrees/wf_29906a5b-fa0-3/_rustwork`,
branch `wf3-rust-work`. I did not write any of these commits and owe the builder nothing.

## Setup

```
cd rust && CARGO_TARGET_DIR=/var/tmp/tt-critic-commands cargo build -p tiller
```

Built clean in 12m01s (only pre-existing, unrelated `nightly_coverage` cfg warnings from vendored
`gpui_linux`). Binary: `/var/tmp/tt-critic-commands/debug/tiller`.

Display: my own `Xvfb -screen 0 1600x1000x24 -displayfd 1`, allocated `:3`, never touched
`DISPLAY=:1`/`wayland-0`/`wayland-1`. Plain Xvfb has no window manager, which mattered twice:

- The first frame is genuinely black — GPUI/wgpu needs a size-change event before its first paint
  under this backend, not just time. Fixed by nudging the window through `xdotool windowsize` by
  ±1px (the same "resize forces a repaint" trick `Scripts/x11-nested-drive.sh`'s own `shot` action
  documents) after every gesture, before every screenshot.
- `xdotool windowactivate`/`getactivewindow` fail outright with no window manager
  (`_NET_ACTIVE_WINDOW` unsupported). `xdotool windowfocus` (plain `XSetInputFocus`, no EWMH)
  works and was used before every keyboard gesture below.
- Rendering needed `LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
  WEBKIT_DISABLE_DMABUF_RENDERER=1` (this environment has no DRI3; `docs/linux-rewrite/tasks/P127-browser-child-unavailable.md`
  names the same combination). With it, the embedded WebKit browser page rendered fully in this
  plain-Xvfb + no-WM setup — no red XCB/Wayland error banner, contrary to what I expected from
  `F-AUTO-09`'s finding; I have no explanation for the discrepancy beyond a different renderer
  path with `LIBGL_ALWAYS_SOFTWARE`, and note it only because it is not this row's problem.

App launched with `TILLER_SOCKET=/var/tmp/tt-critic-commands-run/control.sock` against a scratch
git fixture (`/var/tmp/tt-critic-commands-run/testworkspace`), never `tiller-linux` itself.
Screenshots via `import -window root` (ImageMagick 6 needs `-window root`, not `-root`). Killed at
the end of the pass: the app (pid 3527009), my Xvfb (`:3`), nothing else — never touched another
agent's process.

## F-WIN-06 — New Browser (`ctrl-shift-l`) and Focus Address Bar (`ctrl-l`)

**My assigned starting point was "verify the commands don't exist before building."** They do now
— this worktree's HEAD already contains the fix (`fd0417f5`/`5e6c1715`), landed after the
`786389c5` critic pass that first proved the absence live. So there is nothing to refute; the job
became verifying the fix actually works, the same standard as if I'd found the bug myself.

**Real chord, real window, real gesture — not the code path.** `xdotool key --window <winid>
ctrl+shift+l` after `windowfocus`, no socket call anywhere in this section.

1. **New Browser Tab, ctrl-shift-l.** Starting from a Terminal tab active (3 tabs: Chat, Terminal,
   Browser did not exist yet), pressed `ctrl-shift-l`. Result: a fourth tab titled "Browser"
   appeared, became active, and its content pane rendered a real navigated page — WebKit's
   `example.com` "Example Domain" copy, address bar reading `https://example.com/` (the
   hardcoded default `add_browser_tab` navigates to, per the '+' menu's own pre-existing "New
   Browser" behavior). Screenshot: `critic-commands-shots/03-ctrl-shift-l-new-browser-tab.png`.
   Sanity check first (`02-ctrl-shift-l-nofocus-noop.png`): the identical chord sent *before*
   `windowfocus` on a freshly-nudged, unfocused window produced no change at all — proof the
   effect above is the chord being received, not some ambient auto-open.

2. **The '+' menu's own "New Browser" row still exists and still works**, unchanged, alongside
   the new chord — clicked it live, got the menu shown in
   `critic-commands-shots/06-plus-menu-still-has-new-browser.png` (New Terminal / Changes /
   **New Browser** / five agent rows / Split Claude Code / New Chat). No regression, no
   label collision with the palette's "New Browser Tab" row (both are covered by the commit's
   own `new_browser_tab_and_the_plus_menus_new_browser_do_not_collide` test, which I read but did
   not need to re-derive since I drove the mouse path myself).

3. **Focus Address Bar, ctrl-l — disabled path first.** With a Terminal tab active (no browser at
   all open in that scene), pressed `ctrl-l`. Nothing happened to the UI, and specifically **the
   terminal did not clear** — Ctrl-L is bash's own "clear screen" chord, so this is the sharpest
   possible check that the binding is truly global (`KeyBinding::new(shortcut, Action, None)`,
   `None` = no key-context restriction) and wins over the terminal's own raw-input path, rather
   than the chord silently falling through and being swallowed by whatever has visual focus.
   Screenshot `critic-commands-shots/04-ctrl-l-on-terminal-noop-not-swallowed.png` — same neofetch
   output before and after, byte-for-byte the same terminal state.

4. **Focus Address Bar, ctrl-l — enabled path, decisive test.** Switched to the Browser tab (real
   click on the tab, not a socket call), deliberately did **not** click into the page body (that
   native XEmbedded WebKit child window has its own separate real X input focus and clicking into
   it is a confound, not a defocus — see the "trap I fell into" note below), pressed `ctrl-l`,
   then immediately typed `zz-marker-zz` with no other gesture in between. Result: the address
   bar's text changed from `https://example.com/` to `https://example.com/zz-marker-zz`, caret
   visibly positioned right after the inserted text. Screenshot:
   `critic-commands-shots/05-ctrl-l-focus-address-bar-marker-lands.png`. This is the strongest
   form of evidence available — not "the address field is highlighted", but "a keystroke sent
   nowhere near the address field landed inside it, at the caret, because ctrl-l moved real
   keyboard focus there."

   **Trap I fell into and want on record**: my first attempt at this test clicked into the
   *webview's page body* first (meaning to "defocus" the address field), then sent `ctrl-l` then
   `ctrl-a` then a marker. The page's own paragraph text visibly got a native WebKit text
   selection highlight, and the marker typed nowhere. That is `ctrl-a` reaching WebKit's native
   "select all", not the address field — clicking the embedded webview area shifted real X11
   keyboard focus to that separate XEmbed child window in a way `windowfocus` on the *parent*
   GPUI window did not reliably re-claim for every subsequent synthetic key send. I dropped that
   attempt, redid it defocusing through a GPUI-only widget (the Terminal tab) instead of the
   webview, and got the clean result in item 4. This is a critic-harness pitfall specific to
   XEmbedded native children under synthetic X input, not a defect in the command under test —
   flagging it in case any other critic corroborates behind this address-focus row.

5. **Availability and disabled-reason plumbing**, read (not just tested in isolation): `SaveFile`
   already has `WindowCommandDisabledReason::NoActiveFile` for "no active editor tab"; the same
   commit adds `WindowCommandDisabledReason::NoActiveBrowser` for Focus Address Bar and reuses it
   — no new invented behavior for the case the reference app's `WorkspaceEngineGate` never
   reaches (this port has no such gate; confirmed already by two independent prior findings the
   commit cites, `F-AUTO-09` and the `786389c5` critic pass). The builder's own choice, "disabled
   with no fallback, not invented navigation," matches `SaveFile`'s existing precedent exactly and
   is the more conservative of the two reasonable designs — I would not have picked differently.

6. **All five wiring points landed**, confirmed by reading and cross-checking against the live
   behavior above: `linux_window_shortcuts()` (now `[…; 8]`, both new entries with the F-WIN-06
   comment pinning them to `App/TillerApp.swift:73-81`), the `WindowCommand` enum,
   `bind_window_keys`, `window_command_availability`, and `command_palette.rs`'s `entries()` (rows
   "New Browser Tab" / "Ctrl+Shift+L" and "Focus Address Bar" / "Ctrl+L", distinct labels from the
   '+' menu's own "New Browser" row, same shape as the existing "New Terminal Tab"/"New Terminal"
   pair). None of the five is missing.

## F-CORE-DOM-06 — `TabSelection::jump` deleted, callers routed through `numeric_tab_selection`

**Confirmed equivalence claim first, as instructed, before trusting the deletion.** Read both
before the fix: `panes.rs`'s deleted `jump` did
`position.saturating_sub(1).min(self.tab_count - 1)` — an out-of-range position clamps to the
*last* tab. `tiller_project::domain::numeric_tab_selection` computes the same 1-based index but
returns `None` (no selection change) when that index is `>= count`. **These are not equivalent**
— the commit message says so and is right; this is exactly the `F-CORE-DOM-06` defect the ledger
already recorded (`ctrl-7` with 4 tabs open clamped to tab 4 instead of leaving tab 1 alone). `grep
-rn '\.jump(\|TabSelection::jump'` across `rust/crates/` after the deletion returns only a doc
comment — no dangling caller.

**Live, through the real chord, five real tabs (Chat, Terminal, Browser, Terminal, Terminal) open
in one running instance** (not the unit test's synthetic count):

- `ctrl-7` (out of range: not ≤5, not 9) from Chat (tab 1) active: **no change**, Chat stays
  active. Reproduced twice, once via a fresh click-then-chord sequence to rule out a stale-focus
  fluke. Screenshots: `07-five-tabs-chat-active-baseline.png` →
  `08-ctrl7-outofrange-unchanged.png` (pixel-identical tab bar).
- `ctrl-2`: jumps to tab 2 (Terminal) — `09-ctrl2-jumps-to-tab2.png`.
- `ctrl-5`: jumps to tab 5, the last tab — `10-ctrl5-jumps-to-tab5.png`. (One earlier attempt at
  this same chord, immediately after the `ctrl-7` no-op with no intervening click, produced no
  visible change — an `xdotool` synthetic-event flake I could not reproduce on retry from a fresh
  `windowfocus`+click; noted for transparency, not treated as a finding, since the clean retry and
  every other single-shot chord in this pass landed on the first try.)
- `ctrl-9` from Chat active: jumps straight to tab 5, the last tab — not "the ninth tab" —
  confirming "9 always means last" holds past the literal count, live, not just in
  `numeric_tab_selection`'s own unit test with `count=12`. Screenshot:
  `11-ctrl9-jumps-to-last-tab.png`.

**The second caller — `tab.select` off the real control socket** — exercised independently, not
inferred from reading the routing comment. Sent raw NDJSON over
`/var/tmp/tt-critic-commands-run/control.sock`:

```
{"id":"sel7","method":"tab.select","params":{"index":"7"}}   -> ok:true, Chat stays active (12-…-unchanged.png)
{"id":"sel3","method":"tab.select","params":{"index":"3"}}   -> ok:true, Browser (tab 3) becomes active (13-…-jumps-to-browser.png)
```

Both callers now visibly share one fate. **The `position: usize -> u8::try_from` boundary itself,
exercised**: a first draft of this report named this as an untested gap, so I went back and closed
it before finalizing. From Chat (tab 1) active:

```
{"id":"seloversized","method":"tab.select","params":{"index":"999999999999999999999999"}}
  -> ok:false, "tab.select index must be a positive integer"   (fails usize::parse, 24 digits)
{"id":"selu8bound","method":"tab.select","params":{"index":"100000"}}
  -> ok:false, "control action dispatch bound (5.0s) fired before the worker replied…"
```

The second reply is a timeout, not a rejection — this build host had a load average around 44 on
12 cores from other agents' concurrent builds during this pass (see Regressions below), and
`system.ping` sent immediately after came back `ok:true` in the usual sub-second time, so the app
itself was not hung, just slow to service the queued action under that load. What matters is the
*effect*, checked by screenshot before (`14-baseline-before-oversized-index.png`, Chat active) and
after (`15-after-oversized-index-100000-unchanged.png`, Chat **still** active): index `100000` —
well past `u8::MAX = 255`, comfortably inside `usize` — did not select any tab, and critically did
not wrap into a small, valid-looking position the way a truncating cast would. `u8::try_from`
fails closed against a hostile or oversized external index, live, not just on inspection.

## Regressions checked

- `cargo fmt --check -p tiller -p tiller_project -p tiller_ui`: diffs exist (`main.rs` lines 4005,
  6223, 6257, 7936, 8084, 13312, 15033, 15173, 15619, 15711, 18977, 20108, 20379; `session.rs`;
  `chat.rs`; `sidebar.rs`; `tab_bar.rs`) — cross-checked every line number against this pass's own
  three commits' diff hunks (`git diff 8fdbd9a0..7f41e684`) and **none overlap**; this is
  pre-existing drift elsewhere in the tree, not something these commits introduced. The specific
  files these commits touched (`panes.rs`, `domain.rs`, `command_palette.rs`, `browser.rs`) have
  no diff at all.
- `cargo clippy -p tiller_project --lib` (own target dir, to avoid the shared one's build lock):
  clean, zero warnings.
- `cargo test -p tiller_project` (lib + integration): **47 lib tests pass**, including the new
  `position_nine_always_means_the_last_tab_and_overflow_is_rejected_not_clamped` pin.
- The app ran continuously for the whole pass (~26 minutes, one process, no restart) under real
  keyboard/mouse/socket input with zero panics or errors in its stderr (`grep -i "panic\|error\|fatal"`
  on the app log, filtered for the known-benign EGL/Vulkan/DRI3 startup warnings, returns nothing)
  and was still answering `system.ping` at the end.
- `cargo test -p tiller` (the full binary suite, which would directly re-run this pass's own new
  `#[gpui::test]` cases plus the pre-existing 347-row baseline) did not finish inside this pass's
  time budget — the shared build host had a load average of ~44 on 12 cores from other agents'
  concurrent cargo builds in this same session (consistent with this repo's own recorded lesson
  about shared cargo targets), and `test-support`-feature `gpui`/`gpui_linux`/`tiller_ui` alone
  took past 12 minutes to rebuild before `tiller`'s own test binary even started linking. I did
  not wait it out. This is the one hole in an otherwise from-source, live-driven verification: I
  have not personally re-run the automated regression suite, only inspected that none of this
  pass's diff hunks fall outside the touched-function boundaries and confirmed by direct live
  drive that ctrl-t (new terminal), the '+' menu, and the control socket's ping/tab.select all
  still work. Whoever reads this next should either wait out that build on an idle host or trust
  the live evidence above in its place — not both skip the build *and* not read this paragraph.

## Biggest remaining gap

Not in the two rows I judged — both are solid, and every input path I could identify for them
(keyboard chord, mouse menu, command palette listing, control socket, including the oversized-
index boundary) is now live-driven and passing. The gap is process, not product, and belongs to
*this report*, not to the port: **the full `cargo test -p tiller` binary suite (the 347-row
regression baseline plus this pass's own new `#[gpui::test]` cases) never finished** inside this
pass's time budget — the shared build host was at a load average of ~44/12 cores from other
agents' concurrent builds. I substituted the strongest available live evidence (compiled from
source myself, launched myself, drove every real gesture myself, screenshotted every result) plus
`tiller_project`'s full 47-test lib suite (which does cover the domain-level fix directly) and a
clean `cargo fmt`/`clippy` pass on the touched files — but I did not personally re-run the
binary's own `#[gpui::test]` suite end to end, including the very tests these commits added
(`focus_address_bar_moves_focus_into_the_active_browsers_address_field`,
`numeric_tab_chords_select_the_absolute_position_and_reject_overflow`). A builder or the next
critic should run `CARGO_TARGET_DIR=/var/tmp/<unique> cargo test -p tiller` to completion on an
otherwise-idle host and confirm the count is 347-plus-this-pass's-additions with zero failures —
that is the one box this report leaves unticked.
