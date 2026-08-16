export const meta = {
  name: 'wave-h',
  description: 'Diagnose the overlay/gesture blockers, then close the 19 remaining open inventory rows',
  phases: [
    { title: 'Diagnose', detail: '3 read-only investigations — overlay clicks, modclick, capture staleness' },
    { title: 'Unblock', detail: 'app-side overlay fix and harness-side drive fixes, disjoint trees' },
    { title: 'Build', detail: '5 slices — architectural gaps, tray, window, drive-only' },
    { title: 'Integrate', detail: 'compile, per-crate tests, cross-slice revert sweep' },
    { title: 'Verify', detail: 'one fresh critic per slice' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const HOUSE = `
## House rules

Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY there. Never touch \`/home/enzopalmisano/Scrivania/Progetti/tiller\` — the Swift original,
which is the reference to READ, never to edit.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator is its single writer.

**Commit with explicit file paths only** — never \`git add <dir>\`, \`git add -A\`, or \`git commit -a\`.
In wave B an agent staged a file it did not own and byte-for-byte reverted a landed 167-line fix inside
a commit whose subject was about another crate; nothing flagged it. \`git commit -- <path>\` also does
**not** stage a file git has never seen: for a NEW file you must \`git add <exact/path>\` first, then
commit. After each row, \`grep\` your own file for the symbol you added to confirm it is still there and
run \`git status --porcelain\` to catch new files nobody staged.

On \`index.lock\` contention retry in a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row or finding.** You are killed after 180 s without output. In wave C an agent
completed 15 rows of real work then failed on its final report — the work survived only because it had
committed as it went. **Keep your returned answer small**: the committed markdown is the record, the
schema is an index. One or two sentences per field.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\` from \`${ROOT}\`.
No lock — parallel-safe if \`TILLER_WL_LABEL\` is unique (it names every \`/tmp\` path the instance uses).
**You can see images — open the PNGs with \`Read\` and look at them.**

Read \`docs/linux-rewrite/WAYLAND-LANE.md\` first. Vocabulary:
\`ctl · click · move · type · key · title · shot · rightclick · chord <mod> <key> · down · up · drag ·
scroll <x> <y> <steps> · modclick <mod> <x> <y>\`.

For the **embedded browser only**, the Wayland lane cannot work at all — \`wry\`'s \`build_as_child\`
accepts only an X11 parent. Use \`Scripts/linux-drive.sh\` (which does \`env -u WAYLAND_DISPLAY
DISPLAY=:1\`, takes a global lock, and has no \`ctl\` action — use \`Scripts/control-probe.py <socket>
<method> [k=v …]\`). Full story in \`docs/linux-rewrite/tasks/P127-browser-child-unavailable.md\`.

Standing traps:
- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state** — \`agent\`
  reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven with a positive control.
- **A stale frame is not evidence.** Repaint is lazy. Diagnosing exactly this is one of the tasks in
  this wave; until it is fixed, verify a capture's md5 actually changed before reading it as "the
  feature did nothing".
- **Same-invocation vs separate-invocation.** A gesture and its capture must be in ONE
  \`wayland-drive.sh\` call unless you are deliberately testing restart persistence — a separate
  invocation is a fresh app with fresh state, which silently resets what you just did. A wave-G critic
  lost a real PASS to this and only caught it on re-run.

The lane **does not build** (\`[ -x "$BIN" ] || exit 2\`). The tree is built at HEAD. Run tests
**per-crate**, never \`--workspace\` — two pre-existing tests flake only under workspace-wide
concurrency (\`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\` is
known-flaky and predates this work; re-run the crate alone before believing a failure).
`

const DIAGNOSE_SCHEMA = {
  type: 'object',
  required: ['topic', 'rootCause', 'confidence'],
  properties: {
    topic: { type: 'string' },
    rootCause: { type: 'string', description: 'one or two sentences' },
    confidence: { type: 'string', enum: ['proven', 'likely', 'unproven'] },
    isAppDefect: { type: 'boolean', description: 'true = fix belongs in rust/, false = harness/Scripts' },
    filesToChange: { type: 'array', items: { type: 'string' } },
    fixSketch: { type: 'string', description: 'two or three sentences' },
    rowsUnblocked: { type: 'array', items: { type: 'string' } },
    reportPath: { type: 'string' },
  },
}

const BUILD_SCHEMA = {
  type: 'object',
  required: ['slice', 'rows'],
  properties: {
    slice: { type: 'string' },
    buildsClean: { type: 'boolean' },
    wantedForeignFiles: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        required: ['id', 'action'],
        properties: {
          id: { type: 'string' },
          action: { type: 'string', enum: ['implemented', 'fixed', 'already-correct', 'blocked', 'not-attempted'] },
          commit: { type: 'string' },
          howToExercise: { type: 'string', description: 'one or two sentences' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  required: ['slice', 'rows'],
  properties: {
    slice: { type: 'string' },
    notes: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        required: ['id', 'verdict', 'newEvidenceCell'],
        properties: {
          id: { type: 'string' },
          verdict: {
            type: 'string',
            enum: ['PASSED', 'half-proven', 'FAILED — absent', 'FAILED — defective',
                   'UNREACHABLE', 'N/A — platform', 'NOT EXERCISED'],
          },
          evidenceDiscriminates: { type: 'boolean' },
          newEvidenceCell: {
            type: 'string',
            description: 'ledger evidence, 1-3 sentences, no raw | and no triple backticks',
          },
        },
      },
    },
  },
}

// ---------------------------------------------------------------- diagnose

const DIAGNOSES = [
  {
    key: 'overlay-clicks',
    label: 'diagnose:overlay-clicks',
    prompt: `**The most valuable thing in this wave.** Root-cause why clicks on overlay UI (context menus,
popovers) do not reach their items in the Tiller Linux/GPUI app.

Two independent observations that are probably the same bug:

1. \`F-CHG-18\` (wave G, live): *"across 5 independent live attempts (2x Move to New Pane, Rename,
   Close, an offset probe) no click on any item in the Terminal tab's right-click context menu fired
   or dismissed the menu, while a click outside it did dismiss cleanly."* A click outside dismissing
   but a click inside doing nothing is the signature of a hit-test region that is not where the menu
   is painted.
2. \`docs/linux-rewrite/tasks/P123-popover-click-routing.md\`: the Clone/Create popover **draws in the
   wrong place** — its overlay is positioned relative to the Sidebar's ~280px root element instead of
   the window root, so it is confined to the sidebar column while its dark-scrim styling implies it is
   window-centred.

**Hypothesis to test first, and to discard if the evidence says otherwise:** both are one GPUI
anchoring defect — an overlay/deferred-draw element taking its origin from an ancestor's bounds rather
than the window's, so paint and hit-test disagree (or paint is right and the input region is stale).

This is a **read-only investigation**. Do not fix anything, do not commit code. Produce a diagnosis
another agent will implement.

What would make this conclusive:
- Read how the app builds context menus and popovers (\`tiller_terminal/src/lib.rs\` around the context
  menu render site near line 1471, \`context_menu.rs\`, the sidebar's popover, and any \`anchored\`/
  \`deferred\`/\`occlude\`/\`absolute\` usage). GPUI's \`anchored()\` and \`deferred()\` interact with the
  parent's transform in ways that are easy to get wrong.
- Compare against Zed's own crates (a vendored/reference checkout may be available; if not, reason
  from the GPUI source in the dependency tree) — Zed uses the same primitives correctly and is the
  reference for the intended pattern.
- **Drive it live and measure.** Right-click a terminal pane, capture, and read the menu's painted
  pixel bounds off the frame with ImageMagick. Then click at the painted centre of an item and see
  whether anything happens. If \`debug_bounds\`/\`debug_selector\` are available for the menu, compare
  the *declared* bounds to the *painted* bounds — a mismatch is the proof.
- Rule out the harness: prove the same synthetic click DOES actuate a normal in-window button at a
  known coordinate in the same invocation. That is your positive control, and without it a null
  result means nothing.

Write \`docs/linux-rewrite/tasks/P129-overlay-click-routing.md\` with the evidence, and commit it
(\`git add\` the new path first). Say plainly whether P123 and F-CHG-18 are one bug or two, and if you
cannot tell, say that rather than guessing.`,
  },
  {
    key: 'modclick',
    label: 'diagnose:modclick',
    prompt: `Root-cause why \`modclick\` — the modifier+click gesture in \`Scripts/wayland-drive.sh\` — has
never been proven to actuate anything in the Tiller Linux/GPUI app.

It is implemented but **never proven end-to-end**, so every null result from it is inconclusive rather
than evidence. That blocks \`F-TERM-UI-02\` (cmd/logo+click a URL in the terminal opens it in the
Browser tab), where a wave-G critic reported: *"4 modclick logo attempts on a real rendered URL
(confirmed logo=XKB Super=gpui platform modifier via gpui_linux source) produced no Browser tab in any
same-invocation capture."*

**This is a read-only investigation of the harness plus whatever app code it must actuate.** You may
edit \`Scripts/wayland-drive.sh\` **only** to add diagnostic output or to fix \`modclick\` itself —
nothing under \`rust/\`.

The decisive question is: **does the app ever observe the modifier as held at the moment of the click?**
Get evidence, not inference:
- Read how \`modclick\` sends the modifier through the virtual-pointer/virtual-keyboard protocol. A
  modifier delivered on the keyboard while the click goes through the pointer must be latched in the
  compositor's *keyboard* state and the surface must have keyboard focus for it to count.
- **Build a positive control first.** Find any modifier+click the app already handles and prove
  \`modclick\` fires it — or prove no such binding exists, which is itself the finding. Without a
  positive control a null result cannot distinguish "gesture broken" from "feature absent"; that
  distinction is the entire point of this task.
- \`chord <mod> <key>\` is a proven primitive in this lane. If \`chord\` works and \`modclick\` does not,
  the difference between how the two deliver the modifier is very likely the bug.

Write \`docs/linux-rewrite/tasks/P130-modclick.md\` with the evidence and commit it. If you fix
\`modclick\`, commit \`Scripts/wayland-drive.sh\` alone, and append the proven gesture to the lane
vocabulary section of \`docs/linux-rewrite/WAYLAND-LANE.md\`.`,
  },
  {
    key: 'capture-staleness',
    label: 'diagnose:capture-staleness',
    prompt: `Root-cause the capture-staleness artifact in \`Scripts/wayland-drive.sh\`.

A wave-G critic judging \`F-CHAT-20\` reported: *"mid-stream captures taken seconds apart repeatedly
returned frames with identical stale progress percentages, consistent with a capture-pipeline
staleness artifact in this harness rather than evidence for or against the app's behavior."* If a
capture can return a stale frame, **every** null result in this project is weakened, so this matters
well beyond the one row.

**You may edit \`Scripts/wayland-drive.sh\` and other files under \`Scripts/\`. Nothing under \`rust/\`.**

Relevant history: \`shot()\` used to alternate the output between two resolutions to force a repaint,
which produced a *different* bug — coordinates read off one frame were sent while the output was at
the other size, so clicks landed in dead space and manufactured false negatives on two rows. The
current \`shot()\` nudges the output to \`W2xH2\` and back to the canonical \`W1xH1\` (1715x972),
deliberately ending at one fixed size. Do not reintroduce the alternation; whatever you change must
keep all three of these true, and you must re-verify them:
1. every frame is captured at exactly 1715x972,
2. consecutive captures of a genuinely changing UI have **different** md5s,
3. a click at coordinates read off a frame still lands where the frame showed it.

Design an experiment that separates the candidates rather than assuming one: the app not repainting
(GPUI draws lazily and may skip frames when nothing it tracks changed), the compositor not committing
a new buffer, \`grim\` reading a cached buffer, or the settle delay simply being too short for the
event being watched. A driven animation whose expected pixel value you can predict at time T is a
much better instrument than a live agent stream.

Write \`docs/linux-rewrite/tasks/P131-capture-staleness.md\` with the evidence and commit it. If you
fix it, commit the \`Scripts/\` change separately and update the staleness note in
\`docs/linux-rewrite/WAYLAND-LANE.md\`.`,
  },
]

// ---------------------------------------------------------------- build slices

const SLICES = [
  {
    key: 'H1-gaps',
    rows: ['F-CORE-DOM-07', 'F-CORE-WSP-04', 'F-CORE-WSP-08'],
    brief: `Three **architectural gaps**: types that exist, are unit-tested, and have zero production
callers. Each needs the concept it belongs to actually built and wired — a call site that merely makes
a grep pass will be driven by a critic and graded \`FAILED — defective\`.

- \`F-CORE-DOM-07\` — \`AutoNamingThrottle\` (\`tiller_project/src/domain.rs\`) has zero callers outside
  its own file, its \`lib.rs\` re-export and its test file. \`main.rs\`'s \`auto_naming\`/
  \`summarizer_command\` hits are settings plumbing only: no trigger, no spawn, no sink. The row wants a
  chat/worktree to be auto-named from transcript content, throttled. Wire the throttle to a real
  transcript signal and a real rename.
- \`F-CORE-WSP-04\` — \`LayoutCommand\`/\`classify_layout_command\` (\`tiller_project/src/layout.rs\`) have
  15 refs, all inside that file plus a bare re-export; **zero** under \`rust/crates/tiller/src/\`. The
  app's real pane/tab state (\`OpenTab.panes\`) is a separate structure mutated independently. Nothing
  constructs or applies a \`LayoutCommand\`.
- \`F-CORE-WSP-08\` — \`WorkspaceTabViewState\` has 5 refs, all in \`layout.rs\`/its re-export. The
  persistence target is \`SessionTabState\` (\`session.rs:79\`), which holds only \`root_id\`,
  \`pane_events\`, \`scrollback\` — \`editor_caret\`, \`editor_folds\` and \`chat_draft\` have zero readers
  or writers anywhere in \`session.rs\`/\`chat.rs\`/\`editor.rs\`, so the row's restart-survival bar is
  unreachable by construction until they are persisted and restored.

\`F-CORE-WSP-08\` is the one with a hard, drivable pass bar: set a caret position / fold / chat draft,
restart the app against the same \`TILLER_DB\`, and find them restored. Build toward that bar.`,
  },
  {
    key: 'H2-ctxmenu',
    rows: ['F-TAB-11', 'F-CHG-18'],
    brief: `Both rows live or die on the terminal context menu.

- \`F-TAB-11\` — context-menu items must render **disabled with a reason** when unavailable.
  \`TerminalContextItem\` (\`context_menu.rs:30\`) has only \`label\`/\`action\`/\`route\`; \`items()\` takes
  no parameters; the render site (\`tiller_terminal/src/lib.rs:1471\`) iterates with no disabled branch,
  so every item is always clickable. \`split_disabled_reason\` (\`panes.rs:218\`) already computes the
  right answer but is \`#[allow(dead_code)]\` and referenced only by its own three \`#[cfg(test)]\` tests.
  So the *logic* exists and the *type and render path* do not.
- \`F-CHG-18\` — the drop-handling code is correct and unit-tested, and \`surface.changes.open\` works
  live; what failed was that **no click on any context-menu item did anything** across 5 attempts,
  while a click outside dismissed the menu cleanly.

**Read \`docs/linux-rewrite/tasks/P129-overlay-click-routing.md\` before you start** — a dedicated
investigation ran earlier in this wave, and if it found the overlay anchoring defect, the fix may
already have landed. Build on its finding rather than re-deriving it. If P129 concluded the menu is
unclickable for a reason not yet fixed, say so and scope your work to what can still be done.`,
  },
  {
    key: 'H3-tray',
    rows: ['F-USE-04', 'F-USE-05', 'F-WIN-08'],
    brief: `**A new subsystem: a StatusNotifierItem tray item.** Read
\`docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md\` first — these three rows were
wrongly exempted as platform-impossible and were put back in scope.

The reference behaviour is the Swift original's \`MenuBarExtra\`. **Read it** at
\`/home/enzopalmisano/Scrivania/Progetti/tiller/App/AgentRosterView.swift\` and
\`App/TillerApp.swift:115\` (read-only — never edit that tree).

- \`F-USE-04\` — a global roster reachable outside the main window: one row per worktree with an active
  agent, sorted by the same urgency rule the sidebar uses, or the text "No active agents" when empty,
  plus a Quit item.
- \`F-USE-05\` — clicking a roster row reopens/selects that worktree and activates its worst-status tab.
  Both halves already exist in-product (\`select_worktree\`, tab activation), so this is wiring.
- \`F-WIN-08\` — closing the window must not terminate running panes; re-showing must bring them back
  with the processes still alive. The Swift side does this with a hide-on-close window delegate; the
  tray item is the natural Linux re-show surface, which is why this row is in this slice.

On Linux the mechanism is **StatusNotifierItem/AppIndicator over D-Bus**, which COSMIC supports.
\`zbus\` is already in \`Cargo.lock\` transitively; the \`ksni\` crate is the usual ergonomic wrapper.
Adding a direct dependency is expected and fine — but check what is already vendored/available offline
before assuming a fresh crates.io fetch will work, and if the network is unavailable say so plainly
rather than leaving a half-built module.

Scope honestly. If the tray item can be registered and shows a menu but the roster contents cannot be
wired in the time you have, land the part that works, commit it, and mark the rest \`blocked\` with a
precise description. Partial-but-real beats a stub that greps well.`,
  },
  {
    key: 'H4-window',
    rows: ['F-WIN-09', 'F-WIN-11'],
    brief: `Two independent adaptations, both wrongly exempted before
(\`docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md\`).

- \`F-WIN-09\` — follow the system title-bar double-click preference. macOS reads
  \`AppleActionOnDoubleClick\`; the Linux counterpart is the
  \`org.gnome.desktop.wm.preferences action-double-click-titlebar\` gsetting (values include
  \`toggle-maximize\`, \`minimize\`, \`none\`, \`menu\`). Nothing in \`rust/crates\` reads any titlebar
  preference today. The app uses client-side decorations, so it must read the setting itself and
  apply it to its own titlebar. \`gsettings get org.gnome.desktop.wm.preferences
  action-double-click-titlebar\` is the read; check it resolves on this box before building on it, and
  degrade gracefully (default \`toggle-maximize\`) when the schema is absent — this is sway/COSMIC, not
  necessarily GNOME.
- \`F-WIN-11\` — the update toast's five user-visible states. **The state machine already exists**:
  \`UpdateState\`/\`UpdateEvent\` in \`tiller_project/src/ui.rs\` model Available, Downloading (with
  clamped progress), Installing, UpToDate, Failed and Idle, and are unit-tested by
  \`updater_reaches_every_user_visible_state_and_clamps_progress\`. Its doc-comment says it is
  deliberately independent of whichever Linux update transport is chosen. What is missing is a
  **consumer**: the only reference outside that file is the \`pub use\` re-export in \`lib.rs\`. Build
  the toast UI that renders those states with their actions/messages, and a way to drive it — a
  control-socket method that injects an \`UpdateEvent\` is the cheapest honest test source and makes the
  row drivable without inventing an update server.`,
  },
  {
    key: 'H5-drive',
    rows: ['F-CORE-AUTH-03', 'F-CORE-FILE-03A', 'F-CHAT-05'],
    brief: `**Mostly a driving slice, not a building one.** Two rows have working code that has simply
never been exercised; the third is a real discrepancy that needs a decision.

- \`F-CORE-AUTH-03\` (\`NOT EXERCISED\`) — \`tiller_usage/src/credentials.rs\` implements \`get\`/\`set\`/
  \`delete\`, exactly the three operations the contract names, with \`from_env\`/\`at\` for isolation and
  \`TILLER_CREDENTIALS\` overriding the store path. The contract's VERIFY is: save, read, replace, and
  delete a provider credential, **restart**, and confirm persistence and deletion. That is fully
  drivable — drive it. One thing to surface rather than bury: the store is **plaintext JSON at mode
  0600, not encrypted**, while the contract's third option says "an explicitly chosen *encrypted*
  store". The file argues parity with the \`~/.claude/.credentials.json\` it already reads. Report the
  deviation explicitly so a human can rule on it; do not silently treat it as satisfied.
- \`F-CORE-FILE-03A\` (\`half-proven\`) — \`on_drop::<gpui::ExternalPaths>\` is wired at
  \`tiller_terminal/src/lib.rs:1586\`. Unproven: that **several** files dropped at once preserve the
  user's drop order through resolution and classification, including when one resolves slowly.
  \`text/uri-list\` is ordered by construction so this is likely already correct. Driving a real
  multi-file XDND drop is the hard part — if this lane genuinely cannot synthesise one, say so
  precisely and propose the smallest instrument that could, rather than substituting a unit test.
- \`F-CHAT-05\` (\`half-proven\`) — **a real, live-confirmed discrepancy.** In a genuine offline state
  the distinct placeholder "Agent offline — reconnecting when you send…" does appear, but typing is
  **not** refused: text is accepted and Return silently clears it, where the row says "confirm the
  editor is disabled". A wave-G critic noted the current behaviour matches a tradeoff documented in
  the code's own comments. Decide it on the merits: either disable the editor when offline (matching
  the contract), or record why the documented tradeoff should win and what the contract row should say
  instead. Silently clearing a user's typed message on Return is the worst of the three options — if
  you keep the editor enabled, the text must survive.`,
  },
  {
    key: 'H6-instruments',
    rows: ['F-TERM-UI-02', 'F-CHAT-20', 'F-TERM-SCR-02', 'F-USE-03', 'F-CHAT-33', 'F-TERM-PTY-05'],
    brief: `**Every row here is blocked on proof, not on code.** Each has app code that reads correct and
unit tests that pass; what is missing is an instrument that can drive the behaviour live. Your job is
to build the instruments and then use them. Prefer \`Scripts/\` and \`docs/\`; touch \`rust/\` only where
a row genuinely needs a control-socket door to become drivable, and keep that change minimal.

- \`F-TERM-UI-02\` — logo/cmd+click a URL in the terminal opens it in the Browser tab. \`link_router\`
  tests pass and were shown to discriminate (a critic mutated \`resolve_click_cell\` to drop the origin
  subtraction, the test failed, then reverted cleanly). The live half failed only through \`modclick\`,
  which had never been proven to actuate anything. **Read \`docs/linux-rewrite/tasks/P130-modclick.md\`
  from this wave's diagnosis round first** — if \`modclick\` now has a positive control, this row is
  simply drivable.
- \`F-CHAT-20\` — the transcript follows streamed output to the tail (already live-confirmed), and
  manual scroll-away must decouple it, with re-pin on return to the bottom. The unproven half died on
  stale frames. **Read \`docs/linux-rewrite/tasks/P131-capture-staleness.md\` first.**
- \`F-TERM-SCR-02\` — resize debounce and reflow. \`TERMINAL_RESIZE_DEBOUNCE\`=120ms,
  \`OUTPUT_SETTLE_DEBOUNCE\`=200ms and the \`resize_generation\` CAS are all present and unchanged. Both
  a builder and two critics have now declined to build the live instrument, which is a WINCH trap in
  the pane's shell plus a real divider drag. Build it: run something in the pane that logs on SIGWINCH
  with a timestamp, drag the divider, and read the log — that turns a debounce claim into a
  measurement.
- \`F-USE-03\` — the usage segment must dim to Stale on a fetch timeout. Proven at entity level (a
  \`gpui::TestAppContext\` test drives the real \`apply_outcomes\` and asserts \`segment_dimmed\`), but
  never live, because \`Claude::TIMEOUT\` is 25 s and two critics judged a real 25 s hang too risky
  against the 180 s silence kill. The instrument that removes the risk is an env override or a
  control-socket door that shortens the timeout or injects a timed-out outcome. Build the smallest one
  and drive it.
- \`F-CHAT-33\` — an MCP warning banner on a failed MCP server. **Four independent live attempts have
  now produced no banner**, the last with a genuinely broken \`.mcp.json\` pointing at a nonexistent
  binary. The blocker is that nobody has confirmed the CLI emits a stderr line matching
  \`looks_like_mcp_warning\`'s pattern for that failure shape. **Attack that directly**: run the agent
  CLI by hand with a broken MCP config, capture its raw stderr, and compare it against the predicate.
  Either the predicate's vocabulary is too narrow (a real defect — fix it) or the CLI says nothing on
  stderr (then the row is unreachable by that route and needs a different trigger). Four negatives
  with an unexamined predicate is exactly the "controls bound false negatives only" trap: a passing
  positive control proves the detector is alive, not that its vocabulary is complete.
- \`F-TERM-PTY-05\` — a Codex child in a pane. \`main.rs\` does contain the launch path
  (\`open_command_palette\`, \`NewTabAction::Codex\`). The blocker is environmental:
  \`codex login status\` reports **Not logged in** on this host. **Do not attempt to authenticate and do
  not go looking for credentials.** Establish precisely how much of the row is reachable without auth
  (does the child spawn at all, does the pane attach, does it fail visibly and correctly?) and report
  exactly which clause needs a logged-in CLI. That boundary is the deliverable.`,
  },
]

// ---------------------------------------------------------------- prompts

function buildPrompt(s, diag) {
  const diagNote = diag.length
    ? `\n## What the diagnosis round found\n\n${diag.map((d) => `- **${d.topic}** (${d.confidence}${d.isAppDefect === true ? ', app defect' : d.isAppDefect === false ? ', harness' : ''}) — ${d.rootCause}${d.reportPath ? ` See \`${d.reportPath}\`.` : ''}`).join('\n')}\n`
    : ''
  return `You are building slice **${s.key}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI port of a
macOS SwiftUI app. **337 of 389 contract rows are PASSED and 40 are terminal; these are among the last
19 open.** Every row here has survived several waves, so assume the easy reading is wrong and start
from the recorded diagnosis rather than re-deriving one.

## Your rows

${s.brief}
${diagNote}${HOUSE}
## Your job, per row

1. Verify the recorded root cause still holds at HEAD, then implement it.
2. If a fix truly needs a file another slice owns, do **not** edit it — put the path in
   \`wantedForeignFiles\` and describe the precise change in your committed report so the integrator can
   apply it. **Describe an actual change, not a direction to investigate**: in wave G both foreign-file
   requests were correctly refused by the integrator because they named a hypothesis rather than a fix.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) green.
4. Commit that row alone, explicit paths, conventional-commit subject naming the row id.
5. Append to \`docs/linux-rewrite/wave-h/${s.key}-report.md\` and commit it too.

**An honest \`blocked\` with a design sketch is worth far more than a token call site that makes a grep
pass while changing no behaviour** — a critic will drive it and grade it \`FAILED — defective\` anyway.
Wave G proved this both ways: the rows that moved were the ones where something real got wired.

You are **not** the judge of your own work. Do not write verdicts. \`howToExercise\` is the only thing
your critic gets from you: name the control, the coordinates or \`ctl\` call, and what should visibly
change.
${DRIVE}`
}

function verifyPrompt(s, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}${r.howToExercise ? `. Route: ${r.howToExercise}` : ''}`).join('\n')
    : '- **The builder returned nothing.** It may still have committed real work — check `git log` and\n  the slice report before assuming the rows are untouched. Grade what you actually find.'

  return `Verify slice **${s.key}** of the Tiller Linux/GPUI rewrite.

**You did not build this slice and you have not seen the builder's reasoning.** The standing rule is
that the critic is never the agent that built the piece. Everything below from the builder is a
*claim* — useful for finding the control, worthless as proof.

## The rows, and what the builder claims

${built}

## What the rows are supposed to do

${s.brief}

Inputs: \`docs/linux-rewrite/wave-h/${s.key}-report.md\` (routes, not conclusions),
\`docs/linux-rewrite/wave-h/INTEGRATION.md\`, and any \`docs/linux-rewrite/tasks/P129\`/\`P130\`/\`P131\`
report from this wave's diagnosis round.
${HOUSE}
The tree is built at HEAD; you should not need to compile. Tests **per-crate**, never \`--workspace\`.
${DRIVE}
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass — 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing it
proves nothing. Drive to a state the system would never reach on its own and capture a marker only
your drive could have produced. For rows that are not pixels — process lifecycle, persistence,
clipboard, debounce — the honest instrument is a real restart, a real process tree, or a real
read-back. Name the instrument. (\`xclip\` **cannot** read this app's clipboard — it goes through
smithay-clipboard, not X11 selections; a wave-F critic had to build a wlroots data-control reader.)

Failure modes that have actually bitten this project:
- **A token call site.** Several rows here are zero-caller types that needed wiring. A builder can add
  one call that satisfies a grep while changing no observable behaviour. Drive the behaviour.
- **A fix on the wrong path.** Wave D found a link handler wired only for the non-default mode, and a
  \`focus_path\` called before the async task that populates what it searches. Check the fix is on the
  path a **user** takes, not merely one a test takes.
- **A null result from an unproven instrument.** If a gesture has never been shown to actuate
  anything, its silence is not evidence. Build a positive control or downgrade your confidence.

## Verdicts — exactly one per row

\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires your own live exercise, this pass.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** An all-PASSED slice is far likelier to be a lazy critic than a
  healthy codebase — a wave-F audit downgraded 4 of 15 rows everyone believed were done.
- Equally, be willing to *raise* a row the ledger has wrong: in wave G two rows were sitting at
  \`FAILED — defective\` over defects that had since been fixed or that were harness artifacts.

\`newEvidenceCell\` lands verbatim in the ledger: your own finding, the instrument, the frame, 1-3
sentences. **Strip every raw \`|\`** and **never use triple backticks** — an odd run of backticks
swallows the next cell and breaks the totals gate.

Write \`docs/linux-rewrite/wave-h/${s.key}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema. **That file is the real record** — if the structured answer
fails to validate, the committed file is what survives.`
}

// ---------------------------------------------------------------- run

phase('Diagnose')
log('wave H: 19 open rows. First, three read-only investigations into what is blocking proof.')

const diagnoses = (await parallel(DIAGNOSES.map((d) => () =>
  agent(d.prompt, { label: d.label, phase: 'Diagnose', schema: DIAGNOSE_SCHEMA, effort: 'high' })
    .catch((e) => { log(`diagnose ${d.key} failed: ${String(e).slice(0, 110)}`); return null })))).filter(Boolean)

for (const d of diagnoses) log(`${d.topic}: ${d.confidence} — ${String(d.rootCause).slice(0, 150)}`)

// Only fix what a diagnosis actually established. An unproven diagnosis becomes context for the
// builders instead of a change nobody can justify.
const actionable = diagnoses.filter((d) => d.confidence !== 'unproven' && (d.filesToChange || []).length)
const appFixes = actionable.filter((d) => d.isAppDefect === true)
const harnessFixes = actionable.filter((d) => d.isAppDefect === false)

phase('Unblock')
log(`unblock: ${appFixes.length} app-side, ${harnessFixes.length} harness-side (disjoint trees, so parallel)`)

const unblockTasks = []
if (appFixes.length) {
  unblockTasks.push(() => agent(`Apply the **app-side** fix this wave's diagnosis round established, in
\`${ROOT}\` on branch \`linux/gpui-waku\`. You own files under \`rust/\` only.

${appFixes.map((d) => `### ${d.topic} (${d.confidence})\n\n**Root cause.** ${d.rootCause}\n\n**Fix sketch.** ${d.fixSketch}\n\n**Files.** ${(d.filesToChange || []).join(', ')}\n\n**Report.** ${d.reportPath || '(none)'}\n\n**Rows it unblocks.** ${(d.rowsUnblocked || []).join(', ') || '(unstated)'}`).join('\n\n')}

Read the report before touching code — it contains evidence this summary compresses away. Implement
the fix as described; if the described fix turns out to be wrong when you look at the code, say so and
implement what is actually correct rather than forcing the sketch.

This fix probably gates other rows in this wave, so **prove it works before you finish**: drive the
app live and show the previously-dead interaction now doing something. A build-green claim is not
enough here.
${HOUSE}
${DRIVE}
Return a short plain-text summary: what you changed, the commit, and how you proved it.`,
    { label: 'unblock:app', phase: 'Unblock', effort: 'high' }).catch((e) => `app fix failed: ${e}`))
}
if (harnessFixes.length) {
  unblockTasks.push(() => agent(`Apply the **harness-side** fixes this wave's diagnosis round established, in
\`${ROOT}\` on branch \`linux/gpui-waku\`. You own files under \`Scripts/\` and \`docs/\` only —
**nothing under \`rust/\`**, which another agent is editing concurrently.

${harnessFixes.map((d) => `### ${d.topic} (${d.confidence})\n\n**Root cause.** ${d.rootCause}\n\n**Fix sketch.** ${d.fixSketch}\n\n**Files.** ${(d.filesToChange || []).join(', ')}\n\n**Report.** ${d.reportPath || '(none)'}\n\n**Rows it unblocks.** ${(d.rowsUnblocked || []).join(', ') || '(unstated)'}`).join('\n\n')}

Read each report before touching code. After changing \`Scripts/wayland-drive.sh\`, **re-verify the
three invariants that a previous harness fix broke**, and state each result explicitly:
1. every frame is captured at exactly 1715x972,
2. consecutive captures of a genuinely changing UI have different md5s,
3. a click at coordinates read off a frame lands where the frame showed it.

Then update the lane vocabulary and the staleness note in \`docs/linux-rewrite/WAYLAND-LANE.md\` so the
next agent inherits a true description of what the lane can do.
${HOUSE}
${DRIVE}
Return a short plain-text summary: what you changed, the commits, and the three invariant results.`,
    { label: 'unblock:harness', phase: 'Unblock', effort: 'high' }).catch((e) => `harness fix failed: ${e}`))
}
const unblocked = unblockTasks.length ? await parallel(unblockTasks) : []
for (const u of unblocked) log(`unblock: ${String(u).slice(0, 220)}`)

phase('Build')
const built = {}
for (const s of SLICES) {
  log(`build ${s.key}: ${s.rows.join(', ')}`)
  try {
    const r = await agent(buildPrompt(s, diagnoses), {
      label: `build:${s.key}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'high',
    })
    if (r) built[s.key] = r
  } catch (e) {
    log(`${s.key} FAILED (${String(e).slice(0, 110)}) — its committed work may still stand`)
  }
}

const wanted = Object.entries(built).flatMap(([k, x]) =>
  ((x && x.wantedForeignFiles) || []).map((f) => `${k} wanted ${f}`))
log(`build closed — ${Object.keys(built).length}/${SLICES.length} slices reported, ${wanted.length} foreign-file requests`)

phase('Integrate')
const integration = await agent(`You are the integrator for wave H of the Tiller Linux/GPUI rewrite. Agents committed to
\`linux/gpui-waku\` in one shared worktree: three read-only diagnosers, up to two unblock agents (one
under \`rust/\`, one under \`Scripts/\`), then five build slices one at a time. Base for this wave:
\`5bec7b5\`.
${HOUSE}
In order, committing after each step:

1. \`cd ${RUST} && cargo build -p tiller\` green. If a builder left it broken, fix it — smallest change
   that restores the build, never a revert of someone's work.
2. \`cargo test -p <crate>\` **per crate**, never \`--workspace\`. Known pre-existing flake under
   workspace-wide concurrency:
   \`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\` in
   \`tiller_terminal\` — it predates this work; re-run the crate alone before believing it.
3. **Cross-slice sweep.** For every file touched since \`5bec7b5\`, run
   \`git log --format='%h %s' 5bec7b5..HEAD -- <file>\` and check each commit against
   \`docs/linux-rewrite/wave-h/manifest.json\` (\`slices[<name>].owns\` is the file list, \`.ids\` the
   rows). Compare **file ownership, not commit-subject text** — matching conventional-commit scope to
   slice name produced 12 false positives out of 13 in wave B. Then diff each commit in isolation and
   read its **deleted** lines against its own message, looking for a deletion that quietly removes a
   prior, unrelated fix. Restore anything lost. **Do not assume a commit came from the slice that owns
   the file** — wave C's integrator attributed four \`main.rs\` commits to agents that had never run.
   If you cannot attribute a commit, say so. Note that this wave has a legitimate multi-owner file
   (\`main.rs\`) and a legitimate non-slice committer (the orchestrator, who alone edits
   \`docs/linux-rewrite/INVENTORY-LEDGER.md\`).
4. Apply any fix a builder needed but correctly refused to make in a file it did not own — each is
   described in that builder's report; **apply what is described, do not invent one.** If a request
   names a hypothesis rather than a concrete change, leave it and say so.

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (none requested)'}

5. Rebuild and leave a current binary at \`${RUST}/target/debug/tiller\`. The critics that run next use
   the drive lanes, which **do not build** and exit 2 on a missing binary. Your responsibility.

Write \`docs/linux-rewrite/wave-h/INTEGRATION.md\` and commit it. Return a short plain-text summary:
build, tests, commits examined, genuine reverts found, what you restored, anything unattributable.`,
  { label: 'integrate', phase: 'Integrate' })

log(`integration: ${String(integration).slice(0, 400)}`)

phase('Verify')
const verdicts = await parallel(SLICES.map((s) => () =>
  agent(verifyPrompt(s, built[s.key]), {
    label: `verify:${s.key}`, phase: 'Verify', schema: VERDICT_SCHEMA, effort: 'high',
  }).catch((e) => { log(`verify ${s.key} failed: ${String(e).slice(0, 110)}`); return null })))

const all = verdicts.filter(Boolean)
const rows = all.flatMap((x) => (x.rows || []).map((y) => ({ ...y, slice: x.slice })))
const tally = {}
for (const x of rows) tally[x.verdict] = (tally[x.verdict] || 0) + 1
log(`wave H verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  diagnoses: diagnoses.map((d) => ({ topic: d.topic, confidence: d.confidence, isAppDefect: d.isAppDefect, rootCause: d.rootCause, reportPath: d.reportPath })),
  unblock: unblocked.map((u) => String(u).slice(0, 600)),
  integration: String(integration || ''),
  buildersMissing: SLICES.map((s) => s.key).filter((k) => !built[k]),
  verifiersMissing: SLICES.map((s) => s.key).filter((k, i) => !verdicts[i]),
}
