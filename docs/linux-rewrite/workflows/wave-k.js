export const meta = {
  name: 'wave-k',
  description: 'Seam tiller_control for non-unix so the cross-target gate is honest and green',
  phases: [
    { title: 'Seam', detail: 'gate panel.rs PTY machinery + client/server type positions' },
    { title: 'Verify', detail: 'independent critic: both targets, Linux baseline, gate negative control' },
  ],
}

const ROOT = '/home/enzopalmisano/Scrivania/Progetti/tiller-linux'
const RUST = `${ROOT}/rust`

const HOUSE = `
Worktree: \`${ROOT}\`   Branch: \`linux/gpui-waku\`   Cargo root: \`${RUST}\`
Never edit \`docs/linux-rewrite/INVENTORY-LEDGER.md\` — the orchestrator is its single writer.
**Commit with explicit file paths only**; \`git add <exact/path>\` first for new files. On
\`index.lock\` contention retry in a loop, never delete the lock:
\`for i in $(seq 1 12); do git commit -q -m "..." <paths> && break || sleep 4; done\`
Commit as you go — you are killed after 180 s of model silence. Keep returned answers small.
`

const CONTEXT = `
## Why this exists

The cross-target CI stage used to classify any failure containing \`error occurred in cc-rs:\` as
BLOCKED (an SDK wall on this box, not our code). A critic showed that was a hole: cargo's fail-fast
meant an injected regression and the pre-existing wall raced, and the wall won 1 time in 7.

The gate has now been fixed — \`cargo check ... --keep-going\` plus a classifier that only reports
BLOCKED when **every** error is attributable to the wall. Running it immediately surfaced **30 real
errors that the old classifier had been hiding**, all in \`tiller_control\`:

- \`crates/tiller_control/src/panel.rs\` — a hand-rolled unix PTY: \`libc::{fork, openpty, ioctl,
  killpg, kill, setsid, setenv, winsize, WEXITSTATUS}\`, and \`std::os::fd::{FromRawFd, RawFd}\`.
  Error sites cluster at line 33 (the \`use\`), 107/111 (\`PaneProcess\` fields), 636–743
  (\`spawn_process\`, \`child_exec\`) and 798–840 (\`reap\`, \`terminate\`, \`wait_for_process_group\`,
  \`process_done\`).
- \`crates/tiller_control/src/client.rs:12\` and \`crates/tiller_control/src/server.rs:38\` — a
  \`UnixStream\` in a type position that an earlier pass missed when it seamed the function bodies.

The file's own header comment already says it needs a \`cfg(not(unix))\` twin. Do that.
`

const SEAM_RULES = `
## Seam rules — these are the point of the task

**Honest, not implemented.** Do NOT write a Windows PTY. The non-unix branch must compile and be
plainly unavailable: return \`PaneError\` (add an \`Unsupported\` variant if there isn't one), or the
crate's existing error type, with a message naming the counterpart. The counterpart here is ConPTY
via \`alacritty_terminal\`'s \`tty/windows/\`, which already owns a child's lifetime — say so in a
comment so whoever implements it starts from the right place.

**A silent no-op is the one unacceptable outcome.** This wave exists because a previous pass gated
\`descendant_pids\` as \`cfg(unix)\` when its body reads \`/proc\`; macOS is unix, so it silently
returned an empty list and degraded process teardown with no error and no log. That type-checked
perfectly and was caught by a human reading code. Prefer \`cfg(unix)\` only where the body is
genuinely portable POSIX; use \`cfg(target_os = "linux")\` where it is Linux-specific.

**The Linux public API must not change.** \`tiller\` consumes \`PaneRegistry\` and friends; the
non-unix build may return errors, but the types and signatures the Linux build exposes must be
identical before and after. If a method cannot exist off unix, it should still exist and fail.
`

const SCHEMA = {
  type: 'object',
  required: ['outcome'],
  properties: {
    outcome: { type: 'string', enum: ['green', 'partial', 'blocked'] },
    residualErrors: { type: 'number', description: 'our-code errors left on the Windows target, 0 if clean' },
    commits: { type: 'array', items: { type: 'string' } },
    seams: { type: 'array', items: { type: 'string' }, description: 'one line each' },
    notes: { type: 'string', description: 'two or three sentences' },
  },
}

phase('Seam')
const build = await agent(`Seam \`tiller_control\` so it compiles for non-unix targets.
${CONTEXT}
${SEAM_RULES}
## Your bar

1. \`cd ${RUST} && cargo check --target x86_64-pc-windows-msvc --workspace --keep-going\` produces
   **no error attributable to our own crates**. The \`psm\`/\`cc-rs\` assembler wall and the
   \`libsqlite3-sys\`/\`rusqlite\` C-compile wall will still fail — those are environmental on this box
   and are expected. Check with:
   \`grep -E '^error' <log> | grep -vE 'error occurred in cc-rs:' | grep -vE 'could not compile \\\`(psm|stacker|libsqlite3-sys|rusqlite)\\\`'\`
   That command must print nothing.
2. Same for \`--target aarch64-apple-darwin\`.
3. **Linux unchanged**: \`cargo build -p tiller\` green, and per-crate tests at baseline —
   \`tiller_control\`, plus \`tiller\` 162, \`tiller_ui\` 322, \`tiller_terminal\` 44. Run **per crate**,
   never \`--workspace\` (\`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group\`
   is a known pre-existing flake under workspace-wide concurrency; re-run the crate alone).
${HOUSE}
Write \`docs/linux-rewrite/wave-k/SEAM-report.md\` and commit it — that file is the real record.
You are not the judge of your own work; a fresh critic verifies this.`,
  { label: 'seam:tiller_control', phase: 'Seam', schema: SCHEMA, effort: 'high' })

log(`seam: ${build ? build.outcome : 'no return'} (${build && build.residualErrors} residual)`)

phase('Verify')
const verdict = await agent(`Independently verify the \`tiller_control\` cross-platform seam. **You did not do this work.** The
report is a claim; re-run everything yourself.
${CONTEXT}
${SEAM_RULES}
## What you must establish, by running it

1. **Windows**: \`cd ${RUST} && cargo check --target x86_64-pc-windows-msvc --workspace --keep-going\`.
   Filter the log yourself and report the exact count of errors **not** attributable to the
   \`cc-rs\`/\`psm\`/\`libsqlite3-sys\` environmental wall. Do not read the number off the report.
2. **macOS**: same for \`aarch64-apple-darwin\`.
3. **No Linux regression**: \`cargo build -p tiller\` green; per-crate tests at baseline —
   \`tiller\` 162, \`tiller_ui\` 322, \`tiller_terminal\` 44, \`tiller_persistence\` 39, and
   \`tiller_control\` at whatever it was before this wave (check \`git stash\`/the pre-wave commit
   rather than assuming). **Per crate, never \`--workspace\`.** A cross-platform seam that quietly
   broke the Linux app is the worst outcome available here, so weight this most heavily.
4. **Gate negative control, run at least 3 times.** \`Scripts/ci-linux.sh\`'s
   \`run_cross_target_stage\` was just rewritten to use \`--keep-going\` and to report BLOCKED only
   when every error is a wall error. Prove it now catches a regression **deterministically**:
   reintroduce \`gtk = "0.18.2"\` into the unconditional \`[dependencies]\` of
   \`rust/crates/tiller/Cargo.toml\`, run the real stage command, and confirm it reports FAILED —
   three times, not once, because the defect this replaced was a 1-in-7 race. Then revert and
   confirm \`git status --porcelain\` is clean. If it ever reports BLOCKED or PASS with the defect
   present, that is your headline finding.
5. **Seam honesty**: read every \`cfg(not(unix))\` / \`cfg(not(target_os = "linux"))\` branch this
   wave added. Any that silently succeeds — returns an empty collection, an \`Ok(())\`, or a default
   — while the caller believes real work happened, is a defect. Name it.

## What you must not claim

Do not say macOS or Windows *works*. There is no machine for either and no cross-linker; \`cargo
check\` proves compilation, not behaviour. Say that explicitly.
${HOUSE}
Write \`docs/linux-rewrite/wave-k/VERDICT.md\` and commit it (\`git add\` the new path first). Return a
short plain-text summary: the two residual error counts you measured, the Linux test counts you
measured, the negative-control results across all three runs, and any dishonest seam.`,
  { label: 'verify:seam', phase: 'Verify', effort: 'high' })

return { build, verdict: String(verdict || '') }
