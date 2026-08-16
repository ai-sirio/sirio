export const meta = {
  name: 'wave-g',
  description: 'Close the last 21 open inventory rows of the Tiller Linux/GPUI rewrite',
  phases: [
    { title: 'Build', detail: '6 slices, one per round — every slice needs main.rs, so none overlap' },
    { title: 'Integrate', detail: 'compile, per-crate tests, cross-slice revert sweep' },
    { title: 'Verify', detail: 'one fresh critic per slice, exercising live' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const ROUNDS = [['G1-browser'], ['G2-gaps-a'], ['G3-gaps-b'], ['G4-chat'], ['G5-terminal'], ['G6-sidebar']]
const SLICES = ROUNDS.flat()

const HOUSE = `
## House rules

Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY there. Never touch \`/home/enzopalmisano/Scrivania/Progetti/tiller\` — the Swift original.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator is its single writer.

**Commit with explicit file paths only** — never \`git add <dir>\`, \`git add -A\`, or \`git commit -a\`.
In wave B an agent staged a file it did not own and byte-for-byte reverted a landed 167-line fix inside
a commit whose subject was about another crate; nothing flagged it. Before declaring a row done,
\`grep\` your own file for the symbol you added and confirm it is still there, then run
\`git status --porcelain\` — explicit paths also silently drop new files nobody is thinking about.

On \`index.lock\` contention retry in a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row.** You are killed after 180 s without output. In wave C an agent completed 15
rows of real work then failed on its final report — the work survived only because it had committed as
it went. **Keep your returned answer small**: the committed markdown is the record, the schema is an
index. One or two sentences per field.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\` from \`${ROOT}\`.
No lock — parallel-safe if \`TILLER_WL_LABEL\` is unique (it names every \`/tmp\` path the instance uses).
**You can see images — open the PNGs with \`Read\` and look at them.**

Read \`docs/linux-rewrite/WAYLAND-LANE.md\` first. Vocabulary:
\`ctl · click · move · type · key · title · shot · rightclick · chord <mod> <key> · down · up · drag ·
scroll <x> <y> <steps> · modclick <mod> <x> <y>\`. \`modclick\` was never proven end to end — a null
result from it is inconclusive, not evidence.

**Coordinates are now stable.** \`shot()\` used to alternate the output between two resolutions, so
coordinates read off one frame were sent while the output was at the other size and clicks landed in
dead space — that manufactured false negatives on two rows before a critic caught it. Every frame is
now captured at a fixed 1715x972. Read coordinates off the most recent frame and they are valid.

Standing traps:
- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state** — \`agent\`
  reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven with a positive control.
  Valid \`notify\` statuses: \`running\`, \`needs-input\`, \`done\`, \`error\`.
- **The Clone/Create popover draws in the wrong place** —
  \`docs/linux-rewrite/tasks/P123-popover-click-routing.md\`. Its overlay is positioned relative to the
  Sidebar's ~280px root div instead of the window root, so it is confined to the sidebar column even
  though its dark-scrim styling implies it is window-centred. Aim where it actually draws.
- **A stale frame is not evidence.** Repaint is lazy; verify a capture actually changed before reading
  it as "the feature did nothing".

The lane **does not build** (\`[ -x "$BIN" ] || exit 2\`). The tree is built at HEAD; you should not need
to compile to drive. Run tests **per-crate**, never \`--workspace\`.
`

const BROWSER = `
## This slice is the one that needs the X11 lane

**The embedded browser cannot be exercised on the Wayland lane at all**, and every earlier
"Browser child is unavailable" report came from that, not from a broken feature. \`wry\`'s
\`build_as_child\` accepts only an X11 parent; GPUI on Wayland hands it a Wayland surface, so no
webview is ever constructed. Full root cause and proof:
\`docs/linux-rewrite/tasks/P127-browser-child-unavailable.md\`.

On X11 the browser **works** — this exact sequence was run and produced
\`loading:false\`, \`title:"Example Domain"\`, and \`browser.eval script=document.title\` →
\`"Example Domain"\`:

\`\`\`bash
export TILLER_SOCKET=/tmp/<label>.sock TILLER_DB=/tmp/<label>.sqlite
Scripts/linux-drive.sh out.png '
  python3 Scripts/control-probe.py "$TILLER_SOCKET" project.add path=<a real git repo>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" workspace.select workspace=<id from workspace.list>
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.open url=https://example.com
  sleep 5
  python3 Scripts/control-probe.py "$TILLER_SOCKET" browser.get
'
\`\`\`

Three things that each look exactly like a broken browser and are not:
1. **\`DISPLAY=:1\` alone is not enough.** GPUI prefers Wayland whenever \`WAYLAND_DISPLAY\` is set.
   \`linux-drive.sh\` does \`env -u WAYLAND_DISPLAY\`; a hand-rolled launch that skips that silently
   runs on Wayland and reproduces the failure while looking like an X11 run.
2. **\`project.add\` does not select a workspace.** Without \`workspace.select workspace=<id>\` (the
   param is \`workspace\`, **not** \`path\`) every browser call returns \`no current workspace\` or
   \`no browser surface\`.
3. **\`browser.wait timeoutMs\` above ~5 s is truncated** by the hardcoded \`CONTROL_ACTION_TIMEOUT\`
   (\`main.rs:196\`) and fails with \`control action timed out\` — which reads as "the condition never
   happened". See \`tasks/P126-…\`. Poll \`browser.get\` instead of relying on a long \`wait\`.

\`Scripts/linux-drive.sh\` takes a **global lock** (one X pointer on the display) — hold it briefly.
It has no \`ctl\` action; use \`Scripts/control-probe.py <socket> <method> [k=v …]\`.

Note also that \`F-CTRL-BROWSER-05\` and \`F-CTRL-BROWSER-06\` may need **no code change at all** —
they were graded \`half-proven\` under the broken instrument. Check before writing code, and if they
already work say so with \`already-correct\` and a route your critic can re-run.
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

function buildPrompt(slice) {
  return `You are building slice **${slice}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI port
of a macOS SwiftUI app. **These are the last 21 open rows of a 389-row functional contract; 328 are
already PASSED and 40 are terminal.** Every row here has survived several waves, so assume the easy
reading is wrong and start from the recorded diagnosis rather than re-deriving one.

**Read \`docs/linux-rewrite/wave-g/${slice}.md\` first.** Each row carries a critic's evidence written
today against the current tree, usually naming the exact function and the exact reason it fails.
${slice === 'G1-browser' ? BROWSER : ''}
${HOUSE}
## Your job, per row

1. Verify the recorded root cause still holds at HEAD, then fix it **in the files you own** (your brief
   lists them; no other slice is running while you are).
2. If a fix truly needs a file outside that list, do **not** edit it — put the path in
   \`wantedForeignFiles\` and describe the precise change in your committed report so the integrator can
   apply it. This convention has saved landed work in four consecutive waves.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) green.
4. Commit that row alone, explicit paths, conventional-commit subject naming the row id.
5. Append to \`docs/linux-rewrite/wave-g/${slice}-report.md\` and commit it too.

Several rows in this wave are **architectural gaps, not bugs** — a type exists with zero production
callers (\`BootstrapRestoreOrder\`, \`WorktreeMountPolicy\`, \`AutoNamingThrottle\`, \`LayoutCommand\`,
\`WorkspaceTabViewState\`). Wiring one up may mean building the concept it belongs to. If a row needs
more than this slice can hold, say so with \`blocked\` and sketch the real shape of the work.
**An honest \`blocked\` with a design sketch is worth far more than a token call site that makes a grep
pass while changing no behaviour** — a critic will drive it and grade it \`FAILED — defective\` anyway.

You are **not** the judge of your own work. Do not write verdicts. \`howToExercise\` is the only thing
your critic gets from you: name the control, the coordinates or \`ctl\` call, and what should visibly
change.
${DRIVE}`
}

function verifyPrompt(slice, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}${r.howToExercise ? `. Route: ${r.howToExercise}` : ''}`).join('\n')
    : '- **The builder returned nothing.** It may still have committed real work — check `git log` and\n  the slice report before assuming the rows are untouched. Grade what you actually find.'

  return `Verify slice **${slice}** of the Tiller Linux/GPUI rewrite — the final build wave over a 389-row
functional contract.

**You did not build this slice and you have not seen the builder's reasoning.** The standing rule is
that the critic is never the agent that built the piece. Everything below from the builder is a *claim*
— useful for finding the control, worthless as proof.

## The rows, and what the builder claims

${built}

Inputs: \`docs/linux-rewrite/wave-g/${slice}.md\`, \`docs/linux-rewrite/wave-g/${slice}-report.md\`
(routes, not conclusions), \`docs/linux-rewrite/wave-g/INTEGRATION.md\`.
${slice === 'G1-browser' ? BROWSER : ''}
${HOUSE}
The tree is built at HEAD; you should not need to compile. Tests **per-crate**, never \`--workspace\`.
${DRIVE}
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass — 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing it
proves nothing. Drive to a state the system would never reach on its own and capture a marker only your
drive could have produced. For rows that are not pixels — process lifecycle, persistence, clipboard,
debounce — the honest instrument is a real restart, a real process tree, or a real read-back. Name the
instrument. (Note: \`xclip\` **cannot** read this app's clipboard — it goes through smithay-clipboard,
not X11 selections. A wave-F critic had to build a wlroots data-control reader to check a copy action.)

Two failure modes are specific to this wave:
- **A token call site.** Several rows are zero-caller types that needed wiring. A builder can add one
  call that satisfies a grep while changing no observable behaviour. Drive the behaviour.
- **A fix on the wrong path.** Wave D found a link handler wired only for the non-default mode, and a
  \`focus_path\` called before the async task that populates what it searches. Check the fix is on the
  path a **user** takes, not merely one a test takes.

## Verdicts — exactly one per row

\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires your own live exercise, this pass.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** An all-PASSED slice is far likelier to be a lazy critic than a
  healthy codebase — a wave-F audit downgraded 4 of 15 rows that everyone believed were done.

\`newEvidenceCell\` lands verbatim in the ledger: your own finding, the instrument, the frame, 1-3
sentences. **Strip every raw \`|\`** (it shifts every column after it) **and never use triple
backticks** — an odd run of backticks swallows the next cell and breaks the totals gate.

Write \`docs/linux-rewrite/wave-g/${slice}-verdicts.md\`, committing after **each row** with an explicit
path, then return the schema. **That file is the real record** — if the structured answer fails to
validate, the committed file is what survives.`
}

log(`wave G: the last 21 open rows, ${SLICES.length} slices in ${ROUNDS.length} rounds (every slice needs main.rs, so none overlap)`)

const built = {}
for (let r = 0; r < ROUNDS.length; r++) {
  log(`round ${r + 1}/${ROUNDS.length}: ${ROUNDS[r].join(' + ')}`)
  await parallel(ROUNDS[r].map((s) => () =>
    agent(buildPrompt(s), { label: `build:${s}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' })
      .then((x) => { if (x) built[s] = x; return x })
      .catch((e) => { log(`${s} FAILED (${String(e).slice(0, 110)}) — its committed work may still stand`); return null })))
}

const wanted = Object.entries(built).flatMap(([s, x]) =>
  ((x && x.wantedForeignFiles) || []).map((f) => `${s} wanted ${f}`))
log(`build closed — ${Object.keys(built).length}/${SLICES.length} slices reported, ${wanted.length} foreign-file requests`)

const integration = await agent(`You are the integrator for wave G of the Tiller Linux/GPUI rewrite — the final build wave over a
389-row functional contract. Six agents committed to \`linux/gpui-waku\` in one shared worktree, one at
a time. Base for this wave: \`6ce47a9\`.
${HOUSE}
In order, committing after each step:

1. \`cd ${RUST} && cargo build -p tiller\` green. If a builder left it broken, fix it — smallest change
   that restores the build, never a revert of someone's work.
2. \`cargo test -p <crate>\` **per crate**, never \`--workspace\` (two pre-existing tests flake only under
   full-workspace concurrency; re-run a crate alone before believing a failure).
3. **Cross-slice sweep.** For every file touched since \`6ce47a9\`, run
   \`git log --format='%h %s' 6ce47a9..HEAD -- <file>\` and check each commit against
   \`docs/linux-rewrite/wave-g/manifest.json\` — \`slices[<name>].owns\` is the file list, \`.ids\` the
   rows. Compare **file ownership, not
   commit-subject text** — matching conventional-commit scope to slice name produced 12 false positives
   out of 13 in wave B. Then diff each commit in isolation and read its **deleted** lines against its own
   message, looking for a deletion that quietly removes a prior, unrelated fix. Restore anything lost.
   **Do not assume a commit came from the slice that owns the file** — wave C's integrator attributed
   four \`main.rs\` commits to agents that had never run. If you cannot attribute a commit, say so.
4. Apply any fix a builder needed but correctly refused to make in a file it did not own — each is
   described in that builder's report; apply what is described, do not invent one:

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (none requested)'}

5. Rebuild and leave a current binary at \`${RUST}/target/debug/tiller\`. The critics that run next use
   the drive lanes, which **do not build** and exit 2 on a missing binary. Your responsibility.

Write \`docs/linux-rewrite/wave-g/INTEGRATION.md\` and commit it. Return a short plain-text summary:
build, tests, commits examined, genuine reverts found, what you restored, anything unattributable.`,
  { label: 'integrate', phase: 'Integrate' })

log(`integration: ${String(integration).slice(0, 400)}`)

const verdicts = await parallel(SLICES.map((s) => () =>
  agent(verifyPrompt(s, built[s]), { label: `verify:${s}`, phase: 'Verify', schema: VERDICT_SCHEMA, effort: 'high' })))

const all = verdicts.filter(Boolean)
const rows = all.flatMap((x) => (x.rows || []).map((y) => ({ ...y, slice: x.slice })))
const tally = {}
for (const x of rows) tally[x.verdict] = (tally[x.verdict] || 0) + 1
log(`wave G verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  integration: String(integration || ''),
  buildersMissing: SLICES.filter((s) => !built[s]),
  verifiersMissing: SLICES.filter((s, i) => !verdicts[i]),
}
