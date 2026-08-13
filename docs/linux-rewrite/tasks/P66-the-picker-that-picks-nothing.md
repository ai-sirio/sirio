# P66 — The agent picker is cosmetic. Make the choice reach the process.

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.

## P58 landed, and the part that mattered most was the part you refused to fake

You wired the settings schema, 22 green tests, and marked the rows `builder-claimed, unverified`
rather than PASSED. You also did three things that are worth more than the feature:

- You named the `controls.rs` dependency (*"keep the stepper debug_selectors — my flow test clicks
  them"*) instead of editing a file that had been reassigned to `sonnet` mid-flight. **That
  reassignment was the orchestrator's error, not yours**, and you handled it exactly right.
- You separated the gate's red into *yours* and *not yours*, naming owners.
- You reported "no screenshot — shared `:1` display made the capture unreliable" rather than
  claiming a visual check you had not done.

Two of those calls have since been settled, so you do not re-derive them: `cargo clippy -p
tiller_theme --all-targets` is **clean** (the red you and `codex11` both saw was `sonnet` mid-wiring
and is gone), and `cargo test -p tiller_ui` gives **173 passed, 0 failed**. Your 167/169 and
`codex11`'s 169/169 were both true when measured. The tree simply moves.

## The piece: the picker picks a label, not an agent

The project's finish line is *"connect a real workspace agent over ACP, send messages, verify
streaming and replies."* `codex12` built the picker in P63 and it works: it lists the catalog, gates
on `discover_availability()`, and delivers the chosen adapter to the tab as title, icon and
`agent_id`.

Then `main.rs:3885` does this:

```rust
let (title, agent_icon, agent_id) = chat_tab_identity(adapter);  // adapter -> label only
let chat = cx.new(Chat::launch);                                 // adapter NOT passed
```

And `Chat::launch` (`chat.rs:441`) accepts no adapter at all:

```rust
let command = std::env::var_os("TILLER_ACP_PROGRAM")
    .map(PathBuf::from).map(AgentCommand::new)
    .unwrap_or_else(|| AgentCommand::new("npx")
        .args(["-y", "@agentclientprotocol/claude-agent-acp@latest"]));
```

`Chat::new(command, cwd, cx)` — the constructor that *does* take a command — is private
(`chat.rs:458`, no `pub`).

**So: pick Codex, get a tab titled "Codex" with the Codex icon and `agent_id: codex`, connected to
Claude.** Nobody did anything wrong here; P63 delivered its brief exactly, and no row ever asked for
the other half.

### Why this is the most valuable thing in the queue

Run the acceptance test against Codex today and **it passes**. A real agent connects, streams and
replies. Every observable in the goal's sentence is satisfied — it is just the wrong agent. This is
not a defect the acceptance test catches; it is one the acceptance test **launders into a PASS**.
`pireview` has been warned to check *which* agent answered.

## What is actually missing (it is a small design gap, not just a parameter)

`tiller_agents` has **no ACP command concept**: `grep -rn acp tiller_agents/src/` returns nothing.
`AgentAdapter::command(worktree_path, pane_id, tillerctl_path) -> String` is the **terminal/PTY**
command from Tiller's original design — a shell line to run the CLI in a pane. It is not an ACP
program and must not be reused as one.

The tree already anticipates the shape: `icons.rs:216` strips an `-acp` suffix from agent ids and
`icons.rs:631` asserts `Icon::for_agent_id("codex-acp")`. Ids of the form `<agent>-acp` were
intended.

**Design the mapping yourself.** Not every adapter has an ACP server, and that is a real answer:
Claude has one, Codex has one, and for the rest an honest `None` is correct — better than inventing a
program name that will fail at spawn. An adapter that returns `None` should be **visibly
unselectable or clearly marked**, not silently downgraded to Claude, because silent downgrade is the
exact bug you are fixing.

## Sequencing — do the non-breaking half only

`main.rs` belongs to `codex12`, who is **in it right now**. Two agents in one file is the
`controls.rs` collision, and that one was the orchestrator's fault; do not let there be a second.

**Your half:**

1. Add the per-adapter ACP command to `tiller_agents` (unowned — it is yours for this piece).
2. Add a **new public constructor** on `Chat` that takes it.
3. **Leave `Chat::launch` in place and working.** `main.rs` must keep compiling with zero edits.

`codex12` changes the one line in `add_chat_tab` at its next dispatch. That ordering means neither of
you ever needs the other's file open.

If you find you cannot do this without touching `main.rs`, **stop and say so in your report** rather
than editing it — that answer is useful and the collision is not.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. Named tests.

The test that counts here does **not** assert that a chat connects — that is already true and proves
nothing. It asserts that **the command differs by adapter**: give it the Codex adapter, assert the
resulting `AgentCommand` is Codex's; give it Claude, assert Claude's. A test that connects
successfully is precisely the test that is passing today while the bug is present.

For the `None` case, assert the surface says so rather than falling back.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller_ui/src/chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, and
  `tiller_agents/**` for this piece.**
- **Do not edit** `main.rs`, `tab_bar.rs`, `tiller_control/**` (`codex12`); `changes.rs`,
  `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`, `tiller_terminal/**` (`codex11`);
  `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs` (`sonnet`).
- **Your fmt debt:** `settings.rs`, ~14 sites. Run `cargo fmt` **on your own files only** — never the
  workspace, which rewrites files other agents have open.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. The visual bar is
  **Pop!_OS COSMIC**, not waku. Name any token you need and lack; that list is how `sonnet` learns.
- **Establish the build state yourself.** Five builders edit this tree continuously and it moves in
  and out of red on a timescale of minutes. Another agent's transient red is not a finding — that
  mistake has been made three times today, twice by reports that were accurate when written. Re-run
  before reporting; if still red, name the crate and its owner.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the per-adapter mapping you chose and which adapters got `None` and why, the
new constructor's signature, **explicit confirmation that `main.rs` still compiles untouched**, the
test that proves the command differs by adapter (by name), what the UI does for a `None` adapter,
whether your `settings.rs` fmt half is clean, tokens `tiller_theme` still lacks, and the honest
remainder.
