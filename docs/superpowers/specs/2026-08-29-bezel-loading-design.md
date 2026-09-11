# Bezel thinking orbs and loading system

Date: 2026-08-29
Status: implemented on `worktree/green-valley-a9fb`

## Decision summary

Sirio will adopt the official Bezel loading primitives, but not an arbitrary
palette of loader shapes. The visual source of truth is the Bezel gallery at
commit `2cfff23c96c6d33177a65d523f1827b0941b2eac`, especially the Activity
pattern in `apps/gallery/src/patterns/agent.rs`. Existing Sirio domain states
remain authoritative; Bezel supplies composition and animation only.

The scope includes the live Chat transcript, not Chat History or native agent
transcript files. Chat History is outside this visual migration except for the
shared typography decision. The live transcript follows Transcript's overall
composition; tool calls copy its step-row/group grammar, and reasoning uses
Activity's header/body composition. Diffs copy the compatible paint conventions
from the Diff pattern. Each specialized body keeps its meaning and controls.
Sidebar and Activity Panel remain compact, different compositions.

This remains a two-phase delivery: first migrate the workspace to Bezel's GPUI
family and preserve the Linux patch; then add one `sirio_ui` loading boundary
and apply the visual migration. This is a design decision, not an
implementation plan.

## Evidence and verification boundary

The gallery was read from source *and* observed visually on 2026-08-30 (macOS
26.6.2, gallery built from the pinned commit). Eight pages were captured:
Patterns > Activity, Tool calls, Transcript, Diff, Thinking orbs, and
Components > Loaders, Progress, Skeleton.

The earlier "running process, no visible window" failure was environmental, not
a property of the gallery. `gallery <section>` does open a real 1000x700 window
for the requested section, but when the launching terminal is full-screen that
window lands on a different Space, so a full-screen `screencapture` records the
terminal instead. Capturing per window id is the reproducible method:
`CGWindowListCopyWindowInfo` for the pid, then
`screencapture -x -o -l <window-id> out.png`, which captures the window whatever
Space it sits on.

The capture confirms:

- Thinking orbs catalogues twelve states (Working, Searching, Solving,
  Listening, Connecting, Weaving, Composing, Thinking, Shaping, Focusing,
  Reasoning, Recalling) behind a size selector reading `20 - inline`,
  `64 - avatar`, `96 - large`, `128 - hero`;
- Loaders shows the four bezel orbs (cluster, ring, converge, bloom) at `44`
  and, under "And the older three:", the pulse `8`, gradient `5`, and mini
  `2.5` spinners beside the loading word;
- Progress is two bars in a `280px` column; Skeleton is three redacted rows,
  pulsing on the shared clock with a staggered phase
  (`popover::redacted_rows`), not three static blocks;
- Tool calls renders one row per call with icon, verb, detail and duration,
  groups consecutive `Read` calls under an expandable `Read - 3` row, and shows
  no live loader on any row - confirming the Cluster-on-tool-rows caveat below;
- Transcript renders `Worked - N steps`, a `TODAY` date separator,
  right-aligned user bubbles on a raised surface, and unboxed assistant content
  with fenced code and a Copy affordance;
- Diff renders a path header with `+6 -1` tallies, fixed old/new number
  columns, a sign column, and the success/danger washes;
- Activity opens on the settled state: user bubble, `Thought for 4s` header
  with a chevron, unboxed answer, and `Ask again`.

One claim stays source-only: the *running* Activity header. The page opens
settled by design (`Activity::new` sets `run: None`, so you land on a whole
answer and press to watch it happen), and both ways to reach the running state
failed on this machine - a synthetic click on `Ask again` does nothing without
Accessibility permission for `CGEventPost`, and a locally patched build
(`run: Some(Instant::now())`) never reaches its first frame because freshly
linked binaries hang in `_dyld_start` here, the same class of blockage seen
earlier on the `real_claude-304` binary. The running header is therefore taken
from `patterns/agent.rs:147-194`, which is unambiguous: while running the 14px
glyph slot holds `loaders::orb(loaders::Orb::Cluster, "thinking", 14.0, ...)`
with the label `Thinking`; when the run ends the same slot holds
`theme.disclosure(open)` with `Thought for {:.0}s`.

The immutable source citations used for this audit are:

- [`patterns/agent.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/apps/gallery/src/patterns/agent.rs)
- [`patterns/transcript.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/apps/gallery/src/patterns/transcript.rs)
- [`patterns/diff.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/apps/gallery/src/patterns/diff.rs)
- [`patterns/orbs.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/apps/gallery/src/patterns/orbs.rs)
- [`lib.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/apps/gallery/src/lib.rs)
- [`ui/src/loaders.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/crates/ui/src/loaders.rs)
- [`ui/src/widgets/controls.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/crates/ui/src/widgets/controls.rs)
- [`ui/src/popover.rs`](https://github.com/clearloop/bezel/blob/2cfff23c96c6d33177a65d523f1827b0941b2eac/crates/ui/src/popover.rs)

At the audit date, `main` is identical to this pinned commit. The commit URLs
remain pinned so the evidence is reproducible if `main` advances.

The gallery also distinguishes two things that must not be conflated:
`apps/gallery/src/patterns.rs` describes patterns as complete, copyable screens;
`patterns/agent.rs` composes reusable `widgets::Takeover`, `scroll::follow`,
`loaders::orb`, and disclosure/button APIs into a copyable Activity pattern.
Sirio copies that composition where specified and keeps Bezel primitives behind
its own adapter.

## Goals and boundaries

- Pin `bezel` to `=0.1.3` and the matching `bezel-gpui*` family to `=0.3.6`,
  subject to release-manifest and API verification because Bezel is absent from
  the current lockfile.
- Use Transcript's evidence-supported live-chat composition, Activity's
  evidence-supported reasoning composition, and the compatible tool-row and
  diff paint conventions without changing message, tool, or diff semantics.
- Use a conservative loader matrix for generic waits, determinate progress, and
  subordinate skeletons.
- Preserve first-load versus refresh behavior, stale visible content, Retry,
  Stop, selection/copy, links, persistence, virtualization, and tail-follow.
- Bundle and register Geist and Geist Mono for Windows and Linux before first
  frame; macOS remains on existing SF Pro/SF Mono.
- Use Bezel's shared clock and lifecycle rules. Sirio must not add per-entry
  repeating timers or copy gallery demo RAF/token simulation.

This does not change activity detection, ACP behavior, Git operations, browser
navigation, usage fetching, persistence state machines, or static idle,
needs-input, done, empty, and error semantics. Context usage at
`chat.rs:6637-6689` is a determinate data gauge, not loading, and is out of
scope. The app update toast at `sirio/src/main.rs:12677-12794` is also outside
the `sirio_ui` migration boundary; it is explicitly excluded from this spec's
production matrix, not an accidental omission.

## 1. Runtime migration and loading boundary

### 1.1 One GPUI family

Sirio currently pins `gpui` and `gpui_platform` to Zed commit
`c05e34637b4f7f100a688bf6ac71cb70877fc8ad` and applies local Linux patches
(`rust/Cargo.toml:31-61,89-91`). Bezel must replace those package sources as a
single family; a second upstream `gpui` package or a second Bezel GPUI version
is a failed migration even if compilation succeeds.

```toml
bezel = { version = "=0.1.3", default-features = false }
gpui = { package = "bezel-gpui", version = "=0.3.6", default-features = false }
gpui_platform = { package = "bezel-gpui-platform", version = "=0.3.6", default-features = false, features = ["font-kit"] }
```

Keep existing Linux `wayland` and `x11` features on GPUI users. `sirio_ui`
gets the Bezel dependency; other crates continue using the workspace `gpui`
key. The dependency graph and release manifests must be verified before
implementation because the current lockfile contains no Bezel entries.

### 1.2 Wayland patch contract

The existing `PendingDrop`, latest-position tracking, deferred submit, cleanup,
and pure regression tests address the slow XDND provider race. Preserve them
when rebasing `rust/vendor/gpui_linux/src/linux/wayland/client.rs` onto the
released `bezel-gpui-linux 0.3.6`, update `rust/vendor/README.md` and manifests,
and route the platform override to the patched sibling. Keep only the required
Linux overrides; macOS and Windows use the published 0.3.6 packages.

The phase-one gate is a passing focused patch test, GPUI-using workspace build
and tests, one-family dependency inspection, and `Scripts/ci.sh` printing
`CI OK`. `Scripts/ci-linux.sh` remains the fuller supported-Linux gate.

### 1.3 `sirio_ui::loading`

All Bezel loading imports live behind a Sirio-owned loading module. It exposes
three responsibilities without owning business state: an Activity-derived
thinking indicator, a shared-clock indeterminate loader, and a determinate
progress bar accepting a clamped `0.0..=1.0` fraction.

The adapter maps the resolved Sirio theme and semantic colors to Bezel values;
it does not install Bezel as a second application theme. Generic
`ui::loaders::orb` is caller-sized and shared-clock driven. `agent::orbs::Orb`
is reserved for identity/activity marks where the matrix says so.

## 2. Official Activity composition

The following is the faithful visual contract from `patterns/agent.rs`:

- Activity's inner composition is a centered column with maximum width
  `660px`, horizontal padding `24px`, vertical padding `32px`, and a `10px`
  left margin on the reasoning body; the outer gallery full-bleed wrapper adds
  another `24px` around that inner composition;
- this Activity geometry is not copied to the whole transcript; the live
  transcript follows the separate Transcript pattern below;
- user bubble right-aligned, maximum width `440px`, on a raised surface;
- assistant answer is subordinate unboxed content;
- thought header has a `14px` glyph slot, `6px` gap, `4px` horizontal and
  `5px` vertical padding, and a full-row click target;
- while running, the header uses generic
  `ui::loaders::orb(loaders::Orb::Cluster, "thinking", 14.0, ...)`, not
  `agent::orbs::OrbState::Reasoning` (source evidence: the gallery page opens
  settled, so this state was read rather than seen);
- running label is `Thinking`; after settlement the loader becomes a disclosure
  chevron and the label is `Thought for {:.0}s` only when Sirio has truthful
  thought duration. Otherwise the deterministic label is `Thought`, never a
  fabricated elapsed time;
- `widgets::Takeover` auto-opens untouched reasoning while running and
  auto-closes it when finished. A manual user choice wins; reset returns to
  automatic behavior;
- reasoning body is subordinate under the header with a left border, `12px`
  left padding, `160px` maximum height, `4px` line gap, `14px` right padding,
  `20px` top fade, follow-scroll, and scrollbar.

The live transcript scope is the conversation area above the composer. Direct
evidence in `patterns/transcript.rs` is a `700px` maximum-width transcript
with `24px` horizontal and `28px` vertical padding, `10px` between turns, and
an `8px` work-zone gap. It renders `Worked · N steps`, groups consecutive tool
rows, splits the final answer after the last tool, inserts date separators, and
uses `scroll::follow`. Sirio should copy that overall live-transcript
geometry and composition while preserving virtualization and
`FollowMode::Tail`.

The Activity `660px`/`24px`/`32px` values above apply only to the inner Activity
reasoning composition, not to the whole transcript. The transcript's outer
full-bleed wrapper adds its own `24px` where that wrapper is used; it must not
be double-counted as a replacement for the Transcript pattern's stated
padding. The composer remains outside the live transcript. Chat History and
native agent transcript files remain excluded from this migration except for
shared typography.

## 3. Chat mapping

Relevant current ranges are `Entry` (`chat.rs:209-292`), live root/transcript
(`7865-8174`, especially `7895-8073`), thought renderer (`4926-4984`), diff
renderer (`4520-4670`), tool renderer (`5663-5835`) and grouping
(`5887-5969`), thought/tool toggles (`1397-1463`), streaming indicator
(`8076-8106`), and custom border lifecycle (`6037-6068`).

**Reasoning.** Replace the custom Chat braille spinner and rotating streaming
border with the Activity thought header/body. Active reasoning uses Cluster 14,
`Thinking`, and automatic open; settled reasoning uses disclosure, `Thought`
or a truthful elapsed label, and automatic close unless manually overridden.
`chat-generating-spinner` remains non-persisted in principle, but is replaced
by this active header integrated into the live transcript rather than becoming
a stored Entry.

**Tools.** Where compatible, tool calls copy the gallery `step_row` and grouped
row composition: icon, verb/title, detail, duration/status, failure state,
disclosure/output, separators, and grouped consecutive tools. Preserve
normalized truthful statuses, errors, and cancellation; failed and cancelled
are never relabeled as finished. Preserve full-row expand/collapse, groups,
and subagents. The gallery does not show a live Cluster loader on tool rows.
If Sirio uses Cluster 14 for nonterminal tools, that is explicitly a Sirio
Activity-derived adaptation, not direct Tool Calls evidence.

**Diffs.** The standalone gallery diff pattern is direct evidence for the
paint conventions where compatible: a `760px` standalone maximum reference,
fixed old/new number columns, a fixed sign column, `8px` row gaps, `10px`
horizontal and `1px` vertical row padding, Geist Mono `12px/18px`, 10% success
and danger washes, skip rows, a path header with +/- tallies, and horizontal
scrolling. In Sirio's `700px` transcript, the diff uses available width rather
than forcing `760px`.

Sirio-specific ownership is different: a diff remains subordinate content of
its owning tool call and follows that tool's protocol/lifecycle. It has no
independent timer, lifecycle, or second loader. That ownership is a Sirio
decision, not gallery evidence. Preserve clickable file headers, line numbers,
+/-/context rows, selection, 60-line preview, file opening, and revert. Use
Geist Mono for diff and code body; do not apply italic reasoning styling to it.

Assistant Markdown, user text, permissions, plans, and errors retain their own
semantics. Selection/copy, links, persistence, virtualization, and tail-follow
are acceptance constraints, not optional visual polish.

## 4. Conservative loader evidence

The gallery loader page shows Cluster, Ring, Converge, and Bloom at `44px`,
with pulse `8`, gradient `5`, mini `2.5`, and a loading word. The mini value
means `2.5px` cells and an approximately `6px × 10px` overall footprint, not a
`2.5px` total spinner size. These are catalog
examples, not evidence for assigning every Sirio operation a unique shape.
Agent-orbs catalogs all 12 states at Inline `20`, Avatar `64`, Large `96`, and
Hero `128`; that is catalog evidence only. Activity does not use those states.
Identity marks remain separate from activity.

| Purpose | Evidence-supported treatment |
|---|---|
| Agent activity, Chat reasoning | Activity-derived header; generic Cluster `14` while active; disclosure when terminal |
| Nonterminal tool activity | Transcript step-row/group treatment; a Cluster `14` loader is optional only as an explicitly Sirio Activity-derived adaptation, not gallery Tool Calls evidence |
| Generic first load or wait | One standard generic loader, preferably Cluster `44` for full/empty-state space |
| Compact refresh | One consistent compact mini gradient spinner using the gallery's `2.5` cell size, or another explicitly verified gallery compact treatment. Reserved for loads a user asked for (History, status bar, Browser); a periodic poll gets no indicator at all — see the Changes and Files refresh rows |
| Determinate progress | Bezel progress bar, `280px` demo width where space permits, `4px` track, rounded, truthful fraction; responsive width may be constrained by its surface |
| Skeleton | Three redacted rows, `28px` each, `6px` gaps, `4px` vertical padding, pulsing on the shared clock with a staggered per-row phase, subordinate to a primary first-load signal; never the sole activity signal. Cluster plus skeleton is a Sirio assembly from independently shown primitives; the gallery does not show that combined composition |

Do not allocate Ring, Converge, Bloom, or all OrbState values to product states
without composition evidence. The adapter chooses the one standard generic
loader and one compact treatment consistently.

## 5. Production matrix

Every row specifies composition, indicator, evidence-supported dimensions,
label, alignment, lifecycle owner, and settled/error behavior.

| Surface | Composition and indicator | Label/alignment | Lifecycle owner; settled/error behavior |
|---|---|---|---|
| Chat live transcript | Transcript composition: 700px max, 24px horizontal/28px vertical, 10px turn gap, 8px work-zone gap, date separators, grouped consecutive tools, final answer after the last tool, and `scroll::follow`; Activity inner reasoning remains 660px max with 24px horizontal/32px vertical and 10px reasoning-body left margin; user bubble max 440px right/raised; assistant unboxed | `Worked · N steps` for the work zone; `Thinking`/`Thought` in the Activity-derived reasoning header; user right, assistant/body left | Typed Sirio `Entry`/task lifecycle owns semantics; gallery hierarchy informs composition but does not replace authoritative Entry semantics; takeover auto open/close, manual wins; errors/cancelled remain truthful and terminal; composer outside |
| Sidebar | Existing compact row geometry; one compact standardized running indicator plus identity badge; no reasoning body | Running indicator in existing status slot; identity badge retained | Sidebar row/activity state; unmount when not running; idle/needs-input/done/error static and truthful |
| Activity Panel | Existing status-row hierarchy; compact standardized indicator only for Running; no transcript composition | Existing status label and alignment | Activity status; terminal states static; no reasoning body |
| Changes first load | Full/empty-state Cluster 44 where space permits; three skeleton rows subordinate | `Loading changes…`, centered in surface | Existing task/entries state (`changes.rs:1888-1917`); success settles, error wins and exposes Retry |
| Changes refresh | Settled list remains visible; no indicator — the refresh is silent and the Refresh button stays put. (Revised 2026-09-06: the compact mini treatment originally specified here swapped in for the button on every in-flight task, and the 1 s poll put one in flight every second, so the icon blinked once a second for the duration of every `git status`.) | Existing toolbar alignment | Existing refresh task; the new snapshot lands in place, error preserves data and exposes Retry |
| Files first load | Same full loader/skeleton grammar | `Loading files…`, centered | `right_panel/mod.rs:162-192` settled state and current task; error wins, Retry remains |
| Files refresh/expansion | Existing tree remains visible; no indicator — the periodic walk is silent. (Revised 2026-09-06 for the same reason as Changes refresh: the compact mini treatment overlaid the Files rail icon on every in-flight walk, once a second.) | Existing panel alignment | Existing walk/refresh task; settled tree remains on error where current semantics allow |
| History first load/refresh | First load uses full loader plus subordinate rows; settled refresh keeps content and compact mini | Existing History labels and toolbar alignment | `load_task`/settled state; success settles, error is explicit and retryable |
| File view | Full/empty-state Cluster 44 where space permits | `Loading file…`, centered | Existing `ViewState`; success renders file, error renders error/retry, no loader underneath |
| Clone repository | Determinate Bezel bar, `4px` track, rounded, responsive width up to `280px` where space allows | Existing `Cloning… N%`, fraction/percentage adjacent and truthful | `CloneStatus::Running { progress }`; completion removes bar, error/retry stays explicit |
| Create project | Generic Cluster 44 where space permits | `Creating project…`, centered | Existing create task; success settles, error wins and exposes Retry |
| Usage provider | Compact mini treatment beside provider state | Provider name plus truthful loading state, not bare ellipsis | Provider state; success/error replaces loader; determinate usage ring at `chat.rs:6637-6689` remains out of scope |
| Browser navigation | Compact generic treatment in existing reload/Stop control; preserve Stop | Existing reload/Stop alignment | Browser navigation state; Stop remains explicit, completion/error removes loader |
| Settings agent installation | Existing Settings installation surface (`settings.rs:925-933,3423-3437,3529-3549`); use full first-load or compact refresh grammar according to whether content is absent or settled | Existing installation labels and controls | Settings task state; success settles, error remains explicit with Retry; must not be omitted from implementation scope |
| App update toast | No `sirio_ui` visual migration | Existing toast copy and alignment unchanged | Explicitly outside this narrow `sirio_ui` matrix; any future `sirio`-level change needs its own decision |

The matrix intentionally preserves first-load versus refresh, stale content
during refresh, Retry, Stop, clone determinate progress, and subordinate
skeletons. Files' settled state source is the current `right_panel/mod.rs:162-192`
contract. Sidebar row fields are around `sidebar.rs:272-313`; history uses its
`load_task` and settled state. These references correct earlier assumptions
that treated all surfaces as the same loading model.

## 6. Lifecycle, ownership, and failures

Generic loaders use shared `motion::PulseClock` leases. Reduced motion freezes
at phase `0` and leases no ticks. Unmounted loaders stop renewing and fall off
the clock. Activity is not uniform with those loaders: its gallery pattern uses
an unconditional RAF and has no reduced-motion branch. The orb catalog owns
one 40ms host timer, while the self-ticking `agent::Orb` freezes at `t=.6`
under reduced motion and requires its host to set `visible(false)` off-screen.
These are distinct animation models. Generic `loaders::orb` is caller-sized;
Chat must not create per-entry custom repeating timers, and Sirio must not copy
Activity's RAF or demo streaming into production.

The gallery `agent-orbs` page owns one 40ms host clock and calls `orb_element`.
The self-ticking `agent::Orb` owns a timer/activation subscription, freezes at
`t=0.6`, and requires its host to set `visible(false)` off-screen. These are
gallery-specific ownership facts, not permission to copy their timers into
Sirio. Activity itself has unconditional RAF/progressive demo text and no
explicit reduced-motion behavior; this spec does not claim otherwise. Sirio
uses the centralized adapter and Bezel clock instead.

For every surface, source state owns start, progress, cancellation, success,
settlement, and error. On cancellation or error, stop renewing/unmount the
indicator before showing the terminal state; terminal error and cancelled
labels take precedence over loading labels. Background or hidden windows must
not renew visible animation leases; exact pause behavior follows Bezel's
verified API and is a Sirio integration responsibility, not an unsupported
gallery claim.

## 7. Typography and font assets

The gallery typography page states that `Geist` and `Geist Mono` ship with the
crate, and gallery `lib.rs` registers those fonts before window creation. This
is direct evidence for the family roles and registration order. Windows and
Linux use `Geist` for proportional UI text and `Geist Mono` for code, diffs,
tool output, and terminal text. Static
TTF faces are preferred for predictable native backend behavior unless variable
font support is verified on both platforms. Bundle only the minimum faces and
weights required by current typography, rather than all nine, and register
them before the first frame.

SIL Open Font License 1.1 permits bundling, subject to including copyright and
license text, not selling the font as a standalone product, and observing the
requirements for modified fonts. Preserve fallback stacks for scripts, symbols,
and emoji; terminal text must retain Nerd Font fallback for icon coverage.

Current font resolution is in `sirio_theme/src/lib.rs:829-1172,1249-1293` and
currently discovers installed fonts. Packaging currently covers icons in
`Scripts/build-app-bundle.sh` and `rust/crates/sirio/build.rs`, with no Linux
font packaging. Future implementation must add assets, attribution/license
files, registration-before-first-frame, and Linux/Windows family-resolution
verification. macOS remains on SF Pro/SF Mono. Any prior non-goal excluding
Geist registration is superseded: it is in scope for Windows and Linux.

## 8. Testing and acceptance

Tests must cover the copied Activity pattern and transcript composition without
duplicating Bezel's internal geometry tests:

- one Bezel GPUI family, release-manifest/API verification, and preserved
  `PendingDrop` tests (`client.rs:362-413`, tests around `3072-3119`);
- 700px live transcript column, 24px/28px composition, 10px turn gap, 8px
  work-zone gap, grouped tools, final-answer split, date separators,
  `scroll::follow`, 440px user bubble, virtualization, tail-follow,
  selection/copy, links, and persistence; Activity reasoning separately keeps
  its 660px inner composition and 10px left margin;
- Activity reasoning auto-open/close, manual override/reset, truthful labels,
  and no custom braille spinner or rotating border;
- tool nonterminal/terminal/error/cancelled lifecycle, groups/subagents, and
  diff ownership with no second loader or timer;
- first-load versus refresh visibility, stale content, Retry, Stop, skeleton
  dimensions, and determinate clone width/fraction;
- Settings installation loading;
- centralized clock leases, unmount/cancellation, reduced motion, background
  behavior, and absence of extra repeating timers;
- Geist/Geist Mono assets, registration before first frame, family resolution,
  fallback stacks, and native Linux/Windows verification.

The delivery checkpoints remain:

1. **Runtime phase:** Bezel/GPUI migration, vendor patch, manifests, and
   dependency/CI gates, with no loading visual change.
2. **Loading phase:** centralized adapter, Activity-derived Chat/tool/diff
   composition, conservative matrix, Settings coverage, and typography/assets.

Acceptance requires one GPUI universe, the Wayland fix, evidence-supported
dimensions and labels, truthful terminal states, preserved existing content,
no custom Chat animations or per-entry timers, correct reduced-motion and
unmount behavior, Geist assets on Windows/Linux, and `Scripts/ci.sh` printing
`CI OK`. Status: **implemented** on `worktree/green-valley-a9fb`, per
`docs/superpowers/plans/2026-08-30-bezel-loading.md`. The one requirement left
unbuilt is the date separator: no truthful per-turn timestamp exists in the
transcript model, and fabricating one at render time is forbidden above.

## Rationale and changelog

- Replaced the theoretical `OrbState::Reasoning` Chat treatment with the full
  official Activity composition and its generic Cluster-14 thought header;
  kept any Cluster on nonterminal tool rows explicitly marked as a Sirio
  adaptation rather than direct gallery evidence.
- Replaced arbitrary shape diversity with a conservative, evidence-supported
  loader matrix and kept agent identity separate from activity.
- Scoped “transcripts” to the live Chat conversation area above the composer;
  Chat History and native agent transcript files remain excluded except for
  shared typography, and made Chat, Sidebar, and Activity Panel intentionally
  different compositions.
- Used Transcript as direct evidence for the live work-zone hierarchy and
  Activity as direct evidence specifically for reasoning, while preserving
  typed Sirio Entry ownership; integrated tool calls and diffs into one
  lifecycle grammar without inventing diff timers, duplicate loaders, or false
  terminal labels.
- Replaced the gallery visual-verification blocker with actual observation:
  the five requested pages plus Tool calls, Transcript, and Diff were captured
  per window id, which confirmed the orb catalogue, the loader matrix values,
  the progress and skeleton geometry, and the absence of a live loader on tool
  rows. Only the running Activity header stays source-only, with the reason
  recorded.
- Added Geist/Geist Mono for Windows/Linux and corrected the font packaging and
  licensing requirements.
- Corrected lifecycle, Files/History/Sidebar, Settings, usage-gauge, app-toast,
  Bezel lockfile, and Wayland patch references.
