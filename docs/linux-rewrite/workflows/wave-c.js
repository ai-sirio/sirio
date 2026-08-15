export const meta = {
  name: 'wave-c',
  description: 'Build and independently verify the 117 outstanding inventory rows of the Tiller Linux/GPUI rewrite',
  phases: [
    { title: 'Build', detail: 'two serialised chains (main.rs x4, chat.rs x3) alongside 4 file-disjoint parallel slices + 1 investigate-only slice' },
    { title: 'Integrate', detail: 'compile, test, and sweep for cross-slice edits' },
    { title: 'Verify', detail: 'one fresh critic per slice — never the agent that built it — exercising live' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

// ---------------------------------------------------------------- slice plan
const CHAINS = [
  { chain: 'C-CHAT', links: ['C-CHAT-1', 'C-CHAT-2', 'C-CHAT-3'] },
  { chain: 'C-MAIN', links: ['C-MAIN-1', 'C-MAIN-2', 'C-MAIN-3', 'C-MAIN-4'] },
]
const PARALLEL = ['C-P1', 'C-P2', 'C-P3', 'C-P4']
const INVESTIGATE = 'C-U'
const ALL_SLICES = [...CHAINS.flatMap((c) => c.links), ...PARALLEL, INVESTIGATE]

// ---------------------------------------------------------------- shared text
const HOUSE = `
## House rules — these are not style preferences

Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY inside that worktree. Never touch \`/home/enzopalmisano/Scrivania/Progetti/tiller\` —
that is the Swift original this project is a rewrite of, and it is not yours to edit.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator applies every verdict
as the single writer. An agent editing it directly corrupts the totals gate.

**Commit with explicit file paths only.** Never \`git add <dir>\`, never \`git add -A\`, never
\`git commit -a\`. This is not tidiness: during wave B two agents staged a file they did not own,
and one of them byte-for-byte reverted a landed, tested 167-line fix inside a commit whose subject
line was about a different crate. Nothing in git flagged it. Before you declare a row done,
\`grep\` your own file for the symbol you added and confirm it is still there.

Also run \`git status --porcelain\` before you finish: explicit paths prevent collisions but
silently drop new files nobody is thinking about. If you created a file, commit it by name.

On \`index.lock\` contention, retry in a loop — **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row, not at the end.** You will be killed after 180 seconds without output,
and retried from scratch. In an earlier fleet all four agents died this way and lost 213 files of
real work because each was holding its findings in memory for one report at the end. An honest
partial result that landed beats a perfect one that never did.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\`
run from \`${ROOT}\`. That lane takes **no lock**, so it is safe to run while siblings drive too —
but only if your \`TILLER_WL_LABEL\` is unique, because it names every \`/tmp\` path the instance uses.

Actions: \`ctl <method> [k=v]\`, \`click x y\`, \`move x y\`, \`type <text>\`, \`key <name>\`,
\`title <text>\`, \`shot <name>\`. **You can see images — open the PNGs with \`Read\` and look at them.**

Read \`docs/linux-rewrite/WAYLAND-LANE.md\` before your first drive. Two traps in particular:

- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state.**
  \`agent\` reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\` — proven with a
  positive control. A null result from them is evidence of nothing. Valid \`notify\` statuses are
  \`running\`, \`needs-input\`, \`done\`, \`error\`; \`working\` is rejected.

**The drive script does NOT build** — it runs \`[ -x "$BIN" ] || exit 2\`. That is exactly what makes
it safe to run in parallel. If it exits 2, the binary is missing; build it once yourself, and say so.

The 213 frames under \`reference/linux-progress/sweep-D*/\` are orphans from agents that died before
reporting. **No verdict may cite them** — see that directory's README.
`

const EVIDENCE = `
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass: 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** When a row's correct value and its default are the same value, capturing
that value proves nothing. Drive the system to a state it would never reach on its own, and capture a
marker only your drive could have produced. If you cannot tell your PASS frame from a frame of the
broken build, you have not proven anything — say so and grade it \`half-proven\`.

For rows that are not pixels — persistence, process lifecycle, config — the honest instrument is
usually a real restart or a real process tree, not a screenshot: kill and relaunch against the same
DB and read the value back; spawn a job-control child and confirm the descendant group actually dies.
Always name the instrument you used.
`

const VERDICTS = `
\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires the behaviour exercised live and observed to work. The builder saying it built
  it is not evidence, and neither is a green test.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** A slice that comes back all-PASSED is far likelier to be a
  lazy critic than a healthy codebase. Two of the previous waves' most valuable results were a
  \`FAILED — defective\` and a retracted finding.
`

// ---------------------------------------------------------------- schemas
const BUILD_SCHEMA = {
  type: 'object',
  required: ['slice', 'rows'],
  additionalProperties: false,
  properties: {
    slice: { type: 'string' },
    filesTouched: { type: 'array', items: { type: 'string' } },
    buildsClean: { type: 'boolean' },
    wantedForeignFiles: {
      type: 'array', items: { type: 'string' },
      description: 'files you needed but did not own — describe the fix in notes, do NOT edit them',
    },
    notes: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        required: ['id', 'action', 'howToExercise'],
        additionalProperties: false,
        properties: {
          id: { type: 'string' },
          action: { type: 'string', enum: ['implemented', 'fixed', 'already-correct', 'blocked', 'not-attempted'] },
          commit: { type: 'string', description: 'short sha, or empty if no code change was needed' },
          what: { type: 'string' },
          howToExercise: { type: 'string', description: 'the exact gesture a critic should drive to see this live' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  required: ['slice', 'rows'],
  additionalProperties: false,
  properties: {
    slice: { type: 'string' },
    verdictPath: { type: 'string' },
    framesRead: { type: 'integer' },
    notes: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        required: ['id', 'verdict', 'why', 'evidenceDiscriminates', 'newEvidenceCell'],
        additionalProperties: false,
        properties: {
          id: { type: 'string' },
          verdict: {
            type: 'string',
            enum: ['PASSED', 'half-proven', 'FAILED — absent', 'FAILED — defective',
                   'UNREACHABLE', 'N/A — platform', 'NOT EXERCISED'],
          },
          why: { type: 'string' },
          evidenceDiscriminates: { type: 'boolean' },
          newEvidenceCell: { type: 'string', description: 'ledger evidence text; no raw | characters' },
        },
      },
    },
  },
}

// ---------------------------------------------------------------- prompts
function buildPrompt(slice, extra) {
  return `You are building slice **${slice}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI
port of a macOS SwiftUI app. Your rows are outstanding inventory entries: each is a behaviour the
frozen 389-row functional contract requires, which a critic has judged missing, defective, or only
half-proven.

**Read \`docs/linux-rewrite/wave-c/${slice}.md\` first.** It lists your rows, the files triage named
for each, and the exact evidence a previous critic recorded — including *why* it fell short. That
"evidence on record" is your best guide to what is actually wrong; read it as a bug report.
${extra}
${HOUSE}
## Your job

For each row, in order:

1. Read the row's current evidence and find the real gap in the code. The recorded verdict tells you
   what a critic could not see; \`half-proven\` usually means half the behaviour exists.
2. Implement or fix it **in the files you own**. If the fix genuinely needs a file you do not own,
   do **not** edit it — put the path in \`wantedForeignFiles\` and describe the precise change in
   \`notes\`, so the integrator can apply it. During wave B this convention worked exactly once and
   saved a landed fix.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) and get it green.
4. Commit that row alone, explicit paths, conventional-commit subject.
5. Append what you did to \`docs/linux-rewrite/wave-c/${slice}-report.md\` and commit it too.

Then return the schema.

\`howToExercise\` matters more than it looks: it is the only thing your critic gets from you, and a
critic who cannot find the route grades the row \`NOT EXERCISED\`. Give a concrete gesture — which
control, which coordinates or which \`ctl\` call, and what should visibly change.

You are **not** the judge of your own work. Do not write verdicts and do not claim a row passes; a
separate critic that has never seen your reasoning will exercise every row live. Report honestly
what you could not do — \`blocked\` with a reason is a useful result, an optimistic \`fixed\` is not.
${DRIVE}
You may drive the app to understand current behaviour, and it is usually worth doing before you
change code — but rebuild first if you have edited anything, because the lane runs whatever binary
is on disk.
`
}

function investigatePrompt(slice) {
  return `You are the critic for slice **${slice}** of the Tiller Linux/GPUI rewrite — 9 inventory rows
that triage never managed to map to any source file.

**You are not building anything. Do not edit a single line of Rust.** Your job is to find out what
these rows' behaviours actually do today, exercise them, and grade them. Because you wrote none of
this code, you are a valid critic by construction.

Read \`docs/linux-rewrite/wave-c/${slice}.md\` for the rows and their recorded evidence.
${HOUSE}
${DRIVE}
${EVIDENCE}
## Deliverable

For each row: locate the behaviour in the codebase (\`rg\` is available; the row text names the
feature), exercise it live, and grade it. Where a row turns out to need code that does not exist,
grade it \`FAILED — absent\` and say in \`newEvidenceCell\` **which file a fix would touch** — that
mapping is the durable thing this slice produces, and the next wave depends on it.

Verdicts — exactly one per row:
${VERDICTS}

Write \`docs/linux-rewrite/wave-c/${slice}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema.
`
}

function verifyPrompt(slice, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}. Route it gave you: ${r.howToExercise}`).join('\n')
    : '- The builder returned nothing usable. Work from the slice brief alone, and grade what you find.'

  return `Verify slice **${slice}** of the Tiller Linux/GPUI rewrite.

**You did not build this slice and you have not seen the builder's reasoning.** This project's
standing rule is that the critic is never the agent that built the piece. Everything below that
comes from the builder is a *claim*, including the routes — useful for finding the control, worthless
as proof.

## The rows, and what the builder claims

${built}

Your inputs: \`docs/linux-rewrite/wave-c/${slice}.md\` (the rows and their prior evidence),
\`docs/linux-rewrite/wave-c/${slice}-report.md\` (the builder's own write-up — read it for routes,
not for conclusions), and \`docs/linux-rewrite/wave-c/INTEGRATION.md\` (the integrator's results).
${HOUSE}
The tree is built at HEAD. **You should not need to compile** — if cargo starts a long build,
something changed under you, and that is worth reporting. Drive tests **per-crate**, never
\`--workspace\`: two pre-existing timing-sensitive tests flake only under full-workspace concurrency.
${DRIVE}
${EVIDENCE}
## Verdicts — exactly one per row
${VERDICTS}

A row whose behaviour you could not reach at all is \`UNREACHABLE\`, not \`FAILED\` — those are
different findings and the ledger counts them separately. A row you ran out of time for is
\`NOT EXERCISED\`; say so plainly rather than inheriting the builder's word for it.

\`newEvidenceCell\` lands verbatim in the ledger's evidence column: condense your own finding, name
the instrument and the frame you read, and **strip every raw \`|\`** — one raw pipe shifts every
column after it and breaks the totals gate.

Write \`docs/linux-rewrite/wave-c/${slice}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema.
`
}

// ---------------------------------------------------------------- run
log(`wave C: 117 rows across ${ALL_SLICES.length} slices — 2 serialised chains (main.rs x4, chat.rs x3), 4 parallel slices, 1 investigate-only`)

const built = {}

async function runChain(chain, links) {
  const out = []
  for (const link of links) {
    const pos = `${links.indexOf(link) + 1} of ${links.length}`
    const r = await agent(
      buildPrompt(link, `
**You are link ${pos} of the \`${chain}\` chain.** The earlier links have already landed their
commits and the later links have not started, so nobody else is holding your files right now — but
that also means you must \`git pull\`-free re-read your files from disk before editing: a sibling
link changed them since the brief was written. Do not assume the file matches what the brief quotes.`),
      { label: `build:${link}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' },
    )
    if (r) { built[link] = r; out.push(r) }
    log(`${link} done — ${r ? (r.rows || []).length : 0} rows reported`)
  }
  return out
}

await parallel([
  ...CHAINS.map((c) => () => runChain(c.chain, c.links)),
  ...PARALLEL.map((s) => () =>
    agent(buildPrompt(s, `
This slice runs **concurrently** with the two chains and the other parallel slices. Every file your
rows touch is listed in your brief, and no other slice owns any of them — so long as you stay inside
that list, nobody can collide with you.`),
      { label: `build:${s}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' })
      .then((r) => { if (r) built[s] = r; return r })),
  () => agent(investigatePrompt(INVESTIGATE),
    { label: `investigate:${INVESTIGATE}`, phase: 'Build', schema: VERDICT_SCHEMA, effort: 'medium' })
    .then((r) => { built[INVESTIGATE] = r; return r }),
])

const wanted = Object.entries(built).flatMap(([s, r]) =>
  ((r && r.wantedForeignFiles) || []).map((f) => `${s} wanted ${f}`))
log(`build phase closed. ${Object.keys(built).length}/${ALL_SLICES.length} slices reported. foreign-file requests: ${wanted.length}`)

// ---- barrier is justified: integration needs every builder's commits present at once
const integration = await agent(`You are the integrator for wave C of the Tiller Linux/GPUI rewrite. Twelve agents just
finished building in **one shared worktree**, committing to \`linux/gpui-waku\` concurrently.
${HOUSE}
## What you must do, in this order — commit after each step

1. \`cd ${RUST} && cargo build -p tiller\` and get the workspace green. If a builder left it broken,
   fix it; prefer the smallest change that restores the build over reverting someone's work.
2. \`cargo test -p <crate>\` **per crate**, not \`--workspace\`. Two pre-existing tests are
   timing-sensitive and flake only under full-workspace concurrency — if you see a flake, re-run
   that crate alone before believing it.
3. **Sweep for cross-slice edits — this is the step that matters most.** For every file touched
   since \`f3b6169\`, run \`git log --format='%h %s' f3b6169..HEAD -- <file>\` and read any commit
   whose subject belongs to a *different* slice than the file's owner
   (\`docs/linux-rewrite/wave-c/manifest.json\` holds the ownership map). In wave B exactly this
   sweep found a commit that byte-for-byte reverted 167 lines of a landed, tested fix while its
   subject line talked about another crate. Do not shortcut it by matching commit scope to slice
   name — that produced 12 false positives out of 13 last time, because a \`fix(main)\` commit
   legitimately *is* the main slice's. Compare **file ownership**, not subject text.
   Where you find a genuine revert, restore the lost content and commit it with an explicit path
   and a subject that says what was restored.
4. Apply any fix listed below that a builder needed but correctly refused to make in a file it did
   not own. Each is described in that builder's report; apply the described change, do not invent one.

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (no builder requested a foreign file)'}

5. Rebuild, confirm green, and leave the binary on disk at \`${RUST}/target/debug/tiller\` — the
   critics that run next use the Wayland drive lane, which **does not build** and will exit 2 if the
   binary is missing or stale. This is your responsibility, not theirs.

Write \`docs/linux-rewrite/wave-c/INTEGRATION.md\` — build status, per-crate test results, every
cross-slice commit you examined and what you concluded about it, and any foreign-file fix you
applied. Commit it with an explicit path.

Return a short plain-text summary: build status, test status, how many cross-slice commits you
examined, how many were genuine reverts, and what you restored.`,
  { label: 'integrate', phase: 'Integrate' })

log(`integration: ${String(integration).slice(0, 400)}`)

// ---- verify: one fresh critic per code slice; C-U already produced its own verdicts
const CODE_SLICES = ALL_SLICES.filter((s) => s !== INVESTIGATE)
const verdicts = await parallel(CODE_SLICES.map((s) => () =>
  agent(verifyPrompt(s, built[s]), { label: `verify:${s}`, phase: 'Verify', schema: VERDICT_SCHEMA, effort: 'high' })))

const all = [...verdicts.filter(Boolean), built[INVESTIGATE]].filter(Boolean)
const rows = all.flatMap((r) => (r.rows || []).map((x) => ({ ...x, slice: r.slice })))
const tally = {}
for (const r of rows) tally[r.verdict] = (tally[r.verdict] || 0) + 1
log(`wave C verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  slices: all.map((r) => ({ slice: r.slice, count: (r.rows || []).length, framesRead: r.framesRead, notes: r.notes })),
  builders: Object.fromEntries(Object.entries(built).map(([s, r]) => [s, {
    rows: (r && r.rows || []).length, buildsClean: r && r.buildsClean, wanted: (r && r.wantedForeignFiles) || [],
  }])),
  integration: String(integration || ''),
  missingVerifiers: CODE_SLICES.filter((s, i) => !verdicts[i]),
}
