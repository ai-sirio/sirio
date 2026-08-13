# QUEUE — work promised to a specific agent, owed at its next dispatch

## Standing rule: reassigning a file under a running agent is the orchestrator's bug

**If you reassign a file while an agent is mid-piece, notify that agent immediately. The agent is not
at fault for acting on the brief you gave it.**

Established 2026-08-13 21:30. `controls.rs` was reassigned to `sonnet` in COSMIC-02, *after* P58 was
dispatched to `pi`. P58 legitimately touches shared UI helpers, so `pi`'s working set showed
`controls.rs` — correct behaviour under the brief it was given, and a live collision with `sonnet`
rewriting that exact file (45 px literals, the container-level decision). Had both written, one would
have silently lost the work.

`pi` was notified mid-flight and told to name any `controls.rs` change in its report rather than make
it. The same courtesy was extended to `sonnet` earlier when `tiller_theme` moved.

The general form: **ownership changes are retroactive for nobody.** An agent dispatched under an old
map keeps working from it until told otherwise, and it has no way to discover the change on its own.

## Standing rule: who owns a change that crosses files

File-based ownership keeps four builders out of each other's way, but some changes are
*inherently* cross-file and the rule as first written made them impossible to complete.

**Whoever widens an enum owns every match arm it breaks, in any file — but only those arms.**

Established 2026-08-13 20:20: `codex11` added `ActivityStatus::NeedsInput` for `F-CHG-22` and
broke an exhaustive match at `tiller_ui/src/sidebar.rs:1460`, a file assigned to `pi`. The
workspace stopped compiling (`E0004`), and the agent that caused it was forbidden from fixing it.
Same logic applies to adding a trait method, or a struct field without a default: touch what the
change makes necessary, nothing else.


Findings arrive while the agent who owns the file is mid-piece. Interrupting costs more than it
saves, so they land here instead of in the orchestrator's head. **Delete a line when it is
dispatched, not when it is fixed** — a fixed item still needs its verdict.

## pireview (critic) — next pass

- **Adjudicate `DEAD-MODELS.md`'s 5 full + 2 partial undisclosed zero-consumer PASSEDs**, all judged
  pass 10: `F-CORE-ACT-17`, `F-CORE-ACT-18`, `F-CORE-ACT-23`, `F-CORE-AUTH-01`, `F-CORE-DOM-07`, and
  partials `F-CORE-ACT-22`, `F-CORE-USG-05`. Pass 12 set the precedent in `F-GIT`: a package row may
  stay PASSED **with "zero app callers — wiring owed" disclosed in evidence**. These seven carry no
  such disclosure, so today they read as delivered. Re-mark or annotate — either restores honesty.
- **FABLE-04's four-plus-partial are still unapplied**: `F-CORE-ACT-19`, `F-CORE-ACT-20`,
  `F-CTRL-NOTIFY-03`, `F-CORE-FILE-04`, and the notification half of `F-CORE-ACT-02`. Pass 12's "10
  re-marked" were pireview's own UI audit, a different set.
- **The visual bar has changed.** As of 2026-08-13 ~19:40 the UI target is the **Pop!_OS COSMIC**
  design language, not waku. Judging new UI against waku screenshots will fail work that is
  correct. `sonnet` is building the token layer; the reference is
  `docs/linux-rewrite/COSMIC-DESIGN.md` once it exists.
- **The adjudication backlog is now the cheapest route to inventory coverage.**
  `Scripts/ledger-totals.py` is the **authoritative** counter and the only one to quote. Run at
  21:10 it gives:

  ```
  TOTAL                               389
  PASSED                              153
  FAILED — absent                     141
  builder-claimed, unverified          34
  never independently judged by a critic pass: 58
  ```

  Under the goal's own rule none of those 58 exist yet. But each is a tick for the cost of
  **exercising** rather than **building**, which is the difference between a pass and a piece.
  Against them sit 141 `FAILED — absent` rows that need real construction.

  **An earlier version of this entry said 70, and that number was wrong.** The orchestrator produced
  it with a raw string match over the whole ledger, which also swept the ledger's own totals block
  and any row mentioning the phrase in prose. It also mis-read the drift: *"the count moved 56 → 70
  in under an hour"* described a broken ruler, not movement. `stale-failed.py`'s duplicate copy of
  this figure was **deleted rather than corrected**, so exactly one tool answers this question.

  The ledger *does* genuinely drift — PASSED 157 → 153 and FAILED — absent 143 → 141 across about an
  hour of builder work — so re-run `ledger-totals.py` rather than trusting any number written here.
  Real drift is a reason to pin your input; a bad ruler is a reason to fix the ruler.

- **Absence in this ledger is usually asserted, not demonstrated.** Of 123 `FAILED — absent` rows,
  **2 cite a workspace-wide search and ~120 cite none at all.** That is not a claim the rows are
  wrong — `DEAD-MODULES.md` independently concluded "absent mostly means absent" — but it is why
  three stale FAILEDs were found by accident rather than by process. `stale-failed.py` ranks them by
  how the absence was established; the signal that survived scrutiny is **`scoped`** (a search
  bounded to one crate, the shape that produced all three known stale rows). Its `HIT` signal is
  partly **circular** and documented as such in the script header: a row that cites an identifier in
  its evidence gets flagged for being specific. Treat `HIT` as "read this row", never as "this row
  is wrong".

- **Pin the sweep before adjudicating anything from it.** `DEAD-MODELS.md`'s own warning: the rows
  have a half-life of hours and four agents are editing. Re-run `Scripts/dead-models.py` and diff.
- Pre-COSMIC baseline shot preserved at `reference/linux-progress/baselines/2026-08-13-pre-cosmic.png`
  — `Scripts/linux-shot.sh` overwrites a fixed path, so copy anything worth keeping.

## Stale FAILEDs — the ledger was wrong, not the code

A stale FAILED sends a builder to build something that already exists, which costs the same as a
false PASSED. Both directions expire; see EVIDENCE-STANDARD.md, "Verdicts expire".

- **`F-EDIT-04`** ("no ⌘S binding") — ⌘S **was already wired** in `main.rs`. Found by `codex11` in
  P61 while trying to add it, and restated explicitly in P62. Critic to re-mark.
- **`F-SET-09` and `F-AGENT-SAFE-01`** ("no skill code") — **false.** `skill.rs` carries a tested
  provisioner. The ledger looked for it in `tiller_agents`; it lives in **`tiller_project`**. Found by
  `fable` in FABLE-06. Same shape as the `save_projects` near-miss and the `tiller_git` three-parallel-
  APIs near-miss: *absence at the name you guessed is not absence of the feature.*

## The gate is fmt-blocked, and neither owner can be interrupted safely

`cargo check --workspace` **passes**. `Scripts/ci-linux.sh` fails only on `cargo fmt --check`, in two
files, both owned by agents who are mid-piece:

- **`crates/tiller/src/main.rs`** — 7 sites (`:3096`, `:3105`, `:3857`, `:6633`, `:9195`, `:9206`,
  `:9216`) → **`codex12`**. Note `:3096`/`:3105` are next to the `NeedsInput` match arm `codex11`
  added under the enum-widening rule, so part of this drift is P62's own.
- **`crates/tiller_ui/src/settings.rs`** — ~14 sites → **`pi`**.

**Do not run workspace-wide `cargo fmt` while builders are live.** It rewrites files agents have open
and loses whatever they write back over it. Each owner runs `cargo fmt` on their own file at the end
of their next piece; that is the whole fix.

## ~~The only real-agent ACP proof in the tree is excluded from the gate~~ — CLOSED 21:20, not dispatched

`crates/tiller_acp/tests/real_claude.rs` holds
`real_claude_streams_tool_permission_and_writes_nonce`, which drives **real Claude over real ACP**:
connect, stream, tool-permission round-trip, and a nonce written to disk — the nonce being what
separates "the agent replied" from "the agent did work".

Run at 21:06 with `-- --ignored`, it **passes in 8.16s**. So the ACP substrate is live on this
machine, and a UI-level ACP failure bisects to the chat-surface wiring rather than to ACP.

But it is `#[ignore = "requires the installed Claude credentials and an ACP adapter download"]`, so
**`Scripts/ci-linux.sh` could be fully green while the ACP path was dead.** The project's entire
acceptance test rested on a path the gate never exercised.

**Closed by the orchestrator at 21:20 rather than dispatched**, because it is gate infrastructure
that no builder owns and every builder depends on. `Scripts/ci-linux.sh` now carries an opt-in stage
after `cargo test --workspace`:

```bash
if [ "${TILLER_ACP_REAL:-0}" = "1" ]; then
    run_cargo_stage "real ACP acceptance (TILLER_ACP_REAL=1)" \
        cargo test -p tiller_acp --test real_claude -- --ignored
else
    echo "SKIP: real ACP acceptance — set TILLER_ACP_REAL=1 to exercise it"
fi
```

Three deliberate choices, so nobody undoes them by accident:

- **Opt-in, not default.** An agent pane without Claude credentials must still be able to reach a
  green gate — the same argument the file already makes for its sccache fallback: *an agent that can
  never reach a green gate learns to ignore it.* Default-off also means this change cannot break the
  five builders running the gate right now.
- **A header disclosure**, beside the existing *"CI OK is not a UI verification claim"*: `CI OK` is
  not an ACP claim either, unless `TILLER_ACP_REAL=1`. The defect here was never a missing test —
  the test existed, was well written, and passed. It was a **gate that did not run it**, which is
  worse than no test, because it manufactures confidence: `CI OK` asserted more than the evidence
  supported.
- **Verified, not assumed.** `bash -n Scripts/ci-linux.sh` → `bash -n OK` (checked first, because
  editing shared infrastructure mid-flight can break every builder's gate at once), and the exact
  command the stage runs → `1 passed; 0 failed; finished in 7.88s`.

**The stage is then protected by `Scripts/Tests/test-ci-linux.sh`**, which already existed and which
the edit was checked against — it asserts the gate's stage set, their fastest-failure ordering, and
that the header discloses what the gate does not cover. Four assertions were added so the stage
cannot be silently removed: the stage must exist, must default off (`${TILLER_ACP_REAL:-0}`), must
announce its skip (a silent skip reads as a pass), and the header must keep the disclosure.

Two things that only showed up because the new assertions were **tested by deliberately breaking a
copy of the gate**, rather than assumed to work:

- The marker `real_claude` was **also satisfied by the explanatory comment** above the stage, so
  repointing the stage at a different test left the check green. It is now anchored on the
  invocation, `-p tiller_acp --test real_claude`. A check that passes on the strength of prose
  rather than of the command actually run is the same defect in miniature as the one this whole
  section is about.
- A missing anchor made `line_of` return empty, the ordering arithmetic die under `set -e`, and the
  test **exit non-zero having printed nothing**. It failed correctly and mutely, which sends the
  next agent bisecting the gate. `line_of` now names the missing stage.

Control matrix, all verified: baseline passes; six mutations (stage deleted, guard made
unconditional, skip notice deleted, header note deleted, stage repointed, wrong crate) each fail
with a readable message; and two pre-existing assertions (`CI OK`, `cargo clippy`) still fire,
confirming the additions did not weaken the original contract.

Still owed: **a ledger row**, which this does not create. `pireview` owns that.

Environment measured 21:04: **`claude`, `codex`, `pi` on PATH; `opencode`, `omp` absent.** So
`discover_availability()` should offer exactly three of five — a sharper check of `F-TAB-07`'s
gating than counting menu items.

## The 143 remaining absent rows, clustered and owner-mapped

Measured 21:15 so the next dispatch is instant instead of re-derived. Clusters, largest first:

| cluster | n | owner | note |
|---|---|---|---|
| `F-TAB` | 19 | codex12 | 4 in flight as P65 |
| `F-PRJ` | 17 | **cross-cutting — see below** | freshly exercised pass 13, best evidence in the ledger |
| `F-CHAT` | 16 | pi (`chat.rs`) + sonnet (`composer.rs`) | split the seam before dispatching both |
| `F-SET` | 15 | pi | |
| `F-EDIT` | 11 | codex11 | |
| `F-SID` | 10 | pi | pi's queue is ~45 rows — the deepest backlog of any builder |
| `F-CHG` | 10 | codex11 | 4 in flight as P64 |
| `F-BRW` | 9 | **unowned, greenfield — see below** | |
| `F-TERM-*` | ~10 | codex11 | |
| `F-USE` | 4 | pi | |
| `F-CORE-*` | ~11 | unowned package tier | |

**`F-BRW` is NOT dispatchable — it collides with the goal's own constraint. Do not assign it.**

I scoped it as "the best-shaped unassigned piece" on the strength of its ledger evidence (all 9 rows:
*"no browser surface; browser.\* answers specific unsupported errors"*) and was wrong. That evidence
says where the code isn't. It does not say what the feature *is*. Reading
`01-inventory-app.md:122-130` settles it:

> `F-BRW-01` | Open a browser tab and see browser chrome, URL, favicon/globe, **and content** |
> VERIFY: navigate to a reachable URL, confirm the address field, **page title/content**, and
> favicon appear | SRC: `App/Browser/BrowserPaneView.swift`

The macOS original is a `WKWebView`. Rendering live web content requires a web engine, and the goal
says **"niente webview, niente HTML"**. Eight of the nine rows (`-01` through `-08`) are web-content
or browser-permission rows that inherit the same requirement.

**This is a scope question for the user, not a verdict for the critic and not a task for a builder.**
The inventory is what defines "finito", so 9 of 389 rows currently cannot be finished as written.
Three coherent resolutions, none of which an agent should pick unilaterally:

1. `F-BRW-01..08` become **`N/A — platform`** on Linux, the way other platform-bound rows already
   are. Only the critic may re-mark them.
2. The no-webview rule is read as *"the app chrome is native GPUI, not an Electron shell"* and an
   embedded engine is permitted **for this feature only**. That is a real reading of the sentence,
   and it is the user's to make, not mine.
3. `F-BRW-09` is separable and buildable **today** under either reading: it is "open an HTTP link,
   and bypass to the **system** browser with a modifier". The bypass half needs no engine at all.

**Do not let a builder resolve this by embedding a webview.** Under reading (1) that is wasted work
that also violates the goal — and the goal's own rule is that anything contradicting it is a gap by
definition.

### The ledger already answered this, and then contradicted itself

Precedent exists, set by the critic, in the same pass:

```
| `F-WIN-06` | N/A — platform | no browser; NewBrowser is a typed no-op | pass 8 |
```

`N/A — platform` is an established verdict here — **24 rows carry it**, e.g. `F-WIN-08`
("macOS hide-on-close delegate"). So at pass 8 the critic looked at a browser row, judged that a
browser is not a thing this Linux build has, and marked it **`N/A — platform`**. In the same pass it
marked `F-BRW-01..09` **`FAILED — absent`**.

**Same feature, same pass, two different verdicts.** One says "not applicable here"; the other says
"owed, go build it". That inconsistency is worth more than my scope argument above, because it is
checkable rather than interpretive — and it means ~9 rows of the "143 still to build" backlog may not
be construction work at all.

**For the critic, not a builder.** Reconcile `F-BRW-01..08` with `F-WIN-06` in whichever direction is
right, and say which. `F-BRW-09` is separable either way — its bypass-to-system-browser half needs no
engine and is buildable today.

**`F-PRJ` needs seam design before it is dispatched, not during.** It spans three owners:
project insertion and its error path live in `main.rs` (`codex12`), the project rows in `sidebar.rs`
(`pi`), and `tiller_git/src/clone.rs` already exists (`codex11`) while **no clone UI does** —
`F-PRJ-05/06/07` all read "no clone form exists". Handing this to three builders at once is how the
`SettingsSnapshot` break happened. Scope it as one piece with an explicit seam, or hold it.

Its evidence is the strongest in the ledger — pass 13 exercised it live, e.g. `F-PRJ-04`: *"insertion
failures are eprintln-only (`[projects] {error}` in main.rs) — no error surface in the UI."* That is
a defect named precisely enough to fix without re-investigating.

## Unowned seams found by FABLE-06 — no ledger row exists for these

- **`transcript.rs`** — a `recent_text` reader for Claude and Codex transcripts.
- **`ollama.rs`**.
- **`file_events.rs`** — a watcher sitting right next to the `F-CHAT-14` claim.

## Dispatched — awaiting report, not awaiting dispatch

Kept only so a verdict is not lost. Move to the ledger or delete once reported.

- **`sonnet` / COSMIC-02**: the **menu width** and **geometric hairline** tokens `codex12` named in
  P60 and re-named in P63 — still open, now twice-requested.
- **`codex12` / P65**: ⌘W settled as harness-gap-or-real-defect, and the 21 "not mine" suite failures
  triaged into environmental / other-agent / leftover with counts.

## The ACP acceptance path is open — this is the project's finish line

P63 (reported ~20:55) delivered the New Chat agent picker: the catalog gated by
`discover_availability()`, an honest "no supported agent found on PATH" fallback, and the chosen
adapter carried into the tab as **title, icon and `agent_id`**. Before it, New Chat made a generic
tab and *"connect a real workspace agent over ACP"* could not be performed on any agent in
particular.

It is `builder-claimed, unverified`: drawn tests cover the picker, **nobody has driven a real agent
through it**. That single exercise is the goal's stated acceptance test, so until it is performed the
project is unfinished by definition regardless of every other row. It leads `CRITIC-pass14`.

Closed by P63: `TILLER_SOCKET_ENABLE` now observed on the boot path, `F-EDIT-08` dedupe, and
`F-CHG-22`'s `main.rs:3026` mapping — the point where the fifth status died, now completing
`codex11`'s panel-side half.

## The COSMIC token layer is currently a dead model — dispatched, but the critic should know

As of 2026-08-13 20:25, `grep -n cosmic crates/tiller_theme/src/lib.rs` returns exactly one line:
`pub mod cosmic;`. COSMIC-01 delivered ten files — palette, spacing, radii, semantic colours and a
dependency-free reader of COSMIC's on-disk RON config — and **`Theme` consumes none of them**. Not
one pixel in the app is drawn from a COSMIC token yet.

This is the defect class `DEAD-MODELS.md` exists to catch, in the design system whose whole job is to
be consumed. COSMIC-02 makes wiring it the precondition for everything else in the piece.

**Bearing on judgement:** until that lands, any UI judged today is still waku-styled. Do not mark a
row FAILED for "not COSMIC" before checking whether the token layer had reached production at the
time the surface was built — that is verdict-by-adjacency, and it manufactures false FAILEDs.

## pi — next dispatch

- **`pi` wrote into `tiller/src/main.rs`** during P58 (the `// P58, F-SET-10` comment at `:2524`),
  which is `codex12`'s file. That is how the tree transiently stopped compiling for `codex11`. The
  boundary needs restating, not because the change was wrong but because two authors in one file is
  how the `SettingsSnapshot` break happened.

## Unowned seams — no ledger row exists for these

- **`register_agent_id`**: restored panes never get their identity re-registered, so restored agent
  panes are identity-less until a hook or title arrives.
- **`pane_closed`**: nothing removes a closed pane's state from the activity model — `agent_status`,
  `pane_agents` and the ownership sets grow monotonically. A leak, not a visible defect (pane ids are
  unique), but unowned.

## P64's diff actions are emitted into nothing — the seam codex11 disclosed, verified open

`codex11` finished P64 and reported the seam plainly: *"Seam codex12: sottoscrivere
RightPanelActionEvent e ChangesTabActionEvent in main.rs."* Verified by the orchestrator at 21:15,
with a positive control so the negative means something:

- `main.rs:2528` and `:2533` **do** call `subscribe_right_panel` / `subscribe_changes_tab`, so the
  subscription machinery is live and a grep that finds nothing is finding a real absence.
- Those helpers subscribe `RightPanelEvent` (`main.rs:2745`) and `ChangesTabEvent` (`:3656`) — the
  **old** enums.
- `ChangesTabActionEvent` is emitted at `changes.rs:1074` (`OpenDiff`) and `:1087`
  (`ResolveInTerminal`) and subscribed **nowhere in `main.rs`**.

So the tests are real and green — `changes.rs:2604` and `:2670` subscribe the events inside the test
harness — while the running app has no subscriber at all. **A drawn control, a passing test, and no
effect when a user clicks it: the dead model exactly.** It is worth noting the tests are not at fault
and not dishonest; they prove emission, which is what `changes.rs` owns. What they cannot prove is
the other half of a seam that lives in someone else's file.

**Owner: `codex12`** (`main.rs` is theirs; `codex11` must not edit it — two agents in one file is the
`controls.rs` collision, and that one was the orchestrator's bug). Two subscriptions, mirroring the
helpers already there:

- `ChangesTabActionEvent::OpenDiff(path)` → open the diff surface for `path`
- `ChangesTabActionEvent::ResolveInTerminal(path)` → `TerminalView::for_conflict`
- `RightPanelActionEvent` → the same treatment

**Do not mark any `F-CHG` row PASSED on P64's tests alone.** Until the subscription lands, the
feature is unreachable from the UI, which is `pireview`'s dead-model finding waiting to happen.

**CORRECTION, 21:35 — the clippy claim below was mine and it was wrong.** I wrote that
`tiller_theme` clippy was clean. It is not. `codex11`, `pi` and `codex12` each independently
reported the gate red there, and **all three were right**:

```
error: this `if` statement can be collapsed
   --> crates/tiller_theme/src/cosmic/live.rs:111:5
error: this `if` statement can be collapsed
   --> crates/tiller_theme/src/cosmic/theme.rs:88:9
```

I ran `cargo clippy -p tiller_theme --all-targets` and grepped `^error`. **The gate runs
`cargo clippy --workspace --all-targets --exclude tiller --exclude tiller_ui -- -D warnings`**
(`ci-linux.sh:175`). Without `-D warnings` a `collapsible_if` is a warning, not an error, so my
probe could not see what the gate sees — a check strictly weaker than the one it claimed to
reproduce, returning a clean negative. That is the fourth broken probe today (`cargo` off PATH, `rg`
absent, `#[ignore]` pattern, now this) and the first that was propagated to another agent.

**To reproduce the gate, run the gate's command.** A paraphrase of a check is not the check.

Whoever reports a red must be assumed right until reproduced *with the same invocation*; three
agents were overruled here by a weaker measurement. The two agents did disagree on the `tiller_ui` suite (169/169 vs
167/169): a direct run gives **173 passed, 0 failed**. The tree moved past both. This is the third
time today a transient red from another agent was carried into a report as a finding.

## The agent picker is cosmetic: every chat connects to Claude whatever you pick

**Found by the orchestrator at 21:22 while scoping pi's next piece. This is the highest-stakes
defect currently known, because it sits precisely on the goal's acceptance test and it fails in the
one way that looks like success.**

`main.rs:3885` uses the chosen adapter for labelling only:

```rust
let (title, agent_icon, agent_id) = chat_tab_identity(adapter);  // adapter -> label only
let chat = cx.new(Chat::launch);                                 // adapter NOT passed
```

and `Chat::launch` (`chat.rs:441`) takes no adapter at all — it hardcodes

```rust
AgentCommand::new("npx").args(["-y", "@agentclientprotocol/claude-agent-acp@latest"])
```

overridable only by the process-wide `TILLER_ACP_PROGRAM`. `Chat::new(command, cwd, cx)` — the
constructor that *does* accept a command — is private (`chat.rs:458`, no `pub`), so no caller
outside the crate can choose one.

**Net effect: pick Codex in the picker and you get a tab titled "Codex", carrying the Codex icon and
`agent_id: codex`, talking to Claude.** P63 delivered exactly what it was asked for and reported it
honestly; the identity triple does reach the tab. Nobody built the other half, because no row asked
for it.

### Why this is dangerous rather than merely missing

The goal's acceptance test is *"connect a real workspace agent over ACP, send messages, verify
streaming and replies."* Perform that today against Codex and **it passes**: a real agent connects,
streams, and replies. It is simply the wrong agent. Every observable the test names is satisfied.

So this is not a defect the acceptance test catches — it is a defect the acceptance test **launders
into a PASS**. `pireview` must check *which* agent answered, not merely that one did. The cheapest
discriminator is to ask the agent to identify itself, or to pick an adapter whose reply style
differs, or to watch the spawned process (`pgrep -af claude-agent-acp`) while a "Codex" tab is open.

### The gap is real, not just unwired

`tiller_agents` has **no ACP command concept at all** — `grep -rn acp tiller_agents/src/` returns
nothing. `AgentAdapter::command(worktree_path, pane_id, tillerctl_path) -> String` is the **terminal**
command, Tiller's original PTY design, and is not an ACP program. So this needs a small design
addition, not just a parameter thread.

The tree already anticipates the shape: `icons.rs:216` strips an `-acp` suffix from agent ids and
`icons.rs:631` tests `Icon::for_agent_id("codex-acp")`. Ids of the form `<agent>-acp` were clearly
intended.

### Ownership, sequenced to avoid a second `controls.rs` collision

`chat.rs` is `pi`'s and `main.rs` is `codex12`'s, and `codex12` is **live in `main.rs` right now**.
So this is split deliberately, non-breaking side first:

1. **`pi` (now)** — add the per-adapter ACP command in `tiller_agents` (unowned) and a *new*
   public constructor on `Chat` that accepts it. **Leave `Chat::launch` in place and working**, so
   `main.rs` keeps compiling untouched and `codex12` is not interrupted.
2. **`codex12` (next dispatch)** — change the one line in `add_chat_tab` to pass the adapter.

Doing it in that order means neither agent ever needs the other's file open at the same time.

## A better predictor of a stale FAILED than any text heuristic: has anyone worked there since?

`Scripts/stale-failed.py` ranks `FAILED — absent` rows by how the absence was *worded* — `scoped`,
`no-search`, `bare`. That is the best signal available from the ledger alone, and its `scoped` flag
did find the real shape. But the ledger alone is the wrong input.

Measured at 21:40 on `F-EDIT`, against a live positive control (`AgentAdapter` found first, so a
negative means something). **Three of the eleven `FAILED — absent` rows are already built:**

| row | ledger says | actually |
|---|---|---|
| `F-EDIT-04` | "no ⌘S binding" (pass 3) | `main.rs:116` — `(WindowCommand::SaveFile, "ctrl-s")` |
| `F-EDIT-06` | "no save path" (pass 3) | `editor.rs:641 pub fn save()`, `main.rs:5584 handle_save_file` |
| `F-EDIT-08` | "add_file_tab pushes unconditionally — no dedupe" (pass 3) | `main.rs:4001` collects `open_paths` and dedupes |

Note `F-EDIT-04`'s row is macOS-worded — the Linux binding is correctly `ctrl-s`, so a search for
`⌘S` or `cmd-s` finds nothing and confirms the stale verdict. **A row phrased in the old platform's
vocabulary will keep failing every search made in that vocabulary.**

**The predictor is not the wording, it is the clock.** Every `F-EDIT` verdict dates from pass 3 or 7.
Since then `codex11` ran P61 — *"the editor save path"* — and `codex12` landed the `add_file_tab`
dedupe in P63. The rows went stale because **builders worked in exactly that area afterwards**, and
nobody re-adjudicated.

So the cheap triage is a join the ledger cannot do by itself:

> **old verdict × area a builder has since shipped a piece in = probably stale**

That is checkable from the task briefs in `docs/linux-rewrite/tasks/` plus each row's pass number,
and it is *predictive* where the text heuristics are merely suggestive. It also explains why all
known stale rows were found by builders tripping over them: the builder working in the area is
precisely the person the join would have flagged.

Worth folding into `stale-failed.py` as a second input. Not yet done — recorded here so the next
agent to touch that script does not re-derive it.

## New tool: `Scripts/assigned-but-absent.py` — stale FAILEDs found by assignment, not by wording

`stale-failed.py` ranks absent rows by how the absence was *worded*. This one uses a different and
independent input — **the task briefs**. A row named in a builder's brief is a row someone was told
to build; if the ledger still says `FAILED — absent`, either the builder never reached it or it was
built and nobody re-adjudicated. The second case is a stale FAILED.

**Of 141 `FAILED — absent` rows, 60 (42%) were ever assigned to a builder and 81 never were.** The
script says nothing about the 81 — and prints that it is saying nothing, rather than letting silence
read as a clean bill.

### It found four stale rows on its first run, two of them created within the hour

| row | ledger says | truth |
|---|---|---|
| `F-EDIT-04` | "no ⌘S binding" (pass 3) | `main.rs:116` `(WindowCommand::SaveFile, "ctrl-s")` |
| `F-EDIT-08` | "`add_file_tab` pushes unconditionally" (pass 3) | `main.rs:4001` dedupes |
| `F-TAB-02` | "no All-Tabs overflow control" (pass 8) | **`codex12` built it in P65** |
| `F-TAB-27` | "no resume-chat" (pass 8) | **`codex12` built it in P65** |

`F-TAB-09` (Open File) is the same shape. **This is the important part: the ledger goes stale
continuously, not once.** Three rows went stale in the last hour purely because a builder shipped
and no pass re-read them. Any process that catches stale FAILEDs by accident will always be behind.

Its top-ranked row, `F-TAB-25`, is genuinely still absent — `codex12` explicitly declined it in P65.
That is the tool behaving correctly: **a high rank is a cheap check worth doing, never a verdict.**

### Shortlist for `pireview` (risk ≥ 8), cheapest inventory coverage available

`F-TAB-25`, `F-CHAT-24`, `F-EDIT-08`, `F-TAB-02`, `F-TAB-11`, `F-TAB-18`, `F-TAB-27`, `F-CHAT-08`,
`F-EDIT-01`, `F-EDIT-04`, `F-TAB-16`.

### Two disciplines it enforces, both learned the hard way today

- **`live` identifiers are printed, never scored.** Scoring them re-created the exact circularity
  documented in `stale-failed.py`'s header for `HIT`: a row citing identifiers is a *well-evidenced*
  row, so rewarding their rediscovery rewards rows for being specific. Measured — including that
  term inflated the median enough to push the genuinely-stale `F-SET-09` below it and trip the
  positive control. **The control was right and the formula was wrong.**
- **`F-AGENT-SAFE-01` is declared out of scope rather than made to pass.** It was found by `fable`'s
  module census, never assigned to a builder, so this signal has nothing to say about it. When a
  control fails, the first question is whether the tool's *claim* is too broad — not whether the
  control is too strict. Loosening the control until the script prints is how a triage becomes a
  ranking that nobody should trust.

Briefs that name a row only to quote the known-stale table (`P64`, `P68`) are excluded as circular,
and the exclusion is printed on every run rather than hidden.

## The picker is half-closed: verified fixed on New Chat, still lying on resume and restore

`codex12` landed the P67 picker line. Verified at `main.rs:3911-3922` — `adapter.acp_program()` →
`acp_agent_command()` → `Chat::launch_with_command`. The mapping is genuinely per-adapter and not
uniform, which is the thing worth checking rather than assuming: `claude.rs:126` yields
`@agentclientprotocol/claude-agent-acp`, `codex.rs:60` yields `@agentclientprotocol/codex-acp`, and
`opencode`/`pi`/`omp` return an honest `None`. **On the New Chat path the picker no longer lies.**

Three sites still construct the hardcoded default:

| site | what it is |
|---|---|
| `main.rs:3965` | `resume_chat` |
| `main.rs:6652` | session restore, path A |
| `main.rs:6750` | session restore, path B |

**This is not the same one-line fix repeated three times, and reading it that way is the trap.** The
adapter is not merely unused at those sites — it was never retained to begin with:

- `RetainedChat` (`main.rs:293`) is `{ id, title, transcript }`.
- `TabRecord` (`tiller_persistence/src/model.rs:105-117`) is
  `{ id, worktree_id, title, kind, order_idx, is_active }`.

Neither carries an agent. So **resuming or restoring a Codex chat replays a Codex transcript into a
Claude connection** — the original defect, surviving in the one place where the evidence of the wrong
agent (the transcript) is sitting right next to it. Closing it needs `RetainedChat` widened *and* a
`TabRecord` column with a migration. `tiller_persistence/**` is **unowned by all five current
briefs**, so this is a seam, not an in-piece fix.

Note the severity inversion against the New Chat bug: `resume_chat` sets `agent_icon: None,
agent_id: None`, so the resumed tab does not *claim* to be Codex. It lies less loudly and is
therefore likelier to survive a critic pass that looks for a mismatched label.

### A second gap, introduced by the fix itself

The `None` branch at `main.rs:3911` returns early with only an `eprintln!`. **New Chat → pi / omp /
opencode now does nothing a user can see** — no tab, no toast, no message, just a line on stderr
nobody is reading. Refusing the silent fallback was the right call; refusing it *silently* converts
the defect from "wrong agent" into "menu item that does nothing", which is cheaper to hit and harder
to attribute. `pi` already built the honest vocabulary for this (`acp_status_label()`, and a badge in
Settings); the menu needs the same treatment.

Both are dispatched to `codex12` as an addendum to P67, with the instruction to **name the
persistence widening as a seam rather than build it inside the piece**.
