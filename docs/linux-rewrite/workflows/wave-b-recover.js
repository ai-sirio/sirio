export const meta = {
  name: 'wave-b-recover',
  description: 'Recover the six committed wave-B verdict sets into structured rows, and verify the one slice that never got a verifier',
  phases: [
    { title: 'Recover', detail: '6 agents, one per committed verdicts file — transcribe, never invent' },
    { title: 'Verify B7', detail: 'the slice whose verifier died before writing verdicts, exercised live' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'

const DONE = (args && args.done) || [
  { name: 'B1-tabbar-zorder', n: 8 },
  { name: 'B2-chat', n: 6 },
  { name: 'B3-sidebar', n: 5 },
  { name: 'B4-settings', n: 7 },
  { name: 'B5-acp', n: 3 },
  { name: 'B6-brw-bar', n: 5 },
]

const B7 = {
  name: 'B7-editor-persist',
  n: 8,
  ids: 'F-EDIT-02, F-CORE-FILE-04, F-PERSIST-DB-06, F-PERSIST-DB-11, F-PER-06, F-TERM-PTY-06, F-PRJ-17, F-PRJ-18',
}

const HOUSE = `
Worktree: ${ROOT}   Branch: linux/gpui-waku
Work ONLY inside that worktree. Never touch /home/enzopalmisano/Scrivania/Progetti/tiller —
that is the Swift original this project is a rewrite of.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator applies verdicts
as the single writer.

Commit with **explicit file paths only** — never \`git add <dir>\`, never \`git add -A\`,
never \`git commit -a\`. This is not style: during wave B two agents staged a file they did
not own and one of them byte-for-byte reverted a landed, tested fix inside a commit whose
subject was about a different crate. Nothing flagged it. On \`index.lock\` contention retry
in a loop, NEVER delete the lock.
`

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

function recoverPrompt(s) {
  return `A critic already verified slice **${s.name}** of the Tiller Linux/GPUI rewrite and wrote
its verdicts to \`docs/linux-rewrite/wave-b/${s.name}-verdicts.md\`, which is committed. The
process that would have collected those verdicts as structured data died before it could, so
the prose file is now the only record.

**Your job is transcription, not judgement.** Read that file and return each row's verdict
exactly as the critic recorded it.
${HOUSE}
## The rules that matter here

- **Do not invent a verdict.** Every row you return must be one the file actually states. If
  the file discusses a row but never lands on a verdict, return the verdict the ledger
  currently holds for it (\`docs/linux-rewrite/wave-b/${s.name}.md\` lists that) and say so in
  \`notes\`. Do not guess from the prose's tone.
- **Do not upgrade a verdict.** If the critic wrote \`half-proven\`, it is \`half-proven\`,
  even where the surrounding text sounds positive. You are not re-adjudicating and you did not
  see the frames.
- The slice covers ${s.n} rows. If the file records fewer, return only those and name the
  missing ones in \`notes\` — a short honest list beats a padded one.
- \`newEvidenceCell\` is the text that will land in the ledger's evidence column: condense the
  critic's own finding, keep it factual, and **strip any raw \`|\`** — a raw pipe shifts every
  column after it and breaks the totals gate.
- \`evidenceDiscriminates\` should reflect what the critic said about its evidence. If the
  critic flagged its evidence as weak, non-discriminating, or unchanged-from-before, that is
  \`false\`.

Return the schema. You do not need to write or commit any file.`
}

function verifyB7Prompt() {
  return `Verify slice **${B7.name}** of the Tiller Linux/GPUI rewrite — ${B7.n} rows:
${B7.ids}.

**You did not build this slice and you have not seen the builder's reasoning.** The project's
rule is that the critic is never the agent that built the piece. A previous verifier for this
slice died mid-run; its partial captures may still sit untracked in
\`reference/linux-progress/verify-B7-editor-persist/\`. **Do not trust them** — you did not
take them and no report explains them. Take your own.
${HOUSE}
## Your inputs

- \`docs/linux-rewrite/wave-b/${B7.name}.md\` — the rows, and what triage expected.
- \`docs/linux-rewrite/wave-b/${B7.name}-report.md\` — the builder's own report, including a
  \`howToExercise\` gesture per row. Read it for the routes, not for the conclusions.
- \`docs/linux-rewrite/wave-b/INTEGRATION.md\` — the integrator's results. It reports
  \`cargo build -p tiller\` green, and warns that two pre-existing timing-sensitive tests flake
  only under full-workspace concurrency; drive tests **per-crate**, not \`--workspace\`.

The tree is freshly built at HEAD: \`cargo build -p tiller\` and
\`cargo test --workspace --no-run\` both finished green just now, so **you may run tests but
should not need to compile**. If cargo starts compiling, something changed under you — say so.

## How to verify

**Exercise it live.** A feature nobody has successfully tried does not exist, and a green test
is never a pass — 134 tests once passed over a chat transcript that drew nothing at all.

\`TILLER_WL_LABEL=verifyB7 Scripts/wayland-drive.sh reference/linux-progress/verify-B7-final '<actions>' [settle]\`

That lane takes no lock. Actions: \`ctl <method> [k=v]\`, \`click x y\`, \`move x y\`,
\`type <text>\`, \`key <name>\`, \`title <text>\`, \`shot <name>\`. **You can see images — open
the PNGs with \`Read\` and look.** Read \`docs/linux-rewrite/WAYLAND-LANE.md\` first, in
particular:

- \`title <text>\` sets a pane's OSC title; never write the escape sequence inline, because
  its bare \`;\` splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state** —
  \`agent\` reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven
  with a positive control. A null result from them is not evidence of anything.

Several of your rows are persistence and process-lifecycle rows rather than pixels
(\`F-PERSIST-DB-06/11\`, \`F-PER-06\`, \`F-TERM-PTY-06\`). For those the honest instrument is
usually a real restart or a real process tree, not a screenshot: kill and relaunch on the same
DB and read the value back; or spawn a job-control child and confirm the descendant group
actually dies. Say which instrument you used.

**Discriminating evidence:** when a row's correct value and its default are the same value,
capturing that value proves nothing. Drive to a state the system would never reach on its own
and capture a marker only your drive could have produced.

## Verdicts — exactly one per row

\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires the behaviour exercised live and observed to work. The builder saying it
  built it is not evidence; neither is a green test.
- Evidence that does not discriminate cannot support \`PASSED\`.
- **Be willing to fail your builder.** A slice that comes back all-PASSED is far likelier to be
  a lazy critic than a healthy codebase.

One thing worth knowing about this slice specifically: its builder reported that a sibling
agent's commit silently reverted its \`tiller_terminal/src/lib.rs\` fix, and that it
re-applied and recommitted it (\`915b0d0\`). **Confirm the fix is actually present at HEAD
before you judge \`F-PER-06\`** — do not assume either that it is there or that it is not.

Write \`docs/linux-rewrite/wave-b/${B7.name}-verdicts.md\`, commit it with an explicit path,
and return the schema.`
}

log(`recovering ${DONE.reduce((a, s) => a + s.n, 0)} verdicts from 6 committed critic files, and verifying B7's ${B7.n} rows live`)

const results = await parallel([
  ...DONE.map((s) => () =>
    agent(recoverPrompt(s), { label: `recover:${s.name}`, phase: 'Recover', schema: VERDICT_SCHEMA, effort: 'medium' })),
  () => agent(verifyB7Prompt(), { label: `verify:${B7.name}`, phase: 'Verify B7', schema: VERDICT_SCHEMA }),
])

const ok = results.filter(Boolean)
const rows = ok.flatMap((r) => (r.rows || []).map((x) => ({ ...x, slice: r.slice })))
const tally = {}
for (const r of rows) tally[r.verdict] = (tally[r.verdict] || 0) + 1
log(`${rows.length} rows recovered/verified: ${JSON.stringify(tally)}`)

return {
  tally,
  rows,
  slices: ok.map((r) => ({ slice: r.slice, count: (r.rows || []).length, framesRead: r.framesRead, notes: r.notes })),
  missing: results.map((r, i) => (r ? null : i)).filter((x) => x !== null),
}
