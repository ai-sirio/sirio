# Agent launch resolution — behaviour change and open follow-ups

Companion to `2026-08-23-agent-launch-resolution-design.md`. The spec says
what was built; this says what changed for people already running Tiller,
and what was consciously left open.

The execution ledger this was distilled from lived in `.superpowers/`,
which `.gitignore` excludes — it did not survive its worktree. Everything
below was re-verified against the tree at the time of writing rather than
copied forward on trust, and four entries were dropped because later work
had already closed them.

## Behaviour change (for the release note)

**Restored chats whose agent cannot be launched no longer reappear.**

Before, a persisted chat tab was restored and pointed at whatever ACP
bridge the hardcoded `npx …@latest` default named. That default is gone:
a chat is restored only if its recorded agent still resolves to something
real — a built-in ACP server, or an installed one.

Two kinds of tab therefore vanish on upgrade:

- a chat for an agent that resolves to nothing on this machine (not
  installed, nothing published for this platform, not in the registry);
- a chat persisted by an older Tiller with **no recorded agent identity at
  all**. There is no honest way to guess which agent it belonged to.

This is deliberate, and it is the conservative half of the fix that
motivated the whole effort: a chat whose source cannot be resolved must
never silently connect to a *different* agent's server. Dropping the tab
is the safe failure.

It is, today, a **silent** drop. The reason is written with `eprintln!`
(`main.rs`, the restore loop), which a desktop user never sees — the tab
is simply not there any more. See the first follow-up.

## Follow-ups worth doing

**Tell the user why a restored chat disappeared.** The machinery already
exists and is already user-facing prose: `launch_refusal_reason`
(`main.rs`) turns a refused `LaunchSource` into "it is not installed yet —
install it from Settings → Agents" and its siblings. The restore loop and
the chat-open refusal path both still log instead of calling it. Deciding
*where* that text should surface — a startup notification, or the tab
restored in a disabled state carrying an Install action — is a design
choice, not a defect fix, which is why it is here and not in the branch.

**Give `answers_initialize` a read timeout**
(`tiller_agents/tests/acp_conformance.rs`). It launches a real CLI and
blocks on `read_line`; `kill()` only runs after that returns. A future
release that starts up but never answers `initialize` would hang the test
suite rather than fail it.

**Cap the captured stderr in the npx installer**
(`tiller_registry/src/installer.rs`). `npm` output is accumulated whole in
memory with no ceiling.

**Make `install_npx` unit-testable.** It invokes `npm` directly rather
than through an injected runner, so the npx install path has no unit
coverage at all — its only real exercise is a live install. The same scope
choice was made for the archive installer.

## Follow-ups deliberately parked

**Staging directories survive some failures.** `ChecksumMismatch`,
`UnsafeArchiveEntry` and unpack errors leave the staging directory behind;
only `MissingCommand` cleans up, and an npx install that succeeds but
yields no runnable bin leaks the same way. The startup sweep clears them,
so this costs disk until the next launch, not correctness.

**An unrecognised `integrity` value in a manifest reads as
`Integrity::None`** (`tiller_registry/src/store.rs`). Worth knowing that
this is display-only: it feeds the "no published checksum" note in
Settings, and the real gate is `verify_sha256` at install time, which
fails hard on a mismatch. A manifest written by a future Tiller recording
a scheme this version does not know would therefore be reported as
unverified — conservative, but inaccurate. Distinguishing "none" from
"unknown" needs a third enum variant and is not worth it for a note.

**Documentary gaps, no behaviour at stake:** `uvx` is never exercised by a
test (it shares the `UnsupportedDistribution` branch with `Unknown`);
`current_platform_key` returns `"unsupported"` for an OS/arch outside its
table and nothing downstream names that case, though it resolves correctly
to `NoArtifactForPlatform`; there is no test with `XDG_DATA_HOME` and
`HOME` both set and valid; the OpenCode SKIP branch of the conformance
test asserts nothing while the omp one does; the killed child in
`answers_initialize` is not reaped.

## Open verification debt

**`Scripts/ci-linux.sh` has not been run on this work.** Tiller is a Linux
app and its gate is a Linux gate; the branch was developed and merged on a
Windows host where `Scripts/ci.sh` cannot pass and never has (PTY spawning,
process reaping and the headless-GPUI tests all fail there for
environmental reasons). Every crate this work touches was run per-crate and
is green, and the failures on the merged tree were attributed by name to
those pre-existing domains — but the formal verdict belongs to a Linux run
and has not been taken.

**`omp`'s ACP claim is still withheld.** `builtin_acp` returns `None` for
Oh-My-Pi, not because it lacks an ACP server but because no `omp` binary
was available to answer `initialize`. The conformance test gates this in
both directions: claiming `Some` without a responding binary fails, and a
responding binary with a `None` claim fails too. Install `omp`, run
`cargo test -p tiller_agents`, and let the test say which way it goes.
