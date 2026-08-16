export const meta = {
  name: 'wave-i',
  description: 'Close the last 6 open inventory rows of the Tiller Linux/GPUI rewrite',
  phases: [
    { title: 'Build', detail: '4 slices, serial — every slice may touch main.rs' },
    { title: 'Integrate', detail: 'compile, per-crate tests, cross-slice revert sweep' },
    { title: 'Verify', detail: 'one fresh critic per slice' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`
const SWIFT = '/home/enzopalmisano/Scrivania/Progetti/tiller'

const HOUSE = `
## House rules

Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY there. \`${SWIFT}\` is the Swift original — the reference to **read**, never to edit.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator is its single writer.

**Commit with explicit file paths only** — never \`git add <dir>\`, \`git add -A\`, or \`git commit -a\`.
In wave B an agent staged a file it did not own and byte-for-byte reverted a landed 167-line fix inside
a commit whose subject was about another crate; nothing flagged it. \`git commit -- <path>\` also does
**not** stage a file git has never seen: for a NEW file you must \`git add <exact/path>\` first, then
commit. After each row, \`grep\` your own file for the symbol you added to confirm it is still there and
run \`git status --porcelain\` to catch new files nobody staged.

On \`index.lock\` contention retry in a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row.** You are killed after 180 s without output. **Keep your returned answer
small** — the committed markdown is the record, the schema is an index.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\` from \`${ROOT}\`.
No lock — parallel-safe if \`TILLER_WL_LABEL\` is unique. **You can see images — open the PNGs with
\`Read\` and look at them.** Read \`docs/linux-rewrite/WAYLAND-LANE.md\` first.

Vocabulary: \`ctl · click · move · type · key · title · shot · rightclick · chord <mod> <key> · down ·
up · drag · scroll <x> <y> <steps> · modclick <mod> <x> <y>\`.

Three harness questions were settled in wave H — **do not re-litigate them, build on them**:
- \`docs/linux-rewrite/tasks/P130-modclick.md\` — **\`modclick\` is proven working.** \`WAYLAND_DEBUG=1\`
  wire traces on the app's own connection show \`wl_keyboard.modifiers(Mod4/Logo)\` arriving 25µs–64ms
  before the \`wl_pointer.button\` press. A null result from \`modclick\` is now real evidence.
- \`docs/linux-rewrite/tasks/P131-capture-staleness.md\` — **the capture pipeline is not stale.** A
  driven, predictable animation proved \`grim\`/compositor/\`shot()\` all deliver fresh frames. If two
  captures match, the UI genuinely did not change.
- \`docs/linux-rewrite/tasks/P129-overlay-click-routing.md\` — the overlay anchoring defect was found
  and fixed; context menus now anchor at the click point and their items actuate.

Standing traps that still hold:
- A synthetic click immediately after \`rightclick\` can outrun the menu's first repaint. Insert a
  settle between the two — a wave-H critic reproduced this and it is a harness race, not a defect.
- **Same-invocation vs separate-invocation.** A gesture and its capture must be in ONE
  \`wayland-drive.sh\` call unless you are deliberately testing restart persistence.
- \`title <text>\` sets a pane's OSC title; never write the escape sequence inline (its bare \`;\`
  splits the eval'd action block).
- For the **embedded browser only**, use \`Scripts/linux-drive.sh\` (X11) — \`wry\`'s \`build_as_child\`
  accepts only an X11 parent. See \`tasks/P127-browser-child-unavailable.md\`.

The lane **does not build** (\`[ -x "$BIN" ] || exit 2\`). Tests **per-crate**, never \`--workspace\`
(\`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\` is a known
pre-existing flake under workspace-wide concurrency; re-run the crate alone before believing it).
`

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

const SLICES = [
  {
    key: 'I1-autoname',
    rows: ['F-CORE-DOM-07'],
    brief: `**A real, well-localised regression against the Swift reference — the only \`FAILED\` row left.**

A wave-H critic established: \`AutoNamingThrottle\` is now genuinely wired (throttle, transcript
signal, \`apply_auto_title\`), but \`ClaudeCodeAdapter\`, \`CodexAdapter\` and \`PiAdapter\` **never got
\`summarizer_command\` ported** — all three fall through to the trait default returning \`None\`, which
the repo's own test \`unported_summarizers_answer_none_rather_than_guessing\` documents. The default
chat tab (\`agent_id: None\`) with default settings (\`summarizer_agent: Claude\`) hits exactly that gap,
so \`commands\` is always empty on the route the row itself names and **no title is ever produced**
unless the user first switches Settings → Summarizer to OpenCode or Oh-My-Pi.

The Swift reference implements \`summarizerCommand\` for **all five** adapters — verified:
- \`${SWIFT}/Packages/TillerAgents/Sources/TillerAgents/ClaudeCodeAdapter.swift:99\`
- \`${SWIFT}/Packages/TillerAgents/Sources/TillerAgents/CodexAdapter.swift:32\`
- \`${SWIFT}/Packages/TillerAgents/Sources/TillerAgents/PiAdapter.swift:28\`
- (already ported: \`OpenCodeAdapter.swift:53\`, \`OhMyPiAdapter.swift:49\`)
- the \`nil\` default lives at \`AgentAdapter.swift:50\` — that default exists for adapters that
  genuinely have no summarizer, which is **not** the case for these three.

Port the three, matching each CLI's real invocation as the Swift side does it. Read the Swift, don't
guess the flags — and note the repo's own hard-won lesson that these command builders are
quoting-sensitive (see \`ShellQuote.swift\`'s \`jsonStringLiteral\` and why \`.withoutEscapingSlashes\`
matters for Codex's TOML-parsed \`-c\` override).

Then delete or rewrite \`unported_summarizers_answer_none_rather_than_guessing\`, which currently
asserts the bug. The pass bar is behavioural: on a **default** chat tab with **default** settings, a
real transcript produces an auto-generated title.`,
  },
  {
    key: 'I2-xdnd',
    rows: ['F-CORE-FILE-03A'],
    brief: `**An instrument to build, not a feature.** The app code is already traced correct end to end by
an independent critic, at the vendored pinned Zed \`gpui_linux\` checkout: \`wl_data_device\` \`Enter\`
reads \`text/uri-list\` through one pipe in one background task and collects with a single
\`lines().filter_map(Url::parse)\` into a \`SmallVec\` (no per-file concurrency exists to race), \`Drop\`
carries only a position, \`window.rs\` stores that \`SmallVec\` once behind an \`Arc\`, the terminal's
\`.on_drop::<gpui::ExternalPaths>\` does \`paths().to_vec()\`, and \`terminal_file_drop\` is an
order-preserving map/join. A pre-existing test,
\`a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files\`, drives a real two-file
ordered drop through GPUI's actual mouse dispatch and passes.

What is missing is the **only thing that would make it a pass**: a real compositor-delivered XDND
drop. The critic named the instrument precisely — a \`wl_data_device_manager\`-based drag **source**
client that offers \`text/uri-list\` with several ordered paths and drops it onto the app's surface.

Build that client. Notes that will save you time:
- \`wayland-drive.sh\` runs a nested sway with a virtual pointer; your source client must connect to
  that same \`WAYLAND_DISPLAY\` and drive the drag with the same pointer serial the compositor issued.
- Rust with the \`wayland-client\` crate is the natural choice and matches the tree; if you add a
  crate, own its \`Cargo.toml\` edits and say so. Python with \`pywayland\` is acceptable if it lands
  sooner — the instrument is the deliverable, not its language.
- The contract also names a **slow-resolving provider**. On \`text/uri-list\` the whole list arrives in
  one pipe, so simulate slowness by delaying the pipe write, and say plainly if that is not the same
  hazard the macOS \`NSItemProvider\` clause was written for.
- Wire it into \`Scripts/\` so the next agent can call it as one action, and document it in
  \`docs/linux-rewrite/WAYLAND-LANE.md\`.

If, after a real attempt, a compositor-delivered XDND drop proves undrivable in this nested-sway
setup, say so with the specific protocol step that blocked you. That is a legitimate outcome and far
more useful than a substituted unit test.`,
  },
  {
    key: 'I3-tray-jump',
    rows: ['F-USE-05'],
    brief: `Half of this row is already independently PASSED: with repo2 selected, a real
\`com.canonical.dbusmenu\` \`Event\` \`"clicked"\` on repo1's roster item flipped \`workspace.list\`'s
selected flag back to repo1, reproduced fresh by a critic.

**The unverified half is the jump: clicking a roster row must activate that worktree's
worst-status tab.** Nobody has ever driven it. The wave-H critic's attempt (notify a real tab pane
id, click the tray item, read \`panel.list\`'s active flag) was blocked because the app's
single-window-per-worktree model **drops an unmounted worktree's tab entries from \`panel.list\`** — so
the instrument could not observe the tab it was asking about.

Your job is to make that observable and then observe it. Options, in rough order of preference:
1. Give the control socket a way to read the active tab of a worktree that is not currently mounted,
   or to report the roster's own resolved target — a small, honest door, not a test-only hack.
2. Drive it end to end with **two mounted** worktrees so \`panel.list\` retains both, set a
   distinguishable status on a specific tab of the non-selected one, click the tray row, and read
   which tab became active.
3. If neither works, state exactly which protocol or model constraint blocks it.

The reference behaviour is the Swift original's roster: read
\`${SWIFT}/App/AgentRosterView.swift\` (its \`select(_:)\` calls \`model.worstStatusTab(in:)\` then
\`activateTab\`). \`worst_status_tab\` should already exist on the Rust side — find it and check whether
the tray path actually calls it, because "the code paths look real on static reading" is precisely
what this row has been coasting on.`,
  },
  {
    key: 'I4-settle',
    rows: ['F-WIN-09', 'F-CHAT-33', 'F-CHAT-05'],
    brief: `Three rows that may be **terminal states rather than open work**. For each, either reach it
with a better instrument or record the honest terminal verdict with its reason. Do not leave any of
them at \`half-proven\` by default — that verdict now means "nobody finished thinking about this".

- \`F-WIN-09\` (titlebar double-click preference). The code is correct and a critic proved it reads
  live system state: flipping the real gsetting to \`minimize\` changed \`from_system()\`'s answer. A
  positive control also exists — 2 \`click\` calls never reach \`click_count == 2\` (each incurs a
  \`swaymsg\` round trip, >400 ms apart) but **3 rapid clicks do**. The blocker is that this
  **nested-sway harness force-fullscreens the single window regardless of any WM request**, confirmed
  by two independent instruments (pixel diff and \`swaymsg get_tree\` diff), so "the window minimizes"
  is categorically unobservable there. **Try the X11 lane** (\`Scripts/linux-drive.sh\`, \`DISPLAY=:1\`)
  or a nested sway configured with more than one window so the target is not the sole fullscreen
  surface. If it is genuinely unobservable on every lane available, \`UNREACHABLE\` is the correct
  verdict and you should say which lanes you tried.
- \`F-CHAT-33\` (MCP warning banner). **Five independent live attempts have now produced no banner**,
  the last two with a genuinely broken \`.mcp.json\`. The open question is unchanged and specific:
  **does the agent CLI actually write a stderr line matching \`looks_like_mcp_warning\`'s pattern for
  this failure shape?** Run the CLI by hand outside the app with a broken MCP config, capture raw
  stderr, and compare it against the predicate. Either the predicate's vocabulary is too narrow (a
  real defect — fix it) or the CLI emits nothing (then the row is unreachable through this agent and
  you should say which agent, if any, could trigger it). A passing positive control proves the
  detector is alive, not that its vocabulary is complete — that distinction is the whole task.
- \`F-CHAT-05\` (offline composer). Settled on the merits by two passes: the dangerous half is
  **disproven** — \`offline_enter_never_discards_the_typed_draft\` passes, and all four
  \`self.composer =\` write sites in \`chat.rs\` were shown not to run on a failed offline retry, so a
  typed draft cannot be lost. What remains is a **wording mismatch**: the contract says "confirm the
  editor is disabled" and the code deliberately keeps it enabled with a distinct placeholder
  ("Agent offline — reconnecting when you send…", \`chat.rs:6299\`). Drive the offline state live once
  more to confirm the current behaviour first-hand, then make the call: either implement the literal
  contract, or write the case for the reword ("confirm a failed offline Send never discards what the
  user typed") into \`docs/linux-rewrite/tasks/\` for a human to accept. Say which you did and why.`,
  },
]

function buildPrompt(s) {
  return `You are building slice **${s.key}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI port of a
macOS SwiftUI app. **350 of 389 contract rows are PASSED against a ceiling of 356; these are the last
6 open rows.** Everything easy is long gone: each of these has survived several waves of builders and
independent critics, so start from the recorded diagnosis rather than re-deriving one.

## Your rows

${s.brief}
${HOUSE}
## Your job, per row

1. Verify the recorded diagnosis still holds at HEAD, then implement it.
2. If a fix truly needs a file another slice owns, do **not** edit it — put the path in
   \`wantedForeignFiles\` and describe **the actual change**, not a direction to investigate. In wave G
   both foreign-file requests were correctly refused because they named a hypothesis; in wave H the
   one that named a concrete change was applied.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) green.
4. Commit that row alone, explicit paths, conventional-commit subject naming the row id.
5. Append to \`docs/linux-rewrite/wave-i/${s.key}-report.md\` and commit it too.

**An honest \`blocked\` with a design sketch beats a token call site that makes a grep pass while
changing no behaviour** — a critic will drive it and grade it \`FAILED — defective\` anyway. Wave H
proved both directions: rows moved where something real got wired, and \`F-CORE-DOM-07\` was caught
precisely because the wiring stopped one layer short of the adapters.

You are **not** the judge of your own work. Do not write verdicts. \`howToExercise\` is the only thing
your critic gets from you: name the control, the coordinates or \`ctl\` call, and what should visibly
change.
${DRIVE}`
}

function verifyPrompt(s, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}${r.howToExercise ? `. Route: ${r.howToExercise}` : ''}`).join('\n')
    : '- **The builder returned nothing.** It may still have committed real work — check `git log` and\n  the slice report before assuming the rows are untouched. Grade what you actually find.'

  return `Verify slice **${s.key}** of the Tiller Linux/GPUI rewrite. These are the last open rows in a
389-row functional contract, so this pass decides whether they are actually done.

**You did not build this slice and you have not seen the builder's reasoning.** The critic is never
the agent that built the piece. Everything below from the builder is a *claim* — useful for finding
the control, worthless as proof.

## The rows, and what the builder claims

${built}

## What the rows are supposed to do

${s.brief}

Inputs: \`docs/linux-rewrite/wave-i/${s.key}-report.md\` (routes, not conclusions) and
\`docs/linux-rewrite/wave-i/INTEGRATION.md\`.
${HOUSE}
The tree is built at HEAD; you should not need to compile. Tests **per-crate**, never \`--workspace\`.
${DRIVE}
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass — 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing it
proves nothing. Drive to a state the system would never reach on its own and capture a marker only
your drive could have produced — a wave-H critic drove the update toast with \`version=9.9.9-critic\`
and \`percent=42\`, values the builder never used, so the captures could only have come from that
drive. Do that. For rows that are not pixels — process lifecycle, persistence, clipboard, debounce —
the honest instrument is a real restart, a real process tree, or a real read-back. Name it. (\`xclip\`
**cannot** read this app's clipboard — it goes through smithay-clipboard, not X11 selections.)

Failure modes that have actually bitten this project:
- **A token call site.** A builder can add one call that satisfies a grep while changing no observable
  behaviour. Drive the behaviour.
- **A fix on the wrong path.** Wave D found a link handler wired only for the non-default mode.
  \`F-CORE-DOM-07\` is the same shape: everything was wired except the three adapters the **default**
  route actually uses. Check the fix is on the path a *user* takes.
- **Inherited reasoning.** Re-run it; do not transcribe the report. A wave-E cross-check caught two
  unreproducible \`PASSED\` claims that way.

## Verdicts — exactly one per row

\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires your own live exercise, this pass.
- \`UNREACHABLE\` is a real and honest verdict when no available lane can observe the row — but it is
  a claim about **instruments**, and you must say which lanes you tried. Seven rows in this project
  were once wrongly exempted as platform-impossible when they were merely unbuilt
  (\`tasks/P128-platform-exemptions-overstated.md\`); do not repeat that in the other direction.
- **Be willing to fail your builder.** An all-PASSED slice is likelier to be a lazy critic than a
  healthy codebase. Equally, be willing to *raise* a row the ledger has wrong — wave G found two
  sitting at \`FAILED — defective\` over defects that had since been fixed or were harness artifacts.

\`newEvidenceCell\` lands verbatim in the ledger: your own finding, the instrument, the frame, 1-3
sentences. **Strip every raw \`|\`** and **never use triple backticks** — an odd run of backticks
swallows the next cell and breaks the totals gate.

Write \`docs/linux-rewrite/wave-i/${s.key}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema. **That file is the real record** — in wave H one critic's
structured answer came back empty and its committed file was the only reason its three verdicts
survived. Write the file first, return the schema second.`
}

// ---------------------------------------------------------------- run

phase('Build')
log('wave I: the last 6 open rows. Serial — every slice may touch main.rs.')

const built = {}
for (const s of SLICES) {
  log(`build ${s.key}: ${s.rows.join(', ')}`)
  try {
    const r = await agent(buildPrompt(s), {
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
const integration = await agent(`You are the integrator for wave I of the Tiller Linux/GPUI rewrite — the final build wave over a
389-row functional contract. Four build slices committed to \`linux/gpui-waku\` in one shared worktree,
one at a time. Base for this wave: \`e17c94f6\`.
${HOUSE}
In order, committing after each step:

1. \`cd ${RUST} && cargo build -p tiller\` green. If a builder left it broken, fix it — smallest change
   that restores the build, never a revert of someone's work.
2. \`cargo test -p <crate>\` **per crate**, never \`--workspace\`. Known pre-existing flake under
   workspace-wide concurrency:
   \`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\` in
   \`tiller_terminal\`; re-run the crate alone before believing it.
3. **Cross-slice sweep.** For every file touched since \`e17c94f6\`, run
   \`git log --format='%h %s' e17c94f6..HEAD -- <file>\` and check each commit against
   \`docs/linux-rewrite/wave-i/manifest.json\` (\`slices[<name>].owns\` is the file list, \`.ids\` the
   rows). Compare **file ownership, not commit-subject text** — matching conventional-commit scope to
   slice name produced 12 false positives out of 13 in wave B. Then diff each commit in isolation and
   read its **deleted** lines against its own message, looking for a deletion that quietly removes a
   prior, unrelated fix. Restore anything lost. Note two legitimate exceptions before you flag
   anything: \`rust/crates/tiller/src/main.rs\` has multiple legitimate owners, and the orchestrator
   commits \`docs/linux-rewrite/INVENTORY-LEDGER.md\` alone, outside every slice.
   One deletion in this wave is **expected and correct**: \`I1-autoname\` was told to remove or rewrite
   \`unported_summarizers_answer_none_rather_than_guessing\`, a test that asserted the bug it fixed.
4. Apply any fix a builder needed but correctly refused to make in a file it did not own — **apply
   what is described, do not invent one.** If a request names a hypothesis rather than a concrete
   change, leave it and say so.

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (none requested)'}

5. Rebuild and leave a current binary at \`${RUST}/target/debug/tiller\`. The critics that run next use
   the drive lanes, which **do not build** and exit 2 on a missing binary. Your responsibility.

Write \`docs/linux-rewrite/wave-i/INTEGRATION.md\` and commit it. Return a short plain-text summary:
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
log(`wave I verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  integration: String(integration || ''),
  buildersMissing: SLICES.map((s) => s.key).filter((k) => !built[k]),
  verifiersMissing: SLICES.map((s) => s.key).filter((k, i) => !verdicts[i]),
}
