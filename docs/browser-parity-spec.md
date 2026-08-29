# Browser parity on macOS and Windows

Implementable spec produced by wayfinder map
[#125](https://github.com/ai-sirio/sirio/issues/125). Every requirement below
traces to a closed decision ticket; this document assembles them, it does not
re-decide them.

**Scope.** Host Sirio's existing Browser surface on macOS and Windows at
contract parity with the verified Linux one, and make a Browser tab a tab like
any other — never a sidebar row. The Linux X11 path is not touched.

**Parity means the observable contract of `browser.*`.** An agent hook must
never branch on platform. It does *not* mean identical internals: the three
platforms host the page three different ways, and this spec says so explicitly
where they differ.

## Verified state

Everything here was measured on a real build, not inferred. Linux under forced
X11; macOS on the dev Mac; Windows on the ARM64 VM from
[#129](https://github.com/ai-sirio/sirio/issues/129).

| | Linux (X11) | macOS | Windows |
|---|---|---|---|
| build | ✅ | ✅ | ✅ `aarch64-pc-windows-msvc` |
| webview attaches | ✅ | ✅ | ✅ |
| page loads | ✅ | ✅ | ✅ |
| **page paints** | ✅ | ✅ | ❌ [#144](https://github.com/ai-sirio/sirio/issues/144) |
| geometry: resize/clip/tab-switch | ✅ | ✅ [#139](https://github.com/ai-sirio/sirio/issues/139) | not yet re-tested |
| geometry: scale change | ✅ | ❌ [#141](https://github.com/ai-sirio/sirio/issues/141) | ❌ [#146](https://github.com/ai-sirio/sirio/issues/146) |
| `eval`/`snapshot`/`act`/`console` | ✅ | ❌ [#135](https://github.com/ai-sirio/sirio/issues/135) | ❌ [#147](https://github.com/ai-sirio/sirio/issues/147) |

`browser.screenshot` is unsupported everywhere by decision, and `browser.errors`
has no dispatch arm on any platform — both out of scope, see below.

## Requirements

### 1. Presentation

- **R1.1** A Browser tab never appears as a sidebar row. `main.rs:6592-6606`
  currently maps *all* `self.tabs` into `SidebarTab` with no filter.
- **R1.2** The context-menu "New Browser" entry stays, and is never disabled
  ahead of time — a webview that cannot be created is reported after the fact,
  not predicted (see R5).
- **R1.3** `TabKind::Browser` gets its own icon. It currently falls through
  `sidebar.rs:2606-2610`'s catch-all and draws the chat icon.

### 2. Windows hosting — [#145](https://github.com/ai-sirio/sirio/issues/145), ADR 0002

GPUI creates its Windows window with `WS_EX_NOREDIRECTIONBITMAP` and composes
through DirectComposition, so the DWM never shows an ordinary child HWND — which
is exactly what wry's `build_as_child` produces.

- **R2.1** Sirio sets `GPUI_DISABLE_DIRECT_COMPOSITION` itself on Windows,
  before GPUI initialises. Not a user setting. GPUI reads it once in
  `WindowsPlatform::new`, so it cannot be deferred to when a browser tab opens.
- **R2.2** `shell_chrome::current_platform_material` (`shell_chrome.rs:20-28`)
  stops reporting native blur support for `target_os = "windows"`. Windows
  resolves to `ShellMaterial::Opaque`; translucency there is given up
  deliberately and has no setting to restore it.
- **R2.3** `browser.rs` gains a real `#[cfg(target_os = "windows")]` branch.
  Today Windows runs through `build_production_webview_for_macos` and reports
  `"WKWebView child failed"`. Two proven divergences make this a pattern, not an
  edge case.

### 3. Geometry — [#141](https://github.com/ai-sirio/sirio/issues/141)/[#143](https://github.com/ai-sirio/sirio/issues/143), measured by [#146](https://github.com/ai-sirio/sirio/issues/146)

`native_webview_rect` (`browser.rs:1971-1988`) is not platform-gated and
multiplies by `scale_factor` on every platform. That is right for exactly one of
the three.

- **R3.1** X11 keeps the multiplication — physical pixels, unchanged.
- **R3.2** macOS passes GPUI's logical bounds through unscaled, because
  WKWebView's `setFrame` takes logical points.
- **R3.3** Windows passes them through unscaled for a *different* reason: wry's
  own `set_bounds` (`webview2/mod.rs:1552-1558`) performs the logical→physical
  conversion. Pre-multiplying makes it `× scale²`. State this reason in its own
  right — folding it in with macOS's invites a future "fix" that makes Windows
  follow macOS's API argument, which does not apply.
- **R3.4** The one-shot self-calibration (`browser.rs:2103-2125`) becomes
  Linux-only. Off Linux it does not absorb a quirk, it manufactures one that
  outlives its justification: it latched at 1.0 on macOS and hid a factor-of-two
  error until a second display appeared, and latches at 0.667 on Windows, where
  a DPI change then shrinks the webview by exactly 1/1.5.
- **R3.5** `webview_bounds_recover_physical_target_from_live_fractional_scale_factor`
  (`browser.rs:2341+`) is gated to Linux; it currently asserts the multiplication
  on every platform and so defends the bug.
- **R3.6** The untested identity requirement gets its own test.

### 4. Scripting — [#135](https://github.com/ai-sirio/sirio/issues/135)/[#147](https://github.com/ai-sirio/sirio/issues/147)

`wait_for_script_result` (`browser.rs:897-931`) pumps GTK on Linux and blocks on
`recv_timeout` everywhere else — on the very thread the engine needs to deliver
its callback.

This is a **delivery** failure, not an execution one, and **not a deadlock**: the
script runs (a probe that sets `document.title` had its new title reach the GPUI
chrome), and `recv_timeout` expires leaving the app responsive. It is a
guaranteed timeout.

- **R4.1** `evaluate_script` stops blocking the calling thread. Results travel
  over the event channel that already delivers titles and load events on all
  three platforms.
- **R4.2** Pumping the native message loop is **not** the fix. It would work on
  Windows and is impossible on macOS, where the run loop belongs to GPUI; it
  would leave macOS broken and add a third path.
- **R4.3** The change reaches `browser.eval`, `browser.snapshot`, `browser.act`
  and `browser.console`, which all wait on this. Their socket contract —
  request/response with a timeout — must not change: only who blocks.
- **R4.4** `examples/browser_eval_probe.rs` is the regression tool; it fails
  loudly today on macOS and Windows and must pass after.

### 5. Error presentation

- **R5.1** When a webview cannot be created, the tab explains the cause in
  place. `startup_error` is already rendered (`browser.rs:1643`), but
  `add_browser_tab` (`main.rs:7958-7978`) never inspects it.
- **R5.2** `browser.open` must not report success on a dead surface; it
  currently returns `ok:true` regardless (`main.rs:8086-8093`).
- **R5.3** Error strings stop saying `"WKWebView child failed"` on Windows.

### 6. Profile and permissions — [#137](https://github.com/ai-sirio/sirio/issues/137), [#138](https://github.com/ai-sirio/sirio/issues/138), [#140](https://github.com/ai-sirio/sirio/issues/140)

- **R6.1** One shared profile, not per-worktree, scoped by the session
  database's rule: explicit override → the checkout containing the running
  binary → a stable location when installed.
- **R6.2** Linux and Windows take a Sirio-owned directory through wry's
  `WebContext`. macOS has no path setting at all and uses a
  `data_store_identifier` derived from the same criterion — hence **macOS 14 is
  the minimum supported version** (ADR 0001), below which the identifier is
  silently ignored.
- **R6.3** Windows' default, if nothing is set, is `<exe>.WebView2\EBWebView`
  beside the binary — unwritable under `Program Files`. Setting the directory is
  therefore required on Windows, not merely tidy.
- **R6.4** **Origin grants** (Sirio's, portable, what `browser.permission`
  resolves) and **capability permissions** (the engine's) stay distinct — see
  `CONTEXT.md`. Capability permissions remain with the engine.
- **R6.5** Browser tabs restore with their URL.

## Phasing

macOS first, Windows second — on macOS only scripting is broken, while Windows
needs hosting, geometry and scripting. They ship independently.

1. **Scripting (R4)** — both platforms at once, since one cause serves both, and
   it is what makes the agent-facing half of the contract real.
2. **Geometry (R3)** — both platforms; small, and already fully specified.
3. **Windows hosting (R2)** — unblocks everything else on Windows.
4. **Presentation (R1)** and **error presentation (R5)** — platform-independent.
5. **Profile (R6)** — required before Windows ships installed.

## Testing

- Geometry regressions must change the scale factor **while running**. Launch
  geometry is correct on Windows even today, by two errors cancelling, so a
  launch-only test passes and proves nothing.
- Windows verification runs on the #129 VM. Note it is `aarch64-pc-windows-msvc`,
  while every prior Windows document in this repo targets `x86_64`. A GUI
  launched over SSH lands in an invisible window station — use a Scheduled Task
  with `LogonType Interactive`.
- `sirio_ui` does not depend on `sirio_terminal`, so browser work builds
  without Zig via `cargo build -p sirio_ui --example …`.

## Out of scope

- **Native Wayland** — ruled out 2026-08-19; Linux forces X11.
- **Replacing the engine** (CEF, Servo, custom Chromium).
- **Implementing `browser.errors`** — no dispatch arm on any platform
  (`main.rs:8309-8311`); a contract hole, not a portability gap.
- **`browser.screenshot`** — unsupported on all platforms by decision.
- **WebView2 visual hosting** — the architecturally correct answer for Windows,
  but it needs upstream changes to both wry (no composition support in 0.56.1)
  and GPUI (`DirectComposition` is private). Its own project. Until then Sirio
  depends on a GPUI environment variable that upstream treats as a debugging
  escape hatch: if it is removed, the Windows browser goes blank again.
