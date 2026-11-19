# Engine swap: bezel-gpui 0.3.8 → gpui-pre 0.3.12 (programme step 1a)

Status: draft for approval · 2026-11-17 · branch `ely-chat`

## 1. Why

Enzo asked for everything Ely GPUI Components offers for AI chat
(<https://elygpui.com/components/chat/>, code in
<https://github.com/ELY-Open-Source/Ely-GPUI-Components>). Sirio's vendored Ely is
frozen at `e17e31a6` (= upstream `e3957f78`, 2026-09-30); upstream is 635 commits ahead,
split into crates, and builds on `gpui-pre =0.3.12`. Sirio builds on
`bezel-gpui =0.3.8+zed.82aeef`. A process links one gpui, so Sirio cannot take upstream Ely
(or `ely-documents`) without first moving its engine. This document designs that move and
nothing else.

### 1.1 The programme this step opens

Agreed with Enzo on 2026-11-16, in this order. Each later step gets its own design.

1. **Engine swap and Ely sync**, in two steps (Enzo chose the two-step option on 2026-11-17,
   relayed by coordinator t2s under Enzo's "pick the recommended option" rule):
   - **1a (this document):** Sirio, today's vendored Ely and the bezel crates move to
     `gpui-pre =0.3.12`. The app stays identical.
   - **1b:** vendored Ely is replaced by upstream's crates behind the `ely-gpui-component`
     facade (which still re-exports `forms`, `chat`, `editor`, … under today's paths), with
     Sirio's 58 still-needed local changes re-applied — chat and core first, the file editor
     last. Visible differences are reviewed by Enzo on captures.
2. **Markdown on `ely-documents`:** `bezel-markdown` leaves Sirio entirely (chat, Markdown
   Preview, file view, change requests, `selectable_markdown`). Every Preview feature of today
   stays (raw HTML via `expand_html`, pictures, Mermaid and its cache, sandboxed PlantUML with
   the server off by default, selection and Ctrl+C, Code/Preview, scroll bars). Added: all of
   the renderer's capabilities (LaTeX, GitHub alerts, footnotes, emoji shortcodes, `[[toc]]`,
   heading anchors, right-click menu on a selection), the outline panel and reading progress.
   Out: word count/reading time, split view, zen, print, EPUB, block editor, version history,
   backlinks, templates, thumbnails, rich text editor, page covers.
3. **Chat features from Ely**, one delivery and PR at a time: (1) richer messages —
   streaming Markdown, date separators, an error whose retry resends, a rate-limit notice,
   thinking duration (no token/cost footer); (2) message actions — copy, stop, continue,
   regenerate, quote; (3) edit and branches; (4) web sources and citations from
   `WebSearch`/`WebFetch`; (5) composer and media — drag-and-drop overlay, context chips,
   input hint, images and files in messages; (6) a welcome screen from saved prompts and
   Markdown export. A feature shows only for agents that supply its data (the existing
   `supports_*` pattern in `sirio_chat`). Out by Enzo's call: voice, link previews,
   👍/👎 feedback, shared conversations, project knowledge, suggested follow-ups, model
   parameters, audio/video, a system-prompt editor.

## 2. Goals, non-goals, anti-goals

**Goals**

- G1. The workspace's `gpui` and `gpui_platform` are `gpui-pre =0.3.12` and
  `gpui-pre-platform =0.3.12`; no `bezel-gpui*` or gpui-bound `bezel-zed-*` package remains in
  `rust/Cargo.lock` (checked for every target: `cargo tree --target all`), and `gpui-pre`
  appears in it exactly once.
- G2. Sirio looks and behaves as before on Linux, macOS and Windows: same surfaces, same
  pixels within the tolerance of §6, same keys, same tests passing.
- G3. The three engine patches Sirio carries keep working on the new engine: the Wayland
  drag-and-drop fix, links opening in the user's environment, and the Windows bitblt swap
  chain (with the additive alpha blending it was measured on, §5.3.5).
- G4. Remote images keep loading wherever Markdown shows them today (Preview, chat, change
  request threads), which needs the HTTP client bezel's platform installed implicitly (§5.4).

**Non-goals**

- Syncing Ely with upstream (1b), touching Markdown (step 2), any chat feature (step 3).
- Moving `bezel-theme`/`bezel-syntax`/`bezel-markdown` to their 0.2.x releases.
- Using anything new gpui-pre offers (hover modality policy, `platform_info`, …).

**Anti-goals** (would make this a failure even if it builds)

- Any visible change a user could notice: a moved pixel, a different font weight, a
  scroll that feels different, a key that stops working.
- Losing a drop on Wayland, a link opening without the user's environment in the
  AppImage, or Windows translucency.
- A second gpui in the graph (two `Window` types, two `Global` registries).
- A frame-cost regression in the terminal (`sirio_terminal/tests/still_frame_cost.rs`).

## 3. Constraints

- **Version pin `=0.3.12`, not the newer 0.3.13**, because upstream Ely HEAD (`2d827752`)
  pins `=0.3.12` and step 1b must not move the engine again.
- **One gpui per process.** `bezel-theme` and the vendored `bezel-markdown` depend on
  `bezel-gpui` and Sirio installs `bezel_theme::Theme` as a gpui `Global`
  (`sirio_theme/src/lib.rs:1325`, read at `sirio_ui/src/file_view.rs:1819`); about 390
  lines reference the bezel crates. They must resolve against gpui-pre.
- **Build load.** Builds run with `CARGO_BUILD_JOBS=4` and only after coordinator t2s gives
  a slot (`hc send t2s` first). `Scripts/ci.sh`/`ci-linux.sh` run only on Enzo's request.
- **Commit type** `refactor:` (or `build:`); the workspace version does not move.

## 4. Approaches considered

1. **Two steps — engine first with today's Ely, then the Ely sync (chosen).** Today's
   vendored Ely needs only a handful of fixes on gpui-pre (§5.2), so little work is thrown
   away when 1b replaces it, and a regression in 1a can only come from the engine.
2. **Engine and Ely sync in one branch.** No intermediate state, but two independent sources
   of regressions in one very large diff; a terminal or editor bug becomes hard to attribute.
3. **Ely sync before the engine.** Impossible: upstream Ely needs gpui-pre APIs.

## 5. Design

### 5.1 Workspace dependencies

`rust/Cargo.toml`:

- `gpui = { package = "gpui-pre", version = "=0.3.12", default-features = false }` and
  `gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", default-features = false, features = ["font-kit"] }`.
  The extern names stay `gpui`/`gpui_platform`, so no `use` line changes. Per-crate feature
  lists (`wayland`, `x11`, `test-support`) carry over; gpui-pre's `wayland`/`x11` are empty
  flags in core but still drive `guess_compositor` (`platform.rs:339`), so the wiring stays.
- `[patch.crates-io]`:
  - `bezel-theme` → new `rust/vendor/bezel_theme` (release 0.1.4 source, with its `gpui`
    dependency rewritten to `gpui-pre =0.3.12`). Because the patch replaces the package for
    every consumer, the registry `bezel-syntax` 0.1.4 (which imports no gpui item, only
    `bezel-theme`) follows without being vendored.
  - `bezel-markdown` → the existing `rust/vendor/bezel_markdown`, its `gpui` dependency
    rewritten the same way. Its two existing local changes stay.
  - `gpui-pre-linux` → `rust/vendor/gpui_linux` and `gpui-pre-windows` →
    `rust/vendor/gpui_windows`, each re-based on the released 0.3.12 source (§5.3). The
    `bezel-gpui-linux`/`bezel-gpui-windows` entries go. `gpui-pre-platform` takes both by
    registry name (`package = "gpui-pre-linux"`), so one patch entry each reaches every
    consumer, as today.
- `rust/vendor/ely-palette` and `rust/vendor/ely-gpui-component` already use
  `gpui.workspace = true` and follow the alias.

### 5.2 Source changes

Measured by grep against gpui-pre 0.3.12 (not yet compiled; the compiler is the final list):

| Change in gpui-pre | Sites | Fix |
|---|---|---|
| `ScrollWheelEvent` is a deprecated alias of `ScrollEvent`, which adds a `source` field | 9 struct literals (`sirio/src/main.rs:40073, 47646`; `sirio_terminal/src/lib.rs:10990`; `sirio_ui/src/changes.rs:8501`, `ely_ui.rs:640`, `settings.rs:7435, 7478`, `sidebar/mod.rs:5248`; Ely `motion/tests.rs:88`); 21 type references | Fill `source` with the value gpui-pre's own wheel path uses for a mouse wheel; rename every use to `ScrollEvent` |
| `on_scroll_wheel` deprecated, forwards to `on_scroll` | 7 calls | Rename to `on_scroll` |
| `Keystroke::parse` takes `&'static str` | `sirio/src/main.rs:23472`; Ely `typography/keys.rs:16` and its test helper `:76` | Builder's call (§8) — the strings there are built at run time |
| `Keystroke::unparse(&self, os)` | `sirio_ui/src/text_selection.rs:694` | Pass the running OS |
| `Modifiers::secondary(&self, cx)` | Ely `typography/select.rs:111` | Pass `cx` |
| `KeyBinding::new` takes `&'static str`, parses without an OS and `unwrap`s: a `secondary-` binding **compiles and panics when registered** (bezel resolved `secondary` with `cfg!`) | 21 `secondary-…` bindings (`sirio_ui/src/file_view.rs:505,507,508`; Ely `editor/keys.rs:89-98`, `forms/text/actions.rs`, 8 sites); `format!`-built strings at Ely `editor/keys.rs:72-75,85`, `forms/text/actions.rs:68,69,78,79` | `KeyBinding::load(.., cx)`, which resolves `secondary` for the host OS. Every `KeyBinding::new` in the workspace is audited, not only these |
| The `perf-native` feature of `sirio` pins `bezel-zed-scheduler =0.3.8` (`sirio/Cargo.toml:13,16`) and hands its `SpawnTime` to `gpui::profiler::update_running_task` (`native_perf.rs:6,268`) | 1 crate | Repoint to `gpui-pre-scheduler =0.3.12`; the feature is built in the verification |

The table was measured by grep and the compiler cannot see the `secondary-` panic, so the
start-up of the app and of every probe is part of the verification. Deprecated names are
migrated rather than allowed, so `ci-linux.sh`'s clippy stays clean.
Each change inside `rust/vendor/ely-gpui-component` gets a bullet in its `LOCAL-CHANGES.md`
under a new "gpui-pre 0.3.12 compatibility" heading (step 1b sorts them as "compat").

Checked and unchanged: `Scheduler::as_test` (`sirio_theme/src/lib.rs:1449`), the
`on_hover` signature and its keyboard suppression (bezel's `Hitbox::is_hovered` and
gpui-pre's default `InputModalityAware` agree), the `a11y_role`/`write_a11y_info` hooks
Sirio implements, `WindowOptions` literals (all use `..`), and every trait Sirio implements.

### 5.3 The three engine patches

Each vendored crate is the released `gpui-pre-*` 0.3.12 source plus only the change below,
with `rust/vendor/README.md` rewritten for the new package names (sections, standalone
lock and the `ci-linux.sh` stage that tests `vendor/gpui_linux` keep their shape).

1. **Wayland drag and drop (F-CORE-FILE-03A).** The race is still in gpui-pre, restructured:
   `DragState` carries `uri_read_generation` (`linux/wayland/client.rs:424-441`); `Drop` calls
   `handle_drop()`, which invalidates the pending read and returns early while `window` is
   unset, so a drop that beats a slow provider is discarded. The fix is redesigned on this
   structure with the same contract as today's `PendingDrop`: a drop that arrives before the
   path read resolves is remembered and delivered (`Entered` then `Submit`, at the last
   position `Motion` tracked) when the read completes; a drop after a newer `Enter` is
   still discarded. gpui-pre's own tests that pin the discard (`client.rs:3327-3366`) are
   changed to the new contract, and today's regression tests are carried over.
2. **Links in the user's environment.** `set_child_environment`
   (`sirio/src/main.rs:32116`) has no equivalent in gpui-pre-linux: `xdg-open`
   (`linux/platform.rs:956`) and `open::commands` (`:1187`) inherit the AppImage's
   environment (`LD_LIBRARY_PATH` and friends). The same API is re-added to the vendored
   crate, applied at both spawn points with this contract, which is today's: when a child
   environment is set, the child runs with exactly those pairs plus `XDG_ACTIVATION_TOKEN`
   when gpui has one; gpui-pre's `GraphicalEnvironment::apply_to` is not applied on top,
   because it would remove the user's `WAYLAND_DISPLAY` whenever Sirio forced X11 for itself
   (`sirio/src/display_backend.rs:100`). Unset, gpui-pre's own behaviour stands.
   gpui-pre also changed the order of link opening: it now tries the opener commands first,
   waiting for each one's exit status, and falls back to the desktop portal only if all fail
   (bezel spawned the first that started and waited for none). That is accepted as upstream
   behaviour: the link still opens in the user's browser.
3. **Windows bitblt swap chain (ADR 0003).** `create_swap_chain` is unchanged in gpui-pre
   (`directx_renderer.rs:1324-1346`, reached through `GPUI_DISABLE_DIRECT_COMPOSITION`), so
   the change applies verbatim. Today's vendored crate also carries `sirio_perf` spans in
   four files that the README does not mention; they are carried over and the README is
   corrected to list them.

4. **macOS translucency.** gpui-pre-macos changed the material of the `Blurred` window
   background from `Selection` to `UnderWindowBackground` with `BehindWindow`
   (`gpui-pre-macos-0.3.12/src/window.rs:3940-3950`), because `Selection` has no backdrop
   layer on macOS 27 and blurred nothing. Sirio uses `Blurred` on macOS
   (`sirio/src/shell_chrome.rs:29-36`). **Recommended: accept the upstream material** (it
   repairs the blur on the current macOS) as the one intended visible change of this step,
   checked by hand on a Mac; the alternative is vendoring gpui-pre-macos to restore
   `Selection`. **Decided 2026-11-17 (t2s, recommended option): accept upstream's material.**
   The PR names it as the one visible change, for Enzo to check on a Mac.
5. **Windows alpha blending (found in execution, 2026-11-18).** gpui-pre-windows 0.3.12
   changed `DestBlendAlpha` from `ONE` to `INV_SRC_ALPHA` in `create_blend_state` and
   `create_blend_state_for_path_sprite`. Sirio's Windows translucency (ADR 0003, and the
   opaque panels `sirio/src/shell_chrome.rs` keeps over the frame veil) was measured on the
   additive alpha, so the new blend would let more of the desktop through hover, selection
   and text edges. The vendored crate restores `ONE` at those two states, a fourth
   Sirio-local engine change; the other blend fields are unchanged from 0.3.8.
   gpui-pre-wgpu made the same change to its `paths` pipeline (alpha `One/One` →
   `One/OneMinusSrcAlpha`; `path_rasterization` is unchanged). It is left as upstream: Linux
   never uses a translucent window (`shell_chrome.rs` gives blur to macOS and Windows only),
   so the framebuffer's alpha never reaches the screen there. The macOS Metal blend states
   are compared once the macOS sources are fetched (Task 5); any change joins the Mac hand
   check.

### 5.4 The HTTP client gpui used to get for free

Bezel's `gpui_platform::application()`/`headless()` install a reqwest `HttpClient`
(`bezel-gpui-platform-0.3.8/src/gpui_platform.rs:20,41-42`); gpui-pre-platform's do not, and
gpui-pre falls back to `NullHttpClient` (`gpui-pre-0.3.12/src/app.rs:246`). Sirio starts from
`gpui_platform::application()` (`sirio/src/main.rs:32126`), and the vendored `bezel_markdown`
hands any `://` URL to `gpui::img` (`render.rs:1423-1427`), so without a client every remote
image in Markdown would silently stop loading — a change the compiler does not catch.
Sirio therefore installs an `HttpClient` itself with `.with_http_client(..)`, through one
helper that the app (`sirio/src/main.rs`), every probe and every binary that draws Markdown
(`sirio_ui/examples/*_probe.rs`, `sirio_ui/src/bin/changes_preview.rs`) start from, as
bezel's `application()`/`headless()` did for all of them. Whether it is a port of `bezel-zed-reqwest-client` onto
`gpui-pre-http-client` or a small reqwest-backed implementation is the builder's call; its
behaviour must match today's (same redirects, no new headers that identify the user). The
remote-image case of the Markdown Preview is added to the verification (§6.4).

### 5.5 Documentation

`CLAUDE.md`'s bezel line (engine → gpui-pre, the three bezel crates patched onto it),
`rust/vendor/README.md`, `docs/agents/architecture/crates.md` and
`docs/agents/build-and-release.md` where they name `bezel-gpui`; a release-notes line is
not needed (no visible change).

### 5.6 Frame-rate limits

gpui-pre 0.3.12's `WindowOptions::default()` sets `inactive_frame_interval` and
`low_power_frame_interval` to 33,333 µs (30 fps) and `thermal_frame_interval` to 16,667 µs
(60 fps) (`gpui-pre-0.3.12/src/platform.rs:2806-2808`, `src/window.rs:1953-1966`).

What changes against bezel 0.3.8: 0.3.8 already had the inactive (30 fps) and thermal (60 fps)
limits, but applied them only to forced frames and next-frame-callback frames. gpui-pre applies
all three to every non-mandatory frame, including the ones a `notify` requests, for example
terminal output arriving in an unfocused window. 0.3.8 had no low-power limit at all.

Decision (2026-11-19): the app's main window and the `changes_preview` bin set
`low_power_frame_interval: None`, so the focused window keeps its full frame rate under
power-saving profiles, as on 0.3.8. The inactive and thermal limits now reaching `notify`
frames is accepted as upstream behaviour and listed under Merge Danger in the PR. Examples and
probes stay on the defaults.

A late first frame on macOS (2026-11-19) is a separate problem. On the `macos-15` runner, an
unfocused window on gpui-pre drew a newly opened tab about 10 s late. As a result, host-sessions
case 5 failed ("the cat terminal is unknown") and case 21 went from 7 s to 15 s.

- **Not the inactive limit.** Setting `inactive_frame_interval: None` did not change it (run
  44438260522). The suspect is the `CAMetalDisplayLink` that gpui-pre-macos pauses between frames
  (#810).
- **Fix for case 5.** With no host running, the control socket's registry of the app's own panes
  was published only from render, so a tab opened since the last frame was unknown. Now
  `panel.read`/`state`/`scrollback` for an unregistered id first publish the registry on the GPUI
  thread (`ControlAction::SyncPanes`).
- **Case 21.** It still waits on the frame; #810 tracks it.

## 6. Verification — "the app is identical"

The baseline is captured from the last pre-swap commit with the same harness and display
settings as the run after the swap.

1. **Build and tests.** `cargo build` and `cargo nextest run` per crate for every workspace
   crate (Sirio's suites share a process badly under plain `cargo test`), plus
   `cargo test --manifest-path vendor/gpui_linux/Cargo.toml --locked`. The full
   `Scripts/ci.sh` runs only when Enzo asks.
2. **Pixels.** `Scripts/visual-sweep.sh --out-dir` before and after; every capture pair is
   judged with `Scripts/capture-diff.py` at its default `--tolerance 1` (the repo's standard,
   absorbing Mesa's one-level glyph noise). Gate: every pair within tolerance. A pair beyond it
   is investigated until its cause is known; one caused by the engine itself (text layout or
   the scene changed between zed `82aeef` and the gpui-pre snapshot) is listed in the
   evidence with its cause and goes to Enzo at the end of the branch, never silently allowed
   with `--allow` (those rows are for attributed feature changes).
3. **End to end.** Every suite in `docs/agents/e2e-tests.md` that runs on Linux, each
   printing its own OK line; the Ely chat UI E2E (`Scripts/Tests/test-ely-chat-ui-e2e.py`)
   included.
4. **Engine patches and the HTTP client.** XDND: `Scripts/wayland-drive.sh xdnd …
   --delay-ms 400` against a Terminal pane — the drop lands.
   *Waived for this step (2026-11-19):* it was not run (no sway on this machine, at baseline or
   after). It is replaced by a hand check owed by Enzo on his Hyprland session with the branch
   build (drag a file onto a terminal pane with a slow drop), listed in the PR; the PendingDrop
   logic is covered by the gpui_linux unit tests. Child environment: no repeatable
   check exists today, so one is added and proven on the baseline first: the app is started
   through a fake AppRun mount (the one `Scripts/Tests/test-sirioctl-e2e.sh` builds), a link
   is opened by a real click, and an `xdg-open` stub on the user's login PATH writes its own
   environment, which must hold the user's values (including `WAYLAND_DISPLAY`) and no mount
   entry; it must go red when `set_child_environment` is removed. A real AppImage run is a
   hand check listed as owed. Remote images:
   a Markdown Preview of a file whose image is served over loopback HTTP draws the picture,
   in the app (not a probe); the case is added and proven green on the baseline first. Windows translucency is a hand check on
   Windows.
5. **Cost.** `sirio_terminal/tests/still_frame_cost.rs` and the editor/chat frame probes
   that exist today, compared with the baseline.
6. **Other platforms.** `macos-check.yml` builds and links the app on macOS for the PR.
   `windows-check.yml` compiles no gpui crate, so Windows gets
   `cargo check --target x86_64-pc-windows-msvc --workspace` run directly (the same command
   as `ci-linux.sh`'s stage, without launching the script); if it stops at the SDK wall the
   result is recorded as unverified and the release job's Windows build is the first real
   compile, said so in the PR. A hand check on a Mac and on Windows (open a project, a
   terminal, a chat, the editor, the Markdown Preview, drag a file) is listed as owed.
7. **Baseline reds.** Every suite of 1–5 also runs on the pre-swap baseline. A suite red
   there (memory has `Scripts/Tests/test-update-ui-e2e.sh` red on main since `490c5067`) is
   recorded as pre-existing with its output and excluded from the gate; it is not fixed in
   this step.

The artifact: the before/after sweep directories, the diff report and the E2E OK lines, under
`artifacts/engine-swap/`, re-runnable from the commands above. There is no XDND transcript: the
§6.4 waiver replaces it.

## 7. Risks

- Rendering differences between zed `82aeef` and the gpui-pre snapshot (68 files differ,
  including text layout and the scene) — caught by §6.2; mitigation is to find the cause, not
  to accept a drift.
- The XDND redesign changes gpui-pre's tested behaviour — needs the live Wayland check. Waived for this
  step (2026-11-19), see §6.4: a hand check owed by Enzo on Hyprland.
- ~1,190 `#[gpui::test]` uses run on a new test scheduler — failures there are engine
  behaviour differences and are triaged one by one, never skipped.
- Build load on a shared machine — builds wait for a slot from t2s.

## 8. Left to the builder

- How `Keystroke::parse`'s new `&'static str` is met at the three sites (an owned-string
  API if gpui-pre has one, interning, or parsing differently).
- The exact value for `ScrollEvent::source` at each literal (matching what gpui-pre's
  platform layer sends for the same input).
- Task order and commit slicing inside the branch, as long as each commit builds.
- Which frame probes exist and are compared in §6.5.

## 9. Calls made while writing this (for Enzo to check)

- Pin `=0.3.12`, not 0.3.13, to match upstream Ely (§3).
- `bezel-syntax` is not vendored: patching `bezel-theme` is enough (§5.1).
- Deprecated gpui names are migrated, not allowed (§5.2).
- An HTTP client is installed by Sirio, in the app and every probe, so remote images keep
  loading (§5.4) — from G2 and "every Preview feature stays".
- `KeyBinding::new` with `secondary-` or a built string moves to `KeyBinding::load(.., cx)`
  (§5.2); `perf-native` moves to gpui-pre-scheduler (§5.2).
- The child-environment contract keeps the user's `WAYLAND_DISPLAY`, and gpui-pre's new
  link-opening order is accepted (§5.3.2).
- macOS `Blurred` takes upstream's material (§5.3.4), decided by t2s under the
  recommended-option rule; the PR lists it for Enzo's Mac check.
- Pixel gate at `capture-diff.py --tolerance 1`; engine-caused drift goes to Enzo at the end,
  never `--allow`ed (§6.2).
- A new repeatable child-environment check in the sirioctl E2E's fake mount; the real
  AppImage is a hand check (§6.4).
- Windows compile via the direct `cargo check --target x86_64-pc-windows-msvc`; Mac and
  Windows hand checks owed (§6.6).
- Suites red on the baseline are recorded, not fixed here (§6.7).
