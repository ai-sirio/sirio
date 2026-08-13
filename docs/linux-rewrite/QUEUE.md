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

## P68 landed (`codex11`), and P70 dispatched in its place

**Confirmed stale by exercise, not by grep** — `F-EDIT-04`, `F-EDIT-06`, `F-EDIT-08`. Reported to
`pireview` with verdicts left unchanged, which is the correct division: builders produce evidence,
**only the critic changes a verdict**.

**Built, `builder-claimed, unverified`:** `F-EDIT-01`, `02`, `03`, `05`, `07`, `10`, `11`. Ten named
tests in `file_view`, seventeen in `right_panel`, UI suite 179/179.

`F-EDIT-07`'s proof is `different_languages_produce_different_code_spans` — it asserts *two languages
differ* rather than that one file renders. That distinction is the whole difference between proving a
feature and proving a screenshot, and it is the standard the remaining rows should be held to.

**`F-EDIT-12` refused**, correctly: only the harness-only drag fixture exists, no real drag/drop on
file rows. This is the second time a builder has declined this rather than ship a drag that draws and
drops nothing (`codex12` declined `F-TAB-25` and `F-TAB-18/24` on the same grounds in P65).

### The gate's fmt blocker has moved

It was `pi`'s `settings.rs`. It is now **`main.rs` at 6423, 7230, 9591** — `codex12`'s file, and
`codex12` is live in it. `codex11`'s owned clippy and `cargo build -p tiller -p tiller_control` were
green; the 12 workspace test failures it saw are concurrent-edit noise from a shared worktree, not
findings. Routed to `codex12`, not chased.

**Token `tiller_theme` still lacks:** *compact-action padding*. `codex11` used
`titlebar_control_spacing` as the nearest existing token rather than a literal. Queued for `sonnet`.

## The test suite is not the problem — `Scripts/tests-that-cannot-fail.py`

`F-PRJ-04` records that project-insertion failures are `eprintln!`-only with no error surface. That
is the *same* defect I had just found fresh in `codex12`'s new `None` branch in `add_chat_tab`. Two
instances of one shape is a reason to measure, so I did.

**37 `eprintln!` in the app and UI crates** — 26 in `main.rs`, 10 in `session.rs` — while the UI
crates already have a notice/banner vocabulary (`sidebar.rs` 15, `file_view.rs` 9, `chat.rs` 5). The
worst of them, worth its own row: **`main.rs` `[files] save failed: {error}`** — Ctrl+S fails and the
user is told on stderr. `F-EDIT-06` (the save path) was just confirmed built by exercise, so a
"working" save sits directly beside a silent failure mode.

Following that thread turned up `probe_escape_dispatch` — a `#[gpui::test]` with **zero assertions**
and three `eprintln!`s, two of which *compute* the condition that should have been asserted
(`cx.debug_bounds("settings-category-General").is_some()`) and print it instead. Green forever,
counted among the workspace's passing tests.

So: is the test suite itself rotten? **It is not.** `Scripts/tests-that-cannot-fail.py` says
**5 of 747 tests contain no assertion (0%)**, and three of those five are leftover probe scaffolds
(`probe_escape_dispatch`, `probe_sf_steps`, `probe_pixels`); the other two
(`parses_awkward_input_without_panicking`, `system_mode_never_panics_and_always_resolves`) are
defensible — for those, running to completion *is* the check.

This is a negative result and it is worth having. Given how many false PASSEDs this project has
found, suspecting the tests was reasonable; ruling it out mechanically means the defects are in the
wiring, not the verification.

### The tool was wrong three times before it was right, and the controls did not catch it

Each false-positive class was found only by reading the code the tool had just accused:

| class | example | why it looked assertion-free |
|---|---|---|
| project assertion helper | `dark_palette_matches_waku` | asserts entirely via `expect_color(..)` |
| documented no-op entry point | `helper_process` | a subprocess re-exec; a no-op unless its env var is set |
| wait-until-or-panic | `changes_refresh_automatically_after_an_external_edit` | `pump_until(..)` ends in `panic!`, so the wait *is* the assertion |

**The positive controls passed on every one of those runs** — 736 tests did contain a std assertion,
so the detector was demonstrably alive. That is the lesson worth keeping: **a control proves the
detector is not dead. It bounds false negatives, never false positives.** Widening the vocabulary is
the only fix, and there is no control that would have substituted for reading the code.

The third class was the expensive one to get wrong: it would have accused two strict, honest tests in
`changes.rs` and `right_panel.rs` — a file whose owner had cited them as evidence in the same hour.
That is the clippy mistake's exact shape (broadcasting a weaker probe's output as fact), caught this
time before dispatch rather than after.

**Open, small, and owned:** the three probe scaffolds. `probe_escape_dispatch` is in `main.rs`
(`codex12`); `probe_sf_steps`/`probe_pixels` are in `tiller_ui/src/sfsymbol.rs`, which **no current
brief owns**. Not urgent — they mislead a reader counting green tests, they do not break anything.

## `F-PRJ` unblocked: the 18-row cluster decomposes into five pieces, and one row is two lines

`F-PRJ` was held pending a three-way seam design. It is designed now. The cluster is unusually well
adjudicated — pass 13 exercised it **on the display**, so the evidence is live rather than read —
and it splits cleanly:

| piece | rows | surface |
|---|---|---|
| **P71** silent failures | `F-PRJ-04` | *already exists* — see below |
| add-flow honesty | `F-PRJ-01`, `03` | the `+` menu offering three choices; the non-git Initialize/Add-anyway prompt |
| clone-from-URL | `F-PRJ-05`, `06`, `07` | one new form: URL field, guards, failure/retry |
| create-new-project | `F-PRJ-08`, `09`, `10` | one new form: name field, guards, failure |
| project settings sheet | `F-PRJ-11`, `12` | delete/trash, display-name edit, repository-type switch |
| project icon | `F-PRJ-13`, `14`, `15`, `16` | colour+reset, avatar/GitHub/PNG, glyph grid, emoji |
| worktree base | `F-PRJ-17`, `18` | default parent + custom location, replacing `derive_worktree_*` |

Three observations that change how these should be scheduled:

**The bottleneck is `sidebar.rs`, not the work.** Almost every row above lands in one file, which
`pi` owns. Running two of these pieces at once means two builders in `sidebar.rs`. Sequence them, or
split by *surface* (the settings sheet vs. the add flow) and confirm the split before dispatching.

**Clone needs `tiller_git` (`codex11`), so `F-PRJ-05/06/07` is the one piece that is genuinely
two-owner.** Everything else is one owner plus a `main.rs` call.

**`F-PRJ-04` is not a missing surface. It is a disconnected call, and the fix is ~2 lines.** The
ledger's wording — *"no error surface in the Linux build"* — is wrong about the cause:

- `sidebar.rs:274` `notice: Option<String>`; `sidebar.rs:613` `pub fn set_notice(..)`; rendered at
  `~1856` as `sidebar-notice`; **already used at `sidebar.rs:662`** for `could not open the folder
  picker: {error}` — the same sentence `main.rs` prints to stderr for the *file* picker.
- `main.rs:2304` holds `sidebar: Entity<Sidebar>` and already uses `self.sidebar.update(..)` at
  `:2718`, `:3042`, `:3288`, `:3294`.

This is the third row this week whose *wording* is the reason it looks absent (after `F-EDIT-04`'s
`⌘S`/`ctrl-s` and `F-SET-09`'s `skill.rs`-in-the-other-crate). It is exactly the failure `FABLE-08`
is chasing, and it is a **different** kind from those two: not old-platform vocabulary, but a row
that names the *symptom* ("no error surface") as though it were the *cause* ("this call site does not
use the surface"). Worth telling `fable`: search for the capability, never for the row's diagnosis.

**P71 is written** (`docs/linux-rewrite/tasks/P71-the-failures-only-stderr-sees.md`) and covers the
whole silent-failure cluster in two non-breaking halves — `codex11` gives `FileView` the setter
`Sidebar` already has, `codex12` converts four call sites. It deliberately leaves the other 33
`eprintln!` alone: `[control]`/`[session]`/window-open are diagnostics for whoever runs the binary,
and promoting all of them to UI is a different and worse defect.

## CORRECTION: the "menu item that does nothing" does not exist

I reported a second gap alongside the half-fixed picker: that `add_chat_tab`'s `None` branch made
New Chat → pi/omp/opencode a silent no-op, and I sent it to **both `codex12` and `pireview`** as fact.

**It is false.** `tab_bar.rs:467` builds the chat picker with:

```rust
.filter(|agent| agent.is_available() && agent.acp_program().is_some())
```

Non-ACP agents are never offered. `pireview`'s own live frame `stage-picker-5.png` shows the New Chat
submenu containing exactly **Claude Code** and **Codex**; the top-level OpenCode / Pi / Oh-My-Pi rows
are *terminal launchers*, a different action entirely. The `None` branch is a defensive guard on an
unreachable state and is correct as written. Both agents corrected; `P71` amended to three sites with
the withdrawal recorded in the brief rather than deleted from it.

**How it happened:** I read the code branch and *inferred* the menu instead of reading the menu. The
inference was plausible — an early `return` with only an `eprintln!` genuinely is the shape of a dead
menu item — and it was wrong because the reachability question is answered in a different file, by a
filter I never looked for.

This is the same failure as the clippy episode and as the assertion census: **a probe weaker than the
claim it is used to support.** The pattern is stable enough to state as a rule — *when a finding is
about what a user can reach, code alone cannot settle it; the surface has to be looked at.* The
critic's frames settled both of today's reachability questions in seconds.

Twice in this session the frames overruled me. Earlier I read eight byte-identical `stage-menu`
frames as "the menu will not open" — they were the menu **staged open and stable**, which is the
success signal for that harness. Same error in the other direction: inferring from a proxy rather
than looking at the artefact.

### Two findings from the critic's frames worth keeping

- **The `acp-01..10` series exercised bash, not an agent.** `acp-08-done-50s.png` shows the nonce
  prompt *"Reply with exactly CRIT14-DONE-8219 and nothing else"* typed into the **Terminal** tab,
  with bash answering `Comando «Reply» non trovato`. Any verdict resting on that series would have
  been an ACP claim proven against a shell. The later staged menu/picker approach is the sound one.
- **Critic evidence lives in `/tmp` and does not survive.** The only commit that has ever preserved
  critic frames is `5e55ed0` — the accidental bulk checkpoint. `pass 14`'s 45 frames (3.8 MB) are in
  `/tmp/crit14`. A verdict whose evidence has been deleted cannot be re-checked by anyone, including
  the critic that wrote it. Asked `pireview` to land them under `reference/linux-progress/`.

### What the frames show that is simply good news

The app renders: project sidebar with three worktrees, tab strip with Chat and Terminal, a working
terminal with a themed prompt, the Files panel, and a status bar carrying live Claude/Codex usage.
The New Chat submenu is correct COSMIC-flavoured chrome. Whatever else is open, *it renders* — which
the goal makes the precondition for everything else.

## SCOPE RULING (from the user): the browser is in scope, and a webview is allowed for it

The nine `F-BRW` rows have sat at `FAILED — absent` since pass 8, and the open question was whether
*"niente webview, niente HTML"* put the browser feature out of scope entirely — the `F-WIN-06`
`N/A — platform` precedent.

**The user has ruled: it does not.** *"niente webview"* bans an **Electron-style app shell** — every
pixel of Tiller's own UI stays native GPUI — and does **not** ban a web engine behind the in-app
browser *feature*. A webview crate is permitted **for that surface alone**.

Consequences, so this is not re-litigated next pass:

- The inventory denominator stays **389**, not 380. Nine rows are work, not exemptions.
- `pireview` must not mark `F-BRW` as `N/A — platform`.
- Adding `wry`/WebKitGTK is allowed **only** under the browser surface. It is not a precedent for any
  other row, and Tiller's chrome around the page is still COSMIC-tokened GPUI.

### What is actually there today

`main.rs:4215` is an empty arm with an honest comment: *"Browser is not part of this shell's content
set yet. Keeping the action typed and ignored is preferable to opening a fake pane."* That instinct
is the same one that made two builders refuse a fake drag, and it was right.

**The defect is that the menu offers "New Browser" anyway.** Verified on the running app rather than
inferred — `pireview`'s frame `stage-menu-4.png` shows the entry in the open menu, and the production
list in `tab_bar.rs` does not filter it. So the user clicks a live entry and gets nothing, with no
feedback of any kind.

Note the asymmetry: the **control socket already answers `browser.*` with a specific *unsupported*
error**. The same feature is honest on one surface and silent on the other. That is a smaller,
sharper row than the nine, and it is true under every possible resolution of the scope question.

*(This is the genuine instance of the defect I wrongly attributed to the chat picker an hour ago and
withdrew. The difference is method: there I inferred reachability from a code branch; here it is
settled by a frame of the running app plus an unfiltered production list.)*

### `P72` is a spike, deliberately, and not a build

`docs/linux-rewrite/tasks/P72-the-browser-spike.md`. The unknown that governs all nine rows is
whether a web engine can be **composited inside a GPUI window on Linux/X11** — GPUI paints its own
GPU surface, and a `wry`/WebKitGTK view is a native child window, which commonly lands always-on-top,
z-fighting, mispositioned under fractional scaling, or invisible.

The brief time-boxes that question and treats **all three outcomes as success**: it composites (then
`F-BRW-01..04` is a normal piece); it composites only as a separate top-level window (then `F-BRW-09`
changes shape and returns to the user); or it does not work (a real finding that returns the scope
question with facts). The one instruction that does not depend on the outcome: **fix the dead menu
entry**, because it is a defect under all three.

Sequencing after the spike: `F-BRW-01/02/03/04` (chrome, URL, Back/Forward/Reload/Stop, errors), then
`F-BRW-06/07/08` (Allow/Deny, persisted origins, revoke in Permissions) which needs a
`tiller_persistence` migration — that crate is at **v10** after P70, so the browser-grants migration
is **v11**.

---

## 22:30 — the largest bucket is measurably wrong, and the critic is the bottleneck

**FABLE-08 landed (`b24c62f`).** Of the 141 rows marked `FAILED — absent`, **39 are already
built**, with evidence verified by needle rather than line number and six known-stale control rows
that make the script refuse to print when they stop matching. Artefacts: `Scripts/stale-failed-census.py`,
`docs/linux-rewrite/STALE-FAILED-CENSUS.md`, and a pinned ledger snapshot under `pins/`.

That reframes the position. The headline number is 153 of 389 critic-confirmed, but the largest
bucket — absent — is now known to contain ~39 rows that are not absent at all. **Only pireview can
flip a verdict**, so those 39 are not progress yet; they are queued work behind a single agent.

**So the constraint is critic throughput, not builder output.** Five builders produce faster than
one critic can independently exercise, and the goal's definition of done runs entirely through the
critic. `FABLE-09` acts on that directly: turn the 39 into exercise recipes ordered by cost, so
whole groups close without relaunching the binary between rows. Recipes state **what to do, never
what to conclude** — a recipe that suggests its own answer converts the critic into a confirmer and
destroys the only independent check the project has.

### The identity chain has three links, not two

Twice this was scoped as "the `main.rs` consumer half" and twice it did not close, because the
broken link is not in `main.rs`:

1. `TabRecord.agent_id` — **exists** (P70, migration v10, round-trip test green)
2. `tiller/src/session.rs:286` `SessionTab` — **no `agent_id` field**; built at `session.rs:804`
   from a `TabRecord` whose identity is simply never read
3. `main.rs:6656`/`:6754` `restore_tabs` — `Chat::launch` and `agent_id: None`

The identity is written to SQLite correctly and dropped one layer before `main.rs` could use it.
Anyone reading only `main.rs` sees an identity-free `SessionTab` and concludes P70 never landed.
`P73` covers all three links plus `RetainedChat` (`main.rs:293`) for the resume path — all inside
codex12's own crate, so no seam and no hand-off. The test that counts round-trips a **non-default**
agent: restoring a Claude tab and finding Claude cannot fail, because Claude is what the bug produces.

### Dispatched

- **codex12 → P73** — the three-link identity chain, plus P71 Half B (three `eprintln!` → notice)
- **codex11 → P72** — the browser spike; the ownership block in that brief was written for another
  pane, so its "(codex11)" group is codex11's own files, not a prohibition
- **fable → FABLE-09** — exercise recipes for the 39, ordered by cost
- **pireview** — still inside the ACP end-to-end, now on a `q1-*` sequence with a shot after send
- **sonnet — blocked**, and not by us: `push it` sits unsent in its composer. Submitting it would
  push, which is outward-facing and irreversible, and it was typed to sonnet rather than to the
  orchestrator. `send-text` would destroy it. It stays untouched until the user decides.

### What the pass-14 frames settled

Two chats from the same binary in the same session report **different models** — `idle · Sonnet`
against `idle · GPT-5.6-Luna`. Those names come from the ACP server that answered, so the
per-agent launch reaches a real agent; the old hardcoded default would have made both identical.

And they settled what is still open, by md5 rather than by eye: `n1-02` through `n1-05` are
byte-identical across the whole 15s→30s window, `m2-02` through `m2-final` likewise, with the
composer still showing its grey `Message…` placeholder and status `idle`. Connection and identity,
yes. Message out and reply streaming, **never witnessed**.

Note the reading discipline: in the *menu* frames byte-identity was the signal of **success**
(opened and stable). In a streaming window it is the opposite. Same observation, opposite meaning,
decided by context — which is why the frames get read rather than the filenames.

## 22:45 — the composer's focus is mouse-only, and that may be the whole ACP blocker

Read statically, in `chat.rs` (pi's — read only, nothing edited):

- The full input path **exists and is thorough**: `composer_focus` is a real `FocusHandle` with
  `tab_stop(true)`, `on_composer_key` is wired at `:3078`, character insertion runs through
  `key_char` at `:1447-1452`, `enter`/`return` → `Send` at `:551-552` in the `ChatComposer` key
  context, `shift-enter` → newline. This is not an absent feature.
- **The composer takes focus in exactly one way**: `.on_mouse_down(MouseButton::Left, …)` on its own
  div at `~:2859`, calling `composer_focus.focus(window, cx)`. That is the only call site in the
  file. Keystrokes only land inside the `ChatComposer` context, i.e. only once that focus exists.
- The house tests agree, and say so in their own naming: the helper is `focus_and_type`, and it does
  `debug_bounds("composer")` → `simulate_click(composer.center())` → `run_until_parked()` →
  `simulate_input(text)`. **Even the suite does not assume focus; it clicks for it.**

That is a mechanism consistent with the pass-14 frames — chat tab opened, typing sent, placeholder
`Message…` never cleared, status `idle`, frames byte-identical. **It is not a verdict.** Sent to
pireview as a place to look, explicitly not as a conclusion, because the last time a code branch was
read and a UI behaviour inferred from it (GAP 2) the inference was wrong and cost two agents a
correction.

Two different things fall out, and they must not be merged:

1. **If a click into the composer unblocks it** — the gap is that a freshly opened chat does not put
   the caret in its input, which every agent CLI does. That is a real finding and it is **pi's**, as
   the owner of `chat.rs`. It is a UX gap, not a crash, and only the critic may name it as a row.
2. **If typing still does not land after an explicit click** — the cause is upstream of anything
   read here, and this note explains nothing. Say so; do not stretch it to fit.

### Named, unverified: five mac-only chords in the chat composer

`chat.rs:562-568` registers `cmd-a` (SelectAll), `cmd-c` (CopyTranscript, in both the transcript and
composer contexts), `cmd-left` (Home) and `cmd-right` (End) with **no `ctrl-` counterpart**, while
the workspace registers 22 `ctrl-` bindings elsewhere — so the codebase plainly knows to do this and
these five did not get it.

**This is a candidate, not a defect**, and the distinction is the point. `on_composer_key` also tests
`event.keystroke.modifiers.platform` directly (`:790`, `:1388`), so if GPUI maps `platform` to
Control on the Linux backend, Ctrl+C reaches the handler regardless of what the `KeyBinding` string
says, and there is no gap at all. **What settles it:** press Ctrl+A / Ctrl+C / Ctrl+Left in the chat
composer on the running Linux build and report what happens. Until someone does, this row is a
question, not an accusation — filing it as a defect would be the same error as GAP 2 wearing a
different hat.

## 22:55 — "the gate stops on unrelated tests" is unfalsifiable as stated, and half of it is implausible

Five agents have now reported, in almost the same words, that `Scripts/ci-linux.sh` gets past fmt
and clippy and then *"stopped at workspace tests on unrelated tiller/panes and palette tests"*. It
has been repeated all day and **never once verified**, because each report names a **file**, not a
**test**. Read the file and the claim splits in two:

- **Pure logic tests** — `splitting_preserves_the_original_leaf_and_adds_a_focusable_leaf`,
  `splitting_the_focused_pane_down_preserves_both_panes`, `focus_movement_returns_the_neighbour…`,
  `removing_a_leaf_collapses_the_parent`, `ratios_are_clamped_and_survive_nested_splits`,
  `tab_cycle_wraps_forward_and_backward`, `jumping_to_a_tab_uses_one_based_positions…`. Synchronous
  `#[test]`, in-memory tree, assertions only, no clock, no fs, no threads. **Load cannot make these
  fail.** If one of them is red, something is genuinely broken and "concurrency noise" is a wrong
  attribution that has been protecting a real defect all day.
- **One real-PTY end-to-end test** — `panes.rs:806`
  `real_pty_activity_status_follows_osc_title_then_settled_content`: async, spawns an actual PTY,
  `sleep 0.1` / `sleep 0.2` inside the shell command, `Instant::now()` deadlines at `:770`/`:806`,
  temp dir under `std::env::temp_dir()`. **This one can absolutely be load-flaky**, and with five
  builders compiling it usually is. Here the attribution is fair.

So the sentence is simultaneously true and useless: it is correct for one test in the file and
almost certainly wrong for the other seven, and as written nobody can tell which happened.
`FLAKY_TEST_FINDINGS.md` (2026-08-09, Swift suite) recorded the same shape — *"standalone runs green
(41/41)"* against *"11 fail/12 runs under load 7-8"* — so load-induced failure is a documented
reality in this project and is not a smell being invented here.

**Standing reporting rule, effective now — add it to every brief:** when the gate is red, name the
**failing test**, not the file, and paste the assertion line. "unrelated tests in `panes.rs`" is not
a report; `real_pty_activity_status_follows_osc_title_then_settled_content` timed out at 2s under
load 9" is. A file name lets a real failure hide behind a flaky neighbour indefinitely — which,
given that this has run all day, may be exactly what happened.

**Why this is not being dispatched as a task.** Settling it needs the gate run in a **quiet tree**,
and the tree has not been quiet once today; running it now, with five agents compiling, would
reproduce the very load that makes the honest half of the claim true. It is a scheduling
constraint, not a work item: run it in the first genuine idle window, with the failing test named.

## 23:10 — the composer focus finding, corrected and sharpened

The earlier note said the composer's only focus path is its own `on_mouse_down` in `chat.rs`. **That
phrasing was too narrow and, read literally, wrong** — `main.rs` can focus it: `Chat` implements
`Focusable` returning `composer_focus` (`chat.rs:3040`), and `select_pane` (`main.rs:4339`) resolves
that handle and calls `window.focus(&focus_handle, cx)`. The route exists.

**The real finding is stronger.** The *creation* path never takes that route, and cannot:

```rust
fn add_chat_tab(
    &mut self,
    adapter: Option<&dyn tiller_agents::AgentAdapter>,
    cx: &mut Context<Self>,          // main.rs:3934 — no Window parameter
```

GPUI focus requires `&mut Window`. `add_chat_tab` has none, so it is not a branch that forgets to
focus — **the signature makes focusing impossible**. It sets `focused_pane`, which is internal
bookkeeping, not focus. All four `select_pane` callers (`:4374`, `:4474`, `:4560`, `:4690`) are click
or keyboard paths; none runs on creation.

So a freshly opened chat tab has no caret in its composer until the user clicks. That is consistent
with every pass-14 frame: tab opened, keystrokes sent, `Message…` never cleared, status `idle`,
frames byte-identical.

**One-shot check, sent to pireview:** open a new chat and type *without* clicking — the placeholder
must persist. Then click into the composer and retype. If the second attempt lands, the gap is named
precisely and belongs to `pi` (owner of `chat.rs`): *a newly opened chat does not put the caret in
its own input*, which every agent CLI does. If typing fails even after the click, this explanation is
wrong and must be discarded rather than stretched to fit.

**Why this correction is recorded rather than quietly fixed.** The first version was a claim about
a file that got generalised into a claim about the app — the identical shape as GAP 2, where a code
branch was read and a menu inferred from it. Catching it one message later instead of one pass later
is the only difference, and that difference came from re-reading my own assertion rather than from
anyone challenging it.

## 23:25 — the construction backlog is 100 rows, and half of it sits behind one builder

Measured, not estimated: the ledger's `FAILED — absent` bucket minus the rows FABLE-08 proved are
already built. The extraction self-checks — pulling the `already built` rows out of the census's
generated table yields **39**, exactly the count the census states for itself, so the parse is not
silently dropping rows. (A first, naive attempt grepped every row id mentioned anywhere in the doc
and returned "0 of 131 still absent". Absurd on its face, which is why it was safe: an obviously
wrong number gets checked, a plausible wrong number gets published.)

**100 rows still to build**, by area:

| area | rows | owner |
|---|---|---|
| `F-PRJ` | 17 | pi (`sidebar.rs`) |
| `F-CHAT` | 15 | pi (`chat.rs`) |
| `F-SET` | 10 | pi (`settings.rs`) |
| `F-SID` | 6 | pi (`sidebar.rs`) |
| `F-BRW` | 9 | codex11 — spike in flight |
| `F-TAB` | 7 | codex12 (`tab_bar.rs`) |
| the rest | 36 | spread thin: `F-USE` 4, `F-WIN` 3, `F-TERM-PTY` 3, `F-TERM` 3, `F-CORE-FILE` 3, `F-CORE-ACT` 3, … |

**`pi` owns 48 of the 100.** The construction side has the same shape as the adjudication side: one
agent holding the bulk while others run dry. `F-CHG` and `F-EDIT` have dropped out of the top
entirely — most of codex11's absent rows turned out to be the stale ones, which is why codex11 keeps
finishing early.

Two levers, both the user's call rather than a mid-flight re-cut of the ownership map — that map is
the only reason five builders have run all day without a collision:

1. **Unblock `sonnet`.** It is idle behind an unsent `push it` and already owns the visual layer
   (`tiller_theme`, `controls.rs`, `titlebar.rs`, `composer.rs`). `F-SET` (10 rows) is the natural
   transfer: its COSMIC pass already touches that surface.
2. **Split one of pi's four surfaces.** `settings.rs` and `sidebar.rs` are separable from `chat.rs`,
   where pi is currently working (F-CHAT-24/25/26/27 — pending-question bar, text answer and cancel,
   expired-question state, Plan card).

### Worth checking, not acting on: pi is running a flash model

`pi` shows `deepseek-v4-flash`; `pireview` shows `deepseek-v4-pro`. If that is deliberate — many
small rows, cheaper and faster — fine. If it is the silent reset already recorded in memory (a fresh
context reverts a pane to the global default and the stronger model is lost without any notice),
then the builder holding **48 of the 100 remaining rows** is doing GPUI work with drawn tests on the
weaker model, and nothing anywhere would report it. pi's context is at 7.2% (`↑31k ↓8.7k`, $0.006),
i.e. freshly reset — which is exactly when that reset happens.

---

## 23:55 — The gate's red was misattributed, and the build was down for everyone

Three findings, all from running the gate's own commands instead of paraphrasing them.

### 1. The clippy red does not exist. It was mine to check and I had accepted it on report.

`pireview` reported clippy red at `tiller_theme/src/cosmic/live.rs:111` and `cosmic/theme.rs:88`,
and I told it I would fix them. Running the gate's literal stage-175 invocation:

```
cargo clippy --workspace --all-targets --exclude tiller --exclude tiller_ui -- -D warnings
→ EXIT 0, zero warnings, zero errors
```

Scoped to `tiller_theme` alone: also clean, and the crate is committed at `ce02a06` with no
uncommitted drift. **Those two `file:line` pairs come out of no failing stage.**

The generalisable mistake is worth more than the correction. `fmt` and `clippy` are **separate
stages** (`ci-linux.sh:169` and `:175`), and clippy **excludes `tiller` and `tiller_ui`**. A red
reported as `file:line` with no command attached cannot be assigned to a stage at all — and a file
that clippy never lints will still happily produce a *fmt* diff at some line number. Standing rule
from here: **name the stage and the command, never just the file.**

### 2. The one real fmt failure, and it is a single file

`cargo fmt --all -- --check` → EXIT 1. Sole offender in the entire workspace:
`tiller_ui/src/browser.rs`, 8 diffs, all mechanical (import order, `use` collapsing, match nesting).
`codex11`'s in-flight P72 spike. Routed to `codex11`, not fixed by me — it is being edited live, and
formatting another agent's open file is the collision the ownership map exists to prevent.

### 3. P72's dependency took the whole product build down, not just its own surface

`cargo build -p tiller -p tiller_control` (stage 178) → **EXIT 101**: `gdk-3.0`, `atk`, `cairo`,
`pango` all missing. `wry`/WebKitGTK drags in the GTK3 stack, and without the system headers
**nothing in `tiller` compiles** — so `codex12` finished P73 and could not run a single one of its
tests, and `pireview` could not launch the app.

Fixed at the machine level, which is orchestrator work and no agent's file:
`libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev`. Same command now → **EXIT 0**.

`codex11` had been working around it with a private `spike_sysroot` and a custom `PKG_CONFIG_PATH`.
Told to drop it: a workaround inside `PKG_CONFIG_PATH` is a hidden prerequisite that breaks the next
machine without telling it why. Told to report the system requirement as a **spike result** — the
cost of outcome 1 is not only z-order, scaling and input routing, it is that the product no longer
builds without system GTK headers, and that belongs next to `ci-linux.sh:17`'s existing cargo check.

### The shape this session keeps producing: written, never executed

P73 widened all three links (`session.rs:286` `SessionTab.agent_id`, `RetainedChat`, restore and
resume) and added `drawn_restore_round_trips_codex_identity_through_quit_and_relaunch` and
`drawn_tab_context_resume_chat_reopens_the_retained_session`. **Neither has ever run** — the build
was down when they were written. A test that has never executed has the evidential weight of a
comment. `codex12` is now running them, and has been told to **reintroduce the regression on purpose**
(`agent_id: None` back in the restore arm) and confirm the test goes red. On this exact chain we have
already been wrong twice; "the test passes" and "the test can fail" are different claims.

`fable` closed FABLE-09 at `66b16d0` — `STALE-FAILED-RECIPES.md`, 39 recipes costed into two launches
(30 + 7) plus 2 not UI-exercisable, which is what `pireview`'s pass 15 is consuming.

---

## 00:40 — The critic's "broken right-click" was the harness, and a headless critic is not possible

### The 12 rows that were about to become false negatives

`pireview`'s pass 15 reported *"all right-click recipes are dead on this display — XTEST button 3
produces no state change"*, plus keys landing only after a real click, plus `ctrl-o` opening a portal
*"invisible to X"*. Twelve rows were one pass away from being recorded `FAILED` against working code.

**The session is Wayland**, measured not assumed:

```
XDG_SESSION_TYPE=wayland   WAYLAND_DISPLAY=wayland-1   DISPLAY=:1
```

`DISPLAY=:1` is XWayland. XTEST under XWayland is not a general input injector — it reaches X clients
only, and XWayland restricts synthetic events that would otherwise leak into the compositor. That one
fact explains all three symptoms at once, including the "invisible" portal, which is a native Wayland
surface and was never going to appear in an X screenshot.

**None of it is a product defect.** The fix routed to `pireview` is its own pass-15 discovery applied
one step further: if *keys* need a real click to give the app X focus first, try the same before
button 3. One line, up to 12 rows.

### The headless critic: tried, does not work, do not retry

The obvious next thought is a virtual X display, where XTEST is unrestricted and runs unattended.
It does not work, and the wall is the same for both servers:

| attempt | result |
|---|---|
| `Xvfb :99` + app | window created (`0x200001`, 1470×833), **paints nothing** — screenshot has 1 unique colour |
| `Xvfb` + forced lavapipe (`VK_ICD_FILENAMES=lvp_icd.json`) | fatal presentation error clears, still blank |
| `Xephyr :99` (a real nested X server) | identical — `vulkan: No DRI3 support detected - required for presentation` |

**GPUI/blade requires DRI3 to present, and neither `Xvfb` nor `Xephyr` provides it.** The app launches,
opens a window and serves its control socket on the virtual display — it simply never draws. So the
critic stays on the real display, and its evidence stays dependent on a live session. Recorded here so
nobody spends another hour proving it twice.

One shell hazard found on the way, worth avoiding: `pkill -f "target/debug/tiller"` also matches the
`bash -c` wrapper whose command line *contains* that string, and kills the shell running it. Use
`pkill -x tiller`.

### opencode installed; omp is a name mismatch, not a missing install

`opencode` 1.18.18 is installed and runs — `F-AGENT-OPENCODE-01/02/03` are exercisable, three rows off
`UNREACHABLE`. **`omp` is a different story.** The npm package `omp@1.0.0` has the description
`"new"` — a squatted placeholder, not installed. The real project is `oh-my-pi`, and it ships exactly
one binary, `oh-my-pi`, with no `omp` alias anywhere in the package. `tiller_agents/src/omp.rs:35`
hardcodes `"omp"`.

No symlink was made to paper over that. If our adapter looks for a binary the distribution does not
provide, `F-AGENT-OMP-01/02/03` are not `UNREACHABLE — not installed`; they are defective for the
wrong name. Routed to `pireview` to verify rather than asserted here.

### State at handover

P73 is **closed and proven** — `codex12` ran all four tests green *and* reintroduced `agent_id: None`
to confirm the round-trip test genuinely fails at `main.rs:9749`, then restored it. That is the
distinction between "the test passes" and "the test can fail", on the one chain we had already got
wrong twice.

`cargo test --workspace` is red at this instant, and it is `codex11` mid-widening, not a regression:
it added `title` to `ChatEntry::Permission` and has not yet closed the construction sites at
`tiller_acp/src/chat.rs:326` and `tiller_persistence/tests/persistence_integration.rs:591`, and
`serde_json` is used at `tiller_acp/src/lib.rs:1057` without being declared in that crate's
`Cargo.toml`. The exact list was handed to `codex11` rather than filed as a gate failure — the same
judgement as `browser.rs`'s fmt an hour earlier. **The second site is the one that gets forgotten**,
because `tiller_persistence` is "finished": the widening rule says otherwise.

All six panes are working: `pi` F-CHAT, `sonnet` P76 then P75, `codex11` P74, `codex12` the broken
tiller-bin cluster + the dead browser menu entry, `pireview` pass 16, `fable` FABLE-11.

### Autonomous call: pass-15 frames published to the existing artifact

The user asked earlier to see how the app was coming along and went to bed under a standing
instruction to take the recommended action rather than ask. The pass-15 captures were the first
evidence good enough to answer that question, so they were published — **updating the existing
artifact in place** (`claude.ai/code/artifact/6093d731…`) rather than creating a second link, so
there is one URL for this subject and not a trail of them.

Nothing left the machine except that page: private by default, no repository contents, no
credentials. Not a push.

Two claims were checked before publishing rather than after, both worth keeping:

- "up from nothing on 6 August" was invented. `git log --reverse -- rust/` dates the first Rust file
  on this branch to **12 August**, which is both true and a better fact — the app in those frames is
  two days old.
- The verdict split on the page is the parsed ledger (188/389), not a remembered number.

The strongest frame on the page is the one that shows a **defect**: `k3-01-running.png` and
`k3-02-done.png` are both exactly 44550 bytes. Two byte-identical captures, one taken while a command
ran and one after it exited, are the evidence for `F-TERM-PTY-05`'s missing activity signal. A
reviewer looking at either screenshot alone sees nothing wrong, which is exactly why that row
survived this long.

## Rulings and corrections, late 13 Aug

### I asserted the icon format from a sample instead of measuring it

`ATTRIBUTION.md` and P76 both said all 63 comet icons share `viewBox="0 0 16 16"` at
`stroke-width 1.25`. **Measured: 52 are `0 0 24 24` at `1.5`, four are `0 0 16 16` at `1.25`, and
seven are marks with arbitrary viewBoxes — three of those non-square.** `sonnet` caught it by
diffing two icons rather than trusting the brief, which is the behaviour that should be rewarded.

It matters because the four small-grid icons are `check`, `close`, `plus`, `terminal` — and three of
those are in the mapping table, so they land in the new titlebar. Scaled into one 16px box, `1.5` on
a 24 grid gives `1.0px` and `1.25` on a 16 grid gives `1.25px`: **~25% heavier, right next to each
other.** The brief that warned against re-creating the Phosphor/Solar mismatch was itself the recipe
for re-creating it inside the new set. Both docs corrected in `fd7fd4b`; `stroke-width 1.0` on the
four is the one-attribute fix.

### FABLE-11 corrected the backlog arithmetic — 102, not 92

The brief told `fable` to derive the target as 131 − 39. That assumed all 39 census-built rows were
still inside the 131; **fourteen had already left** (8 PASSED, 5 half-proven, 1 turned defective).
Live count at cut time was 127 absent, and 25 built-but-absent rows belong to the critic rather than
a builder. **Target = 102 rows → 52 construction pieces + 4 defect pieces.** It re-measured instead
of inheriting the number, which is the second time tonight that has paid.

### Ownership rulings requested by FABLE-11

- **`tiller_agents/**` → `codex11`.** P69's line giving it to `pi` is stale. `codex11` already owns
  `tiller_acp/**` and has just worked inside it; adapters and transport are one subsystem. B-04 and
  B-10 cut with the agents half to `codex11`, the `chat.rs` half to `pi`.
- **`tiller_usage/**` → `codex12`.** Usage is session-scoped and `session.rs`/`main.rs` are already theirs; it is
  also the lightest backend load of the three. Unblocks B-70 and B-62's backend half.

### The two no-piece rows, checked against the macOS source rather than argued

Both were "a builder decided this is out of scope". The reference app is the only arbiter, so I
looked, and **they went opposite ways** — which is the argument for looking.

- **`F-SID-16` / `F-SID-17` — the builder was right.** No `onMove`, `.draggable`, `dropDestination`
  or `NSItemProvider` anywhere in the Swift sidebar (the only `onMove` hits are a `fractionMoved`
  local in `WorkspaceSplitController`). **macOS Tiller has no sidebar drag-reorder.** These are not
  absent features, they are **bad inventory rows** — and the right verdict is "no referent", not
  `FAILED — absent`. That is a verdict change, so it goes to `pireview`, not into the ledger by my
  hand.
- **`F-TAB-25` — the builder was wrong and it gets built.** `App/SidebarView.swift:717` is literally
  `Button("Attach to Current Terminal") { model.adoptPane(paneId) }`. P65 refused it on its own
  authority after building Move-to-Pane, a different feature. Builders do not narrow scope; features
  stay Tiller's. Cut as a **seam** — Half A `codex12` (adopt on the model), Half B `pi` (the sidebar
  menu entry that calls it).

**The general shape:** two rows sat unbuildable for days because nobody asked whether the reference
had the feature at all. `fable` is now auditing the whole inventory for that class (FABLE-12) —
including the 22 `N/A — platform` rows, in reverse, since some may be perfectly feasible on Linux and
archived out of convenience.

### P72 is answered, and it changes what the browser can be

`codex11` produced the overlap frame:
`reference/linux-progress/p72-browser-spike-xlib-bridge-overlap.png`. **WebKit renders above GPUI,
always.** So the in-app browser cannot be a tab inside the window — any GPUI element that should sit
over it lands under it. Routed to `codex12`: the dead **New Browser** menu entry's honest destination
is a window, not an in-tab surface.

### P74 closed with a falsifiable proof

`codex11` showed RED-before / PASS-after on the same thread-affinity test rather than only the green
run, and measured the reap on real processes (ACP children 1 → 0). Same discipline `codex12` used to
close P73. That is the distinction between "the test passes" and "the test can fail".

### codex12 lost twenty minutes to an environment fact already in this log

`cargo: command not found` — `~/.cargo/bin` is not on the codex pane's PATH, though
`~/.cargo/bin/cargo` exists as a rustup symlink. I hit this myself earlier tonight. Unblocked with
the absolute path. **Environment facts belong in the brief, not in this file only** — a builder does
not read the orchestrator's queue before running its first command.

### An unowned file is why three defective rows had no assignee

`rust/crates/tiller/src/panes.rs` appears in **nobody's ownership map** — not in the WORK-BREAKDOWN
table, not in any brief. Three `FAILED — defective` rows (`F-CORE-ACT-06`, `-07`, `-11`) are
defective *solely because their cited tests fail*, reproducibly across pass 14 and pass 16 with the
file untouched in between, and none of them were on anyone's board. Assigned to `codex12` —
same crate as `main.rs` and `session.rs`, which it already holds.

That failing test is also the likeliest reason **`Scripts/ci-linux.sh` has never printed `CI OK`**: a
stably-red test keeps `cargo test --workspace` red no matter what is in flight. Earlier tonight the
gate's redness was attributed entirely to `codex11`'s mid-edit enum widening. That was one cause, not
the cause — a second, older one was sitting underneath it the whole time, and attributing a symptom
to the first plausible cause is the exact mistake this log already records once.

**The hypothesis handed over, explicitly as a hypothesis:** `panes.rs:606` ends with

```
let _ = refresh_process_signal(&mut activity, "pane-process", std::process::id());
assert_eq!(activity.status("pane-process"), Some(AgentStatus::Running));
```

It passes the **test binary's own PID** as the shell PID and then requires the status to survive. The
test process has no `codex` child, and `CLAUDE.md` says process-owned status clears **only** on
`processGone` — which a child scan finding no agent *is*. So clearing may be correct and the
**assertion** may be the thing that is wrong.

Two worlds, and they lead opposite ways: if the code is wrong the rows are genuinely defective and
get built; **if the test is wrong, the code is healthy and three rows are mis-verdicted** — they go
back to `pireview` for a new verdict, not to a builder for construction. `codex12` was told to make
the test fail and pass under its own hand before deciding which, and told not to mark the rows
itself even though the change would flatter us. Only `pireview` writes a verdict.

### Ledger at this point

196 PASSED / 127 absent / 22 N/A / 15 half-proven / 13 defective / 12 not exercised / 3 unreachable /
1 builder-claimed = 389. Up 8 PASSED since 23:00, and `builder-claimed, unverified` has collapsed
from 8 to 1 — pass 16 is converting claims into verdicts rather than accumulating them.

### A new way to manufacture a false PASSED: the chord the code binds vs the chord a user presses

`chat.rs` still binds five shortcuts in macOS convention — `cmd-a` (SelectAll), `cmd-c` ×2
(CopyTranscript), `cmd-left`/`cmd-right` (Home/End). `codex12`'s own comment at `main.rs:6055` says
what that means here: *"GPUI's platform modifier is the cross-platform spelling of ⌘ on macOS and
**the Super key on Linux**."*

So on this machine those bind to **Super+A, Super+C, Super+Left, Super+Right**. A Linux user presses
Ctrl+A and Ctrl+C and nothing happens; Super+Left/Right never arrive at all, because the compositor
takes them for window tiling. That it is an oversight rather than a decision is settled by the rest
of the tree: **`panes.rs` carries 37 correctly-translated `ctrl-` bindings**. `chat.rs` is the
untranslated remainder.

**The generalisable danger is the verification, not the binding.** The critic drives with XTEST. If
it exercises *the chord the code binds* rather than *the chord a Linux user would press*, it can mark
a row PASSED that no human can reach. The right test for copy is not "Super+C copies" — it is
**"Ctrl+C copies"**. This is a false-positive shape none of the previous ones covered: not a dead
control, not a stale verdict, but a live control reachable only through a door nobody opens.

Routed to `pi` (owns `chat.rs`) with the fix — `ctrl-a`, `ctrl-c`, and delete the two `cmd-` arrows
outright since plain `home`/`end` are already bound beside them. Affects at least `F-CHAT-29`, which
is already in `fable`'s recipe set and therefore about to be exercised.

### I verified P72's z-order myself, and the conclusion needs one qualification

I had relayed `codex11`'s "WebKit stays above GPUI" to two other agents on the strength of its report,
so I read the evidence rather than keep passing it on. **It holds, and the spike is well built.**

`browser.rs`'s `BrowserSpike` renders a marker that is a **sibling of the sidebar** on a full-width
`.relative().flex_1()` container — `left: 260`, `width: 520`, so it spans x ∈ [260, 780] while the
`build_as_child` webview starts at x = 336. Its label is `"GPUI → WebKit"`. The capture shows only
**"GPUI →"**: the 444px that should cover the web view, including the word "WebKit", is not drawn.
Because the container is full width, GPUI is not clipping it — the child X window is painting over
it. The spike's own comment says *"either result answers the z-order part of P72"*, which is why it
is worth trusting: it was built so both outcomes would be informative.

**The qualification.** What is proven is that a **child X window** (`build_as_child`) composites above
GPUI. That is a fact about this embedding strategy, not about web content in general, and B-02 is the
largest piece on the board — so the choice should be made with the alternatives named:

1. **Separate top-level window** for the browser. Simplest, and what was routed to `codex12` for the
   honest New Browser entry.
2. **Keep the child window, and make any GPUI chrome that must appear above it a separate X window
   too** (override-redirect popup). This is how real browsers put dropdowns over page content, and it
   preserves the in-tab surface. It is more work, not impossible work.
3. Offscreen-render the page into a texture and composite it inside GPUI — full control of z-order,
   but it sacrifices the interactivity that makes the feature worth having. Named for completeness;
   not recommended.

**Only option 1 was communicated.** Options 2 and 3 must reach `codex11` before B-02 starts, or a
nine-row architectural decision gets made by default rather than on purpose.

One process note: the spike's harness lives in `/tmp/tiller-p74-browser-be1wPZ`, but it is only 24
lines and `#[path]`-includes the real `browser.rs` from the repo — so the proof **is** replayable.
That is the right shape for a spike, and worth copying: put the logic in the tree and keep only the
launcher outside it.

### I was wrong about right-click, and the wrong answer was the expensive kind

Recorded because the shape of the error matters more than the fix.

Earlier tonight I diagnosed the critic's non-working right-click as an XWayland/XTEST restriction,
told `pireview` so twice, and wrote it into `ENVIRONMENT.md` **as an established fact**. Then I read
the harness instead of reasoning about the platform:

```
click() { xdotool mousemove --sync $((WIN_X+$1)) $((WIN_Y+$2)); sleep 0.2; xdotool click 1; sleep 0.4; }
```

`click 1`, hardcoded. **No `rclick`, no `--button`, no button-3 path anywhere in the script.** The
right-click was never being sent. It was not failing to arrive.

The evidence against my own diagnosis was in front of me the whole time: **button 1 travels that
identical route** — same `mousemove`, same XTEST — and lands in every frame the critic has captured.
XWayland does not discriminate by button for a focused X client. If one arrives, three arrives.

**Why this class of wrong answer is the expensive one:** "the platform forbids it" closes the avenue.
A restriction makes twelve rows permanently unverifiable and invites an `UNREACHABLE — platform`
verdict on each. A missing helper makes them a two-line fix. I picked the pessimistic reading, then
promoted it to a fact in the one file written specifically to stop unverified environment claims from
circulating. Being confidently wrong in the authoritative file is worse than being wrong in a
message, because everyone downstream inherits it without the doubt.

Fixed in `df76698`: an `rclick` helper mirroring `click` verbatim, and `ENVIRONMENT.md` corrected to
say the cause was unproven and probably false.

**Handed to `pireview` with two conditions**, because a helper the orchestrator wrote is not evidence:
verify it produces a real context menu before trusting any verdict from it; and if a genuine
delivered button-3 still does nothing, that is a **finding, not a restriction** — the app binds 22
`MouseButton::Right` handlers (7 `right_panel.rs`, 7 `sidebar.rs`, 5 `main.rs`,
3 `tiller_terminal/lib.rs`), so silence would be a defect worth a verdict.

**The generalisable rule: check for a missing helper before concluding a platform restriction.**
Verify the instrument can perform the action at all before concluding the subject cannot receive it.

---

## 2026-08-14, 00:15 — the roster lost two panes, one of them the critic

Both `pi` panes died within minutes of each other on the same account-level error:

```
429 {"type":"GoUsageLimitError","message":"Monthly usage limit reached. Resets in 10 days."}
```

It is not model-scoped: `deepseek-v4-flash` (`pi`, builder) and `deepseek-v4-pro` (`pireview`,
critic) both hit it. `~/.local/share/opencode/` holds no `auth.json`, so **no second provider is
configured** and neither pane can be moved to another model. The two exits are a paid-balance opt-in
on the user's account or ten days. **Not taken:** the user is asleep, and authorising spend on their
account is theirs to do, not mine.

**`pi` delivered before it died**, and the work is in the tree: `F-CHAT-24/25/26/27` (Plan card,
pending-question bar, text answer, cancel, expiry), an ACP fix worth keeping — the permission handler
was blocking the connection's single dispatch task, so a dead agent with a pending question hung for
five minutes, now fixed with a child-exit watchdog — and the `ctrl-` chord corrections. Its own gate
run was clean on its own crates.

### The consequence that mattered

The project's completion condition is written in terms of the critic: *done only when the full-app
critic ticks every inventory entry by exercising it live.* With `pireview` gone, verification stops
permanently, and 193 unproven rows can never become PASSED no matter how much gets built.

**The role moved to `fable`** (`tasks/CRITIC-pass17-handover.md`). This deviates from the letter of
the user's rule and is recorded rather than done quietly. It honours the rule's purpose: `fable` has
never written Rust in this project, so it is disqualified from judging nothing — a stronger
independence position than `pireview` held, which had at least authored ledger entries.

### A snapshot, before anything else

The working tree held **+19,297 lines across 61 modified files and 150 untracked**, including whole
source modules in no object database at all (`browser.rs`, `composer.rs`, `command_palette.rs`,
`tiller_acp/src/chat.rs`). This repo's own history contains *"recover Rust/GPUI rewrite after local
git object database loss"*. Two panes had just died and three more were mid-edit.

Snapshotted to `../tiller-snapshots/` as a zstd tarball including `.git` — 26 MB, 1911 files,
verified readable. **Deliberately not `git stash` and not `git add`**: five agents share one worktree,
and touching the index would have polluted the next `git commit` any of them ran. A tar file is
invisible to them.

### `pi`'s files, and one deliberate choice

`sidebar.rs` went to `codex12` rather than to `sonnet`, which owns the rest of the `tiller_ui` chrome
and was the obvious home. `F-SID-16`/`F-SID-17` (sidebar reorder) and `F-TAB-18` (tab reorder) are
**one drag primitive**, and `tab_bar.rs` is already `codex12`'s; split across two owners it gets
written twice. `chat.rs`/`status_bar.rs` → `sonnet`, which closes the `composer.rs`/`render_composer`
seam. `tiller_markdown/**` → `codex11`, which already consumed it.

## The ruling I retracted, and the rule it yields

`F-SID-16`, `F-SID-17` and `F-TAB-18` stay `FAILED — absent`. I had queued a change to "no referent"
with `pireview` after grepping `SidebarView.swift` for `onDrag|onDrop|ReorderScope` and finding
nothing. `fable` disproved it in `FABLE-12` by searching from the other end: `App/RowReorder.swift`
is the complete machinery, wired at `SidebarView.swift:50`, `:55`, `:600` and `TabBarView.swift:221`.
The call sites read `.reorderable(model:id:scope:)` — a project-local extension method — so the
orthodox vocabulary sits **one indirection away and my grep could not match by construction.**

The ledger was never corrupted; `pireview` died before applying it. But the evidence column on those
rows still reads *"drag reorder removed by design"*, which is false — nothing was removed and no such
decision was taken. An evidence string asserting a decision nobody made is worse than an empty one,
because it closes the work instead of flagging it. `fable` will correct it.

**The rule: a search that finds nothing is a fact about the query, not about the code.** Search from
the referent's side — start at the file that would implement it and ask what cites it. This is the
second time in one night I promoted "my probe found nothing" to "the thing is not there"; the first
was concluding XWayland forbids right-click when the harness had no button-3 path. Same shape,
different instrument.

## The denominator stays 389

`FABLE-12` measured zero rows without a referent (149 checked), and found the inventory never covered
the ACP subsystem at all — proposing +14 rows for it, plus 17 more it called disputable. The 14 are
real and accepted as a finding. **The headline denominator does not move**: the user pinned 389 and
is asleep, so the ACP rows go into a marked appendix and progress is reported as "N/389, plus 14
newly-found ACP rows not yet in the denominator". Both numbers visible, the call left to them.

## 00:45 — the browser architecture, decided by counting instead of arguing

`codex11` was told not to choose an architecture until it had answered one measurable question:
**how many of the nine `F-BRW` rows actually need GPUI chrome above the webview?** The webview is a
native child window, so it sits above GPUI's GL surface and cannot be drawn over — proven by the P72
capture where an accent-coloured marker reads "GPUI →" with the rest occluded.

Its count, row by row from `01-inventory-app.md:122`:

| needs chrome above WebKit | rows |
|---|---|
| no | `F-BRW-01`, `-02`, `-03`, `-04`, `-05`, `-07`, `-08`, `-09` |
| yes | `F-BRW-06` — the Allow/Deny permission dialog |

**One row out of nine.** It also drew a distinction worth keeping: `F-BRW-05` would need an overlay
only if we chose a *floating badge*, and the inventory requires the indicator, not that position.
Separating "what the inventory requires" from "one way to draw it" is exactly where architectural
decisions get made by accident.

### What one row does to the three options

It kills two of them. Option 3 (offscreen texture) rewrites the render pipeline and re-forwards input
by hand — for one dialog. Option 2 (override-redirect popup) adds a second GPUI surface that must
track the parent through every move and resize — also for one dialog.

**A fourth option neither of us had named is the right one: don't overlay `F-BRW-06` at all.** Real
browsers do not put permission prompts over page content — Firefox and Chrome anchor them *below the
address bar*, in browser chrome. That area is GPUI's, above the webview's rectangle rather than above
its pixels. Put the prompt there and the count goes to **zero**, option 1 wins with no compromise,
and the working spike is untouched.

Sent with two real conditions, not formalities: check what the macOS reference actually does first —
if it is a modal sheet over the page, say so rather than adapting the reference to the convenient
answer — and if the doorhanger fails, the fallback is hiding the child webview while the dialog is up,
not option 2.

**The generalisable move: when an architectural choice looks expensive, count how many cases actually
need the expensive property before comparing designs.** Eight of these nine rows never needed the
argument at all.

## A note on ownership, logged rather than litigated

`codex11` edited `tiller/src/panes.rs`, which `OWNERSHIP.md` assigns to `codex12`. The edit is
**correct** — the failing test passed `std::process::id()` and demanded the pane stay `Running`, but
`refresh_process_signal` walks that pid's *children*, the test process has no agent child, so
`processGone` and a cleared status is the right behaviour under `CLAUDE.md`'s process-owned rule. It
asserts `None` now, and a real e2e test below it spawns an actual child.

It stays. But it nearly cost real time: `codex12` had just been dispatched to investigate that exact
test and would have redone it, or worse, reverted it. Both panes have been told. **The rule stands —
say so before entering a file that is not yours.**

## 01:05 — a second reason the gate has never printed CI OK

`Scripts/ci-linux.sh` fingerprints every `rust/` file at line 120 and again at line 321, and fails
the run if anything changed in between:

```
FAILED: new Rust worktree drift appeared during the gate
```

The intent is right — a verdict about a tree that moved underneath it is worth nothing. But this is a
**shared worktree with three builders writing continuously.** Measured at 01:05:

| window | `.rs` files written |
|---|---|
| last 5 minutes | 4 (`main.rs`, `sidebar.rs`, `settings.rs`, `panes.rs` — three different authors) |
| last 15 minutes | 5 |

A `cargo test --workspace` run spans minutes. At that write rate the fingerprint window almost never
stays still, so **the gate is unpassable by construction while the roster works** — and the check
sits at the very end, so a full expensive run is spent before it dies on the last line.

This matters because it is *independent* of the five failing binary tests. Fixing those would not
have turned the gate green, and whoever fixed them would have been left hunting a phantom.

### The fix is a distinction, not a deletion

```
FAILED  = the code is broken. Go find the bug.
VOID    = the measurement is invalid because the tree moved. Re-run; there is nothing to find.
```

Recommended to `codex12` (which owns the gate tonight, so the file was deliberately **not** edited
under it): a distinct message, a distinct exit code (`75`/`EX_TEMPFAIL`) so callers can tell them
apart, and a list of *which* files drifted rather than a fingerprint diff, so whoever re-runs knows
whether the drift was theirs.

The script's own sccache comment already states the principle it is now violating: *an agent that
can never reach a green gate learns to ignore the gate.* That happened here for a different reason.
**A `FAILED` that actually means "try again" is worse than no gate**, because it sends people hunting
bugs that do not exist.

**Stated honestly to `codex12`:** this was measured, not observed. The write-rate is real and the
code path is plain, but nobody has yet watched the gate die on this specific line — so it was sent
as a diagnosis to confirm or refute, not as an instruction.

## 01:20 — the right-click question is closed, with a frame

`reference/linux-progress/p17-rclick-term.png`, captured by the critic through the new `rclick`
helper, shows the terminal context menu open: Copy, Paste, Set Title, Split Left/Right/Above/Below,
Clear, Close.

**Right-click works.** The XWayland restriction this project believed in for part of tonight never
existed; the harness simply had no button-3 path. Twelve rows are verifiable, `ENVIRONMENT.md` now
says so with the frame cited, and the claim has been corrected on the published progress page too —
where it had been stated as fact.

Worth recording how it was settled: the critic verified the helper **before** trusting a verdict
from it, having been told explicitly that a helper the orchestrator wrote is not evidence. That
instruction is what turned a plausible-looking two-line fix into a proven one.

### And the frame contains a second finding

Every long menu label is truncated at the same x — `Copy C…`, `Set Titl…`, `Split Le…` — while
`Copy` and `Paste` render whole. **The Files panel is painting over the context menu**, and the cut
falls exactly on its left edge.

Sent to the critic *before* it writes verdicts on those rows, because this is precisely the shape
that produces a wrong one: the menu opens, has the right items, and receives the click. Judging the
covered items as missing would mark `FAILED — absent` a surface that exists and responds, and send
somebody to rebuild what is already built. It is a **z-order defect and its own row.**

It also must not be merged with the P72 webview occlusion despite the resemblance. There, a native
child window sits above GPUI's GL surface and cannot be reordered — a constraint. Here both elements
are GPUI's and the paint order is ours — a bug. One is architecture, the other is a fix.

---

## 2026-08-14, 01:10 — the transplant rule got its first mechanical check, and it found two files

The goal says it without qualification: *"Da tutti i riferimenti si prende ispirazione, mai codice:
ogni riga di Tiller va scritta da zero. Se il critico trova codice trapiantato da un riferimento, è
un gap, sempre."* Nobody had ever checked it. `Scripts/transplant-check.py` now does.

**`rust/crates/tiller/examples/hw.rs` and `examples/bisect.rs` were Zed's `gpui/examples/hello_world.rs`,
117 of 119 lines verbatim** — same `HelloWorld` struct, same `rgb(0x505050)` on `rgb(0x0000ff)`
border, same `format!("Hello, {}!", self.text)`. The only edits were a window size and a stripped
`#![cfg_attr(target_family = "wasm", …)]`. Both came in with `c63378e`, the recovery commit, so no
current pane wrote them; nothing in the tree referenced them. **Deleted.** They had already done
their job — proving GPUI paints on this box — and that finding lives in `ENVIRONMENT.md`, which is
where it belongs.

### The first measurement said clean, and it was the wrong measurement

The check was run twice. The first pass counted **shared lines** and found ~2% overlap, worst case
13 of 701 in `chat.rs` — I read that as clean and said so. It was true and it was useless. A
119-line verbatim copy is a rounding error in a 145-file tree, and the aggregate hid it completely.

**When the rule is per-artefact, the metric has to be per-artefact.** The second pass counted
**consecutive** substantive lines, and that is the metric that holds up: two people solving the same
problem against the same API land on the same signatures, but they do not land on the same three
statements in the same order.

### The other 54 hits are convergence, and the test for it is not similarity

54 of the 56 candidates are noise, but *"they look alike"* is not why. The question is **whether an
alternative existed**:

- **`icons.rs`, 17 hits against `zed:img.rs`** — `type PrepaintState = Option<Hitbox>`, `fn id()`,
  `fn source_location()`, `fn request_layout(`. These are GPUI's `Element` trait members in the order
  the trait declares them. Every implementor emits this run. No alternative exists.
- **`chat.rs` against `waku:input.rs`** — the `request_layout` parameter list. Same reason.
- **`file_link.rs` against `waku:right_panel.rs`** — `b'0'..=b'9' => Some(byte - b'0')` and its two
  siblings. That is *the* hex-nibble match; writing it differently would be writing it worse.
- **`.file_name().map(…to_string_lossy…).filter(|n| !n.is_empty())`** in three of our files and in
  waku. Idiomatic Rust. Worth noting it appears three times **in our own tree** — that is
  duplication to fold up, a different smell entirely, and not a transplant.

### Keep the tool honest about what it cannot prove

`transplant-check.py` **exits 2 when `_tiller-refs` is absent** rather than passing. A check that
finds nothing because it looked at nothing must not report the same result as a check that looked
and found nothing. Precision is deliberately poor — 2 real in 56 — because the failure that matters
is the missed transplant, and every candidate gets a human read.

Run it before any claim that the tree is written from scratch:

```bash
Scripts/transplant-check.py            # 0 clean · 1 candidates · 2 references missing
```

---

## 2026-08-14, 01:45 — thirteen rows are one defect, and it has a mechanism

Counted across the ledger tonight: `F-PRJ-04`, `F-SET-09`, `F-SET-16`, `F-SET-22`,
`split_disabled_reason` behind `F-TAB-11`, `clone_repository` behind six `F-PRJ` rows, and
`F-CORE-ACT-24/25/26`, `F-CORE-DOM-03`, `F-CORE-WSP-04/08`, `F-CORE-FILE-06`. **Thirteen instances
of one shape: logic built, usually tested, reached by nothing.** It is comfortably the most common
defect in this port — larger than any cluster of genuinely unbuilt features.

It is not carelessness, it has a mechanism. A builder told to "implement X" writes X, writes a test
for X, watches both go green, and reports done. **Compiles, is tested, and is reachable are three
independent properties, and the first two are the only ones anything checks.** Nothing in the loop
ever asked the third question until the ledger was phrased from the user's side.

Verified independently rather than inherited, because the ledger was wrong in this exact way about
`F-SID-16/17`. Whole-workspace references against references inside `crates/tiller/src/`:

```
partition 2/0 · ids_to_evict 2/0 · LayoutCommand 14/0 · WorkspaceTabViewState 5/0
default_project_base 2/0 · FileSystemEventMonitor 4/0 · clone_repository 1/0
```

`LayoutCommand` is the sharpest: fourteen references, eight commands, a `classify` function, a green
suite, and the application has never constructed one. `clone_repository`'s single reference is its
own `pub use`.

### Why this makes them the cheapest rows on the board

Two briefs went out on it — **P77** (`codex11`, six `F-PRJ` rows on the unreachable clone backend)
and **P78** (`codex12`, the seven `F-CORE` call sites). Neither is a feature build. Both are wiring
jobs against code that already passes its own tests.

**P78 carries the warning that matters**, because the obvious fix produces the defect again: adding a
call that compiles is not the row. Before wiring, establish which of three cases each row is —
genuinely absent, already done by an ad-hoc path beside the tested one (then *replace*, do not call
both), or already satisfied by another route entirely (then say which, and wire nothing). Two code
paths computing eviction differently is worse than one dead one.

### The rule this suggests for every future brief

**State the observable consequence, not the symbol.** `ids_to_evict` called and discarded passes no
row; something must be evicted and the eviction must be visible. `WorkspaceTabViewState` "persists"
only if it survives a relaunch. A brief that names a function invites the thirteenth instance; a
brief that names what the user would see does not.

### Correction, 02:05 — the dead-control shape has a second mechanism, and it is the orchestrator's

The entry above blamed the builder's loop: writes X, tests X, reports done. That is true of some of
the thirteen. It is **not** true of the largest one, and the correction matters more than the count.

`codex11` finished a 1283-line browser tonight — `BrowserState` with address handling,
back/forward/reload/stop, navigation lifecycle, errors, permission allow/deny/revoke, agent-driving
and link routing, plus `BrowserSurface` with `Render`. Verified reachability:

```
BrowserState     0 references outside browser.rs
BrowserSurface   2 references outside browser.rs — both in crates/tiller_ui/examples/
```

**A browser no user can open.** But `codex11` did exactly what it was told: its report says
*"lib.rs, main.rs, settings.rs non toccati: seam d'integrazione lasciata all'integratore"*, and
`main.rs` is `codex12`'s file. Ownership discipline **required** it not to mount.

So the second mechanism is: **the orchestrator cuts a two-half seam and dispatches only Half A.**
Half A is a brief. Half B is one line in someone else's file, named in prose at the bottom of that
brief, and never dispatched. Nothing in this project tracked it — seams are named across P53, P67,
P69, P71, P73, P77, P78, FABLE-11 and a dozen QUEUE entries, in prose, with no place that says
whether the other half ever shipped.

`docs/linux-rewrite/SEAMS.md` now exists as that record, and `OWNERSHIP.md` points at it instead of
keeping a second stale copy — it had `add_chat_tab`'s `&mut Window` listed as waiting long after
`codex12` shipped it at `main.rs:4201`.

**The structural fact underneath:** `codex12` owns `main.rs`, and `main.rs` is where nearly every
Half B lands. The map makes one pane the integration bottleneck by construction while three builders
generate Half As faster than it consumes them. Batch the mounts into one integration pass rather
than paying a context reset and a build cycle per seam — and where a seam looks permanent, prefer
giving both halves to one owner, which is what killed the `chat.rs`/`composer.rs` and
`tiller_markdown`/`file_view.rs` seams for good.

---

## 2026-08-14, 02:40 — the roster was the wrong shape, because the constraint is verification

The stopping condition is not "the builders finished". It is *the full-app critic ticks every
inventory entry by exercising it live*. **Critic throughput is therefore the project's rate limit**,
and three builders against one critic was the wrong shape for it.

Measured rather than guessed:

- `fable` spent **44 minutes** on a plain-launch batch of roughly six rows — careful work, not slow
  work: re-capturing to defeat paint lag, verifying its own helper before trusting a verdict from it.
- In the same window the three builders shipped roughly **28 rows' worth** of work into the queue.
- `ADJUDICATION-BACKLOG.md` already lists **45 rows reachable with the route named**, awaiting
  nothing but exercise.

Adding building capacity to that does not move the finish line. **`sonnet` becomes the second critic
when P80 lands** — `tasks/CRITIC-2-second-critic-handover.md`. Builders drop to `codex11` and
`codex12`; critics rise to two.

The independence rule survives the change intact, which is the only reason it is allowed. The goal
says the critic must never be the agent that built the piece — it names `pireview`, but the binding
constraint is independence, not the pane. `sonnet` may not judge `F-SET-*`, `F-PRJ-13..16`,
`F-PER-07`, `F-USE-*`, or anything resting on the files it wrote; the handover lists them and says
that when in doubt it hands the row to `fable`. **A wrongly-claimed independence is worse than a slow
queue.** The two critics start from opposite ends of the backlog so they do not drive the same row.

### The bottleneck this does not fix

`codex12` owns `main.rs`, and `main.rs` is where nearly every Half B lands. It is now carrying P79
(nine `F-BRW` rows), P78 (seven `F-CORE` call sites) and P77's Half B (seven `F-PRJ` rows) — **23
rows queued on one pane** while `codex11` and `sonnet` generate more Half As.

Per `SEAMS.md`'s own advice the two mounts go as **one integration pass**, not two dispatches: they
touch the same files and cost one build cycle instead of two. The deeper fix — that the ownership
map makes one pane the integrator by construction — is recorded there and not attempted mid-flight.

## 2026-08-14, 03:10 — the codebase defends its own incompleteness

Sizing the browser mount for `P83` turned up two mechanisms that keep a feature dead, beyond the two
already recorded (a builder wiring nothing, an orchestrator dispatching only Half A). Both are worse,
because in both cases **the tree actively holds the gap open** and neither is visible from the ledger.

**A test that pins the stub.** `main.rs:9781` asserts each of the ten `BROWSER_METHODS` fails with
`"unsupported on Linux"`; `main.rs:9817` asserts capabilities advertise none of them. The suite is
green *because* the browser is absent. Wire it and two tests go red — which reads exactly like a
regression, and the obvious "fix" is to revert the wiring.

**The shell asserting the feature is impossible.** Three `unreachable!()` calls sit between a browser
tab and the screen — `tab_icon` (`main.rs:2156`), shell persistence (`:2803`), `tab_width` (`:3904`)
— one carrying the design claim *"Browser surfaces are external to the shell."* Half true: the page
pixels genuinely are a native WebKitGTK child window. The false half — that the *tab* is external —
panics on render, on layout and on save.

**And a correction to how this file has been sizing seams.** `SEAMS.md` estimated the browser mount
at "often a dozen lines" on the strength of a reference count of zero. That count was right and the
estimate was wrong: **a reference count tells you a seam is open, not what it costs to close.** The
same shape as the transplant metric — the aggregate measurement was true and hid the per-file fact.
Before sizing a mount, grep the consuming file for the variant's own name.
