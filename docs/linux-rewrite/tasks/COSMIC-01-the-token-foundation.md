# COSMIC-01 — The token foundation: Tiller in Pop!_OS COSMIC dress

**You are sonnet, pane `w1:p5`, running Claude Code.** You are new to this project, so this brief is
everything you need. Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch
`linux/gpui-waku`.

## The change of direction

Tiller is a native Linux app in Rust on **GPUI** (Zed's UI framework) — no webview, no HTML. Until
tonight its visual reference was [waku](https://github.com/egoist/waku). The operator has changed
that: **the UI is being redone in the Pop!_OS COSMIC design language**, taking
[libcosmic](https://github.com/pop-os/libcosmic) as the source. You have full freedom in the redo.

Three other agents are building features in parallel right now. You are not competing with them —
you are laying the surface everything else will be drawn on.

## What I already checked, so you do not have to

**libcosmic is built on iced.** Its own description: *"a platform toolkit based on iced for creating
applets and applications for the COSMIC™ desktop."* iced and GPUI are two independent frameworks,
each owning its own window, event loop and renderer. **You cannot use libcosmic's widgets in this
app.** Do not spend an hour discovering that.

But that is not the end of it, and the interesting part is the exception:

**`cosmic-theme` has zero GUI dependencies.** Its Cargo.toml is `palette`, `hex_color`, `almost`,
`serde`, `serde_json`, `ron`, `csscolorparser`, `cosmic-config`, `configparser`, `dirs`, `thiserror`.
No iced. No renderer. It is a **pure data and token crate** — which means the COSMIC design system
itself is genuinely available to a GPUI app, even though the toolkit is not.

So the split for this piece is: **tokens from COSMIC, widgets written from scratch in GPUI.**

## Your first judgement call: depend, or transcribe

Try depending on `cosmic-theme` directly. If it builds cleanly, that is the better answer, and there
is a real prize behind it: `cosmic-config` can read the **user's actual COSMIC configuration**, so on
Pop!_OS Tiller would pick up their real accent colour and light/dark preference instead of guessing.
An app that follows the desktop's own accent is a different quality of native than one that ships a
palette.

If `cosmic-config` drags in a daemon dependency, fights the build, or wants a running service with no
sane fallback, then transcribe the token *values* into our own crate instead and move on. Values from
a published design system are a specification, not someone's source code.

**Either answer is defensible. State which you chose and why** — and if you depend on the crate,
make sure the app still renders correctly on a machine with no COSMIC config present.

## The design system, concretely

The parts you are implementing, from the `cosmic-theme` model:

**Container hierarchy — this is COSMIC's distinctive structural idea.** Three nested layers,
`background` → `primary` → `secondary`, each a component carrying its own base colour, `on` (text)
colour, divider, and hover/pressed overlays. Nested surfaces step through the hierarchy rather than
each picking its own grey. Tiller's shell is a natural fit: window background, then the sidebar and
terminal host, then cards and popovers inside them. Getting this hierarchy right is most of what
makes something look like COSMIC rather than merely dark.

**Spacing scale** (exact defaults, `u16`):

```
space_none   0     space_s    16     space_xxl   64
space_xxxs   4     space_m    24     space_xxxl 128
space_xxs    8     space_l    32
space_xs    12     space_xl   48
```

**Corner radii**: `radius_0`, `radius_xs`, `radius_s`, `radius_m`, `radius_l`, `radius_xl`, each
`[f32; 4]` so corners can differ per side.

**Semantic component colours**, not raw palette entries: `button`, `accent`, `accent_button`,
`success` / `success_button`, `destructive` / `destructive_button`, `warning` / `warning_button`,
`icon_button`, `link_button`, `list_button`, `text_button`, plus `accent_text`, `control_tint`,
`text_tint`, `window_hint`, `shade`.

**Other settings**: `active_hint` outline width defaults to `3`, window `gaps` to `(0, 8)`, and there
is a `frosted` blur strength with 14 levels and an `alpha_map` running roughly 0.6–0.90.

Fetch the crate source for anything above that you need in more detail — the numbers here are the
ones I verified, not the whole model.

## The piece

**1. Build the token layer.** `crates/tiller_theme/` is yours (see Rules). It is currently one
55 KB `lib.rs` exposing `Theme`, `Typography` and `ThemeMode`, consumed as `tiller_theme::Theme` in
13 places. Add the COSMIC layer in **new files**; do not rewrite `lib.rs` wholesale while other
agents are compiling against it. The existing `Theme` type is the seam — the rest of the app should
keep compiling.

**2. Get light and dark both right.** COSMIC ships both and the hierarchy inverts between them; a
dark-only implementation is half a design system.

**3. Prove it on one surface.** Pick a self-contained one and restyle it end to end so the hierarchy,
spacing scale and semantic colours are all visibly exercised. One surface done properly is worth more
than five surfaces half-converted, and it becomes the reference the other agents copy.

**4. Write `docs/linux-rewrite/COSMIC-DESIGN.md`** — the token mapping and the rules you want the
other builders to follow, so the conversion can proceed in parallel without three people inventing
three greys.

## Evidence

This project has a hard rule, in `docs/linux-rewrite/EVIDENCE-STANDARD.md`: **a verdict made by
reading code is not a verdict.** Measured on this project's own ledger, rows judged by reading ran
~21% false; rows judged by executing ran ~0%. Read that file before you report anything.

For your tier: **drawn tests** using GPUI's `TestAppContext` / `VisualTestContext`, elements located
with `.debug_selector(id)`, hardened with the full `run_until_parked()` pump — the existing tests in
`tiller_ui` show the pattern.

**The display works** — `Scripts/linux-shot.sh` returns `1470x833 · 8401 colours`. Take screenshots,
both modes. For this piece a screenshot is real evidence and not merely nice to have, because the
claim *is* visual; but it does not replace a drawn test. A shot proves it looked right once, a test
proves it stays right.

The gate is `Scripts/ci-linux.sh`. It is currently red for reasons that are not yours — formatting
drift in `tiller_ui/src/sidebar.rs` (another agent is live in that file) and PTY tests that abort
inside GPUI's deterministic scheduler (being fixed in another piece). Judge yourself on
`cargo test -p tiller_theme` plus whatever you touch, and **name not-yours failures separately**
rather than absorbing them.

## Rules

- **`crates/tiller_theme/**` is yours** from now. `pi` is live in `tiller_ui/src/settings.rs`,
  `sidebar.rs` and `chat.rs`, and `codex12` has `tiller_ui/src/tab_bar.rs` — **do not edit those
  four files.** If your reference surface needs a change in `tiller_ui`, pick a file none of those
  four touch, and say in your report which files you took.
- `codex11` is in `tiller_terminal`, `codex12` in `tiller/src/main.rs` and `tiller_control`. Stay out.
- **Take inspiration, never transplanted code.** Depending on the published `cosmic-theme` crate is
  not transplanting — that is a dependency, and the operator has explicitly allowed it. Copying
  libcosmic's *widget source* into our tree is, and the critic treats transplanted code as a gap
  every time.
- There is a ledger, `docs/linux-rewrite/INVENTORY-LEDGER.md`, with 389 feature rows. **Do not edit
  it** — only the critic changes a verdict. Mark anything you claim as `builder-claimed, unverified`.
- **Proceed without asking for approval.** The design decisions in this brief are yours.

## Reporting

**12 lines or fewer**: depend-or-transcribe and why, whether the user's real COSMIC accent is picked
up, what the token layer exposes and how the existing `Theme` seam survived, the surface you
restyled and the files you took, light and dark both shown, tests by name, screenshot paths, the gate
result with not-yours failures named separately, and the honest remainder.
