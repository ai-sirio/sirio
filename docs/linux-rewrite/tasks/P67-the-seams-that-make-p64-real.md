# P67 — Two seams and a picker: make three finished features actually reachable

**You are codex12, pane `w1:p3`.** Your context was just reset, so this brief is everything you need.

## P65 closed both debts, with verdicts rather than deferrals

You settled ⌘W as a **test-harness gap, not a product defect** — the thing two previous passes had
carried without deciding. And you triaged the 21 failures into real buckets: 12/21 reproduced (9
environmental, 2 concurrent, **1 leftover:
`restore_replays_persisted_terminal_scrollback`**), 9 not reproduced. Naming the single leftover is
what makes that useful; "all environmental" would have been cheaper and worth nothing.

You also declined `F-TAB-25` and `F-TAB-18/24` explicitly rather than shipping a drag that draws and
drops nothing. That is the right call and it is recorded as such.

Your fmt half is clean, so the gate's fmt blocker is now `pi`'s `settings.rs` alone.

**One correction you are owed:** you reported the gate still red on two `clippy::collapsible_if` in
`tiller_theme`. You were right. The orchestrator told other agents that crate was clean, having run
a weaker probe (`cargo clippy -p tiller_theme` with no `-D warnings`) than the gate's own
`ci-linux.sh:175`. Three agents reported that red and all three were correct. `sonnet` is fixing it.

## This piece is all wiring. Nothing here needs new UI.

Three features are already built, already tested, and **unreachable from the running app** because
the subscription half lives in your file. Each is a dead model until you close it — a drawn control,
a passing test, and nothing happening when a user clicks.

### 1. P64's diff actions are emitted into nothing

`codex11` finished the diff surface and disclosed the seam plainly. Verified open at 21:15:

- `main.rs:2528` / `:2533` already call `subscribe_right_panel` / `subscribe_changes_tab`, so the
  machinery is live and the absence below is real, not a grep artefact.
- Those helpers subscribe `RightPanelEvent` (`main.rs:2745`) and `ChangesTabEvent` (`:3656`) — the
  **old** enums.
- `codex11` added **new** ones. `ChangesTabActionEvent` is emitted at `changes.rs:1074`
  (`OpenDiff`) and `:1087` (`ResolveInTerminal`), and subscribed **nowhere in `main.rs`**.

Mirror the helpers already there:

- `ChangesTabActionEvent::OpenDiff(path)` → open the diff surface for `path`
- `ChangesTabActionEvent::ResolveInTerminal(path)` → `TerminalView::for_conflict`
- `RightPanelActionEvent` → same treatment

`codex11`'s tests are honest and green: `changes.rs:2604`/`:2670` subscribe inside the harness, which
proves *emission* — the half `changes.rs` owns. Only your file can prove the other half.

### 2. The agent picker picks a label, not an agent

**This one sits directly on the project's finish line**, which is *"connect a real workspace agent
over ACP, send messages, verify streaming and replies."*

`main.rs:3885`:

```rust
let (title, agent_icon, agent_id) = chat_tab_identity(adapter);  // adapter -> label only
let chat = cx.new(Chat::launch);                                 // adapter NOT passed
```

`Chat::launch` (`chat.rs:441`) takes no adapter and hardcodes
`npx -y @agentclientprotocol/claude-agent-acp@latest`. So **pick Codex, get a tab titled "Codex",
Codex icon, `agent_id: codex` — talking to Claude.** Your P63 did exactly what it was asked; nobody
built the other half because no row asked for it.

It fails in the way that looks like success: the acceptance test *passes* against Codex, because a
real agent really does connect and reply. It is just the wrong one.

**`pi` is adding the non-breaking half right now** (P66): a per-adapter ACP command in
`tiller_agents` plus a new public constructor on `Chat`, leaving `Chat::launch` working so your file
keeps compiling untouched. **Check whether P66 has landed before you start this part.** If it has,
your change is one line in `add_chat_tab` — pass the adapter. If it has not, do items 1 and 3 and say
so; do not build your own mapping, or you and `pi` will build two.

### 3. Your own leftover

`restore_replays_persisted_terminal_scrollback` — the one failure your triage could not attribute.
Settle it the way you settled ⌘W: harness gap or real defect, and say which.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. **Named drawn tests** (`TestAppContext` /
`VisualTestContext`, `.debug_selector(id)`, full `run_until_parked()` pump).

For the subscriptions, the test that counts **drives the control and asserts the workspace changed**
— a diff tab opened, a terminal spawned with the conflict path. Asserting that an event was emitted
re-proves `codex11`'s half and leaves the defect exactly where it is.

For the picker, assert **the command differs by adapter**. A test that connects successfully is
precisely the test passing today while the bug is present.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Yours: `tiller/src/main.rs`, `tiller_control/**`, `tiller_ui/src/tab_bar.rs`.**
- **Do not edit** `chat.rs`, `settings.rs`, `sidebar.rs`, `status_bar.rs`, `tiller_agents/**` (`pi`,
  live in these now); `changes.rs`, `right_panel.rs`, `editor.rs`, `file_view.rs`, `tiller_git/**`,
  `tiller_terminal/**` (`codex11`); `tiller_theme/**`, `controls.rs`, `titlebar.rs`, `composer.rs`
  (`sonnet`).
- **Standing rule:** whoever widens an enum owns every match arm it breaks, in any file — but only
  those arms.
- Colours, spacing and radii from `tiller_theme::Theme`, never a literal. Visual bar is **Pop!_OS
  COSMIC**, not waku. Name any token you need and lack.
- **Establish the build state yourself, with the gate's own commands.** `sonnet` is mid-fix in
  `tiller_theme`; a transient red in someone else's crate is not a finding. If still red, name the
  crate and its owner.
- **Never copy code from the reference checkouts.** Mark rows `builder-claimed, unverified`, never
  `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the subscriptions landed and the drawn tests that prove a *user-visible*
effect (by name), whether P66 had landed and so whether the picker line was done or deferred,
`restore_replays_persisted_terminal_scrollback` settled as harness-gap or defect and which, the gate
run with its own invocation and any not-yours failures named separately, tokens `tiller_theme` still
lacks, and the honest remainder.
