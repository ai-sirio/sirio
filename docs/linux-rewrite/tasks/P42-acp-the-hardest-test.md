# P42 — ACP: the hardest test, unverified since the app changed underneath it

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P38 landed, and your handoff was the right response to my mistake

Persistence probed where it actually fails: schema migration, a 64 MiB logical database cap, atomic
rollback, session-reference upsert/load/delete, 22 integration tests. That layer had carried every
persistence claim made today and had never been tested itself.

Then, when I told you to stop editing `tiller_ui`, you wrote a handoff instead of arguing or
deleting — including the notes that only someone who had just been in the code would know: Changes
parent rows use `on_click` while child actions stop propagation; `TabBar` key bindings are per-`App`
rather than process-global; async frame refreshes must be explicit before `debug_bounds`; `Discard
All` intentionally retains untracked files. Those four lines are worth more than the tests they
describe, because they are what the next person would otherwise rediscover by breaking something.

The collision was my fault — I put an instruction in a broadcast, and broadcasts reach agents
mid-piece. Facts belong in broadcasts; work belongs in briefs.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_acp -p tiller_agents
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

**Do not touch `tiller_ui/**` or `tiller_theme/**`** — pi owns them and is reconciling your handoff
into its own work right now. `tiller/src/main.rs`, `tiller_control/**` and `tiller_git/**` are
codex12's.

**Claim `tiller_acp/**` and `tiller_agents/**`** — both unowned — and say so in your reply.

## The piece

The goal calls this the hardest test, in its own words: *connect a real workspace agent via ACP,
send messages, verify streaming and responses.* It was proven once, at about 01:00, with a real
`claude` agent, a streamed reply, a tool call, a permission round-trip and a nonce found on disk.

**That was thirteen hours and roughly twenty pieces ago.** Since then the shell was rebuilt around
it, panes gained a command layer, the session store changed shape twice, process lifetimes were
fixed, and the control socket grew from six methods to several dozen. Nothing has re-exercised ACP
since. It is the oldest surviving PASSED claim in the project and the least likely to still be true.

Two tiers, and both are yours:

**`tiller_acp` — the protocol.** Connection setup and teardown, the streaming path, message framing,
tool-call requests, permission requests and their answers, cancellation, and what happens when the
agent process dies mid-stream or sends malformed frames. All headless: this is a subprocess and a
protocol, not a view.

**`tiller_agents` — the adapters.** `AgentCatalog` covers five CLIs; on this machine `claude`,
`codex` and `pi` are installed and **`opencode` and `omp` are not**, so entries naming those are
**UNREACHABLE, not FAILED**. Only `claude` speaks ACP.

Two rules from the original that must still hold, and that a rewrite is exactly the moment to break:

- **`prepare` writes only worktree-local hook config.** It must never touch user-global files
  (`~/.claude/settings.json`, `~/.codex/config.toml`). Verify this by watching the filesystem, not
  by reading the code — check the mtimes of those user-global files before and after.
- **The Codex `-c notify=[...]` override is parsed as TOML**, so its JSON must be built with
  `.withoutEscapingSlashes`; `\/` is not a valid TOML escape and Codex fails silently at config
  load, before its TUI ever appears. The critic confirmed this still holds against codex 0.147.0 —
  keep it holding.

## Evidence

**A real `claude` agent, driven end to end**, with a nonce: send a message that makes the agent
write a file whose name contains a random string, and find that file on disk. Nothing weaker
distinguishes a working agent from a plausible transcript.

Then the failure paths, which are the part nobody tested at 01:00: kill the agent process
mid-stream; send it a malformed frame; deny a permission request instead of granting it; cancel a
request in flight. Each should produce a stated outcome rather than a hang or a silent empty state.
This project has now found five surfaces that rendered failure as ordinary emptiness.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **A capability nobody exercised does not exist** — and **ask what the smallest true version of
  your evidence is.** F-PER-06 was proven true and one process too narrow; a streaming test that
  only checks the final buffer is the same mistake.
- **Count the entries yourself.**
- Proceed without asking for design approval; the acceptance criteria above are the gate.

## Reporting

Reply in **12 lines or fewer**: whether ACP still works end to end and the nonce that proves it,
what each failure path does, whether `prepare` stayed out of user-global files with the mtime
evidence, how many entries you assessed and their counts, and the honest remainder.
