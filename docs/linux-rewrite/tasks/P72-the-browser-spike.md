# P72 — The browser: a feasibility spike first, a surface second

**This brief is everything you need; your context was just reset.**

## The scope ruling that makes this piece exist

The goal says *"niente webview, niente HTML"*. That has been read until now as banning the browser
feature outright, and the nine `F-BRW` rows have sat at `FAILED — absent` since pass 8 for that
reason.

**The user has ruled otherwise, explicitly:** *"niente webview"* bans an **Electron-style app
shell** — Tiller's own UI must be native GPUI, every pixel — and does **not** ban a web engine behind
the in-app browser *feature*. A webview crate is permitted **for this surface alone**.

Record that ruling wherever you would otherwise be tempted to re-litigate it. It is the user's call,
it has been made, and the nine rows are **in scope**.

## Read this before you write any code: the piece is a spike

`main.rs:4215` currently reads:

```rust
// Browser is not part of this shell's content set yet. Keeping the
// action typed and ignored is preferable to opening a fake pane.
NewTabAction::NewBrowser => {}
```

That instinct was right — refusing a fake pane is the same judgement that made two builders decline a
drag that draws and drops nothing. **The gap is that the menu offers "New Browser" anyway**, so the
user clicks a live entry and nothing happens, with no feedback. Confirmed on the running app, not
inferred: `pireview`'s frame `/tmp/crit14/stage-menu-4.png` shows the entry in the open menu, and the
production list in `tab_bar.rs` does not filter it. Note the asymmetry worth fixing in the same
breath — the control socket already answers `browser.*` with a specific *unsupported* error, so the
same feature is honest on one surface and silent on the other.

**The unknown that decides everything: can a web engine be composited inside a GPUI window on
Linux/X11 at all?** GPUI paints its own GPU surface. A `wry`/WebKitGTK view is typically a native
child window, and a child window over a GPU-composited surface commonly ends up either always-on-top,
z-fighting with GPUI's own layers, mispositioned under fractional scaling, or invisible.

**So: answer that question before building a browser.** Time-box it. Produce the smallest artefact
that settles it — a window with a GPUI-drawn sidebar on the left and a live web page on the right,
screenshotted, with a GPUI element deliberately overlapping the web view so the z-order is visible.

Three outcomes, all acceptable, and **the negative ones are worth as much as the positive one**:

1. **It composites.** Report how, and what constraints it imposes (scaling, input routing, z-order).
   Then `F-BRW-01/02/03/04` become a normal build piece.
2. **It composites only as a separate top-level window.** Say so. That is still a browser, and it
   changes the UX rows (`F-BRW-09`'s "internal browser tab") into something the user must rule on.
3. **It does not work.** Say so plainly with the evidence. That is a real finding and it returns the
   scope question to the user with facts instead of speculation.

**Do not build nine rows' worth of browser on the assumption that step 1 holds.** If the spike
succeeds early, the *next* piece is `F-BRW-01/02/03/04` (chrome, URL field, Back/Forward/Reload/Stop,
error states). `F-BRW-06/07/08` (Allow/Deny, persisted origins, revoke in Permissions) is a third
piece and needs a `tiller_persistence` migration — that crate is at **v10** as of P70, so yours would
be **v11**, appended to `MIGRATIONS` and nothing else.

## The one thing to fix regardless of the spike's outcome

The dead menu entry. Either hide "New Browser" until the surface exists, or make clicking it say
something. **It is a defect under every one of the three outcomes**, so it does not depend on the
spike and should not wait for it. If you hide it, hide it in a way that is trivially reversible —
this feature is now in scope, not cancelled.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. For the menu fix, a **named drawn test**
(`TestAppContext` / `VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump)
asserting the entry's new behaviour — absent from the menu, or present and producing a visible
response.

For the spike, the evidence is a **screenshot of the real thing**, not a passing test. A test that
constructs a webview and asserts it was constructed proves nothing about compositing, which is the
entire question. Put the image somewhere durable and reference it by path — **not `/tmp`**, which is
where the critic's evidence currently goes and why its verdicts are not replayable.

## B-02 decision after the spike

The macOS reference was checked before choosing the Linux composition. `BrowserPermissionStore`'s
`confirm(origin:)` builds an `NSAlert` and presents it with `beginSheetModal(for:)`: Allow/Deny is a
sheet attached to the application window, not a doorhanger already living below a browser address
bar. Linux therefore deliberately adapts that interaction to a browser-style permission doorhanger
in Tiller's GPUI chrome, between the toolbar and the native WebKit child. It is not painted over
WebKit pixels.

The inventory count is consequently **0/9 rows requiring GPUI chrome over the webview**. F-BRW-01,
02, 03, 04, 05, 07, 08, and 09 use the toolbar, adjacent chrome, or the Permissions surface;
F-BRW-06 uses the GPUI doorhanger above the child-window rectangle. The earlier count of 1/9 was
the count for the unadapted modal-sheet placement, not a requirement of the chosen design.

B-02 chooses **option 1: the existing X11 native child WebKit view**. Option 2 (an
override-redirect GPUI popup) is rejected because it would add a second GPUI surface that must track
the parent on every move and resize for one prompt. Option 3 (offscreen WebKit texture) is rejected
because it would replace the proven child-window path, add manual input forwarding, and rebuild the
rendering pipeline for the same one prompt. If the doorhanger cannot be kept visible in the GPUI
chrome, the fallback is to hide the native child while the prompt is shown and restore it after the
decision; it is not a new composition architecture.

The durable B-02 grant contract is schema **v11** (`browser_origin_grant`): Allow survives relaunch,
and the host can revoke one origin or all origins. The view deliberately exposes that contract rather
than opening a second persistence path. The new `wry` dependency brings GTK/WebKitGTK's native
event-loop integration; the surface pumps GTK from GPUI as the spike did. It requires the real system
development packages `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, and `libxdo-dev`.
The gate should make those prerequisites explicit with a `pkg-config` check beside the cargo check in
`Scripts/ci-linux.sh` (around line 17), instead of hiding them behind `spike_sysroot` or
`PKG_CONFIG_PATH`.

The real X11 evidence remains [`reference/linux-progress/p72-browser-spike-xlib-bridge-overlap.png`](../../../reference/linux-progress/p72-browser-spike-xlib-bridge-overlap.png):
WebKit is a native child above GPUI, so the child owns its pixels and input, while GPUI must update
its logical bounds on layout changes and keep all interactive chrome outside that rectangle. This
is the accepted positive spike outcome; it does not claim that GPUI can paint over WebKit.

Integration seam: `tiller_ui/src/lib.rs` is explicitly integrator-owned and remains untouched here.
The integrator must add `pub mod browser;`, construct `BrowserSurface` for a `TabKind::Browser`,
forward `BrowserEvent::OpenExternal` to the system-browser path, and connect the Allow return value
to `AppDatabase::save_browser_origin_grant`. The Permissions surface must load
`AppDatabase::browser_origin_grants`, pass the snapshot to `BrowserState`, and call the single/all
revoke methods. Those calls are seams, not claims that the existing `main.rs` or `settings.rs`
already exposes the feature.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane starts in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours:** a **new** `tiller_ui/src/browser.rs` (or a new crate if the spike says so), plus whatever
  spike scaffolding you need under `Scripts/` or a new `examples/` binary.
- **`main.rs` is `codex12`'s.** The `NewBrowser` arm and the menu entry live there and in
  `tab_bar.rs`, both theirs. **Name the menu fix as a seam for `codex12`** rather than editing it —
  that hand-off pattern is why P64/P66/P67/P70 all ran in parallel without a single collision.
- **Do not edit** `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`);
  `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**`
  (`codex11`); `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`).
- **A new dependency is permitted here and only here.** Adding `wry`/WebKitGTK is exactly what the
  ruling allows. Say in your report what you added, its transitive weight, and whether it drags in a
  GTK main loop that fights GPUI's.
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal — the browser *chrome* is
  Tiller UI and must be COSMIC, even though the page content is not ours to style.
- **Establish the build state with the gate's own commands**, not a paraphrase:
  `grep -n clippy Scripts/ci-linux.sh` and run exactly what it says. The orchestrator once ran a
  weaker clippy without `-D warnings`, called the tree clean while the gate was red, and overruled
  three agents who were right. Do not inherit that mistake. The gate currently stops on a
  **not-yours** `main.rs` fmt failure whose line numbers move as `codex12` edits.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: which of the three spike outcomes you got and the screenshot path proving it,
what you added as a dependency and whether its event loop fights GPUI's, the constraints a working
composite imposes (scaling, input routing, z-order), whether you fixed the dead menu entry or named
it as a seam for `codex12`, tests by name, the gate run with its own invocation with not-yours
failures named separately, tokens `tiller_theme` still lacks, and the honest remainder — including,
if the spike failed, exactly what you tried so nobody repeats it.
