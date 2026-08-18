# FINISH-leftovers — settings / activity / persistence leftovers (wf-rest3)

Fresh critic pass, lane label `wf-rest3`. Predecessor on this exact shard was killed by an API
error having committed nothing — this file starts clean. Binary pinned once from HEAD at the start
of this pass:

```
git rev-parse HEAD          # 960de6722aafac6b572f00872fc30b155841910a
cargo build --manifest-path rust/Cargo.toml --workspace   # exit 0, warm
cp rust/target/debug/tiller /tmp/wf-rest3-tiller
export TILLER_WL_BIN=/tmp/wf-rest3-tiller
```

Committing after every row per the brief. Rows below are filled in incrementally — a row present
here with a verdict is finished; anything still owed is listed at the end as NOT EXERCISED.

---

## F-SET-15 — select System default vs. a second account, active badge moves

**Confirming the orchestrator's reclassification, not re-deriving it independently — the brief
asked for exactly this.** Re-read `rust/crates/tiller_ui/src/settings.rs` fresh this pass:

```
$ grep -n "account_row\|selected_account\|active_account\|account_selection" \
    rust/crates/tiller/src/main.rs rust/crates/tiller_ui/src/settings.rs
tiller_ui/src/settings.rs:2445:            .child(controls::account_row(
```

Exactly one call site (`settings.rs:2445`), hardcoded `"System default"` / `true` — the whole
"Accounts" card is built from that single literal row, and `account_row`'s own definition
(`controls.rs:367`, `pub fn account_row(label: &'static str, subtitle: &'static str, active: bool,
theme: Theme) -> Div`) takes `active` as a caller-supplied constant, not a read from any selection
state. `grep -c 'selected_account\|active_account\|account_selection'` is 0 in both files. The
comment directly above the call site (`settings.rs:2440-2444`) confirms this is deliberate today:
"The 'Active' badge on the System default row is a *selection* marker... It stays true by
construction, not because anything was checked at render time." There is no second row to select
and no state that could move a badge even if a second row existed.

**Verdict: FAILED — absent.** Agreeing with the orchestrator's reclassification and not softening
it: the clause asks to select between System default and a second account and watch the active
badge move; there is exactly one hardcoded row and zero selection state anywhere in the settings
surface.

---

## F-CORE-ACT-20 — notification suppression follows real window focus, not a hardcoded true

**Reading the sibling's report before touching it, per the brief.** `CRITIC-waveK-builder-claims.md`
§2 (commit `86ae6b33`, ancestor of this pass's HEAD `960de672`) is a **critic** pass (`wf-judge`,
did not build the work), independently re-driving the exact mandatory gap the original builder
(`CENTER-PANE-DESYNC.md`) left open — "repeat independently... with a real notification daemon (not
a stub)". It ran the real, installed `notification-daemon` (GNOME) 3.20.0 on a private D-Bus
session bus (never touching the operator's own `/run/user/1000/bus`), with a positive-control
`GetServerInformation` handshake confirming the real daemon (not a stub) owned the name, then:

- Window genuinely defocused (a second real Wayland client, `foot`, took focus — confirmed via
  `swaymsg get_tree` reporting `foot focused:true` / Tiller `focused:false`), pane still the visible
  tab, a genuine `needs-input`→`done` transition: `dbus-monitor` captured a real `Notify` method
  call with the real daemon's own allocated id (`uint32 2`), full transcript committed at
  `reference/linux-progress/waveK-critic/act20-real-daemon-notify-transcript.txt`.
- Window genuinely refocused (`foot` killed, sway auto-refocused Tiller, confirmed via
  `get_tree`), another genuine transition (`done`→`error`): the monitor log's line count was
  **byte-identical before and after** (784 lines both times) — zero new bus traffic, confirming
  suppression is restored by real focus alone with pane visibility held constant throughout.

This is a hard discriminator (a real daemon's allocated notification id appearing exactly when the
window is genuinely defocused, and disappearing exactly when it is genuinely refocused, both
confirmed independently via `swaymsg get_tree`) and it closes precisely the gap the row's clause
and the prior builder's own report named as unproven. The report and its screenshots/transcript are
committed (`git log` confirms `86ae6b33` and the transcript file are both ancestors of/present at
this pass's HEAD), so this is not an unreplayable claim.

**Verdict: PASSED.** Adopting the critic-pass verdict from `CRITIC-waveK-builder-claims.md` §2 —
a critic (not the builder) exercising live against a real notification daemon, not a code read.
Not re-driven a third time in this pass; the sibling's evidence already satisfies the standard
(named daemon, named transcript file, named commit) and re-running it would not change the verdict.

---

## Harness trap discovered this pass: `wayland-drive.sh` restarts the app on EVERY invocation

Recorded here because it is not yet in `WAYLAND-LANE.md`/`ENVIRONMENT.md` and cost real time before
being understood. `TILLER_WL_KEEP=1` only skips the kill **at exit**; the unconditional
`kill_ours TILLER_SOCKET "$SOCK" tiller` / `kill_ours SWAYSOCK "$SWAYSOCK" sway` calls at the **top**
of the script (before the `cleanup()`/trap machinery) fire on every single invocation regardless of
`KEEP`. A second `Scripts/wayland-drive.sh` call against the same label kills the still-running app
from the previous call and boots a fresh process — the sqlite-persisted tab/worktree layout survives
(restored on the next boot), but every **live PTY** (a running `claude`/`codex`/`opencode` process)
does not. Multiple actions must be issued in **one** script invocation with in-script `sleep N`
(the action DSL is plain bash, so `sleep` works directly) whenever a live agent process needs to
survive between steps — splitting a launch-then-prompt sequence across two calls silently kills the
agent and replays a fresh shell, which reads exactly like "the app ignored my input" (a fresh
neofetch banner instead of the agent's own screen). This is why every F-CORE-ACT-1[78] and -24 drive
below is one long single invocation.

---

## F-CORE-ACT-17 — worktree status is the priority result; identity is the first matching pane

**Live drive, real agents, one worktree, two real panes with genuinely different statuses.**
`TILLER_WL_LABEL=wf-rest3`, binary pinned above, worktree `/tmp/wfrest3-repoA` (fresh scratch git
repo). One script invocation: opened a Terminal tab, typed `claude` + Enter, accepted the real
trust-folder prompt, let Claude Code v2.1.234 boot to its own idle screen ("Welcome back Enzo!") —
`reference/linux-progress/wf-rest3/01-claude-real-boot.png`. Its own OSC title converged to the
Claude idle convention (`✳ …`), which `tiller_activity::title::detect_claude`
(`rust/crates/tiller_activity/src/title.rs:144`) maps to **`AgentStatus::NeedsInput`** — confirmed
in the frame as the tab's own status glyph (`tab_status_glyph`, `main.rs:2770`), a literal `?`
rendered next to "Terminal" in the tab title, not the idle `○`.

Then, same script, added a second real pane via the tab-strip `+` menu → **Codex** — a genuine
`codex` binary process (confirmed on the host: `ps` showed
`codex -c notify=["…tillerctl","notify","--session","pane-1","--status","needs-input"]`, i.e. its
own real native-hook config, not a stub), sitting at its own real "Sign in with ChatGPT" menu.
`panel.list` confirms `pane-1`/`agent:"codex"`.

**The discriminator**: the worktree row's leading status dot (`activity_status`,
`right_panel.rs:1288`, fed by `status_for_panes`/`agent_id_for_panes`, wired at `main.rs:5483/5490`
— confirmed by grep as real call sites, not a dangling definition) rendered at pixel `(40,171)` in
`reference/linux-progress/wf-rest3/03-claude-needsinput-vs-codex-running-priority.png` sampled as
**`#E0B36A`** (`convert … -crop 1x1+40+171 txt:-`) — an **exact** match, not an approximation, for
`rust/crates/tiller_theme/src/lib.rs:260`'s `warning` color (`rgb_hex(0xE0B36A)`), which is exactly
`theme.tab_needs_input` (`tiller_theme/src/lib.rs:330`: `tab_needs_input: warning`). Zoomed crop:
`reference/linux-progress/wf-rest3/04-priority-dot-zoom-e0b36a.png`. With Claude(pane-0) resolved to
`NeedsInput` (priority 1) and Codex(pane-1) resolved to `Running` (priority 2, Layer D's default for
an identified-but-not-yet-title-classified process — `AgentStatus::priority()`,
`status.rs:40`: Error=0 < NeedsInput=1 < Running=2 < Done=3), the worktree row's own rendered color
is pixel-exact for `NeedsInput`, i.e. the **lower-numbered/higher-priority** status won over the
`Running` pane, live, from two genuinely different agent binaries with genuinely different real
statuses — not two panes with the same status, and not a code read.

**What is not independently re-driven**: the "first matching agent in worktree tab/pane order" half
of the identity clause specifically (as opposed to the priority-status half, which the pixel-exact
color proves) — with only one pane at each status in this drive there was no *tie* to break, so
which-pane-wins-on-a-priority-tie was not exercised. `main.rs:5490`'s `agent_id_for_panes` wiring is
confirmed real (not dead code) by the same grep as above, and the multi-agent priority-tie unit test
already in the tree was not re-read this pass to avoid re-treading the prior sweep's "tests replayed
green" evidence, which the ledger already records.

**Verdict: half-proven.** Upgraded from the prior sweep's evidence (unit tests + a single-agent-only
live case): this pass adds a genuine **two-agent, two-status, cross-priority** live drive with a
pixel-exact, code-cross-referenced color discriminator — real progress on the priority-pick half of
the clause. The tie-break sub-case of the identity clause remains unexercised live.

---

## F-CORE-ACT-18 — running-agent list is de-duplicated and in fixed catalog order

**Live drive, same session, extended**: added a *second* Codex tab (the `+` menu's item list
shifted after the first Codex tab existed, so a second click on the same coordinate landed on
"Codex" again rather than "OpenCode" as intended — kept deliberately, since it produced a genuine
**repeated-agent** case for free) and then an OpenCode tab. Confirmed via the host process tree,
not just the UI: three real, distinct OS processes —

```
codex   PID 617428   -c notify=[...--session pane-1...]
codex   PID 625495   -c notify=[...--session pane-2...]
opencode PID 626962
```

— two genuinely separate `codex` binaries (repeated agent, distinct PIDs) plus one `opencode`
binary (different agent), all real installed CLIs, `AGENT_CATALOG`-adapter-launched (each with its
own real hook config visible in its own argv). `panel.list` confirms 4 panes:
`pane-1`/`agent:codex`, `pane-2`/`agent:codex`, `pane-3`/`agent:opencode`.

**The discriminator**: the worktree row's trailing badge trail —
`reference/linux-progress/wf-rest3/06-running-agent-badges-crop.png` — shows **exactly two** icons,
not three: one codex-blue circular mark, one opencode-orange square mark, **in that order**. This is
`running_agent_ids` (`model.rs:488`, filtered to `AgentStatus::Running` at line 495) rendered as
`AgentMark`s in `CATALOG_IDS` order (`model.rs:44`: `["claude","codex","opencode","pi","omp"]`,
codex index 1 before opencode index 2) — both the **de-duplication** (two live `codex` panes
collapse to one badge) and the **catalog-order** (codex-before-opencode, not discovery order, since
the *second* Codex tab was opened chronologically before the *first* OpenCode tab's badge would have
sorted differently under discovery order) halves of the clause are exercised simultaneously by one
frame, from three genuinely different real agent-CLI processes. Full frame:
`reference/linux-progress/wf-rest3/05-dedup-catalog-order-codex-x2-opencode.png`. Wiring confirmed
real (not dead code): `main.rs:5494` calls `.running_agent_ids(&refs, &tiller_activity::CATALOG_IDS)`
directly into the rendered row struct.

**Verdict: PASSED.** A live, multi-process (2 genuinely repeated + 1 different real agent CLI),
pixel-visible dedup+catalog-order result, cross-referenced to the exact filter/sort call sites —
this closes the row's own stated clause more completely than the prior sweep's single-agent test
replay, and a fresh critic re-reading this evidence can reproduce the same three-process setup and
see the same two-badge, catalog-ordered result.

---

## F-CORE-ACT-24 — session references: stable content ID vs. live pane ID; resumable vs. prunable

**Live drive, real quit+relaunch, `/proc` argv discriminator — not a code read.** The running-agent
session above left a genuine `session_ref` row in this lane's scratch DB
(`/tmp/wf-rest3.sqlite`, `session_ref` table: `pane-2 -> ec4ad3c0-340f-473d-b701-64bda157e276`,
tied to the *second* Codex tab, `root_id=2`, `agent_id=codex` in the `tab` table) — a real row this
pass did not hand-insert. No on-disk Codex rollout/session file matches that id anywhere under
`~/.codex` (`find ~/.codex/sessions -iname '*ec4ad3c0*'` → empty), because that Codex pane never
completed real ChatGPT OAuth in this headless sandbox (a live, non-fakeable environment fact, not a
missing step).

Triggered a genuine quit+relaunch by letting the harness's own `kill_ours` (see the trap section
above) kill the live `tiller` process and boot a fresh one against the **same** DB — a real SIGTERM
and a real fresh `exec`, not a simulated restart. After the tabs restored, clicked the second Codex
tab to bring it forward (this app lazily spawns a tab's PTY only once the tab is actually mounted —
observed directly: immediately after restore only the *active* tab's process existed in `ps`, the
other three panes had no process at all until clicked). The resulting process, identified
unambiguously by its own `--session pane-2` argument (not by guesswork):

```
$ ps aux | grep codex
enzopal+  691900  ...  codex -c notify=["…tillerctl","notify","--session","pane-2","--status","needs-input"]
```

**No `resume` subcommand anywhere in the real, live argv** — contrast the adapter's own
`resume_command` (`tiller_agents/src/codex.rs:41-49`): `"codex -c {} resume {}"`. This is
`resumable_session_refs`/`restored_agent_shell` (`main.rs:10448`/`10500`, confirmed real call sites
at `main.rs:10546`/`10715`, not dead code) taking the **plain-command** branch rather than the
resume branch, live, for a pane whose saved reference exists but whose
`tiller_agents::AgentSessionValidator::is_likely_valid` check must have failed (no matching on-disk
session file) — exactly the documented behavior at `main.rs:10441-10446`'s own comment: "a session
id whose on-disk transcript/rollout file has since vanished is treated as unresumable... rather than
handed to `--resume`". Screenshot: `reference/linux-progress/wf-rest3/07-restore-pane2-fresh-not-resumed.png`
(fresh "Sign in with ChatGPT" banner, not a resumed session).

**What remains UNREACHABLE, named honestly**: the *positive* resumable branch (`--resume <id>`
actually appearing in a relaunched process's argv) needs a session reference whose on-disk file the
validator accepts, which needs a real agent CLI to actually complete an authenticated turn in this
sandbox. Three real blockers, all checked directly rather than assumed:
- **Claude** genuinely cannot produce one here: the pane's own real Claude Code banner read
  "Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker" (screenshot
  `01-claude-real-boot.png`) because this harness's own process tree is itself a nested Claude Code
  session, and the spawned pane inherits that marker — `~/.claude/projects/-tmp-wfrest3-repoA/`
  confirmed to hold no transcript file after a completed real turn. Overriding this by force-unsetting
  the marker was deliberately not attempted: it is a real safety mechanism against two live Claude
  sessions colliding, not an incidental test obstacle, and this pass declines to defeat it for
  convenience.
- **Codex/OpenCode** both genuinely require live ChatGPT/provider OAuth (`codex auth list`-equivalent
  interactive flow; `opencode auth list` → "0 credentials"), unavailable headless in this sandbox —
  the same class of blocker `F-CORE-USG-07` already names for "no working Codex account".

**Verdict: half-proven.** Materially stronger than the prior sweep's unit-tests-only evidence: this
pass adds a live quit+relaunch with a real DB-persisted reference, a real validator rejection, and a
`/proc`-argv-confirmed non-resume outcome — the *prunable/validator-rejected* half of the clause is
now proven live, not just by test. The *resumable* positive half stays unproven, and is named here
as environment-blocked (no CLI in this sandbox can complete an authenticated, resumable session)
rather than softened into a claim this pass did not earn.

---

## F-SET-18 — Install, update, retry, and unsupported/not-found states for agents

**Live drive, both directions, via `ctl surface.settings.open` + real clicks — settings rendered
fully inside the lane's fixed 1715x972 frame with no scrolling needed** (all five agent rows and
their action pills fit on screen; the documented instrument gap about scroll not moving the
Settings surface did not come up this pass because nothing here required scrolling).

**Positive control — all 5 CLIs really on PATH**: `surface.settings.select section=agents` (via
click) showed every row `Available` / `ACP chat available` or `No ACP server`, sourced from a
genuinely fresh `ps`-visible filesystem probe (`Refreshed 22:20:22`), not a cached guess.

**Negative control, PATH stripped to `/usr/bin:/bin` for the app process itself** (not a UI
fabrication — the actual environment `tiller` resolves agents against): all five rows flipped live
to a red **"Not found on PATH"** pill, each with "Not installed — install the &lt;X&gt; CLI to use
it."; Claude/Codex/OpenCode (the three ids `AgentAvailability::install_command`,
`tiller_agents/src/lib.rs:85`, has a real command for) additionally show an **Install** button, while
Pi/Oh-My-Pi correctly show none — matching the source's exhaustive match arm exactly, live.
Screenshot: `reference/linux-progress/wf-rest3/08-set18-negative-control-not-found.png`.

**Clicked Install on the Codex row** (real click, not simulated): a genuine `"Installing… running in
a new terminal tab."` line appeared under the row
(`reference/linux-progress/wf-rest3/09-set18-installing-inprogress.png`), and a real new tab was
created — confirmed over the control socket, not just visually: `panel.list` →
`{"id":"pane-4","tab":"Install codex","title":"Install codex"}`. That pane's own content
(`reference/linux-progress/wf-rest3/10-set18-install-tab-real-shell.png`) showed a real shell that
had already run and exited (`?2` in the prompt's own exit-code segment) — `npm install -g
@openai/codex` genuinely failed because `npm` is not on the stripped PATH, not a staged failure.

**After the real failure, restarted the app (same quit+relaunch mechanism as F-CORE-ACT-24 above)
and reopened Agents settings**: the Codex row is **back to the plain "Not found on PATH" + Install**
state — no `Retry` label, no `Update to latest`, no distinguishable "failed" indication anywhere. A
real failed install is visually indistinguishable from never having tried. Screenshot:
`reference/linux-progress/wf-rest3/11-set18-after-failed-install-no-retry.png`.

Cross-checked against the type system, not just a grep: `AgentAvailability::status_label()`
(`tiller_agents/src/lib.rs:68-74`) is an exhaustive two-arm match — `"Available"` or
`"Not found on PATH"` — and the struct carries no version/progress/error field at all, so a
"failed"/"update"/"unsupported" state has nowhere to be stored even before reaching a renderer. This
validates the prior sweep's grep-based finding (`EVIDENCE-STANDARD`'s "validate the negative before
trusting the zero": here the *positive* half of the same clause — Install, itself — DID render and
DID fire a real subprocess, so the probe is proven capable of finding a state when one exists; its
zero for the other four states is therefore trustworthy, not a narrow-grep miss).

**Verdict: half-proven.** Install and its in-progress ("Installing…") feedback are real and now
proven live end-to-end (button → new real terminal tab → real subprocess spawn, confirmed via the
control socket). Update-to-latest, failed-Retry, and unsupported are confirmed **absent** — live,
not just by source-reading — by driving a real install to a real failure and observing no
distinguishable state survives it. Not `FAILED — absent` outright only because the row's own first
verb ("Install") and the not-found/available binary state genuinely work; the row is a conjunction
and only some of its conjuncts hold.

---

## F-SET-14 — Manage agent accounts: Add Account, waiting/cancel/retry, re-authenticate, remove

**Live drive, real subprocess, two independent repeats — sharpens rather than repeats the prior
sweep's finding.** `surface.settings.open` → clicked into AI Providers → clicked Codex's **Add
Account**. Confirmed on the **host**, not just visually: a genuine subprocess tree spawned —

```
x-terminal-emulator -e codex login      (PID 808560)
  `- codex login                        (PID 808705)
```

— and the spawned window's own real content is a genuine, freshly-generated OAuth URL
(`https://auth.openai.com/oauth/authorize?...&state=<random>...`) with "Starting local login server
on http://localhost:1455." This is `Settings::launch_account_login`
(`tiller_ui/src/settings.rs:1513`) reached for real, not a stub: its own `Command::new(
"x-terminal-emulator").arg("-e").arg(program)...` is exactly what `ps` shows running.

**The Signing-in/Cancel half does not visibly occur — reproduced twice, immediately and at +2s.**
`launch_account_login`'s own first three lines set `self.account_login_pending = Some(provider)`
and call `cx.notify()` **synchronously**, before the async spawn even begins (`settings.rs:1523-
1526`), and the card's render branches on exactly that field
(`self.account_login_pending == Some(provider)`, `settings.rs:2386`) to swap "Add Account" for a
"Signing in…" indicator + Cancel button (`settings.rs:2401`). Two independent live drives (fresh
app boot each time, same click sequence) both show the Codex card **still reading plain "Add
Account"** — not "Signing in…" — in a forced-repaint frame taken immediately after the click
(`reference/linux-progress/wf-rest3/13-set14-immediately-after-click-still-add-account.png`) and
again two seconds later
(`reference/linux-progress/wf-rest3/14-set14-two-sec-after-still-add-account.png`), while the real
subprocess is confirmed alive on the host throughout. This **sharpens** the prior sweep's finding
(fin-set2-cancel3: "Codex row still showed plain Add Account post-click ... process stayed alive") —
that pass attributed the miss to the spawned window reflowing Tiller's layout and moving the Cancel
button's coordinates out from under a stale click target. This pass shows the transition does not
occur even in the **very first** post-click frame, before any plausible reflow-driven coordinate
drift: there is no "Signing in…"/Cancel button rendered anywhere in the frame for any coordinate to
hit, not merely one whose position moved. `cancel_account_login` (the handler this state would need
to reach) remains unexercised by any live click in either pass, now for a more precise reason.

**Re-authenticate and remove are structurally absent, not merely untargetable.** Confirmed the same
way F-SET-15 was: `controls::account_row` (`controls.rs:367`) — the only component that renders any
account row, "System default" or otherwise — takes exactly `(label, subtitle, active, theme)` and no
click callback of any kind; it renders text and two static badges ("This device"/"Active") and
nothing else. There is no code path by which a click on the one existing row could reach a
re-authenticate or remove action, independent of F-SET-15's "only one row exists" finding.

**Verdict: half-proven.** Add Account's real-subprocess half is proven live (a genuine OAuth URL and
a genuine running login server, confirmed on the host both times). The waiting/cancel half is now
more precisely characterized as unreachable — the UI transition that would expose Cancel does not
render, reproduced twice, not just "the click missed." Re-authenticate and remove are confirmed
structurally absent (no callback, no button) rather than merely blocked by F-SET-15's missing second
account.

---

## F-SET-22 — Customize each agent's accent color

VERIFY: Open an Agent Colors row, choose a color, start/show that agent, and confirm its accent
color changes. SRC: `App/AppearanceSettingsView.swift:61`.

**The picker half is real and click-driven, not merely unit-tested.** `grep -rn "agent_colors\["
rust/crates` outside tests turns up exactly one production read site,
`tiller_ui/src/settings.rs:2147` (`let selected = self.agent_colors[index];`, used only to decide
which swatch gets the selection ring on render) plus the two write sites reachable from a swatch
click. The existing unit test `agent_color_click_selects_a_new_accent_and_persists` passes
(`cargo test --manifest-path rust/Cargo.toml -p tiller_ui -- agent_color_click_selects_a_new_accent_and_persists`
→ 1 passed in 0.63s), and this pass reproduced the same effect live: a fresh drive opened Settings →
Appearance and screenshotted the Agent Colors grid
(`reference/linux-progress/wf-rest3/15-set22-appearance-before.png` — no swatch in the Claude Code
row carries a selection ring), then clicked the purple swatch on the Claude Code row and
screenshotted again
(`reference/linux-progress/wf-rest3/16-set22-claude-purple-selected.png` — the purple swatch on the
Claude Code row now renders with a white selection ring around it, a state change that only exists
if the click was received, the row/index resolved, and `agent_colors[0]` was actually written and
re-rendered). This is a hard discriminator: the ring only appears around the entry matching
`self.agent_colors[index]`, so its presence at the clicked swatch and nowhere else proves the write
happened, not just that the click landed.

**The picker is deliberately wired to nothing that renders an agent's brand color.** `main.rs:2740-
2755`'s own doc comment on `WorktreeActivity::agent_brand` says this in as many words: the sidebar's
per-worktree agent tint used to be `settings::AgentAccentColor` (this same eight-swatch picker), and
Claude's picker slot at the time was `theme.tab_needs_input` — so a *running* Claude worktree painted
the identical color as one that *needed input*. The fix was to point every "show this agent's brand
color" consumer (`WorktreeStatusGlyph`/`right_panel.rs:1288`'s `activity_status()`, tab dots) at
`tiller_theme::AgentBrandColor` instead, a separate fixed table the picker never writes to — mirroring
the Swift reference's own `App/AgentAccentColor.swift`, which says outright it is "unrelated to
`AgentIcon.color(for:)`". So the VERIFY clause's second half — "start/show that agent, and confirm
its accent color changes" — has no code path that could pass: there is no rendering surface in this
tree, ported or otherwise, that reads `agent_colors[]` for anything but redrawing the settings row's
own selection ring.

**Verdict: half-proven.** The picker's own state machine (click → select → persist → re-render the
ring) is proven live and by unit test — genuinely working, not stubbed. But it is proven working at
something the VERIFY clause never asked about: no agent's *displayed* accent color anywhere in the
app (tab bar, sidebar status dot) changes as a result, by design, matching the ledger's existing
"zero rendering consumers" characterization exactly.

---

## F-CORE-DOM-02 — Worktree defaults: base-branch/location precedence

VERIFY: Configure each combination of project base/primary branch and location override, create a
worktree, and inspect its branch and path. SRC: `WorktreeDefaults.swift:5`.

Fresh run this pass: `cargo test --manifest-path rust/Cargo.toml -p tiller_project -- domain::tests
--test-threads=1` → `domain::tests::defaults_prefer_explicit_values_then_primary_and_sibling ... ok`.
The test itself (`tiller_project/src/domain.rs:138`) exercises both halves of the clause against
`resolve_worktree_defaults`: no explicit base branch falls through to the primary branch and the
project root's sibling directory; an explicit base branch and an explicit location both override
their respective fallback in the same call. This is the exact precedence logic the VERIFY clause
names, proven by direct assertion on inputs/outputs, not adjacency.

**Verdict: half-proven**, matching the ledger's existing characterization unchanged: the pure
precedence logic is proven by a passing, on-point unit test (reconfirmed fresh this pass), but I did
not additionally live-drive the New Worktree dialog through every combination to confirm the UI
threads these three inputs to `resolve_worktree_defaults` without its own bug — the logic is right,
whether every caller wires it correctly end-to-end is not independently re-checked this pass.

---

## F-CORE-DOM-03 — Deterministic Linux project-location default

VERIFY: Start with no project defaults and observe the proposed project location; repeat with a
Linux replacement root and confirm it is deterministic. SRC: `ProjectDefaults.swift:3` (PLATFORM:
`NSHomeDirectory` needs an explicit Linux home/config/data-root policy).

Fresh run this pass: `cargo test --manifest-path rust/Cargo.toml -p tiller_ui --
project_form_parent_proposes_the_deterministic_default_base --test-threads=1` → `ok`. The test
(`tiller_ui/src/sidebar.rs:3961`, itself doc-commented "F-CORE-DOM-03") is the row's own live
discriminator: with `TILLER_PROJECTS_DIR`/`XDG_DATA_HOME` cleared, it asserts
`Sidebar::project_form_parent() == tiller_project::default_project_base()` — i.e. the New/Clone
Project form's proposed parent is not an independently-computed guess, it is literally the same
deterministic function call — then sets `TILLER_PROJECTS_DIR` to a temp-dir replacement root and
re-asserts the proposal tracks it. `default_project_base()` itself
(`tiller_project/src/domain.rs:6`) is the Linux policy the PLATFORM note asks for:
`TILLER_PROJECTS_DIR` > `XDG_DATA_HOME`/Tiller/projects > `$HOME`/Tiller/projects, in that order,
still unchanged as of this pass's read.

**Verdict: half-proven.** The deterministic-default policy and the form's use of it are both
directly proven (this is closer to PASSED than most half-proven rows here), but the test is a unit
test against `Sidebar::project_form_parent()`, not a full live drive of the New Project dialog UI
showing the proposed path text on screen — keeping this at half-proven rather than upgrading it
unilaterally, consistent with not softening/hardening verdicts without a live UI confirmation this
pass.

---

## F-CORE-DOM-05 — Manual ordering: known/unknown/no-op moves

VERIFY: Reorder known and unknown project/worktree IDs, including moving an item onto itself, and
inspect the resulting sequence. SRC: `ManualOrder.swift:6`.

Fresh run this pass: `domain::tests::ordering_ignores_unknown_and_noop_moves ... ok`. The test
(`tiller_project/src/domain.rs:156`) drives `move_item` through exactly the three cases the clause
names: an unknown id (`move_item(&mut ids, 9, Some(1))` → `false`, sequence unchanged), a no-op move
(moving `2` before itself → `false`, unchanged), and a real move (moving `1` before `3`, then `1` to
the end via `None`) — both producing the expected reordered sequence and a `true` return.

**Verdict: half-proven**, unchanged from the ledger: the ordering algorithm itself is fully proven
by direct assertion; no live drag-reorder of a project/worktree sidebar list was driven this pass to
confirm the UI's drop-target resolution calls `move_item` with the right `(item, before)` pair.

---

## F-CORE-DOM-07 — Auto-naming throttle: 30s and 200-char gates, first-run exempt

VERIFY: Feed transcript growth below each threshold and above both thresholds while observing
generated-name requests. SRC: `AutoNamingThrottle.swift:7`.

Fresh run this pass, two independent test sites:
- `domain::tests::auto_naming_requires_first_run_or_both_throttles ... ok`
  (`tiller_project/src/domain.rs:176`).
- `cargo test --manifest-path rust/Cargo.toml -p tiller_project --test p99_naming_throttle --
  --test-threads=1` → all 3 pass: `the_first_run_is_never_throttled`,
  `growth_below_each_threshold_is_throttled_and_above_both_requests`,
  `recording_a_request_resets_both_baselines` — the file's own header names this row directly
  ("P99 exercise of `F-CORE-DOM-07`").

Together these assert every branch the clause asks for: first run always fires regardless of
growth; below-30s-elapsed suppresses even with growth past 200 chars; below-200-chars-growth
suppresses even past 30s elapsed; both thresholds cleared fires again.

**Verdict: half-proven**, unchanged: the throttle gate itself is exhaustively proven by two
independent passing test suites; no live agent turn was driven this pass to confirm a real running
agent's transcript growth reaches this gate through the production call site unmodified.

---

## F-CORE-DOM-08 — MainActor once-gate fires exactly once

VERIFY: Trigger the same one-shot restore or setup callback multiple times and confirm it has one
observable effect. SRC: `OnceGate.swift:3`.

Fresh run this pass: `domain::tests::once_gate_runs_only_the_first_callback ... ok`
(`tiller_project/src/domain.rs:191`): fires the gate twice, asserts the first call returns `true`
and runs the closure, the second returns `false` and does not, and the shared counter ends at
exactly `1` — a hard discriminator (a broken gate reading `2` would fail the `assert_eq!` outright,
not merely "look right").

**Verdict: half-proven**, unchanged from the ledger's own note: this is a pure internal type with no
UI surface to live-drive at all (there is nothing to click that would exercise it any more directly
than the unit test already does), so half-proven here reflects "logic proven, no live-surface
counterpart exists" rather than "an untested UI half remains".

---

## F-CORE-USG-07 — Codex usage fetching: credentials, refresh, headers, parsed windows

VERIFY: Run with valid, refresh-needed, missing, and rejected credentials and inspect the resulting
usage state and window data. SRC: `CodexUsageFetcher.swift:3`.

Not re-driven fresh this pass (adopting the existing wave D evidence, same day, same host, after
confirming it is not stale): `git log --oneline -- rust/crates/tiller_usage/src/codex.rs` shows no
commits since wave D's pass touched this file, and a fresh grep this pass reconfirms its central
claim still holds — `USAGE_URL` (`tiller_usage/src/codex.rs:25`) remains a hardcoded
`https://chatgpt.com/backend-api/wham/usage` constant with no environment-variable override, unlike
`token_url()` (`codex.rs:267`) which does read `TILLER_CODEX_TOKEN_URL`. Wave D's own evidence
(ledger row, wave D, x86 box, 2026-08-18) proved three of four branches live through the real,
unmodified `fetch_usage`: missing-creds (real `~/.codex/auth.json` is apikey-mode, genuinely fails
OAuth load), rejected-creds/reactive-refresh (real 401 from the real backend, through
`fetch_usage → refresh_token`), and bearer/account header construction (the 401 actually reaching
OpenAI's auth layer proves the headers were sent). The valid-credentials/200-success branch remains
UNREACHABLE in this environment: no override seam for `USAGE_URL`, and this environment's Codex has
no working account to hit it with regardless.

**Verdict: half-proven** — adopting wave D's verdict and evidence directly, having confirmed this
pass that the file has not changed since and the specific claim (no `USAGE_URL` override seam) still
holds by direct grep, not by trusting a stale note.

---

## Remaining rows: NOT EXERCISED (F-WIN-03, F-WIN-10, F-PER-01, F-PER-05, F-PER-07)

This pass's priority order (per the assignment) was the F-SET/F-CORE-ACT rows, all eight of which are
above, plus F-CORE-DOM-02/03/05/07/08 and F-CORE-USG-07 as a bonus (cheap: unit-test-backed, no live
drive needed). The remaining five rows below all require a genuinely fresh, expensive live drive —
a real file-picker portal round-trip, a real toast-triggering gesture hunt, or a real completed agent
turn followed by a full app quit/relaunch cycle — and I ran out of remaining budget to do any of them
honestly rather than superficially. Per the assignment's own instruction, these are reported as
**NOT EXERCISED** rather than adopted from stale ledger evidence or guessed at:

- **F-WIN-03** (⌘O/⌘S open/save). The existing ledger evidence (wave H, 2026-08-14) claims the
  environment has no session D-Bus, making the ashpd portal call UNREACHABLE. I did *not* re-adopt
  this: a quick check this pass (`dbus-send --session ... ListNames`) found a session bus **does**
  exist at `/run/user/1000/bus` in this shell, which the wave-H note's own environment apparently
  lacked or didn't check — and `Scripts/wayland-drive.sh` sets no `DBUS_SESSION_BUS_ADDRESS` of its
  own, so the nested app likely inherits whatever this shell has. Whether an actual portal backend
  (`org.freedesktop.portal.Desktop` with a `FileChooser` implementation) is registered on that bus
  inside the nested compositor is a separate question I did not answer. Flagging this as a concrete
  discrepancy worth a fresh live check next pass, rather than re-asserting UNREACHABLE on old
  evidence that this pass's own quick probe already partially contradicts.
- **F-WIN-10** (transient toast messages). Existing evidence: mechanism confirmed by source, but the
  exact triggering gesture failed 5 times in the pass that wrote it. A sixth blind attempt without a
  clearer plan for which operation reliably raises a toast would not be meaningfully stronger
  evidence than what is already on record.
- **F-PER-01** (persist projects/worktrees/tabs/chats/scrollback across quit/relaunch). Worth
  recording precisely why this one is not a simple stale-evidence adoption: the same wave's ledger
  contains a **second, contradicting** row bearing directly on this clause — `ACP-13` ("FAILED —
  absent | the UI chat path never writes `chat_turn` or `session_ref` rows (0 rows after two
  completed exchanges; WAL-aware read) — the store works, the surface does not call it (owning row
  F-PER-01, overturned this pass)"). That is a hard discriminator (a real WAL-aware row count after a
  real completed exchange) and it directly overturns F-PER-01's own half-proven text in the same
  ledger. I traced the one persistence fix landed since wave H
  (`7190fde5 fix(F-CHAT-34): stop save_tabs from cascading away chat transcripts`, 2026-08-15,
  `tiller_persistence/src/db.rs`) and confirmed by commit date that ACP-13's own test already ran
  *after* that fix and still found 0 rows — so the fix does not resolve the contradiction, and I have
  no fresh live evidence of my own to settle which of the ledger's two self-contradicting entries is
  current. Reporting NOT EXERCISED rather than picking a side without driving it myself.
- **F-PER-05** (restore closed launch-snapshot tabs). Existing evidence is already NOT EXERCISED from
  today's wave B pass; no new attempt made this pass either.
- **F-PER-07** (persist project icon/name/settings across quit/relaunch). Same as F-PER-05: already
  NOT EXERCISED from today's wave B pass, not attempted this pass.

---
