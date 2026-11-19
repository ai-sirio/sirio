# Ely upstream sync: vendored Ely onto upstream's crates (programme step 1b)

Status: approved by Enzo via t2s (2026-11-19) · 2026-11-19 · branch `feat/ely-upstream-sync-full` (from `main` `d7dd8433`)

## 1. Why

Sirio's chat, menus, forms, change-request surfaces and file editor are drawn with Ely GPUI
Components. Sirio vendors Ely as a single pruned crate (`rust/vendor/ely-gpui-component`). That crate
is frozen at upstream `e3957f78` (2026-09-30) and carries the local changes listed in its
`LOCAL-CHANGES.md`. Upstream has since moved 682 commits ahead and split into crates
(`crates/ely-core`, `ely-chat`, `ely-agent`, `ely-editor`, `ely-documents`, …). Step 1a (#806,
`d7dd8433`) moved Sirio onto upstream's engine, `gpui-pre =0.3.12`. The two can therefore link in
one process for the first time.

This step replaces the vendored crate with upstream's own crates, so that:

- step 2 (Markdown on `ely-documents`) and step 3 (chat features) build on current upstream code
  instead of porting it file by file;
- every later sync is a mechanical diff per crate.

### 1.1 The programme this step belongs to

Agreed with Enzo on 2026-11-16, in this order. Each step gets its own design.

1. **Engine swap and Ely sync**, in two steps (Enzo, 2026-11-17):
   - **1a, done:** Sirio, the vendored Ely and the bezel crates moved to `gpui-pre =0.3.12` with the
     app unchanged. Merged as #806 (`d7dd8433`). Enzo's hand checks on nightly
     0.46.0-nightly.202611190814 passed.
     - Follow-up #810: on macOS a background window drew its first frame late. It gets its own PR,
       #812, which vendors `gpui-pre-macos` with upstream Ely's CVDisplayLink change (`0b983008`).
       That PR is separate from 1b so the Mac checks of the two do not confound each other.
   - **1b (this document):** the vendored Ely is replaced by upstream's crates behind the
     `ely-gpui-component` facade, which still serves today's `ely_gpui_component::forms`,
     `::chat`, `::editor`, … paths. Sirio's local changes are re-applied, or dropped where upstream now
     provides the same thing. Chat and core come first, the file editor last. Visible differences
     are reviewed by Enzo on captures.
2. **Markdown on `ely-documents`:** `bezel-markdown` leaves Sirio entirely (chat, Markdown Preview,
   file view, change requests, `selectable_markdown`).
   - **Stays:** every Preview feature of today: raw HTML via `expand_html`, pictures, Mermaid and its
     cache, sandboxed PlantUML with the server off by default, selection and Ctrl+C, Code/Preview,
     scroll bars.
   - **Added:**
     - all of the renderer's capabilities: LaTeX, GitHub alerts, footnotes, emoji shortcodes,
       `[[toc]]`, heading anchors, a right-click menu on a selection;
     - the outline panel and reading progress.
   - **Out:** word count and reading time, split view, zen, print, EPUB, block editor, version
     history, backlinks, templates, thumbnails, the rich text editor, page covers.
   - Upstream's patched `pulldown-cmark` (`e71b644b`) is decided in step 2, not here.
3. **Chat features from Ely**, one delivery and PR at a time:
   1. richer messages: streaming Markdown, date separators, an error whose retry resends, a
      rate-limit notice, thinking duration (no token or cost footer);
   2. message actions: copy, stop, continue, regenerate, quote;
   3. edit and branches;
   4. web sources and citations from `WebSearch`/`WebFetch`;
   5. composer and media: drag-and-drop overlay, context chips, input hint, images and files in
      messages;
   6. a welcome screen from saved prompts, and Markdown export.

   A feature shows only for agents that supply its data (the `supports_*` pattern in `sirio_chat`).
   Out by Enzo's call: voice, link previews, 👍/👎 feedback, shared conversations, project knowledge,
   suggested follow-ups, model parameters, audio/video, a system-prompt editor.

### 1.2 Decisions already taken for this step

Enzo took these on 2026-11-19 through coordinator t2s, choosing the recommended option each time:

- **Source:**
  - The source is upstream's git repository, at a single frozen revision, `8d7723ec`
    (2026-11-18): its gate is green on macOS, Linux and Windows, and only its `pixels-macos` and
    `bench` jobs are red.
  - crates.io is ruled out, because the published `ely-core` 0.2.0 pins `gpui-pre =0.3.10`. The
    repository's `ely-core` 0.3.0 is on `=0.3.12` but unpublished.
  - A plain git dependency cannot carry Sirio's local changes, so the crates are vendored.
- **Shape:** whole upstream crates for the closure Sirio needs (§5.2), plus Sirio's own thin facade.
  Upstream's root facade is not used, because it pulls in all 30 crates.
- **Cost to measure:** build time and CI time, before and after. A heavy crate that no Sirio path
  reaches is evaluated for exclusion (§5.3), because Actions minutes are limited.
- **Acceptance bar** (§6):
  - every local change is re-applied, or shown with a proof to be absorbed by upstream;
  - the existing suites and E2Es are green;
  - a before/after capture sweep lists every visible difference, and Enzo approves that list.
- **Calls proposed by me and accepted:**
  - upstream's licence is now MIT only (the vendored base was MIT OR Apache-2.0);
  - `ely-core` is taken with `default-features = false`;
  - `ely-palette` and the `sirio_perf` spans are rebuilt on the new crates;
  - no new user-visible Ely feature arrives in this step.
- **Release:** the PR is `refactor:` and makes no version step. It merges after 0.46.0 is tagged.
  The cycle opened after that tag is 0.46.1, and the first `feat:` of that cycle moves it to 0.47.0.
- **Cost bound** (Enzo, 2026-11-19, in answer to the builder check):
  - `ely-documents` stays in 1b. Sirio reaches it through `documents::source` (`AttachmentChip`,
    the messages), and step 2 needs it.
  - `ely-charts` and `ely-collab` are cut only if `--timings` shows a gain.
  - `pr.yml` may grow by at most 10 min over today's 30. Past that, CI runs fewer of the vendored
    crates' upstream tests first, then dependencies are cut, and the numbers go to Enzo before the
    PR is opened.
  - The cold `cargo build -p sirio` is measured, with no ceiling.

## 2. Goals, non-goals, anti-goals

**Goals**

- **G1. Upstream crates are in the workspace.**
  - The closure of §5.2 sits under `rust/vendor/ely/` as workspace members.
  - Each crate is upstream `8d7723ec` plus only the changes recorded in the new `LOCAL-CHANGES.md`.
  - `diff -r` against `git archive 8d7723ec crates/<crate>` shows nothing else.
- **G2. Sirio's code does not move.**
  - `rust/vendor/ely-gpui-component` becomes a thin facade. It serves the same
    `ely_gpui_component::<chapter>` paths, `init_chat`, `init_editor` and `Assets`.
  - Sirio's 72 importing files change only where a symbol moved (`Newline`, `GitStatus`, §5.5).
- **G3. Every behaviour Sirio relies on survives.**
  - Every item of the local-change checklist (§5.4) is re-applied or proven absorbed.
  - Every proof test it cites still passes.
- **G4. One gpui and nothing duplicated.**
  - `gpui-pre` appears once in `rust/Cargo.lock`.
  - No Ely type exists twice: the old vendored modules are gone, and `ely-palette` is the single
    home of `Palette`/`Syntax`.
- **G5. The cost is measured and bounded** (§6.5):
  - measured before and after: cold and incremental `cargo build -p sirio`, the size of the target
    directory, and the CI wall time of `pr.yml`, `macos-check.yml` and `windows-check.yml`;
  - bounded: `pr.yml` at most 10 min over its 30 min baseline (§1.2).

**Non-goals**

- Any new user-visible Ely capability. Upstream code that Sirio does not reach comes along unused:
  `StreamingMarkdown`, `InlineChat`, the Markdown renderer, i18n catalogues.
- Markdown on `ely-documents` (step 2), chat features (step 3), and upstream's patched
  `pulldown-cmark`.
- Moving the engine. Upstream is on `gpui-pre =0.3.12`, as Sirio is.
- Upstream's Gallery, its other 22 crates, its fonts, and its global focus keys (Tab, Shift-Tab and
  F6 under `ElyFocus`).

**Anti-goals** (would make this a failure even if it builds)

- A behaviour Sirio's users have today silently lost. Examples:
  - a composer chord;
  - the wheel on a long draft;
  - the effort list's bars;
  - the editor's sideways scroll, hard tabs or highlighter;
  - a Dialog Enter that confirms on a stale press.
- Tab or F6 changing what they do anywhere in Sirio. The terminal's keys above all: Tab stays the
  shell's.
- An unlisted visible change: a moved pixel nobody can attribute.
- A second copy of an Ely type, or a second gpui.
- A CI cost increase nobody measured.

## 3. Constraints

- **Build load.** Use `CARGO_BUILD_JOBS=4`, with at most two builds on the machine. Coordinator t2s
  arbitrates; another worker is building A11. Agents never run `Scripts/ci.sh`,
  `Scripts/ci-linux.sh`, `cargo fmt`, rustfmt or clippy.
- **Disk.** `/home` has about 51 GB free. One debug target of this branch is 7.0 GB before the
  change. Only one target exists for this branch; no throwaway target directories.
- **Toolchain.** Upstream declares `rust-version = "1.98"`. CI pins 1.99.1 (#739) and the local
  toolchain is 1.98.1, so this fits.
- **One revision.** `8d7723ec` stays frozen for the whole step. A later upstream fix is a follow-up,
  never a mid-step move.
- **Commits.**
  - Type `refactor:`, so the workspace version does not move and the plan has no version step. The
    PR merges after the 0.46.0 tag (§1.2).
  - No Claude attribution.
  - Squash-merge erases a pristine vendor commit, so provenance is recorded in `LOCAL-CHANGES.md`
    and checked by `diff -r` (§5.6), never by history.
- **Shell safety.** `rm`, `mv` and `git clean` take absolute paths only. No `git stash`. No bare
  `sirioctl`. Private Xvfb only on displays :120 to :139. E2Es run with
  `TMPDIR=/home/epalmisano/.cache/st/rail-e2e-tmp`.

## 4. Approaches considered

1. **Whole upstream crates plus a thin Sirio facade (chosen by Enzo).**
   - For: a sync becomes `diff -r` per crate, and step 2 finds `ely-documents` already in place.
   - Against: compiled Ely grows from about 37k to about 160k lines, plus `ratex-*`, `usvg`,
     `html5ever` and `mermaid-rs-renderer`, on every build and CI run (§5.3, §6.5).
2. **Keep the pruned single crate and re-port upstream file by file.**
   - For: less code compiled.
   - Against: it is a port, not a sync. The next sync costs the same again, and step 2 would have to
     port `ely-documents` from scratch.
3. **A Sirio fork of upstream as a git dependency**, with the local changes as commits in the fork.
   - For: the smallest diff in this repository.
   - Against: it creates an outward-facing repository Enzo would own, and CI fetches it from outside.
     The provenance check moves out of the reviewable tree.
4. **Published crates from crates.io.** Not possible: the published `ely-core` 0.2.0 pins
   `gpui-pre =0.3.10`.

## 5. Design

### 5.1 Where things live

```text
rust/vendor/
├── ely/                          # NEW: upstream crates at 8d7723ec, plus recorded local changes
│   ├── LOCAL-CHANGES.md          # provenance, rev, per-crate local changes, proofs
│   ├── LICENSE                   # upstream MIT notice
│   └── crates/
│       ├── ely-core/  ely-chat/  ely-agent/  ely-editor/
│       └── ely-git/   ely-documents/  ely-collab/  ely-charts/   # (subject to §5.3)
├── ely-gpui-component/           # REWRITTEN: thin facade, Sirio-owned
│   └── src/lib.rs                # chapter re-exports, init_chat, init_editor, Assets
└── ely-palette/                  # KEPT: Palette/Syntax/Mode, re-extracted from ely-core 8d7723ec
```

- **Upstream crates.** Each crate's `Cargo.toml` is written out literally, because upstream's
  `[workspace.dependencies]` does not exist here; the dependency versions are upstream's. Engine
  dependencies use Sirio's workspace `gpui`/`gpui_platform`, with the Linux `wayland`/`x11`
  features, the same pattern the vendored crate uses today. `publish = false`.
- **`ely-core`** is taken with `default-features = false` and without its `platform` feature:
  - no bundled fonts;
  - no `gpui_platform` dependency of its own.
- **Lints.** Upstream's `[lints]` tables reference a workspace that does not exist here and are
  dropped.
  - The vendored Ely crates are not Sirio-owned code. `Scripts/ci-linux.sh`'s
    "clippy (owned crates)" stage therefore excludes them by name, as it excludes `sirio` and
    `sirio_ui`.
  - Without that exclusion, the `Copy` that `ely-palette` adds would turn upstream's `.clone()`
    calls into `clippy::clone_on_copy` errors under `-D warnings`.
  - The old crate avoided this by rewriting its clones (checklist row 61); this step excludes the
    crates instead, so upstream's code stays verbatim.
- **Files taken from each crate.** `Cargo.toml`, `src/`, and `icons/`, `assets/` and `LICENSE`
  where present. Upstream's `AGENTS.md` files (agent instructions for upstream's repository) and
  `examples/` are not taken. The vendor check of §5.6 compares that same set.
- **Tests.**
  - `Scripts/ci.sh` runs `cargo nextest run --workspace` with no crate features. Upstream gates
    its window suites on its crates' `test-support` feature, so CI does not run those. Its plain
    `#[cfg(test)]` unit tests (about 190 modules across the eight crates) do run in CI; see §10
    for this correction and what follows from it.
  - No Sirio manifest may enable an Ely crate's `test-support`. It turns on `gpui/leak-detection`,
    which would panic every Sirio gpui test that leaks a handle.
  - A Sirio-local test inside a vendored crate stays in a plain `#[cfg(test)]` module, as the
    popover tests are today, so CI keeps running it.
  - The upstream suites run per crate by hand: `--features test-support`, all eight crates in one
    invocation.
- **The facade** depends on the crates it re-exports and on nothing it does not re-export.
  - Its chapters map as upstream's root facade maps them: `forms`, `buttons`, `theme`, `menus`, …
    → `ely_core::<chapter>`; `chat` → `ely_chat`; `agent` → `ely_agent`; `editor` → `ely_editor`;
    `git` → `ely_git`.
  - It keeps today's `sirio-chat` default feature name, so that no consumer manifest changes, and
    `test-support` forwards to every crate's `test-support`.

### 5.2 The closure

Sirio reaches `ely-core`, `ely-chat`, `ely-agent`, `ely-editor` and `ely-git` directly. Upstream's
own dependency edges then pull in the others:

| Crate | Lines | Needed because |
|---|---|---|
| `ely-core` | 87k | every chapter Sirio uses from it: forms, buttons, theme, menus, overlays, primitives, … |
| `ely-chat` | 7k | `chat` chapter |
| `ely-agent` | 4k | `agent` chapter |
| `ely-editor` | 11k | the file editor (`editor` chapter) |
| `ely-git` | 6k | `GitStatusBadge`, `DiffStat` |
| `ely-documents` | 34k | `ely-chat`: `documents::source`, `MarkdownRenderer`. Also `ely-agent`, `ely-collab` |
| `ely-collab` | 3k | `ely-agent`: `collab::Decision` only |
| `ely-charts` | 10k | `ely-editor`: `charts::tint` and `charts::compact` only |

### 5.3 Excluding a heavy crate or dependency

Enzo asked for exclusion to be evaluated where no Sirio path needs a crate. The rule:

1. **Measure first.** The first task builds the closure and records `cargo build -p sirio --timings`.
   That gives each Ely crate's compile time and each heavy third-party dependency's:
   `mermaid-rs-renderer`, `ratex-*`, `usvg`, `html5ever`.
2. **A crate or dependency is excluded only if all three hold:**
   - no Sirio code path reaches it;
   - cutting it is a small local change in the crate that depends on it (a local copy of `tint` and
     `compact` in `ely-editor`, a local `Decision` in `ely-agent`, or an off-by-default cargo feature
     around a renderer in `ely-documents`), recorded in `LOCAL-CHANGES.md`;
   - it saves a measurable share of the cold build (the plan fixes the threshold).
3. **Step 2 needs are kept.** `ely-documents` and its renderer dependencies stay (Enzo,
   2026-11-19). Only `ely-charts` and `ely-collab` are candidates, and each is cut only if
   `--timings` shows a gain.
4. **The `pr.yml` bound** of §1.2 decides what happens if the after-run exceeds 40 min:
   - first, CI runs fewer of the vendored crates' upstream tests;
   - then dependencies are cut;
   - Enzo gets the numbers before the PR is opened.

### 5.4 Sirio's local changes

The checklist is the 234-row table in `docs/superpowers/plans/` (local, gitignored), extracted from
today's `LOCAL-CHANGES.md` and checked against `8d7723ec`. Every row ends in one of four outcomes:

- **Re-applied:** upstream has no equivalent (most of the 234). The change goes into the upstream
  file it now lands in, with its proof test.
- **Absorbed:** upstream already provides it. Examples:
  - all the ports from upstream `main`;
  - `Dialog::held`/`focus_first` public;
  - `compat::load_key` → upstream's `secondary!`/`word_key!`;
  - `GitStatus` in `ely_core::lists`;
  - `code_colors`, `fuzzy`/`marked`/`Fit` and `tint` now public.

  A proof is required: the test that covered it passes on the upstream code, or a grep shows the
  same definition.
- **Dropped with a reason:** cosmetic or obsolete. Examples:
  - `.clone()`→copy rewrites;
  - the `InlineChat` stub in `row.rs`, now that upstream's `InlineChat` exists;
  - dead re-export deletions;
  - `compat.rs` copies of now-public helpers.
- **Re-decided:** former exclusions that now come with the crate. These are the four named below
  and the 24 `not-ported` rows of the checklist (176 to 178, 186 to 206). The builder verifies each
  one against the upstream code, the four rows marked unverified included. Each is recorded in
  `LOCAL-CHANGES.md`. Only those with an effect a user can see or feel go to Enzo, through §6.4's
  list. The four:
  - `HoverListenerMode::InputModalityIndependent` on scroll, hover card and tooltip. The local
    "fork's `on_hover`" deviation existed only because bezel-gpui lacked the mode, so upstream's
    code is taken.
  - `select` logs an out-of-range selection instead of asserting (upstream's code is taken).
  - a11y roles and labels, i18n strings: taken as upstream ships them.
  - Upstream's `Dialog` focuses its first control a frame after opening, and disabled controls set
    AccessKit's disabled flag (upstream CHANGELOG): taken, and checked by the E2Es.

**What proves a re-applied row.** The behaviour already exists and is in use; re-applying it is
not new code. So no unit test is written after the fact, per the repository's testing rules. Each
row's proof is the first of these that applies:

1. the test the checklist cites;
2. an existing Sirio test or E2E that exercises it, named in `LOCAL-CHANGES.md`;
3. a step of §6.3 or a pair of §6.4 that shows it;
4. a named hand check in the PR.

A row with none of these is reported as unproven, never assumed. The anti-goal behaviours of §2
each need a named proof from 1 to 3.

Where a local change can live in the facade or in Sirio instead of inside an upstream crate, it
goes there. The upstream crates then stay as close to `8d7723ec` as possible:

- **Initialization** (`init_chat` is idempotent; `init_editor` has its own guard):
  - no global Tab, Shift-Tab or F6 binding;
  - no font or asset validation at init.

  This is facade code that calls the pieces upstream's `setup` calls, minus the focus bindings.
  Upstream's `setup` is not called.
- **Assets.** Sirio's `ely/` asset namespace is implemented by the facade's `AssetSource`, which
  maps it onto upstream's paths. Upstream's icon references stay verbatim where possible. The three
  added lucide icons (`ClipboardList`, `FilePen`, `ShieldOff`) are kept.
- **Palette.**
  - `ely-palette` is re-extracted from `ely-core/src/theme/{palette,syntax}.rs` at `8d7723ec`, with
    `Copy` on `Palette` and `Syntax` (Sirio's `Theme` is `Copy`) and `color_blind_chart` public.
  - `ely-core`'s `theme` re-exports these types instead of defining them. That is one recorded
    local change in `ely-core`, so that one `Palette` type exists.

Changes that have no seam outside a crate are made inside it, each recorded per crate with file and
proof:

- **The text field:**
  - `detached`/`attach`/`first_frame`, `host_context`, `reset_text`, `set_rows`;
  - the multi-line wheel and the `wheeled` hold;
  - the three delete actions and their chords;
  - `MULTI_LINE_CONTEXT`, the `is_word` word unit;
  - `min_h` on `Input`.
- **The composer:** `PromptInput::custom`/`band`/`send_probe`/`trailing`/`ready`, `SendButton`
  variant and selectors, `InputHint::enter`.
- **Agent controls:** permission and control custom constructors and their selectors; the tool and
  thinking expansion hooks.
- **Menus:** `MenuPanel`/`layer`/`Hang`/`panel_surface`/`row_frame`; `MenuItem`
  `note`/`tooltip`/`selectors`/`trailing`/`muted` and their read-back accessors.
- **Dialogs and popovers:** the Dialog text colour and the seen-press Enter rule;
  `Popover::on_close`/`open`.
- **Tabs:** the `compact` label condition.
- **The editor,** all of its hosting, performance and sideways-scroll changes:
  - Task 1c through "Sideways scroll", including the `sirio_perf` dependency and spans in
    `ely-editor`;
  - the non-macOS `shift-alt-up/down` add-cursor keys;
  - `Copy` propagating on an empty selection.

### 5.5 Symbols that moved

From the API map (in the same local plans directory): 108 of the 127 Ely symbols Sirio uses are
unchanged, and 16 are Sirio's own (they return with §5.4). Three moved:

- `Assets` is served by the facade (§5.4).
- `forms::Newline`: upstream's text field has its own `Newline` action
  (`ely-core/src/forms/text/actions.rs`, bound to `shift-enter`) but does not re-export it. The
  local change is that one re-export (checklist row 69).
  - `ely_editor::Newline` is a different action.
  - The proof is `enter_sends_and_shift_return_inserts_a_newline` (`sirio_ui/src/chat/mod.rs`).
- `git::GitStatus` is `ely_core::lists::GitStatus`. The facade re-exports it at `git::GitStatus`,
  and the local copy goes.

### 5.6 Provenance and documentation

- **`rust/vendor/ely/LOCAL-CHANGES.md`** replaces today's file and records:
  - the upstream URL and the rev `8d7723ec`;
  - the closure and any exclusion with its measurement;
  - per crate, each local change: file, what it is, why, its proof test;
  - the absorbed, dropped and re-decided rows with their reason;
  - the check command: `git -C <upstream clone> archive 8d7723ec crates/<crate> | tar x -C <empty dir>`,
    then `diff -r` against `rust/vendor/ely/crates/<crate>`. Every hunk must be one this file lists.
- **Other files to update:**
  - `rust/vendor/README.md`;
  - `docs/agents/architecture/file-editor.md` (it names `LOCAL-CHANGES.md`'s hooks);
  - the Ely line of `CLAUDE.md`'s external references, where paths change;
  - `docs/testing/ely-agent-chat.md`.
- **`ely-palette`'s header** names its new source files.

### 5.7 Order of work

The old crate and the new crates register gpui actions under the same names (`ely_input::*`,
`ely_editor::*`, `ely::FocusNext`). One binary that links both panics with "Action with name …
already registered". A staged switch, chapter by chapter, is therefore impossible.

Until the single switch, no Sirio crate depends on a new Ely crate, not even as a dev-dependency or
from an example.

1. **Vendor the closure verbatim and measure** (§5.3). Sirio stays on the old crate.
2. **Re-apply the local changes crate by crate:** core, then chat and agent, then the editor
   (largest, last), then git.
   - After each one, the whole closure builds and the upstream suites of every crate that
     depends on the changed one pass. A local change must not break an upstream caller inside the
     closure.
   - When a local signature conflicts with one an upstream crate calls, the local API takes a new
     name and Sirio's call sites follow, recorded. One example is the editor's line-indexed
     highlighter, since `ely-documents` calls upstream's `highlighter`.
   - Sirio-side proofs cannot run until the switch.
3. **The switch,** in one commit. The facade is rewritten, the old sources are deleted, Sirio's
   moved symbols are re-pointed, and every Sirio-side proof runs. A failure is traced to its
   checklist row through `LOCAL-CHANGES.md`.
4. **Exclusions** decided by §5.3.
5. **Verification** (§6). `LOCAL-CHANGES.md` is completed last, with proofs that actually ran.

## 6. Verification

The baseline is `main` `d7dd8433`, measured with the same harness and display settings as the run
after. A suite red on the baseline is recorded as pre-existing, with its output, and is not fixed
here. Memory already has `test-update-ui-e2e.sh` red on `main`, and the Ely chat E2E's `restore`
scenario red before and after the 2026-11-17 sync.

1. **Build and tests.** `cargo build -p sirio`, then `cargo nextest run` per crate:
   - every Sirio crate;
   - every vendored Ely crate with `--features test-support`.

   Upstream's own inline tests in the vendored crates must pass, with two kinds of recorded
   exception:
   - a test that a recorded local change deliberately alters, which is rewritten as today's
     `edit::words` test was;
   - the two `ely-documents` tests of upstream `e71b644b`
     (`a_note_in_a_markdown_link_frees_the_math_after_it`,
     `a_link_around_a_footnote_reference_stays_a_link`). They depend on upstream's patched
     `pulldown-cmark`, which step 2 decides. They are `#[ignore]`d with that reason, recorded, and
     restored in step 2. A `[patch.crates-io]` would also change the parser of Sirio's current
     Markdown (`bezel_markdown`, `sirio_markdown`). Under nextest, every
   proof test the checklist cites is selected by name and passes; a filter that selects zero tests
   is a failure.
2. **Local-change accounting.** The checklist's four outcomes are filled for all 234 rows, and every
   `diff -r` hunk against upstream maps to a `LOCAL-CHANGES.md` entry. This is the critic's first
   input.
3. **End to end.** Every Linux suite in `docs/agents/e2e-tests.md`, each printing its own OK line,
   including:
   - the Ely chat UI E2E (`Scripts/Tests/test-ely-chat-ui-e2e.py`, dark and light);
   - the editor probe;
   - the forge probe;
   - the composer band and effort-list E2Es.

   All run isolated (a private `HOME`/XDG and private Xvfb).
4. **Pixels.** Captures before and after with the isolated framed harness 1a used, judged by
   `Scripts/capture-diff.py` at its default tolerance.
   - Every pair beyond tolerance is listed with its cause: an upstream change (name the commit or
     file) or a re-decided row of §5.4.
   - Unattributed differences are bugs.
   - The list, with the image pairs, goes to Enzo for approval. That list is the acceptance gate
     for visible change.
5. **Cost.**
   - **Build time.** `/home/epalmisano/.cache/st/1b-buildtime.sh` before and after (cold, Ely
     touched, `sirio_ui` touched, target size). Before, at `d7dd8433` with `CARGO_BUILD_JOBS=4`:
     cold 543 s, Ely touched 32 s, `sirio_ui` touched 31 s, target 7.0 GB.
   - **CI wall time** of the PR's runs against the baseline: `pr.yml` 30 min 10 s (run
     44466550074), `macos-check.yml` 23 min 51 s (44450730490), `windows-check.yml` 8 min 52 s
     (44450730389).
     - CI does not run the upstream window suites (§5.1), but it does run upstream's plain unit
       tests (§10), so `pr.yml`'s growth is compile time plus their run time.
     - It is estimated before the PR: on this machine, time `cargo nextest run --workspace --no-run`
       at the baseline and after the switch, add the featureless run time of the eight crates'
       tests, and scale the total by the baseline's ratio of CI time to local time.
     - The real figure comes from the PR's first run.
   - **If the estimate exceeds the bound** (§1.2): the first lever of §1.2 is keeping upstream's
     plain unit tests out of CI (gating them on `test-support`, §10); the next are the cuts of
     §5.3. Enzo gets the numbers before the PR.
   - **Frame cost.** The editor and chat frame probes that exist today, compared with the baseline.
6. **Other platforms.**
   - `macos-check.yml` builds and links the app.
   - Windows: `cargo check --target x86_64-pc-windows-msvc` as 1a ran it.
   - Owed hand checks, listed in the PR: on a Mac and on Windows, the composer chords, a long draft
     under the wheel, the effort list, a dialog's Enter, and the editor (sideways scroll, hard tabs,
     find).

The artifact: the before/after capture directories, the diff report, the build-time logs, the E2E
OK lines and the filled checklist, under `artifacts/ely-sync/`, re-runnable from the commands
above.

## 7. Risks

- **Re-applying about 180 local changes onto code 682 commits newer.** The editor carries the most
  drift (`state.rs`, `view.rs`, `line.rs`). Mitigation: editor last, one task per LOCAL-CHANGES
  section, proof tests first.
- **Upstream behaviour changes arriving with the crates:**
  - Dialog focus timing, hover modality, the pointer capture of `forms::surface` cards, the
    `Scrollbar` range over a `ListState`;
  - caught by §6.3 and §6.4 and listed, never silently allowed.
- **Global keys.** Calling upstream's `setup` anywhere would bind Tab and F6 globally. Mitigation:
  only the facade's init is used. A test asserts that Tab reaches a terminal pane and a text field
  as before.
- **Build and CI cost.** Measured (§6.5). The cuts of §5.3 are the lever.
- **Disk.** One target, absolute-path cleanup, and no parallel targets.

## 8. Left to the builder

- Task slicing inside §5.7 and the exact exclusion threshold of §5.3.
- How the `ely/` asset namespace maps onto upstream's paths (§5.4), as long as upstream's icon
  references stay verbatim where possible.
- The fate of `forms::Newline` (§5.5).
- Which frame probes exist and are compared (§6.5).

## 9. Calls made while writing this (for Enzo to check)

- **Closure, not the full upstream.** Eight crates, subject to the §5.3 measurement; upstream's
  root facade is not used.
- **Sirio's own init, not upstream's `setup`.** No global Tab or F6, no font loading, no asset
  validation (§5.4).
- **Hover modality** is taken from upstream (`InputModalityIndependent`), since gpui-pre has it.
  A re-decided row: listed in §6.4's differences if visible.
- **`ely-palette` stays a separate crate, with `Copy`,** and `ely-core` re-exports it, so Sirio's
  `Theme` stays `Copy` and one `Palette` type exists.
- **Upstream's patched `pulldown-cmark` is not taken** (step 2). Its two dependent `ely-documents`
  tests are ignored and recorded (§6.1). Upstream's `gpui-pre-macos` patch is not taken here either;
  it is #812's.
- **No unit test is written after the fact for a re-applied change.** Its proof is an existing test,
  an E2E or a capture, or a named hand check (§5.4).
- **All 24 former `not-ported` rows are verified by the builder.** Only those with a visible effect
  go to Enzo (§5.4).
- **The vendored Ely crates are outside `ci-linux.sh`'s owned-crates clippy stage** (§5.1), so
  upstream code stays verbatim.
- **Only `Cargo.toml`, `src/`, `icons/`, `assets/` and `LICENSE` are taken from each crate,** not
  upstream's `AGENTS.md` or `examples/` (§5.1).
- **Commit type `refactor:`.** No version step; the PR merges after the 0.46.0 tag (Enzo's call, §1.2).

## 10. Deviations found while building

Recorded as they were found; each was ruled on in the build's ledger and, where it says so, agreed
with Enzo through t2s.

- **Upstream's unit tests run in CI (corrects §5.1 and §6.5).** The design assumed upstream gates
  its tests on `test-support`. It gates its window suites, but about 190 plain `#[cfg(test)]` unit
  test modules across the eight crates (core 107, documents 33, charts 19, editor 19, chat 9, git
  7, collab 4, agent 1) compile and run in `Scripts/ci.sh`'s featureless `nextest --workspace`.
  - They stay in CI: they are cheap and they catch breakage in Sirio's ports (t2s, 2026-11-19).
    Their run time is part of the `pr.yml` estimate of §6.5.
  - A few of those modules call `init_for_tests`, which upstream defines only under
    `test-support`, so the featureless build did not compile from the import on. They are gated
    like upstream's own `mod tests` (`LOCAL-CHANGES.md`, "Featureless test build"). From that fix
    on, every task also checks `cargo nextest run --workspace --no-run` with no features.
  - Three of them assert wall-clock budgets (`ely-documents` `tree/html_block.rs`,
    `tree/sources.rs`, `tree/emoji.rs`) and fail under load. Sirio keeps wall-clock tests out of
    its gate, so those three functions compile only with `test-support`: a `#[cfg]` on the
    function, not an `#[ignore]`, so the two ignores of §9 stay the only ones and the three still
    run in the closure check (t2s, 2026-11-19).
- **The closure's upstream suites run on two test threads.** `ely-documents`'
  `references_and_long_runs_read_as_a_browser_reads_them` has a 2 s budget that fails when the
  whole closure runs in parallel in a debug build; at two threads all pass.
- **Row 17 (dialog Enter) is absorbed, not re-applied.** Upstream confirms on the key-down `Enter`
  action, so the release of a press made before the dialog opened cannot confirm it; the new test
  of Task 3 was green on the verbatim code. Enter now confirms on press rather than on release, a
  difference listed for Enzo (§6.4).
- **`PromptInput::ready` never existed.** Send eligibility is the `ready` argument of
  `PromptInput::custom`; the checklist named a method.
- **Rebased before the switch.** The branch moved onto `main` at `993fca9f` (0.46.1 cycle, with
  #812 and A11 #817) before the facade switch, and the baseline of §6 was re-taken on the rebased
  commit before the switch, so the before/after comparison measures this step alone. The workspace
  version is `main`'s; this step moves nothing.
