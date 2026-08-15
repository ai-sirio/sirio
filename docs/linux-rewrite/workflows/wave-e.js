export const meta = {
  name: 'wave-e',
  description: 'Close the last 34 open inventory rows of the Tiller Linux/GPUI rewrite',
  phases: [
    { title: 'Build', detail: '8 slices in 5 conflict-free rounds — no two concurrent slices share a file' },
    { title: 'Integrate', detail: 'compile, per-crate tests, cross-slice revert sweep' },
    { title: 'Verify', detail: 'one fresh critic per slice, exercising live with the extended lane' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const ROUNDS = [
  ['E-C-3'],
  ['E-C-2', 'E-P3'],
  ['E-C-1', 'E-P4'],
  ['E-C-4', 'E-P2'],
  ['E-P1'],
]
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

**Commit after EVERY row.** You are killed after 180 s without output. In wave C an agent completed
15 rows of real work and then failed on its final report — the work survived only because it had
committed each row as it went.

**Keep your returned answer small.** Two wave-C agents died on the structured-output retry cap by
returning huge objects. The committed markdown file is the record; the schema is only an index. One
or two sentences per field.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\` from \`${ROOT}\`.
No lock — parallel-safe provided \`TILLER_WL_LABEL\` is unique (it names every \`/tmp\` path the
instance uses). **You can see images — open the PNGs with \`Read\` and look at them.**

**The lane was extended on 2026-08-15 and is much more capable than older notes suggest.** Read
\`docs/linux-rewrite/WAYLAND-LANE.md\`. Vocabulary now:
\`ctl · click · move · type · key · title · shot · rightclick · chord <mod> <key> · down · up ·
drag · scroll <x> <y> <steps> · modclick <mod> <x> <y>\`.
\`rightclick\`, \`chord\`, \`drag\` and \`scroll\` each shipped with a positive control frame.
\`modclick\` is implemented but was **never proven end-to-end** — treat a null result from it as
inconclusive, not as evidence, and say so.

Standing traps:
- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state** —
  \`agent\` reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven with a
  positive control. Valid \`notify\` statuses: \`running\`, \`needs-input\`, \`done\`, \`error\`.
- **The Clone/Create popover renders in the wrong place** — see
  \`docs/linux-rewrite/tasks/P123-popover-click-routing.md\`. Its overlay is positioned relative to
  the Sidebar's ~280px root div instead of the window root, so it is confined to the sidebar column
  even though its dark-scrim styling implies it is window-centred. Clicks aimed where it *looks*
  like it should be miss. Aim at where it actually draws.

The lane **does not build** (\`[ -x "$BIN" ] || exit 2\`) — that is what makes it parallel-safe.
Frames under \`reference/linux-progress/sweep-D*/\` are orphans; **no verdict may cite them.**
`

const EVIDENCE = `
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass — 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing it
proves nothing. Drive the system to a state it would never reach on its own and capture a marker only
your drive could have produced. If you cannot tell your PASS frame from a frame of the broken build,
say so and grade \`half-proven\`.

For rows that are not pixels — persistence, process lifecycle, config — the honest instrument is a real
restart or a real process tree, not a screenshot. Name the instrument you used.
`

const VERDICTS = `\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires the behaviour exercised live and observed to work. A builder's word is not evidence;
  neither is a green test.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** An all-PASSED slice is far likelier to be a lazy critic than a
  healthy codebase.`

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
          newEvidenceCell: { type: 'string', description: 'ledger evidence, 1-3 sentences, no raw | characters' },
        },
      },
    },
  },
}

function buildPrompt(slice) {
  return `You are building slice **${slice}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI port
of a macOS SwiftUI app. **These are the last 34 open rows in a 389-row functional contract; 315 are
already PASSED.** Every row here has survived several waves, so assume the easy reading is wrong and
the recorded diagnosis is worth trusting.

**Read \`docs/linux-rewrite/wave-e/${slice}.md\` first.** Each row carries a critic's evidence written
yesterday against the current tree — usually a precise root cause, not a vague complaint. Several rows
name the exact function and the exact reason it fails. Read it as a bug report and start from its
diagnosis rather than re-deriving one.
${HOUSE}
## Your job, per row

1. Start from the recorded root cause; verify it still holds at HEAD before acting on it.
2. Implement or fix it **in the files you own** (your brief lists them; no concurrent slice owns any).
   If a fix truly needs a file outside that list, do **not** edit it — list the path in
   \`wantedForeignFiles\` and describe the precise change in your committed report so the integrator
   can apply it. This convention has saved landed work in three consecutive waves.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) green.
4. Commit that row alone, explicit paths, conventional-commit subject naming the row id.
5. Append to \`docs/linux-rewrite/wave-e/${slice}-report.md\` and commit it too.

Some rows in this wave are **architectural gaps, not bugs** — a type exists with zero production
callers (\`BootstrapRestoreOrder\`, \`WorktreeMountPolicy\`, \`AutoNamingThrottle\`,
\`LayoutCommand\`, \`WorkspaceTabViewState\`). Wiring one up may mean building the concept it belongs
to, which can be genuinely large. If a row needs more than this slice can hold, say so with
\`blocked\` and describe what the real shape of the work is. **An honest \`blocked\` with a design
sketch is worth far more here than a token call site that makes a grep pass while changing no
behaviour** — a critic will drive it and grade it \`FAILED — defective\` anyway.

You are **not** the judge of your own work. Do not write verdicts. \`howToExercise\` is the only thing
your critic gets from you; name the control, the coordinates or \`ctl\` call, and what should visibly
change.
${DRIVE}`
}

function verifyPrompt(slice, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}${r.howToExercise ? `. Route: ${r.howToExercise}` : ''}`).join('\n')
    : '- **The builder returned nothing.** It may still have committed real work — check `git log` and the\n  slice report before assuming the rows are untouched. Grade what you actually find.'

  return `Verify slice **${slice}** of the Tiller Linux/GPUI rewrite — the final wave over a 389-row
functional contract.

**You did not build this slice and you have not seen the builder's reasoning.** The standing rule is
that the critic is never the agent that built the piece. Everything below from the builder is a
*claim* — useful for finding the control, worthless as proof.

## The rows, and what the builder claims

${built}

Inputs: \`docs/linux-rewrite/wave-e/${slice}.md\` (rows + the previous critic's root cause),
\`docs/linux-rewrite/wave-e/${slice}-report.md\` (the builder's write-up — routes, not conclusions),
\`docs/linux-rewrite/wave-e/INTEGRATION.md\` (the integrator's results).
${HOUSE}
The tree is built at HEAD; you should not need to compile. Run tests **per-crate**, never
\`--workspace\` — two pre-existing timing-sensitive tests flake only under full-workspace concurrency.
${DRIVE}
${EVIDENCE}
Two failure modes are specific to this wave, and both should make you more skeptical, not less:

- **A token call site.** Several rows are zero-caller types that needed wiring. A builder can add one
  call that satisfies a grep while changing no observable behaviour. Drive the behaviour; do not
  accept "it now has a caller" as a pass.
- **A fix on the wrong path.** Wave D found two of these — a link handler wired only for the
  non-default mode, and \`focus_path\` called before the async task that populates what it searches.
  Check that the fix is on the path a **user** actually takes, not merely on a path a test takes.

## Verdicts — exactly one per row

${VERDICTS}

A behaviour you could not reach is \`UNREACHABLE\`, not \`FAILED\` — the ledger counts them separately.
A row you ran out of time for is \`NOT EXERCISED\`; say so rather than inheriting the builder's word.

\`newEvidenceCell\` lands verbatim in the ledger: your own finding, the instrument, the frame, 1-3
sentences, and **strip every raw \`|\`** — one raw pipe shifts every column after it and breaks the gate.

Write \`docs/linux-rewrite/wave-e/${slice}-verdicts.md\`, committing after **each row** with an explicit
path, then return the schema. **That file is the real record** — if the structured answer fails to
validate, the committed file is what survives, so never leave it until the end.`
}

// ---------------------------------------------------------------- run
log(`wave E: 34 rows in ${SLICES.length} slices across ${ROUNDS.length} conflict-free rounds — no two concurrent slices share a file`)

const built = {}

for (let r = 0; r < ROUNDS.length; r++) {
  const round = ROUNDS[r]
  log(`round ${r + 1}/${ROUNDS.length}: ${round.join(' + ')}`)
  await parallel(round.map((s) => () =>
    agent(buildPrompt(s), { label: `build:${s}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' })
      .then((x) => { if (x) built[s] = x; return x })
      .catch((e) => { log(`${s} FAILED (${String(e).slice(0, 110)}) — its committed work may still stand`); return null })))
}

const wanted = Object.entries(built).flatMap(([s, x]) =>
  ((x && x.wantedForeignFiles) || []).map((f) => `${s} wanted ${f}`))
log(`build closed — ${Object.keys(built).length}/${SLICES.length} slices reported, ${wanted.length} foreign-file requests`)

const integration = await agent(`You are the integrator for wave E of the Tiller Linux/GPUI rewrite — the final build wave over a
389-row functional contract. Eight agents committed to \`linux/gpui-waku\` in one shared worktree,
scheduled so that no two concurrent slices shared a file. Base for this wave: \`eb34610\`.
${HOUSE}
In order, committing after each step:

1. \`cd ${RUST} && cargo build -p tiller\` green. If a builder left it broken, fix it — smallest change
   that restores the build, never a revert of someone's work.
2. \`cargo test -p <crate>\` **per crate**, never \`--workspace\` (two pre-existing tests flake only under
   full-workspace concurrency; re-run a crate alone before believing a failure).
3. **Cross-slice sweep.** For every file touched since \`eb34610\`, run
   \`git log --format='%h %s' eb34610..HEAD -- <file>\` and check each commit against
   \`docs/linux-rewrite/wave-e/manifest.json\`'s **file** ownership map. Compare **file ownership, not
   commit-subject text** — matching conventional-commit scope to slice name produced 12 false positives
   out of 13 in wave B. Then diff each commit in isolation and read its **deleted** lines against its own
   message, looking for a deletion that quietly removes a prior, unrelated fix. Restore anything lost,
   with an explicit path and a subject saying so. **Do not assume a commit came from the slice that owns
   the file** — wave C's integrator attributed four \`main.rs\` commits to agents that had never run. If
   you cannot attribute a commit, say so rather than inferring.
4. Apply any fix a builder needed but correctly refused to make in a file it did not own — each is
   described in that builder's report; apply what is described, do not invent one:

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (none requested)'}

5. Rebuild and leave a current binary at \`${RUST}/target/debug/tiller\`. The critics that run next use
   the Wayland lane, which **does not build** and exits 2 on a missing binary. Your responsibility.

Write \`docs/linux-rewrite/wave-e/INTEGRATION.md\` and commit it. Return a short plain-text summary:
build, tests, commits examined, genuine reverts found, what you restored, anything unattributable.`,
  { label: 'integrate', phase: 'Integrate' })

log(`integration: ${String(integration).slice(0, 400)}`)

const verdicts = await parallel(SLICES.map((s) => () =>
  agent(verifyPrompt(s, built[s]), { label: `verify:${s}`, phase: 'Verify', schema: VERDICT_SCHEMA, effort: 'high' })))

const all = verdicts.filter(Boolean)
const rows = all.flatMap((x) => (x.rows || []).map((y) => ({ ...y, slice: x.slice })))
const tally = {}
for (const x of rows) tally[x.verdict] = (tally[x.verdict] || 0) + 1
log(`wave E verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  integration: String(integration || ''),
  slices: all.map((x) => ({ slice: x.slice, count: (x.rows || []).length, notes: x.notes })),
  buildersMissing: SLICES.filter((s) => !built[s]),
  verifiersMissing: SLICES.filter((s, i) => !verdicts[i]),
}
