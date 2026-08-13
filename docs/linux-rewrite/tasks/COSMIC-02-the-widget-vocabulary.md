# COSMIC-02 — Wire the token layer to something, then convert the vocabulary

**You are sonnet, pane `w1:p5`.** Your context was just reset, so this brief is everything you need.

## COSMIC-01 landed well, and it has one problem you could not have seen

You made the right call and you made it the expensive, honest way. `cosmic-theme` enables
`cosmic-config`'s `subscription` feature unconditionally, which pulls `iced_futures`; you proved that
by compiling a scratch probe and reading `cargo tree`, not by reading a changelog. Cargo's feature
unification leaves a consumer no lever, so **transcribe over depend** was correct. And you did not
give up the prize: `cosmic/live.rs` reads COSMIC's real on-disk RON config, so a user's actual
system theme can reach this app without a GUI dependency.

Then there is this:

```
$ grep -n "cosmic" crates/tiller_theme/src/lib.rs
29:pub mod cosmic;
```

**One line. `Theme` consumes none of it.** Ten files of palette, spacing, radii, semantic colours and
a live config reader, and not one pixel anywhere in the app is drawn from any of it.

Read that against `docs/linux-rewrite/DEAD-MODELS.md`, which `fable` produced this week: a function
built, tested, marked PASSED, and called by nothing is **this project's most common defect** — 58 of
them in one sweep. COSMIC-01 as it stands is that defect, in the design system whose entire job is
to be consumed. This is not a criticism of the work; the brief told you to build the foundation and
you built it. It is the reason this piece exists, and it is the first thing to fix.

The tree is green now (`cargo check --workspace` exit 0 — `codex11` fixed the `E0004` that blocked
you), so nothing stands between you and a running app.

## The piece

### 1. Wire `cosmic` into `Theme` — nothing else in this brief counts until this is true

`Theme` is at `crates/tiller_theme/src/lib.rs:656`, with `light()`, `dark()`, `for_mode()` and
`system()`. Those are the seams. When you are done, a colour, spacing value or radius that reaches
the screen must be **traceable to a COSMIC token**, and `live.rs` must be reachable in production —
not merely reachable from a test.

The test that proves this is not "the tokens parse". It is **a drawn test asserting a real widget
renders a COSMIC-derived value**, and a check that the live reader is actually consulted on the boot
path. Your own `live.rs` is currently as dead as everything else in `cosmic/`; a config reader
nothing calls is a file, not a feature.

### 2. `controls.rs` — the whole widget vocabulary, and it is unclaimed

`crates/tiller_ui/src/controls.rs`, 408 lines, 15 `pub fn`: `card`, `row`, `row_view`, `separator`,
`toggle`, `segmented`, `stepper`, `stepper_with_unit`, `badge`, `subsection_header`, `action_row`,
`account_row`, `button`, `color_swatch`, `section`.

**This is the highest-leverage file in the tree for you**, and it is why you get it rather than a
surface. It has ~80 call sites — `card` 17, `row` 17, `separator` 17, `toggle` 7, `button` 7 — inside
`settings.rs`, `sidebar.rs`, `chat.rs`, `changes.rs` and more, every one of which belongs to another
builder. Converting `controls.rs` propagates COSMIC into files you are forbidden to touch, with no
ownership collision at all. One file, most of the app.

What is actually in there:

- **45 hardcoded `px()` literals across 20 distinct values** — `10.0` nine times, `2.0` five times,
  `28.0` four, `7.0`/`5.0`/`44.0` three each. COSMIC's scale is `4 8 12 16 24 32 48 64 128`. Almost
  none of these are on it.
- **`card()` uses `theme.radii.user_pill`** — a *pill* radius on a *card*. A waku token borrowed for
  a shape it was never meant for.
- **`row_view()` takes `_theme` and ignores it**, so it hardcodes everything it draws.
- Colour literals: **zero**. Colours already come from the theme. Spacing and radii do not. That
  asymmetry is the actual work.

### 3. The design decision, which is yours and is genuinely hard

COSMIC's container hierarchy is **nested**: `background → primary → secondary`, each a full Component
with `base`, `on`, `divider`, `hover`, `pressed`. A card sitting on `background` should draw itself
from `primary`. A card nested *inside* another card should draw from `secondary`.

Every function in `controls.rs` takes `theme: Theme` by value and **cannot know which level it is
being drawn at.** `card(theme)` produces the same pixels wherever it lands.

You have real options — thread a container level through the signatures, resolve it from GPUI's
element context, keep a flat mapping and accept that nesting is wrong, or something better. Each
costs something: threading touches ~80 call sites in other builders' files, which the ownership rules
make expensive; a flat mapping is cheap and quietly wrong at depth.

**Pick one and defend it in two sentences in your report.** If you choose the flat mapping, say
plainly that nested containers are wrong and that it is deliberate — a known, disclosed limit is
worth more than a silent one. What this project cannot afford is the third option, where the choice
is never named and nobody knows which was made.

### 4. Two tokens `codex12` asked for, from `QUEUE.md`

**menu width** and a **geometric hairline**, named while building the tab context menu in P60. Note
`separator()` currently uses `theme.hairline` with a hardcoded `px(1.0)` height and `px(10.0)`
margin — the geometric hairline probably belongs there too.

### 5. If it fits: `composer.rs` (746 lines) — also unclaimed

Take it only if the vocabulary conversion is genuinely done. It is better to hand back one converted
`controls.rs` with proof than two files with neither proven.

## Evidence — and this time the visual half is owed

Your COSMIC-01 remainder was, verbatim: *"prova visiva mancante finché l'altro agente non sblocca
tiller_ui."* That blocker is gone, so the debt is now payable and it is part of this piece.

- **Screenshots in both light and dark.** COSMIC ships both and the hierarchy inverts between them; a
  dark-only implementation is half a design system. Compare against
  `reference/linux-progress/baselines/2026-08-13-pre-cosmic.png`.
- **`Scripts/linux-shot.sh` writes a fixed path and every agent clobbers it.** Copy anything worth
  keeping to `reference/linux-progress/` under a name of your own, immediately.
- **Named drawn tests** per `docs/linux-rewrite/EVIDENCE-STANDARD.md` — `TestAppContext` /
  `VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump. For a visual change the
  screenshot is real evidence and not merely nice to have, because the claim *is* visual — but it
  does not replace a drawn test, because a screenshot proves one frame on one machine.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_theme/**`, `tiller_ui/src/controls.rs`, `tiller_ui/src/titlebar.rs`, and
  `composer.rs` if you reach it.**
- **Do not edit** `settings.rs`, `sidebar.rs`, `chat.rs`, `status_bar.rs` (`pi`), `changes.rs`,
  `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**` (`codex11`),
  `tab_bar.rs`, `tiller/src/main.rs`, `tiller_control/**` (`codex12`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms. If changing `Theme`'s shape breaks another builder's file, fix exactly the breakage and
  nothing else.
- Expect transient red from four builders editing live. **Name not-yours failures separately.**
- **libcosmic is inspiration, not a source.** The user explicitly permitted *using* the library; you
  established on evidence that its dependency graph forbids it here. Everything else in this repo is
  written from scratch — transplanted code is a gap, always.
- **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: whether `Theme` now consumes `cosmic` and how a drawn pixel traces to a token,
whether `live.rs` is reachable in production or still only from tests, how many of the 45 literals
became COSMIC spacing and what you did with `user_pill`, **the container-level decision and your two
sentences defending it**, the two tokens for `codex12`, screenshots in both modes with their saved
paths, tests by name, the gate with not-yours failures named separately, and the honest remainder.
