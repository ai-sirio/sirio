export const meta = {
  name: 'wave-f',
  description: 'Independently re-verify the 15 PASSED rows whose provenance is not structurally independent (P125)',
  phases: [
    { title: 'Re-verify', detail: '4 fresh critics, verification only, each willing to downgrade' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const SLICES = ['F1-editor', 'F2-project', 'F3-browser', 'F4-terminal']

const HOUSE = `
Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Work ONLY there. Never touch \`/home/enzopalmisano/Scrivania/Progetti/tiller\` — the Swift original.

**Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\`.** The orchestrator is its single writer.
**Do not edit any Rust.** This wave changes no code; you are here to judge, not to fix. If you find a
defect, grade it and describe it — someone else will fix it.

Commit with **explicit file paths only** — never \`git add <dir>\`, \`git add -A\`, \`git commit -a\`.
Other agents are committing to this same worktree while you work. On \`index.lock\` contention retry in
a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after EVERY row.** You are killed after 180 s without output, and retried from scratch. Your
committed markdown file is the real record — if your final structured answer fails to validate, that
file is what survives. Never leave it until the end. Keep the returned object small: one or two
sentences per field.
`

const DRIVE = `
## Exercising the app

\`TILLER_WL_LABEL=<uniqueLabel> Scripts/wayland-drive.sh <outdir> '<actions>' [settle]\` from \`${ROOT}\`.
No lock — parallel-safe provided \`TILLER_WL_LABEL\` is unique (it names every \`/tmp\` path the instance
uses). Other agents are driving right now, so pick a label nobody else would.
**You can see images — open the PNGs with \`Read\` and look at them.**

Read \`docs/linux-rewrite/WAYLAND-LANE.md\`. Vocabulary:
\`ctl · click · move · type · key · title · shot · rightclick · chord <mod> <key> · down · up · drag ·
scroll <x> <y> <steps> · modclick <mod> <x> <y>\`.
\`rightclick\`, \`chord\`, \`drag\`, \`scroll\` each shipped with a positive-control frame. \`modclick\`
is implemented but was **never proven end-to-end** — a null result from it is inconclusive, not evidence.

Standing traps:
- \`title <text>\` sets a pane's OSC title. Never write the escape sequence inline — its bare \`;\`
  splits the eval'd action block in half.
- **\`panel.list\`'s \`agent\` and \`title\` fields are dead instruments for activity state** — \`agent\`
  reads \`""\` even after a \`notify\` that returned \`{"queued":"true"}\`, proven with a positive control.
  Valid \`notify\` statuses: \`running\`, \`needs-input\`, \`done\`, \`error\`.
- **The Clone/Create popover draws in the wrong place** — \`docs/linux-rewrite/tasks/P123-popover-click-routing.md\`.
  Its overlay is positioned relative to the Sidebar's ~280px root div instead of the window root, so it is
  confined to the sidebar column even though its dark-scrim styling implies it is window-centred. Clicks
  aimed where it *looks* like it should be will miss. Aim where it actually draws. (This matters directly
  for \`F-PRJ-05\` and \`F-PRJ-08\`.)

The lane **does not build** (\`[ -x "$BIN" ] || exit 2\`) — that is what makes it parallel-safe. The tree
is built at HEAD; you should not need to compile. Run tests **per-crate**, never \`--workspace\`.
Frames under \`reference/linux-progress/sweep-D*/\` are orphans; **no verdict may cite them.**
`

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
          heldUp: { type: 'boolean', description: 'true if the prior PASSED survived your own live re-exercise' },
          newEvidenceCell: { type: 'string', description: 'ledger evidence, 1-3 sentences, no raw | characters' },
        },
      },
    },
  },
}

function prompt(slice) {
  return `You are re-judging slice **${slice}** of the Tiller Linux/GPUI rewrite — a native Rust + GPUI
port of a macOS SwiftUI app, tracked against a frozen 389-row functional contract.

**Read \`docs/linux-rewrite/wave-f/${slice}.md\` first**, then
\`docs/linux-rewrite/tasks/P125-rows-without-independent-provenance.md\` for why these rows were pulled.

## Why you exist

Every row in your slice is currently marked **PASSED** and is counted in the ledger's headline total of
315. But each one's \`judged\` stamp names **the orchestrator itself**, or a critic who read *another
agent's report* rather than exercising the behaviour. The project's standing rule is that a feature
nobody has independently tried does not exist — so these rows' PASSED status is, right now, unearned
bookkeeping rather than a demonstrated fact.

**Your job is to find out whether they are actually true.** You wrote none of this code and you did not
produce the evidence on record, so you are a valid critic by construction.
${HOUSE}
## How to judge

Re-exercise each row **from scratch, live**. The evidence already on record is a *claim*: read it to
learn where the control is, never as proof that it works. Several of these rows were recorded a full
day and many commits ago, against a binary that no longer exists — a behaviour that worked then may
have regressed since, and nothing in the ledger would show it.

**Discriminating evidence.** If a row's correct value and its default are the same value, capturing that
value proves nothing. Drive the system to a state it would never reach on its own and capture a marker
only your drive could have produced. If you cannot tell your PASS frame from a frame of a broken build,
say so and grade \`half-proven\`.

For rows that are not pixels — process lifecycle, persistence, clipboard, debounce timing — the honest
instrument is a real restart, a real process tree, or a real read-back, not a screenshot. Name the
instrument you used.
${DRIVE}
## Verdicts — exactly one per row

\`PASSED\` · \`half-proven\` · \`FAILED — absent\` · \`FAILED — defective\` · \`UNREACHABLE\` ·
\`N/A — platform\` · \`NOT EXERCISED\`

- \`PASSED\` requires **your own** live exercise, this pass. Confirming that the previous evidence text
  reads convincingly is not a re-verification, and re-running a unit test is not either — 134 tests once
  passed over a chat transcript that drew nothing at all.
- Set \`heldUp: true\` only when the prior PASSED survived your own live re-exercise.
- **A slice that comes back all-PASSED with no new frames is a failed pass, not a healthy one.** The
  expected outcome of an audit like this is that some rows do not survive. Two of the previous waves'
  most valuable findings were a \`FAILED — defective\` on work everyone believed was done, and a
  retracted claim. Downgrading a row here is the *success* case, not a failure to close it.
- If a row genuinely still holds, say so plainly and cite the frames you took — that is a real result too.

\`newEvidenceCell\` lands verbatim in the ledger: your own finding, the instrument, the frame you read,
1-3 sentences, and **strip every raw \`|\`** — one raw pipe shifts every column after it and breaks the
totals gate.

Write \`docs/linux-rewrite/wave-f/${slice}-verdicts.md\`, committing after **each row** with an explicit
path, then return the schema.`
}

log('wave F: re-judging 15 PASSED rows whose provenance is not structurally independent (P125)')

const results = await parallel(SLICES.map((s) => () =>
  agent(prompt(s), { label: `reverify:${s}`, phase: 'Re-verify', schema: VERDICT_SCHEMA, effort: 'high' })))

const ok = results.filter(Boolean)
const rows = ok.flatMap((r) => (r.rows || []).map((x) => ({ ...x, slice: r.slice })))
const tally = {}
for (const r of rows) tally[r.verdict] = (tally[r.verdict] || 0) + 1
const held = rows.filter((r) => r.heldUp).length
log(`wave F: ${rows.length} rows re-judged — ${JSON.stringify(tally)}; ${held} held up under independent re-exercise`)

return {
  tally,
  held,
  downgraded: rows.filter((r) => r.verdict !== 'PASSED').map((r) => ({ id: r.id, verdict: r.verdict })),
  rows,
  missing: SLICES.filter((s, i) => !results[i]),
}
