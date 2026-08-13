# P75 — Settings: the controls that render but do nothing

**This brief is everything you need; your context may have just reset.**

## Ownership has changed. Read this first.

`tiller_ui/src/settings.rs` was `pi`'s. **It is now yours**, by the user's decision, because `pi` was
holding four surfaces and 48 of the 92 rows still to construct while you sat idle. `pi` has been told.
It is currently working in `chat.rs` and is not in your file.

You keep everything you already own — `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` —
and this is a natural extension of it, not a detour: your COSMIC pass already defines the vocabulary
these controls should be built from.

**A `push it` was sitting unsent in your composer. It was never executed and has now been discarded
by the user's instruction.** Do not push anything. Commit locally on `linux/gpui-waku` as everyone
else does.

## The 11 rows, and the shape that unites most of them

| row | verdict | what the ledger says |
|---|---|---|
| `F-SET-09` | FAILED — defective | provisioner **exists and is tested** (`tiller_project/skill.rs` `agent_skill_install_command`) but has **zero app callers**; the Install Skill button at `settings.rs:1739` renders and reaches nothing |
| `F-SET-16` | FAILED — absent | "Search agents" is a **static text child in a pill-shaped div, not an input** (`settings.rs:1183`); Refresh's handler is a literal no-op |
| `F-SET-22` | FAILED — absent | Agent Colors renders five rows of coloured glyphs and **display-only** `color_swatch` pills — no `on_click`, so no colour choice exists to exercise |
| `F-SET-21` | FAILED — absent | exactly one Files icon choice on Linux — `SEGMENTED_FILE_ICONS=["Material"]` (`settings.rs:30`), `file_icon_choices()` (`settings.rs:161-168`) |
| `F-SET-11` | FAILED — absent | Loading/Loaded/Stale + dimming render, but the four unavailable reasons (Not found / Logged out / Timed out / Error) **all render the same `—`** — 4 visuals for 7 claimed states |
| `F-SET-12` | FAILED — absent | no cookie UI or state |
| `F-SET-13` | FAILED — absent | no cookie UI or state |
| `F-SET-14` | FAILED — absent | status-only provider cards; no add / re-auth / remove |
| `F-SET-15` | FAILED — absent | no multi-account model |
| `F-SET-17` | FAILED — absent | no agent registry |
| `F-SET-18` | FAILED — absent | availability badges only; no install / update / retry actions |

**Four of these are one shape: a control that renders and does nothing.** `F-SET-09`, `F-SET-16`,
`F-SET-22` and arguably `F-SET-21`. That shape has already been diagnosed twice elsewhere in this
project under different names — `F-PRJ-04` turned out to be *a wired surface that four call sites
declined to use*, not a missing surface. **Check before you build.** For each of the four, the first
question is not "how do I build this" but "what already exists that this control should be reaching?"
`F-SET-09` tells you outright: the provisioner is built and tested and has no callers. That row needs
a *connection*, not a feature.

**Start with those four.** They are the cheapest rows on your list and they are the ones most likely
to be mis-specified as absent.

The remaining seven are genuine construction and need state that does not exist (cookies, provider
accounts, an agent registry). Take them in the order above; if you run out of time, having four rows
truly closed beats eleven rows half-started.

## One row needs a decision, not code

`F-SET-21`: Linux offers exactly one Files icon set, so a "choice" of one is not a defect to fix by
inventing a second icon set. Either the row is `N/A — platform`, or the control should say why it has
one option. **Do not invent a second icon pack.** Name what you think it is and why; the verdict is
`pireview`'s.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

The test that counts **provokes the real behaviour through the control**, not the function behind it.
For `F-SET-09`, click the Install Skill button and assert the provisioner ran — a test that calls
`agent_skill_install_command` directly proves the thing that was never broken. This distinction is
the single most repeated lesson in this project: the mechanism was built, the wiring was not, and a
test aimed at the mechanism reports success while the user sees a dead button.

Note: `pump_until(..)` in `changes.rs`/`right_panel.rs` ends in `panic!`, so a wait *is* an assertion.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. **Your
  pane may start in the main repo on `rust/gpui-rewrite` — `cd` first.**
- **Yours:** `tiller_ui/src/settings.rs` (new), `tiller_theme/**`, `controls.rs`, `titlebar.rs`,
  `composer.rs`.
- **Do not edit** `chat.rs`, `sidebar.rs`, `status_bar.rs` (`pi`); `main.rs`, `session.rs`,
  `tiller_control/**`, `tab_bar.rs` (`codex12`); `changes.rs`, `right_panel.rs`, `editor.rs`,
  `file_view.rs`, `tiller_git/**`, `tiller_terminal/**`, `tiller_acp/**`, `tiller_agents/**`,
  `browser.rs` (`codex11`). `tiller_project/**` is unowned — if `F-SET-09` needs a change there,
  claim it in your report.
- **Standing rule:** whoever widens a struct or enum owns every construction site and match arm it
  breaks, in any file — but only those.
- Colours, spacing and radii from `tiller_theme::Theme`, **never a literal** — and here you are both
  the consumer and the author of that vocabulary. Tokens the others have asked for and still lack:
  `menu-width`, `geometric-hairline`, `compact-action`, plus browser-chrome tokens. Adding them is
  yours.
- **Visual bar is Pop!_OS COSMIC**, not waku.
- **Establish the build state with the gate's own commands**, not a paraphrase. Measured minutes ago:
  `cargo fmt --all -- --check` green, `cargo clippy --workspace --all-targets --exclude tiller
  --exclude tiller_ui -- -D warnings` = EXIT 0, `cargo build -p tiller -p tiller_control` = EXIT 0.
  `cargo test --workspace` is being worked by `codex12` right now.
- System GTK headers are installed globally. **No `PKG_CONFIG_PATH`, no sysroot.**
- **Never copy code from the reference checkouts** — every line from scratch. Mark rows
  `builder-claimed, unverified`, **never** `PASSED`; only `pireview` sets a verdict.
- **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: which of the four dead-control rows turned out to need a connection rather
than a feature and what already existed; the drawn tests by name that drive the **control** and not
the mechanism; what you concluded about `F-SET-21` and why; which of the seven construction rows you
reached and which you did not; tokens you added to `tiller_theme` and any still missing; whether
`F-SET-09` required claiming `tiller_project`; the gate run with its own invocation with not-yours
failures named separately; and the honest remainder.
