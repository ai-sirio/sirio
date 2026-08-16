export const meta = {
  name: 'wave-j',
  description: 'Make the tree compile for macOS and Windows, and gate it so the debt cannot return',
  phases: [
    { title: 'Gate', detail: 'target-gate the Linux-only deps and glue' },
    { title: 'Windows', detail: 'cargo check --target x86_64-pc-windows-msvc to green' },
    { title: 'macOS', detail: 'cargo check --target aarch64-apple-darwin to green' },
    { title: 'CI', detail: 'add both targets to Scripts/ci-linux.sh' },
    { title: 'Verify', detail: 'independent critic, with a negative control on the gate' },
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
\`git commit -- <path>\` does **not** stage a file git has never seen: for a NEW file \`git add <exact/path>\`
first. After each step run \`git status --porcelain\` to catch new files nobody staged.

On \`index.lock\` contention retry in a loop, **never delete the lock**:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`

**Commit after every meaningful step.** You are killed after 180 s of model silence, so do not sit
inside one 20-minute command with nothing to say. Keep returned answers small.
`

const CONTEXT = `
## Background

Read \`docs/linux-rewrite/PORTABILITY.md\` first — it is the assessment this wave implements, and it
already did the reconnaissance so you do not have to repeat it.

The short version: **the foundations are portable.** GPUI has macOS and Windows backends in the pinned
rev \`c05e346\` (the Windows implementation lives inside \`gpui_platform\` — there is deliberately no
\`gpui_windows\` directory, do not re-derive that as "no Windows support"). \`alacritty_terminal\` ships
both \`tty/unix.rs\` and \`tty/windows/\` (ConPTY). \`wry\` covers WKWebView and WebView2. The clipboard
goes through GPUI. What is Linux-only is our own thin glue plus one careless manifest.

**The pattern to follow already exists in this repo**: \`crates/tiller_ui/Cargo.toml\` correctly gates
\`gtk\`/\`wry\`/\`raw-window-handle\` under \`[target.'cfg(target_os = "linux")'.dependencies]\` and has a
separate \`cfg(target_os = "macos")\` section; \`crates/tiller_theme/Cargo.toml\` does the same. Copy
that shape rather than inventing one.

## Scope discipline — read this twice

The goal of this wave is that the tree **compiles** for macOS and Windows, and that a gate keeps it
that way. It is **not** to implement the macOS and Windows features.

There is no macOS or Windows machine here, and no cross-linker. \`cargo check\` does not link, so it is
achievable and is a real gate; running the app is not achievable at all. This project's standard is
*a feature the critic hasn't successfully tried does not exist* — 350 rows were graded by driving the
real app and photographing it, and **no such claim can be made for macOS or Windows from this box.**

So: where a Linux feature has no portable implementation yet, put it behind a \`cfg\` seam with an
**honest** non-Linux branch. Honest means it compiles, and it either does nothing while saying so
(a log line, a returned \`Err\`, a \`None\`) or is plainly named unimplemented. A silent no-op that
looks like a working feature is the one outcome worse than a compile error, because it converts a
loud problem into a quiet lie. Do not write a macOS tray or a Windows tray on speculation.
`

const TIMING = `
## A practical warning about \`cargo check --target\`

A foreign-target check builds all ~408 dependency crates for that target from scratch. The first run
is slow — expect tens of minutes, and it can exceed a single command timeout.

Do not sit in one giant silent command. Work **bottom-up through the dependency graph**, committing
and reporting as you go:

\`\`\`
cd ${RUST}
cargo check --target <triple> -p tiller_persistence   # leaf-ish, warms the shared dep graph
cargo check --target <triple> -p tiller_project
cargo check --target <triple> -p tiller_agents
cargo check --target <triple> -p tiller_terminal
cargo check --target <triple> -p tiller_ui
cargo check --target <triple> -p tiller               # the binary, last and hardest
\`\`\`

Each returns sooner, each gives you a bounded error list, and the shared dependencies are only built
once. \`x86_64-pc-windows-msvc\` and \`aarch64-apple-darwin\` \`rust-std\` are already installed. There is
~360 GB free, so disk is not a constraint; the two extra target triples are expected to cost ~20 GB
each.
`

const BUILD_SCHEMA = {
  type: 'object',
  required: ['slice', 'outcome'],
  properties: {
    slice: { type: 'string' },
    outcome: { type: 'string', enum: ['green', 'partial', 'blocked'] },
    commits: { type: 'array', items: { type: 'string' } },
    remainingErrors: { type: 'number', description: 'count still failing, 0 if green' },
    seamsAdded: { type: 'array', items: { type: 'string' }, description: 'cfg seams introduced, one line each' },
    notes: { type: 'string', description: 'two or three sentences' },
  },
}

const SLICES = [
  {
    key: 'J0-chat05',
    phase: 'Gate',
    brief: `**One row, carried over from wave I: \`F-CHAT-05\`, currently \`FAILED — defective\`.** Unrelated to
portability — it is here because this wave is serial and the fix is small.

The contract says: when the agent is offline, *confirm the editor is disabled*. The Rust rewrite keeps
the composer **enabled** with a distinct placeholder ("Agent offline — reconnecting when you send…",
\`crates/tiller_ui/src/chat.rs\`), and a previous builder argued that was a deliberate, better UX than
the contract's wording, proposing the row be reworded.

**A wave-I critic checked that argument against the Swift original and it does not hold.**
\`ChatComposerView.swift\`'s \`canInteract\` excludes exactly this failed-connection state
(\`.disconnected\`) via \`ChatController.ChatState\` — so the reference genuinely **does** disable the
composer here, and has no retry-and-resend feature at all. The reference matches the literal contract;
the rewrite diverges from it. Read those two Swift files yourself before you start
(\`${SWIFT}/App/...\` — find them with grep; do not edit that tree).

What is already established and must not regress: a typed draft **survives** a failed offline Return
byte-for-byte (proven live with the marker \`cRiTiC-9f3q\`, and by
\`offline_enter_never_discards_the_typed_draft\`). Disabling the composer must not reintroduce draft
loss — the user's text is more important than either behaviour.

Implement the reference behaviour: disable the composer while disconnected, keeping the distinct
placeholder so the user knows why. Then check the neighbouring states the critic flagged as separately
unexercised — the permission-wait half of the row — and say what you found.

Pass bar: a live drive with \`TILLER_ACP_PROGRAM\` pointed at a missing binary shows the composer
refusing input, and a previously-typed draft still intact.`,
  },
  {
    key: 'J1-gate',
    phase: 'Gate',
    brief: `Target-gate the Linux-only dependencies and glue so the non-Linux builds can even begin.

Three concrete items, all named in \`PORTABILITY.md\`:

1. **\`rust/crates/tiller/Cargo.toml\`** — \`gtk = "0.18.2"\` and \`ksni = "0.3.6"\` sit in the
   unconditional \`[dependencies]\`. Both are Linux-only. Move them under
   \`[target.'cfg(target_os = "linux")'.dependencies]\`, keeping their existing comments (they explain
   *why* each is there — the GTK main-loop pump for \`browser.wait\`, and the StatusNotifierItem tray
   for F-USE-04/05 and F-WIN-08).
2. **\`rust/Cargo.toml\`** — \`gpui\` and \`gpui_platform\` are pinned
   \`default-features = false, features = ["wayland", "x11"]\` unconditionally. Those features are
   Linux-only. **Read the long comment above those lines before touching them**: \`default-features =
   false\` is a hard-won choice, and with \`wayland\`/\`x11\` off \`gpui::guess_compositor()\` reads
   neither \`$DISPLAY\` nor \`$WAYLAND_DISPLAY\` and the app runs with no window at all. Whatever you do
   must keep the Linux build byte-for-byte equivalent in behaviour. Workspace dependencies cannot
   themselves be target-conditional, so the honest shapes are either per-crate target sections or a
   feature on our side that the Linux build enables — pick one, and say in the commit message why.
3. **The code that uses them** — \`crates/tiller/src/main.rs:5832\` pumps
   \`gtk::events_pending()\`/\`gtk::main_iteration_do(false)\`, and \`crates/tiller/src/tray.rs\` is
   entirely \`ksni\`. Put both behind \`cfg(target_os = "linux")\` with an honest non-Linux branch (see
   the scope discipline above — a seam that says it does nothing, not one that pretends to work).

Do not chase every remaining compile error in this slice; the next two slices do that per target.
Your bar is: the manifests are correct, the Linux build and tests are **unchanged**, and
\`cargo check --target x86_64-pc-windows-msvc -p tiller_persistence\` gets past dependency resolution.

**Prove the Linux side did not regress before you finish**: \`cargo build -p tiller\` green and
\`cargo test -p tiller\` still passing at its pre-wave count (161).`,
  },
  {
    key: 'J2-windows',
    phase: 'Windows',
    brief: `Drive \`cargo check --target x86_64-pc-windows-msvc\` to green, crate by crate, bottom-up.

Expect the errors to cluster in a few families rather than being scattered:
- \`libc\` used outside a \`cfg(unix)\` branch. \`libc\` compiles on Windows but exposes far less; seven
  crates depend on it (\`tiller_acp\`, \`tiller_control\`, \`tiller_usage\`, \`tiller_markdown\`,
  \`tiller_git\`, \`tiller_project\`, \`tiller_terminal\`).
- Process enumeration through \`/proc/\` — in \`tiller_terminal/src/lib.rs\`,
  \`tiller_activity/src/process.rs\`, \`tiller_ui/src/settings.rs\`. The Windows counterpart is
  Toolhelp32, but **do not implement it**; put it behind a seam that compiles and reports
  unimplemented. Note the macOS answer is already written in the Swift original
  (\`${SWIFT}/App/ForegroundProcessAgent.swift\` uses libproc), which is worth a comment for whoever
  implements it later.
- Unix-only path and permission assumptions — mode \`0600\` on the credential store
  (\`tiller_usage/src/credentials.rs\` already has \`cfg(unix)\` branches, so check what its non-unix
  path actually does), unix domain sockets in \`tiller_control\` (the control socket is a real design
  question on Windows — a named pipe is the counterpart; seam it, do not build it).
- \`gsettings\` in \`tiller_ui/src/titlebar.rs\`.

For each seam you add, leave a one-line comment naming the platform counterpart, so the later
implementation work is a lookup rather than a rediscovery.

Report \`remainingErrors\` honestly. **Partial is an acceptable outcome and a false green is not** — if
some crate cannot be made to check without inventing a Windows implementation, say which and why.`,
  },
  {
    key: 'J3-macos',
    phase: 'macOS',
    brief: `Drive \`cargo check --target aarch64-apple-darwin\` to green, crate by crate, bottom-up.

This should be **easier than Windows**: macOS is unix, so every \`cfg(unix)\` branch already applies,
and \`crates/tiller_ui/Cargo.toml\` already has a working \`[target.'cfg(target_os = "macos")'.dependencies]\`
section with objc2 bindings. Much of the tree may check green with no change at all.

Where it does not, the same seam discipline applies: compile honestly, do not implement. The macOS
counterparts are unusually well documented for this project because the Swift original **is** the
macOS implementation — for the tray, \`${SWIFT}/App/AgentRosterView.swift\` plus
\`App/TillerApp.swift:115\` show that \`MenuBarExtra\` is exactly an \`NSStatusItem\`; for process
enumeration \`App/ForegroundProcessAgent.swift\` already uses libproc
\`proc_listchildpids\`/\`proc_name\`; for the titlebar preference the setting is
\`AppleActionOnDoubleClick\`. Reference those files in your seam comments.

Do not touch anything \`J2-windows\` fixed unless it is actually wrong — if a shared seam needs
widening from \`cfg(target_os = "linux")\` to \`cfg(unix)\`, that is fine and expected, but say so.
Report \`remainingErrors\` honestly; partial beats a false green.`,
  },
  {
    key: 'J4-ci',
    phase: 'CI',
    brief: `Make the gate keep this true, so the debt cannot silently return.

\`Scripts/ci-linux.sh\` is the repo's Linux verification gate (353 lines). It already has a
\`run_cargo_stage\` helper and a staged structure with \`PASS: <stage>\` / \`FAILED: <stage>\` output —
add two stages in that existing style, do not invent a parallel mechanism:

- \`cargo check --target x86_64-pc-windows-msvc --workspace\`
- \`cargo check --target aarch64-apple-darwin --workspace\`

Requirements:
- **The gate must still be green on Linux at the end.** Run it and show that.
- These two stages are slow on a cold target dir. Make that cost visible and predictable — put them
  late in the run so fast failures still fail fast, and say in a comment roughly what they cost.
- Handle the missing-target case explicitly. A contributor without \`rust-std\` for those triples
  should get a clear, actionable message naming \`rustup target add\`, not a confusing compile error.
  Decide whether that is a skip or a failure and **write down which and why** — a stage that silently
  skips is how this class of debt returns.
- Mirror whatever \`Scripts/ci.sh\` needs so the two gates do not disagree about what "green" means.

Also update \`docs/linux-rewrite/PORTABILITY.md\`: replace its "Recommended order" section with what
was actually done, what each target's real state is, and what is seamed-but-unimplemented. That file
is the handoff for whoever does the real per-platform work.`,
  },
]

function buildPrompt(s, prior) {
  const priorNote = prior.length
    ? `\n## What earlier slices in this wave did\n\n${prior.map((p) => `- **${p.slice}** — ${p.outcome}${typeof p.remainingErrors === 'number' ? `, ${p.remainingErrors} errors left` : ''}. ${p.notes || ''}${(p.seamsAdded || []).length ? `\n  Seams: ${(p.seamsAdded || []).join('; ')}` : ''}`).join('\n')}\n`
    : ''
  return `You are working on slice **${s.key}** of the Tiller cross-platform wave. Tiller is a native Rust +
GPUI port of a macOS SwiftUI app for running AI coding agents side by side. The Linux port is
complete — 350 of 389 contract rows PASSED, each graded by a critic driving the real app. The user has
now required that it also be compatible with macOS and Windows.

## Your task

${s.brief}
${priorNote}${CONTEXT}
${TIMING}
${HOUSE}
Commit each coherent step with explicit paths and a conventional-commit subject. Append your findings
to \`docs/linux-rewrite/wave-j/${s.key}-report.md\` and commit that too — **that file is the real
record**; the returned schema is only an index.

You are **not** the judge of your own work; a fresh critic verifies this wave afterwards. Do not
declare anything green that you have not actually run.`
}

// ---------------------------------------------------------------- run

const done = []
for (const s of SLICES) {
  phase(s.phase)
  log(`${s.key}…`)
  try {
    const r = await agent(buildPrompt(s, done), {
      label: s.key, phase: s.phase, schema: BUILD_SCHEMA, effort: 'high',
    })
    if (r) { done.push(r); log(`${s.key}: ${r.outcome}${typeof r.remainingErrors === 'number' ? ` (${r.remainingErrors} errors left)` : ''}`) }
  } catch (e) {
    log(`${s.key} FAILED (${String(e).slice(0, 110)}) — its committed work may still stand`)
  }
}

phase('Verify')
const verdict = await agent(`Independently verify the Tiller cross-platform wave. **You did not do this work and have not seen
the reasoning behind it.** Everything in the slice reports is a claim.

Worktree \`${ROOT}\`, branch \`linux/gpui-waku\`, cargo root \`${RUST}\`. Read
\`docs/linux-rewrite/PORTABILITY.md\` and the \`docs/linux-rewrite/wave-j/*-report.md\` files for
routes, not for conclusions.

## What you must establish, by running it yourself

1. **\`cargo check --target x86_64-pc-windows-msvc --workspace\`** — green, or an exact count and list
   of what still fails. Run it; do not read the report's number.
2. **\`cargo check --target aarch64-apple-darwin --workspace\`** — same.
3. **No Linux regression.** \`cargo build -p tiller\` green and per-crate tests still at their
   pre-wave counts: \`tiller\` 161, \`tiller_ui\` 322, \`tiller_terminal\` 44, \`tiller_persistence\` 39,
   \`tiller_usage\` 10, \`tiller_acp\` 12. Run **per crate**, never \`--workspace\`
   (\`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\` is a known
   pre-existing flake under workspace-wide concurrency; re-run the crate alone before believing it).
   A cross-platform fix that quietly broke the Linux app would be the worst possible outcome of this
   wave, so weight this heavily.
4. **A negative control on the CI gate.** This is the most important check and the easiest to skip.
   A gate that passes proves nothing until you have seen it fail for the right reason. Temporarily
   reintroduce the exact defect it exists to catch — move \`ksni\` (or \`gtk\`) back into the
   unconditional \`[dependencies]\` of \`rust/crates/tiller/Cargo.toml\` — confirm the new stage in
   \`Scripts/ci-linux.sh\` **fails**, then revert cleanly and confirm \`git status --porcelain\` is
   clean. If the gate stays green with the defect reintroduced, the gate is decorative and that is
   your headline finding.
5. **\`F-CHAT-05\`, the one non-portability row in this wave.** Drive it live yourself:
   \`TILLER_ACP_PROGRAM\` pointed at a missing binary, then try to type into the chat composer. The
   contract requires the editor to be **disabled** while the agent is offline — the Swift reference's
   \`ChatComposerView.canInteract\` excludes \`.disconnected\`, which is what settled this. Two things
   must both hold, and the second is the one a fix could easily break: the composer refuses input,
   **and** a draft typed before going offline is still intact byte-for-byte. Use your own marker
   string so the capture can only have come from your drive. Return a verdict for this row from the
   standard vocabulary (\`PASSED\` / \`half-proven\` / \`FAILED — absent\` / \`FAILED — defective\` /
   \`UNREACHABLE\`) plus a 1-3 sentence evidence cell with no raw \`|\` and no triple backticks.
6. **Seam honesty.** For each \`cfg\` seam the wave added, read the non-Linux branch. It must not be a
   silent no-op that looks like a working feature — it should log, return an error, or be plainly
   unimplemented. A macOS build where the tray silently does nothing while appearing wired is exactly
   the failure mode this wave was told to avoid. Name any seam that fails this test.

## What you must not do

Do not claim macOS or Windows *works*. Nothing here can establish that: there is no macOS or Windows
machine and no cross-linker, so \`cargo check\` is the ceiling of available evidence and it proves
compilation, not behaviour. Say so explicitly in your report. This project once wrongly exempted seven
rows as platform-impossible when they were merely unbuilt
(\`docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md\`); signing off an untested platform
is the same error pointed the other way.
${HOUSE}
Write \`docs/linux-rewrite/wave-j/VERDICT.md\` with your evidence and commit it (\`git add\` the new
path first). Return a short plain-text summary: the two check results with real numbers, the Linux
test counts you measured, the negative-control outcome, any dishonest seam you found, and your
\`F-CHAT-05\` verdict with its evidence cell.`,
  { label: 'verify:portability', phase: 'Verify', effort: 'high' })

log(`verdict: ${String(verdict).slice(0, 400)}`)

return { slices: done, verdict: String(verdict || '') }
