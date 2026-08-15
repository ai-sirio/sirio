export const meta = {
  name: 'wave-d',
  description: 'Close the remaining 74 inventory rows: extend the drive harness, build the gaps, verify independently',
  phases: [
    { title: 'Harness', detail: 'add the missing drive primitives that block 19 rows, and diagnose the popover click gap' },
    { title: 'Build', detail: 'D-MAIN chain x8 (51 rows) alongside 3 file-disjoint parallel slices + 1 investigate-only' },
    { title: 'Integrate', detail: 'compile, per-crate tests, cross-slice revert sweep' },
    { title: 'Verify', detail: 'one fresh critic per slice, exercising live with the extended harness' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const MAIN_LINKS = ['D-MAIN-1', 'D-MAIN-2', 'D-MAIN-3', 'D-MAIN-4', 'D-MAIN-5', 'D-MAIN-6', 'D-MAIN-7', 'D-MAIN-8']
const PARALLEL = ['D-P1', 'D-P2', 'D-P3']
const INVESTIGATE = 'D-U'
const CODE_SLICES = [...MAIN_LINKS, ...PARALLEL]

const HOUSE = `
## House rules

Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY there. Never touch \`/home/enzopalmisano/Scrivania/Progetti/tiller\` — the Swift original.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator is its single writer.

**Commit with explicit file paths only** — never \`git add <dir>\`, \`git add -A\`, or \`git commit -a\`.
In wave B an agent staged a file it did not own and byte-for-byte reverted a landed 167-line fix
inside a commit whose subject was about another crate; nothing flagged it. Before declaring a row
done, \`grep\` your own file for the symbol you added and confirm it is still there. Then run
\`git status --porcelain\` — explicit paths also silently drop new files nobody is thinking about.

On \`index.lock\` contention retry in a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row, not at the end.** You are killed after 180 s without output. In wave C an
agent did 15 rows of real work and then failed on its final report — the work survived only because
it had committed each row as it went. An honest partial result that landed beats a perfect one that
never did.

**Keep your final structured answer small.** Wave C lost two agents to the structured-output retry
cap by returning huge objects. Put the detail in your committed markdown report and keep the returned
fields short — one or two sentences each. The report file is the record; the schema is just an index.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\`
from \`${ROOT}\`. No lock — safe in parallel, provided your \`TILLER_WL_LABEL\` is unique (it names
every \`/tmp\` path the instance uses). **You can see images — open the PNGs with \`Read\` and look.**

Read \`docs/linux-rewrite/WAYLAND-LANE.md\` first. Two standing traps:

- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state.** \`agent\`
  reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven with a positive
  control. A null result from them proves nothing. Valid \`notify\` statuses: \`running\`,
  \`needs-input\`, \`done\`, \`error\` — \`working\` is rejected.

**The lane does not build** (\`[ -x "$BIN" ] || exit 2\`); that is what makes it parallel-safe. If it
exits 2 the binary is missing — say so rather than silently building.

The frames under \`reference/linux-progress/sweep-D*/\` are orphans; **no verdict may cite them.**
`

const EVIDENCE = `
## What counts as evidence

**A feature nobody has successfully tried does not exist.** A green test is never a pass — 134 tests
once passed over a chat transcript that drew nothing at all.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing
it proves nothing. Drive the system to a state it would never reach on its own and capture a marker
only your drive could have produced. If you cannot tell your PASS frame from a frame of the broken
build, say so and grade \`half-proven\`.

For rows that are not pixels — persistence, process lifecycle, config — the honest instrument is a
real restart or a real process tree, not a screenshot. Always name the instrument you used.
`

const VERDICTS = `\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires the behaviour exercised live and observed to work. A builder's word is not
  evidence; neither is a green test.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** An all-PASSED slice is far likelier to be a lazy critic than a
  healthy codebase.`

// Deliberately permissive: wave C lost two agents to schema-validation retry exhaustion.
const BUILD_SCHEMA = {
  type: 'object',
  required: ['slice', 'rows'],
  properties: {
    slice: { type: 'string' },
    reportPath: { type: 'string' },
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
          howToExercise: { type: 'string', description: 'one or two sentences — the gesture a critic should drive' },
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
    verdictPath: { type: 'string' },
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
          newEvidenceCell: { type: 'string', description: 'ledger evidence text, 1-3 sentences, no raw | characters' },
        },
      },
    },
  },
}

// ---------------------------------------------------------------- phase 1: harness
const harnessPrompt = `The Tiller Linux/GPUI rewrite proves its features by driving the real app over a Wayland
lane. That lane's action vocabulary has become the ceiling on what can be *proven*, independent of
what has been *built*: **19 of the 74 remaining inventory rows are blocked by a missing drive
primitive, not by missing app code.** Your job is to raise that ceiling.

You own exactly these files:
- \`Scripts/wayland-drive.sh\`
- \`Scripts/wayland-virtual-pointer.c\`
- \`docs/linux-rewrite/WAYLAND-LANE.md\`

No other agent touches them. **Do not edit any Rust.**
${HOUSE}
## What is missing, and which rows it blocks

Read \`docs/linux-rewrite/WAYLAND-LANE.md\` and \`Scripts/wayland-drive.sh\` first — the existing
vocabulary is \`ctl / click / move / type / key / title / shot\`, and \`title\` was added recently, so
its implementation is the worked example of how to add an action.

1. **Right-click** (8 rows, e.g. \`F-TERM-06\` needs the terminal context menu; \`F-TAB-11\`,
   \`F-TAB-23\`). Nothing can open a context menu today.
2. **Modifier chords** — modifier+click and modifier+key (\`F-CORE-TERM-02\` needs Shift+F10;
   \`F-TERM-UI-02\` needs platform-modifier+click on a terminal link). \`key\` wraps \`wtype -k\` for a
   single named key with no composition; \`click\` has no modifier parameter.
3. **Button-held drag** (4 rows: \`F-EDIT-12\`, \`F-CHG-18\`, \`F-CORE-FILE-03\`). There is no way to
   press, move, and release — not even to compose one by hand from \`move\`.
4. **Scroll / axis** (\`F-CHAT-20\`).

Add each as a first-class action with the same shape as the others: usage check, \`verify_nested_sway\`,
a clear \`FAIL:\` message when its tool is absent, and a guard entry in the keyboard/pointer
start-up condition if it needs one (note the existing
\`grep -Eq '(^|[;[:space:]])(type|key|title)([;[:space:]]|$)'\` pattern — extend it, do not replace it).

**Prove each new primitive with a positive control before you claim it works.** A right-click action
that silently does nothing looks exactly like an app with no context menu — and this project has
already nearly filed a fabricated defect that way. For each new action, drive it against a target
known to respond and capture the frame that shows it responding. An action you cannot demonstrate is
worse than no action, because the next critic will trust it.

## The one that may not be a harness problem at all

Three rows (\`F-PRJ-06\`, \`F-PRJ-07\`, \`F-PRJ-09\`) are blocked on what has been recorded as an
"anchored-popover input-delivery gap". A wave-C critic narrowed it materially: in the Clone-repository
popover, **text entry does land** when the field is clicked first, but **button clicks do not** —
Cancel left the form open across two drives despite a visible hover highlight, and the submit button
never registered.

Hover highlight rendering while the click does not register is not obviously a lane limitation. It is
equally consistent with a real GPUI hit-testing or event-routing defect in the overlay — which would
be a genuine user-facing bug, not a test-harness gap. **Determine which it is**, and say so plainly.
Useful discriminators: does a click land on a button in a *non*-popover surface in the same drive
(control)? does the app log the mouse-down at all? does the same click work on the \`DISPLAY=:1\`
X11 lane (\`Scripts/linux-drive.sh\`, which takes a global lock — use it briefly and release it)?
If it is an app defect, **do not fix it** (you own no Rust) — file
\`docs/linux-rewrite/tasks/P123-popover-click-routing.md\` with your evidence, and say so in \`notes\`.

## Deliverable

Update \`WAYLAND-LANE.md\` to document every new action with its exact syntax and its positive-control
frame, and commit after **each** primitive so a kill costs one action, not all of them. Return a short
summary: which primitives now work, which you could not make work and why, and your verdict on the
popover question.`

// ---------------------------------------------------------------- prompts
function buildPrompt(slice, extra) {
  return `You are building slice **${slice}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI port
of a macOS SwiftUI app. Your rows are inventory entries a fresh critic judged **yesterday** as
missing, defective, or only half-proven.

**Read \`docs/linux-rewrite/wave-d/${slice}.md\` first.** Each row carries that critic's evidence,
written on 2026-08-15 against the current tree. Read it as a bug report — it tells you exactly what a
skeptical observer could not see, which is usually a precise description of the gap.
${extra}
${HOUSE}
## Your job, per row

1. Find the real gap. \`half-proven\` usually means half the behaviour exists; \`FAILED — absent\`
   means it does not exist at all; \`FAILED — defective\` means it exists and misbehaves.
2. Implement or fix it **in the files you own**. If the fix truly needs a file you do not own, do
   **not** edit it — list the path in \`wantedForeignFiles\` and describe the precise change in your
   committed report so the integrator can apply it. This convention saved a landed fix in wave B and
   worked again in wave C.
3. \`cd ${RUST} && cargo build -p tiller\` (or \`-p <yourcrate>\`) green.
4. Commit that row alone, explicit paths, conventional-commit subject naming the row id.
5. Append to \`docs/linux-rewrite/wave-d/${slice}-report.md\` and commit it too.

You are **not** the judge of your own work — a separate critic who has never seen your reasoning will
exercise every row live. Do not write verdicts. Report \`blocked\` honestly where you are stuck; an
optimistic \`fixed\` wastes a critic's whole pass.

\`howToExercise\` is the only thing your critic gets from you, and a critic who cannot find the route
grades the row \`NOT EXERCISED\`. Name the control, the coordinates or \`ctl\` call, and what should
visibly change. Keep it to one or two sentences in the schema; put the long version in your report.
${DRIVE}
A separate agent is extending the lane with right-click, modifier chords, drag and scroll while you
work. If a row needs one of those to demonstrate, build the feature anyway and say so — the critic
will have the primitive by the time it runs.`
}

function verifyPrompt(slice, builder) {
  const built = builder && builder.rows && builder.rows.length
    ? builder.rows.map((r) => `- \`${r.id}\` — builder says *${r.action}*${r.commit ? ` (${r.commit})` : ''}${r.howToExercise ? `. Route: ${r.howToExercise}` : ''}`).join('\n')
    : '- **The builder for this slice returned nothing.** It may still have committed real work — check\n  `git log` and the slice report before assuming the rows are untouched. Grade what you actually find.'

  return `Verify slice **${slice}** of the Tiller Linux/GPUI rewrite.

**You did not build this slice and you have not seen the builder's reasoning.** The standing rule is
that the critic is never the agent that built the piece. Everything below from the builder is a
*claim* — useful for finding the control, worthless as proof.

## The rows, and what the builder claims

${built}

Inputs: \`docs/linux-rewrite/wave-d/${slice}.md\` (rows + the previous critic's evidence),
\`docs/linux-rewrite/wave-d/${slice}-report.md\` (the builder's write-up — routes, not conclusions),
\`docs/linux-rewrite/wave-d/INTEGRATION.md\` (the integrator's results).
${HOUSE}
The tree is built at HEAD; you should not need to compile. If cargo starts a long build, something
changed under you — report it. Run tests **per-crate**, never \`--workspace\`: two pre-existing
timing-sensitive tests flake only under full-workspace concurrency.
${DRIVE}
**The lane was extended today.** Re-read \`docs/linux-rewrite/WAYLAND-LANE.md\` — right-click,
modifier chords, button-held drag and scroll may now exist as first-class actions. Several rows in
this wave were previously graded \`UNREACHABLE\` *only* because those were missing, so a row whose
ledger evidence says "no drag primitive exists" may well be driveable now. Check the current
vocabulary rather than trusting the recorded excuse.
${EVIDENCE}
## Verdicts — exactly one per row

${VERDICTS}

A behaviour you could not reach at all is \`UNREACHABLE\`, not \`FAILED\` — the ledger counts those
separately. A row you ran out of time for is \`NOT EXERCISED\`; say so rather than inheriting the
builder's word.

\`newEvidenceCell\` lands verbatim in the ledger: condense your own finding, name the instrument and
the frame, keep it to 1-3 sentences, and **strip every raw \`|\`** — one raw pipe shifts every column
after it and breaks the totals gate.

Write \`docs/linux-rewrite/wave-d/${slice}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema. **That file is the real record** — if your final structured
answer fails to validate, the committed file is what survives, so never leave it until the end.`
}

const investigatePrompt = `You are the critic for slice **${INVESTIGATE}** of the Tiller Linux/GPUI rewrite — 5 inventory
rows that no triage pass has managed to map to a source file.

**You are not building anything. Do not edit a single line of Rust.** You wrote none of this code, so
you are a valid critic by construction. Read \`docs/linux-rewrite/wave-d/${INVESTIGATE}.md\`.
${HOUSE}
${DRIVE}
${EVIDENCE}
For each row: locate the behaviour (\`rg\` is available), exercise it live, and grade it. Where a row
needs code that does not exist, grade \`FAILED — absent\` and name **which file a fix would touch** —
that mapping is the durable thing this slice produces and the next wave depends on it.

Verdicts — exactly one per row:
${VERDICTS}

Write \`docs/linux-rewrite/wave-d/${INVESTIGATE}-verdicts.md\`, committing after **each row** with an
explicit path, then return the schema.`

// ---------------------------------------------------------------- run
log(`wave D: 74 rows — harness track (unblocks 19) runs alongside a D-MAIN chain of ${MAIN_LINKS.length} links, ${PARALLEL.length} parallel slices, 1 investigate-only`)

const built = {}

// The wave-C cascade bug: one link failing killed the 45 rows in the links behind it.
// Each link is isolated so a failure costs one link, never the tail of the chain.
async function runChain(links) {
  const done = []
  for (let i = 0; i < links.length; i++) {
    const link = links[i]
    try {
      const r = await agent(
        buildPrompt(link, `
**You are link ${i + 1} of ${links.length} in the \`D-MAIN\` chain.** Earlier links have already
landed their commits; later links have not started. Nobody else holds \`main.rs\` right now — but
**re-read it from disk before every edit**, because an earlier link has changed it since your brief
was written. Do not trust the line numbers the brief quotes.`),
        { label: `build:${link}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' },
      )
      if (r) { built[link] = r; done.push(r) }
      log(`${link} done — ${r ? (r.rows || []).length : 0} rows`)
    } catch (e) {
      log(`${link} FAILED (${String(e).slice(0, 120)}) — chain continues; its work may still be committed`)
    }
  }
  return done
}

const [harness] = await parallel([
  () => agent(harnessPrompt, { label: 'harness:wayland-lane', phase: 'Harness', effort: 'high' }),
  () => runChain(MAIN_LINKS),
  ...PARALLEL.map((s) => () =>
    agent(buildPrompt(s, `
This slice runs **concurrently** with the D-MAIN chain and the other parallel slices. Every file your
rows touch is listed in your brief and no other slice owns any of them — stay inside that list and
nobody can collide with you.`),
      { label: `build:${s}`, phase: 'Build', schema: BUILD_SCHEMA, effort: 'medium' })
      .then((r) => { if (r) built[s] = r; return r })),
  () => agent(investigatePrompt, { label: `investigate:${INVESTIGATE}`, phase: 'Build', schema: VERDICT_SCHEMA, effort: 'medium' })
    .then((r) => { built[INVESTIGATE] = r; return r }),
])

log(`harness track: ${String(harness || 'no result').slice(0, 500)}`)

const wanted = Object.entries(built).flatMap(([s, r]) =>
  ((r && r.wantedForeignFiles) || []).map((f) => `${s} wanted ${f}`))
log(`build closed — ${Object.keys(built).length} slices reported, ${wanted.length} foreign-file requests`)

const integration = await agent(`You are the integrator for wave D of the Tiller Linux/GPUI rewrite. A dozen agents just
committed concurrently to \`linux/gpui-waku\` in **one shared worktree**. Base for this wave: \`5626555\`.
${HOUSE}
Do these in order, committing after each:

1. \`cd ${RUST} && cargo build -p tiller\` green. If a builder left it broken, fix it — prefer the
   smallest change that restores the build over reverting anyone's work.
2. \`cargo test -p <crate>\` **per crate**, never \`--workspace\` (two pre-existing tests flake only
   under full-workspace concurrency; re-run a crate alone before believing a failure).
3. **Cross-slice sweep — the step that matters most.** For every file touched since \`5626555\`, run
   \`git log --format='%h %s' 5626555..HEAD -- <file>\` and check each commit against
   \`docs/linux-rewrite/wave-d/manifest.json\`'s **file** ownership map.
   Compare **file ownership, not commit-subject text** — matching conventional-commit scope to slice
   name produced 12 false positives out of 13 in wave B, because a \`fix(main)\` commit legitimately
   *is* the main slice's. Then diff each commit in isolation and read its **deleted** lines against
   its own commit message, looking for the wave-B pattern: a deletion that quietly removes a prior,
   unrelated fix. Restore anything genuinely lost, with an explicit path and a subject saying so.
   One caveat learned in wave C: **do not assume a commit came from the slice that owns the file.**
   Wave C's integrator attributed four \`main.rs\` commits to agents that had never run. If you
   cannot attribute a commit, say so rather than inferring.
4. Apply any fix a builder needed but correctly refused to make in a file it did not own — each is
   described in that builder's report; apply what is described, do not invent one:

${wanted.length ? wanted.map((w) => `   - ${w}`).join('\n') : '   (none requested)'}

5. Rebuild and leave a current binary at \`${RUST}/target/debug/tiller\`. The critics that run next
   use the Wayland lane, which **does not build** and exits 2 on a missing binary. That is your
   responsibility, not theirs.

Write \`docs/linux-rewrite/wave-d/INTEGRATION.md\` (build status, per-crate tests, every cross-slice
commit examined and what you concluded, any foreign-file fix applied) and commit it.

Return a short plain-text summary: build, tests, commits examined, genuine reverts found, what you
restored, and any commit you could not attribute.`,
  { label: 'integrate', phase: 'Integrate' })

log(`integration: ${String(integration).slice(0, 400)}`)

const verdicts = await parallel(CODE_SLICES.map((s) => () =>
  agent(verifyPrompt(s, built[s]), { label: `verify:${s}`, phase: 'Verify', schema: VERDICT_SCHEMA, effort: 'high' })))

const all = [...verdicts.filter(Boolean), built[INVESTIGATE]].filter(Boolean)
const rows = all.flatMap((r) => (r.rows || []).map((x) => ({ ...x, slice: r.slice })))
const tally = {}
for (const r of rows) tally[r.verdict] = (tally[r.verdict] || 0) + 1
log(`wave D verdicts: ${rows.length} rows — ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  harness: String(harness || ''),
  integration: String(integration || ''),
  slices: all.map((r) => ({ slice: r.slice, count: (r.rows || []).length, notes: r.notes })),
  buildersMissing: CODE_SLICES.filter((s) => !built[s]),
  verifiersMissing: CODE_SLICES.filter((s, i) => !verdicts[i]),
}
