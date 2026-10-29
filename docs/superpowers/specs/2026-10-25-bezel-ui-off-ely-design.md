# Bezel's UI crates out of Sirio, and an effort list in place of the slider

Date: 2026-10-25. Status: approved by Enzo; revised the same day for his "Aspetto di Ely" decision.

## Intent

Enzo wants the `bezel` UI library gone from Sirio, with Ely GPUI Components (already vendored at
`rust/vendor/ely-gpui-component`) doing the work instead. "Gone" is measured on the dependency
graph, not on the number of call sites: the `bezel` crate and the crates only it brings must no
longer be in `rust/Cargo.lock`.

The scope was narrowed in conversation to **the UI library**, not the rendering engine:

- **Removed:** `bezel` (0.1.4) and with it `bezel-ui`, `bezel-motion`, `bezel-agent`; also
  `bezel-editor`, which is dead code (only its `init` is called).
- **Stays, by Enzo's decision:**
  - The engine: `bezel-gpui` and its family (`bezel-gpui-*`, `bezel-zed-*`, the vendored
    `gpui_linux`/`gpui_windows`), which Ely itself runs on.
  - `bezel-theme`, `bezel-markdown` and `bezel-syntax`, which Ely has no equivalent for
    (tree-sitter highlighting, a selectable Markdown renderer, the syntax palette). These three
    depend only on `bezel-theme` and the engine, never on `bezel`, so they do not keep the
    removed crates alive.

Inside that scope (Enzo's decision of 2026-10-25, taken after research showed that almost no Ely
component draws like its bezel counterpart):

- **Ely has a component with the same role:** use it as Ely draws it. That surface takes Ely's
  look, in line with the chat, the editor and the change-request surfaces that are already on
  Ely. This covers text fields, tooltips, menus and popovers, buttons, badges, error and warning
  strips, cards, the progress bar, icons, and the scrollbars that scroll a `ScrollHandle`.
- **Ely has nothing for it:** rebuild it in Sirio on Ely's primitives and palette:
  - animations: the frame clock, hover fades, reduced motion, pause when inactive;
  - the loaders (cluster and bloom orbs, the gradient spinner);
  - the sidebar tree's keys;
  - the worktree combobox;
  - the Settings rows and empty state;
  - follow-to-bottom, and the scrollbars Sirio already draws itself.

One deliberate, visible change rides along: the chat's effort **slider** becomes an effort
**list**, and every new chat starts on **High**.

This continues the bezel → Ely programme of 2026-10-01 (SP1, the theme on Ely's Palette, merged as
#600; spec `2026-10-01-sirio-theme-on-ely-palette-design.md`). It replaces that programme's SP2–SP7
with one perimeter. The change-request surfaces already moved separately.

## Goals

1. `rust/Cargo.lock` has no `bezel`, `bezel-ui`, `bezel-motion`, `bezel-agent` or `bezel-editor`
   package. `cargo tree -i bezel-ui` (and the other four) finds nothing.
2. No `bezel::` path remains in any Sirio crate, example or test. The remaining bezel crates are
   reached only as `bezel_theme`, `bezel_markdown` (crate `markdown` today) and `bezel_syntax`
   (crate `syntax` today).
3. A surface may change only to an Ely component's own look. Every visual change replaces one
   bezel widget with one Ely component. It is captured before and after, in light and dark, and
   Enzo sees it. Behaviour does not change: keys, focus, IME, paste, Ctrl+C, scroll following,
   reduced motion and pause when inactive all work as they do today.
4. The effort control is a list (design "C" below), and every new chat starts on High.
5. The vendored `rust/vendor/bezel_ui` override, its `[patch.crates-io]` entry and its
   `vendor/README.md` section are gone. The one thing that override did (a text field with no
   fill of its own, for the chat composer) still works, through Ely.

## Non-goals

- Replacing `bezel-gpui` (the engine) with Zed's `gpui`.
- Replacing `bezel-theme`, `bezel-markdown` or `bezel-syntax`. That is a later project: Ely has no
  tree-sitter and no Markdown renderer.
- Redesigning. A surface takes the Ely component's default look; no new layouts and no
  restyling beyond what the component draws. A piece rebuilt in Sirio keeps today's look.
- Upstreaming anything to bezel or to Ely.

## Anti-goals (a failure even if it compiles)

- A visual change that is not an Ely component's own look, or one nobody saw: a missing icon, a
  clipped field, text on the wrong colour, a menu drawn off-screen.
- The chat composer losing any of its keys because Ely's text field binds them itself:
  - Enter to send and Shift+Enter for a newline;
  - Up/Down for the history and the mention/slash popups;
  - Cmd/Ctrl+V pasting an image first;
  - Escape and Tab.

  The same goes for the model search field's Enter and arrows.
- A regression in behaviour the bezel pieces carried and nobody wrote down:
  - the scrollbar drag anchoring;
  - the chat list following new output;
  - reduced motion and "pause when inactive";
  - IME and paste in the composer;
  - the Ctrl+C rules in `docs/agents/architecture/text-selection.md`.
- A scrollbar whose drag stops following the pointer. Research showed that bezel, Sirio's own
  bars and Ely all anchor the drag at the first move past gpui's 2 px threshold. That behaviour
  is today's, not an Ely defect, and it stays as it is.
- Hiding the old slider behind a flag instead of removing it.

## The effort list (decided)

The surface:

- The chip "Effort · High" stays where it is, a peer of the model chip.
- Clicking it opens, above the composer, a list built the same way as the Thinking and Model
  pickers: Ely's `Menu`/`MenuPanel` rows, with arrows, Enter and Esc. It replaces the slider.

The rows, in order (mock-up "C", chosen by Enzo):

1. **The levels.** The levels the agent advertises, in the agent's order: for Claude Opus,
   Low, Medium, High, Extra high, Max.
   - Each row has a small stepped bar on the right: as many steps as there are levels, filled up
     to that level's position (*Claude's call*: the count is the advertised levels, excluding
     Default and Ultracode).
   - An agent with three levels shows three steps.
2. **Ultracode** (where the catalog offers it, i.e. where `xhigh` is offered) has no bar but a
   muted note "+ workflow" (*Claude's call*). It is xhigh plus orchestration, not one step
   further on the scale.
3. **A separator, then Default.** The row is muted, with the note "model's own". It is shown only
   when the agent offers a reset value (today, native Claude's `EFFORT_DEFAULT`). Kept by Enzo's
   decision.

Behaviour:

- **Selection:** the selected row has a check. Choosing a row closes the list (*Claude's call*:
  the slider stayed open because it was dragged, a list does not need to). Esc closes without
  change. Clicking outside closes.
- **The chip:** it always names the current level. When the session is on Default, it reads as
  it does today for the reset value.
- **Starting on High.** Every chat starts on High when the agent offers a level whose value is
  `high`. Otherwise it stays on whatever the agent reports. This holds for:
  - **One place for every agent.** When an effort report arrives that is not `high` but offers
    it, `Chat` sends one `set_config_option` for it, the same request a click sends. The CLI is
    told for real, and the chip names the effort the agent actually runs at.
  - **Native Claude:** that request becomes the worker's `SetEffort` and the CLI's `set_effort`
    control request. Today `selected_effort` starts at `default` and nothing is sent until the
    user picks a level.
  - **ACP agents** (Codex, OpenCode, Pi, Oh-My-Pi): the request is `session/set_config_option`.
  - **No loops:** an agent that answers with another level is asked once, not again. The guard
    is the last automatic request's ladder and reported level.
- **Scope of a choice:** it holds for that chat only. The next new chat starts on High again.
  Effort is not persisted today, and this change does not start persisting it.
- **A restored chat** (reconnected after a restart) also starts on High, since nothing
  remembered its level (*Claude's call*).
- **Switching model inside a chat** (*Claude's call*):
  - a level the user chose stays if the new model offers it;
  - otherwise the chat goes to High if offered, else to what the agent reports;
  - a model without effort (Haiku) hides the chip, as today.

Removed with the slider: `effort_stop_share`, `effort_fraction_for_stop`,
`effort_stop_for_fraction`, the `SliderDrag`/`slider_fraction` use and the one
`Controls::slider` call.

The existing effort tests that drive the slider (`effort-stop-*`, `effort-slider`) or assert that
a fixture's current effort is `medium` go red by design. They are rewritten for the list and the
High default, not patched to pass.

## Inventory: every bezel use and where it goes

Measured on `975850c1`: 47 files. About 160 `bezel::ui` paths (93 of them
`bezel::ui::input::init`, mostly in tests), 65 `bezel::theme`, 23 `bezel::motion`, 1
`bezel::gpui`.

**How to read this table:**

- **Ely:** an Ely type with the same role replaces it, drawn as Ely draws it. The surface may
  look different afterwards; that change is captured and shown to Enzo.
- **Sirio:** Ely has nothing for it. Rebuilt in Sirio on Ely primitives and Sirio's palette,
  keeping today's look.
- **Rename:** the same type, reached through `bezel_theme` instead of `bezel::theme`.

"Checked" means the Ely type was found in `rust/vendor/ely-gpui-component/src` during this design;
it says nothing about how the result looks.

| bezel use | Where (main files) | Goes to | Notes |
|---|---|---|---|
| `ui::input::TextField`, `input::init`, `Shape::Grow`, `Copy`/`Cut`/`Paste`/`InsertNewline` | composer, settings pages, project forms, worktree picker, browser, session panel, `selectable_markdown`, `sirio/main.rs`, tests | **Ely** `forms::text::TextInput` (`multi_line(min,max)`, actions in `forms/text/actions.rs`), checked | 64 uses of `TextInput` already exist. Ely's field draws its own frame and caret, and binds Enter, Up/Down and paste under its fixed `ElyInput` key context. Vendor Ely is changed so that a host can keep those keys: the composer's Enter/Shift+Enter, history and popup arrows, image-first paste, Escape and Tab, and the model search field's Enter and arrows, behave as today. The change and the transparent fill builder (Ely's `LOCAL-CHANGES.md` line 108 already names the bezel override it replaces) are logged there. Ely's Copy/Cut/Paste/Newline actions are exported so context menus dispatch them. `TextInput::new` needs a `Window`; Ely gains a `TextInput::detached(cx)` constructor that does not (a vendor change, logged), so the seven constructions stay where they are. |
| `ui::tooltip::Tooltip`, `with_keystroke` | `controls.rs`, `file_view.rs`, `open_list.rs`, `pane_launcher.rs`, agents page, chat, main | **Ely** `primitives::tooltip` (`text`, `with_meta`, `rich`), checked | The keystroke is shown through `with_meta` or `rich` with Ely's `typography::keys`. Ely's tooltip is solid with a smaller radius; that look is accepted. |
| `ui::popover` (`Popup`, `popover_card`, `anchored_menu_below`, `tracked_upper`), `ui::surface` (`popover`, `Surfaced`) | `tab_bar.rs`, `sidebar`, `loading.rs`, `transcript.rs`, `files.rs`, `controls.rs`, `composer_view.rs`, main | **Ely** `overlays::popover`, `layout::surface`, `menus`; anchoring and redaction helpers that Ely lacks (`anchored_menu_below`, `tracked_upper`, `redacted_rows`, `reap_popup`) → **Sirio** | Most menus already go through Ely's `layer`/`MenuPanel`. Popovers take Ely's solid fill and radius; bezel's glass goes. A popover still never shows through what is under it (translucency is never in the foreground). |
| `ui::scroll` (`ScrollbarState`, `ScrollbarDrag`, `MIN_THUMB`, `FollowState`, `scrollbar`) | `sirio_terminal/scrollbar.rs`, `horizontal_scroll.rs`, `trace_tab.rs`, `file_view.rs`, `chat/list_scroll.rs`, `thought.rs`, `ci_log_tab.rs` | A bar over a gpui `ScrollHandle` → **Ely** `layout::scroll::Scrollbar`. A bar whose thumb Sirio computes itself (the terminal's scrollback, the chat list, the CI log) and `FollowState` → **Sirio**, with today's `thumb`/`offset_for_thumb`/`MIN_THUMB` maths | Ely's bar draws with its own colours and width; that look is accepted where it is used. All of them, Ely's included, anchor the drag at the first move past gpui's 2 px threshold, as today. Following new output is Sirio's and does not change. |
| `motion::Painter` (frame lease), `Fade`, `hover_listener`/`hover_t`, `menu_in`, `AppExt` (reduced motion, pause when inactive) | 15 files: chat, composer, loading, list scroll, trace, CI log, thought, file view, tab bar, sidebar, files, `ely.rs`, main | **Sirio**: a small `sirio_ui` motion module | Ely's `motion` is a set of animated elements (`Transition`, `Spinner`, `Skeleton`, …) over gpui animations. It has no shared frame clock, repaint lease or app-level reduced-motion/pause switch. Same semantics as bezel-motion 0.1.4 (MIT); porting its code with attribution is allowed. |
| `ui::loaders` (`orb` Cluster/Bloom, `mini_gradient_spinner`) | `loading.rs`, `orbit.rs` | **Sirio** | Ely has spinners and skeletons, not these shapes. Pixel-matched. |
| `ui::tree` (`Row`, `step`, `Direction`, `init`) | `sidebar/mod.rs`, `sidebar/row.rs`, main | **Sirio** | Keyboard navigation over branch/leaf rows. Same keys, same behaviour. |
| `ui::combobox` (`Combobox`, `ComboboxEvent`, `init`) | `worktree_picker.rs`, main | **Sirio** on Ely's `TextInput` + list (`ListBox`/`MenuPanel`) | Ely's `Select` is not a type-to-filter combobox. |
| `widgets::Controls::slider`, `SliderDrag`, `slider_fraction` | chat `controls.rs` | **Removed** by the effort list | |
| `widgets::Controls::progress_bar` | `loading.rs` | **Ely** `motion::progress::ProgressBar` | |
| `widgets::Buttons::button`, `ButtonStyle` | settings pages, `turn_rail.rs`, `ely_tests.rs` | **Ely** `buttons::Button` + `ButtonVariant`, checked | Ghost → Ely's ghost variant, Prominent → its primary; each button takes Ely's look. |
| `widgets::Scaffolding` (`page_header`, `page_subtitle`, `group_box`, `card_row`, `row_title`, `meta_line`) | settings: `agents_page`, `git_hosting_page`, `language_servers_page`, `settings.rs` | **Sirio**: settings scaffolding in `sirio_ui::settings` | Ely has no settings-page scaffolding. |
| `widgets::Status` (`error_strip`, `warning_strip`, `step_output`) | settings pages, `tool_calls.rs` | strips → **Ely** `Alert` (with the severity of the strip), its content a `selectable_text(..)`; `step_output` → **Sirio** | Error and warning text stays selectable. Revised by plan 2's pre-flight: `InlineMessage` cannot host selectable text, `Alert` can. |
| `widgets::Content` (`empty_state`, `badge_active`) | settings pages | `badge_active` → **Ely** `data_display::badge`; `empty_state` → **Sirio** | |
| `widgets::Takeover`, `status_dot` | `chat/thought.rs`, chat | **Sirio** | `Takeover` is an `Option<bool>` ("auto until the user toggles"); `status_dot` is a 6 px round dot. |
| `ui::icons` (`Assets` fallback, `icon()`, `TRASH_BIN_MINIMALISTIC`, `MAGNIFER`, `DOCUMENT`) | `ely.rs`, `sidebar/sessions.rs`, `git_hosting_page.rs`, `composer_view.rs` | **Ely**: each icon becomes Ely's Lucide icon with the same meaning | Four paths are served only by bezel's `Assets` fallback in `ely.rs`'s `AppAssets`: `magnifer`, `document`, `trash-bin-minimalistic`, `danger-triangle`. Each is swapped (search, file, trash, alert) before the fallback goes. A path with no Lucide counterpart keeps its SVG, copied into `rust/assets/icons/` with its licence (Solar Icons, CC BY 4.0). |
| `theme::Theme` (`of`, `dark`, `light`, `surface_radius`, `panel_radius`), `ink`, `HighlightKind`, `SyntaxPalette`, `Appearance`, `current_appearance`, `lock_appearance` | ~30 files, `sirio_syntax`, `sirio_terminal` | **Rename** to `bezel_theme::…` (same types) | Mechanical. `sirio_ui` and `sirio_terminal` take `bezel_theme` directly. |
| `bezel::gpui` | 1 site | **Rename** to `gpui` | |
| `bezel-editor` | `sirio/Cargo.toml`, `init` only | **Removed** | |

`sirio_terminal` depends on `bezel` only for its scrollbar and `ink`. After the change it reaches
the scrollbar through `sirio_ui`, which it already depends on, and `ink` through `bezel_theme`. No
new crate edge is needed; `docs/agents/architecture/crates.md` line 67 is updated.

## Constraints

- **Rules from `CLAUDE.md` that apply here:**
  - text meant to be read uses `selectable_text(..)`, Ctrl+C is never swallowed, and a terminal's
    Ctrl+C stays SIGINT;
  - the file editor keeps every hook `LOCAL-CHANGES.md` lists;
  - perf labels stay content-free.
- **Vendored Ely:** any change to `rust/vendor/ely-gpui-component` is listed in its
  `LOCAL-CHANGES.md`, with why.
- **Testing is end to end.** Each delivery ends in a re-runnable artifact. No unit test is
  written after its code. Tautological and change-detector tests are not added. Existing
  `#[test]`s that only pinned bezel internals are removed, not ported.
- **Verification:**
  - iterate with `cargo build -p` / `cargo nextest run -p`;
  - `Scripts/ci.sh` runs only when Enzo asks for it;
  - a public type that changes is checked with `--workspace --all-targets` (enum variants break
    other crates' test targets).
- **macOS is the reference platform:** `macos-check.yml` must stay green. Nothing here is
  platform-specific, but the tray, titlebar and translucency paths are drawn on macOS.

## Delivery

Two independent deliveries, so the effort feature is not tangled with the migration's visual
changes (*Claude's call*):

1. **`feat:` the effort list and the High default.** A small PR against `main`.
   - It is the 0.36.1 cycle's first `feat:` (v0.36.0 was tagged and #667 opened 0.36.1 while
     it was built), so it moves the version from `0.36.1` to `0.37.0`.
   - Evidence: an E2E run (the Ely chat UI E2E, `Scripts/Tests/test-ely-chat-ui-e2e.py`, or the
     session-panel harness) that:
     - opens the list and captures it in light and dark;
     - picks a level;
     - checks that a new chat opens on High, asserted on the request that reaches the agent
       (native Claude's `set_effort`, an ACP agent's `set_config_option`), not only on the chip.

     It needs the lavapipe environment for the window captures.
2. **`refactor:` bezel UI off Sirio.** One or more PRs. The split is the plan's call, for
   example rename + motion, then inputs and tooltips, then popovers and scroll, then settings
   and the rebuilt pieces, then removal. Each PR leaves `main` building, and changes only the
   look of the surfaces it moves to Ely.
   - Evidence:
     - a `Scripts/visual-sweep.sh` / `Scripts/Tests/test-visual-sweep.sh` capture set taken
       **before** the first PR and **after** each one, in light and dark, with every changed
       region listed against the inventory row that changed it. Enzo reviews the set before the
       PR is merged;
     - `Scripts/Tests/test-theme-sweep.sh`;
     - the E2Es for the touched areas (editor, markdown probes, chat session panel, open list);
     - the acceptance checks below.

### How the change is captured (*Claude's calls*, settled after the builder check; revised for
"Aspetto di Ely")

- **Settings.** `Scripts/visual-sweep.sh` cannot open Settings: the control socket has no method
  for it, and the script lists those pages as gaps. A `sirio_ui` probe example renders each
  Settings page and the Changes tab on its own, the way `markdown_selection_probe` and
  `ely_chat_probe` already do. It runs under `--xvfb` and is wrapped by a script in
  `Scripts/Tests/` that prints an `OK` line. No control method is added, so nothing new is
  reachable over the socket.
- **Where:**
  - Linux captures run on this machine under Xvfb with lavapipe.
  - macOS is checked by Enzo by hand, once, after the last refactor PR, on a build of that branch
    with translucency on and off. A short checklist of the surfaces to open comes with the PR.
    No macOS capture is automated: `macos-check.yml` draws nothing.
- **Which diffs are allowed:**
  - Captures are taken with reduced motion on and the clock-driven parts (caret, spinners, trace
    timers) masked by fixed rectangles listed beside the script.
  - Outside the masks, every changed region is attributed to an inventory row marked **Ely**. A
    changed region in a surface the PR did not move to Ely, or in a piece rebuilt in **Sirio**, is
    a defect and is fixed before the PR.
  - Each PR carries a before/after page of its changed captures, light and dark side by side, for
    Enzo to review.
  - Before the first refactor PR, two runs of the same build are diffed to prove the setup is
    deterministic; a setup that is not deterministic is fixed before it gates anything.
- **Behaviour, not only pixels.** Before the PR that removes the piece behind it, each named
  behaviour has a check that goes red when it breaks. An existing E2E or probe is reused where one
  exists; otherwise one is added, in the shape of `Scripts/Tests/`:
  - scrollbar drag anchoring (the thumb stays under the pointer);
  - the chat list following new output;
  - reduced motion;
  - pause when inactive, as idle CPU with a spinner on screen in an unfocused window (bezel-motion
    records 22 % of a core without the pause and 2 % with it);
  - IME and paste in the composer;
  - Ctrl+C in a text field, in selected text and in the terminal (SIGINT).

## Acceptance

- `grep -E '^name = "bezel(-ui|-motion|-agent|-editor)?"$' rust/Cargo.lock` prints nothing.
- `grep -rn 'bezel::' rust/crates` prints nothing.
- `rust/vendor/bezel_ui` does not exist, and `rust/Cargo.toml` has no `bezel` or `bezel-editor`
  entry.
- In the before/after visual sweep, every changed region traces to an inventory row marked
  **Ely**, and Enzo has reviewed each PR's set.
- The effort E2E artifact shows the list (light and dark) and a new chat on High, as the agent
  received it.
- The Settings probe and the behaviour checks above print their `OK` lines on the last refactor
  PR's head.
- Enzo's macOS hand check is recorded in that PR.
- `Scripts/ci.sh` prints `CI OK` when Enzo asks for it, and `macos-check.yml` and
  `windows-check.yml` are green on the PRs.
- Docs no longer name bezel as the shell's UI library:
  - `CLAUDE.md` External references;
  - `docs/agents/architecture/crates.md`;
  - `rust/vendor/README.md` (the `bezel_ui` section goes, and the `bezel_markdown` section stops
    saying it is used by `bezel-editor`).

## Calls Claude made (for Enzo to check)

- The stepped bar counts only the advertised levels, without Default and Ultracode. Ultracode
  carries "+ workflow" instead.
- Choosing a level closes the list.
- A restored chat starts on High. A model switch keeps a chosen level if the new model has it.
- Enzo decided (2026-10-25, "Aspetto di Ely") that a piece Ely has a component for takes Ely's
  look. From that, my calls: icons become Ely's Lucide icons; bars over a `ScrollHandle` become
  Ely's bar, while the bars Sirio computes itself stay Sirio's; Ghost/Prominent buttons map to
  Ely's ghost/primary.
- Motion becomes Sirio's own small module, ported from bezel-motion (MIT) with attribution.
- Two deliveries: the effort feature first and separate, then the refactor.
- Native Claude is told High at launch, so the chip names the CLI's real effort.
- Settings captures come from a probe example, not from a new control-socket method.
- The capture gate: outside masked, clock-driven regions, every changed region traces to an
  Ely row; a change anywhere else is a defect. This runs after a determinism check.
- Behaviour checks come before the PR that removes the piece behind them.
- macOS: one hand check by Enzo, after the last refactor PR, with and without translucency.

## Left to the builder

- The exact split of the refactor into PRs, and their order.
- How Ely's text field yields its keys to the host (an opt-out per binding, a host key context
  that wins, or another hook), as long as it is logged in `LOCAL-CHANGES.md` and the composer
  and model search keys behave as today.
- Module names and placement inside `sirio_ui` for the rebuilt pieces.
- For each scrollbar site, whether it scrolls a `ScrollHandle` (Ely's bar) or computes its own
  thumb (Sirio's).

## Open questions

None. The builder check raised five, and all five are settled above:

- Settings captures;
- the diff rule and where captures run;
- the macOS translucency check;
- behaviour checks;
- High reaching the CLI.
