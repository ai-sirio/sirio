# T3-set triage plan — F-SET, 15 rows

Read-only pass. No code changed, no verdicts set, `INVENTORY-LEDGER.md` untouched.
Manifest: `docs/linux-rewrite/triage/T3-set.md`.

**Headline finding, read this before the rows below:** four of these fifteen rows carry
evidence that is *stale relative to the current tree*, not wrong-in-principle — the code
they describe as missing has since been built and, in one case, already photographed live.
`docs/linux-rewrite/INVENTORY-LEDGER.md`'s own copy of the evidence was never refreshed
after the builds landed. See `F-SET-12`, `F-SET-13`, `F-SET-14`, `F-SET-17` below. Dispatching
build briefs against any of these four without re-reading the tree first would waste a fleet
slot re-building something that already exists.

---

## F-SET-04 — half-proven

**Needs: build.** Size **L**.
**Files:** `rust/crates/tiller/src/main.rs` (`restore_tabs` ~7581–7690, `add_agent_tab` ~4595–4625,
the split-agent spawn path ~5060–5090).

The DB half (`resume_agent_sessions` persists and survives relaunch) is genuinely closed —
confirmed by the row's own evidence and by `main.rs:8025`/`:8058` + `session.rs`. The
functional half is not merely unproven, it is **structurally impossible today, and worse than
the row's own evidence states**: `restore_tabs` (`main.rs:7581`) reconstructs a restored
`"terminal"` tab by calling a bare `TerminalView::new(&cwd, cx)` — a plain login shell —
**never** `adapter.command()` or `adapter.resume_command()`, regardless of the setting or of
which agent originally owned the pane. Both real agent-spawn sites (`add_agent_tab`,
the split variant) always call `adapter.command()`; `resume_command` has zero production call
sites anywhere in the workspace (confirmed by grep across all five adapters — only trait defs
and `adapters_tests.rs` reference it). So a restart doesn't just skip `--resume`/`--continue`
(F-CORE-ACT-24's finding) — it doesn't relaunch the agent process *at all*; a restored agent
tab is a bare shell.

**Approach:** on restore, when a restored tab's `agent_id` is known and `resume_agent_sessions`
is true, look up a saved `session_ref` for that pane's persisted identity (the `session_refs`
map is already loaded at `main.rs:8139` via `session_store.load_session_refs()`) and launch via
`adapter.resume_command(...)` when a ref exists, falling back to `adapter.command()` otherwise
— instead of the current bare-shell branch. This needs a stable identity to key the session ref
by; today only chat panes push a `session.ref` over the socket per `pane_id`, so terminal-hosted
agent panes may never populate one — that's its own gap inside this same fix, not a separate
row. Touches restore-identity plumbing, the session-ref contract, and both spawn call sites in
the busiest file in the tree — genuinely L, not M.

---

## F-SET-09 — half-proven

**Needs: build.** Size **M**.
**Files:** `rust/crates/tiller_ui/src/settings.rs`, `rust/crates/tiller/src/main.rs`.

The route half is real and wired, confirmed by reading the code: `Settings::on_install_skill`
→ `WorkspaceAction::InstallSkill` → `main.rs:2341` opens a real terminal tab running
`skill_install_shell(command)` (the provisioner, `tiller_project::agent_skill_install_command()`,
is genuinely tested and genuinely invoked). What's missing is the feedback conjunct the row's
own clause names (`01-inventory-app.md:207`: "observe success or error feedback, and confirm
the control changes accordingly"): no install-status field exists anywhere (confirmed by
grep), and the "Install Skill" button renders identically whether idle, running, or finished.

**Approach:** `tiller_terminal` already emits `TerminalActivityEvent::ChildExited { status }`
in production (`lib.rs:916`) for exactly this purpose. When `main.rs` spawns the install-skill
terminal tab, subscribe to that terminal view's `ChildExited` event and route success/failure
back into `Settings` (a new `WorkspaceAction` variant, or a direct entity update) so
`settings.rs` can hold a transient install-status field and render it beside the button —
mirroring the existing `account_action_error` pattern used by the provider cards.

---

## F-SET-10 — half-proven

**Needs: both.** Size **M**.
**Files:** `rust/crates/tiller_ui/src/status_bar.rs`, `rust/crates/tiller/src/main.rs`.

The refresh-**interval** half is genuinely wired end to end and only unexercised live:
`Settings`' `on_change` → `cx.observe(&settings, ...)` at `main.rs:2552` calls
`StatusBar::apply_preferences`, and the fetch loop inside `status_bar.rs` re-reads
`bar.refresh_interval` every cycle, so a changed interval takes effect at the next tick
without a relaunch. That half is `needs: exercise` — change the stepper, wait through one
interval, confirm cadence shifted.

The **Refresh-now** half is not just unexercised, it is **structurally absent**, and this
contradicts `ADJUDICATION-BACKLOG.md`'s claimed route ("Refresh re-runs discovery... sole
caller `main.rs:2590`") — that route text is stale/wrong, flag it. `StatusBar::on_refresh`
is a real builder method (`status_bar.rs:160`), but `render()` (`:301`+) draws no button that
calls it — only a settings-gear icon is drawn. `on_refresh`'s only caller anywhere in the
workspace is `tiller_ui/examples/chrome_demo.rs:91` — exactly the "references only from
`examples/`" signature `SEAMS.md` calls out as an open seam. None of `main.rs`'s live
`StatusBar::new(...)` construction sites (`:2554`, `:3463`, `:8221`) ever call `.on_refresh(...)`.

**Approach:** draw a refresh icon-button in `status_bar.rs`'s `render()` next to the existing
settings gear, wire it to `on_refresh`, add a way for the loop to trigger an immediate fetch
out of band from the interval timer, and call `.on_refresh(...)` at the live `StatusBar`
construction sites in `main.rs`. Then exercise both halves together.

---

## F-SET-11 — half-proven

**Needs: exercise.** Size **M** (environment manipulation, not code).
**Files:** none — this is a live-drive task, not a source change.

All 5 clause states (`01-inventory-app.md:209`: valid, missing-PATH, logged-out, timeout,
failure) already exist as real, reachable code: `ProviderUsageState::{Loading, Loaded, Stale,
Unavailable(reason)}` with `reason ∈ {NotInstalled, LoggedOut, TimedOut, Error}`
(`tiller_usage/src/model.rs:64–113`), each fed by real fetchers (`claude.rs`, `codex.rs`,
`opencode_go.rs`, `ollama.rs`) and rendered distinctly by `status_bar.rs`'s
`segment_text`/`segment_dimmed`. Only "valid/Loaded" has been driven live so far.

**Approach, one gesture per remaining condition:**
- **NotInstalled** — temporarily remove `claude`/`codex` from `PATH` (P120 already used this
  exact instrument safely on a neighboring row).
- **LoggedOut** — a CLI with absent/expired local credentials.
- **TimedOut** — Codex's fetch is a real HTTP call with a 15s timeout (`codex.rs:31`); Claude's
  is a CLI-poll with a 25s timeout (`claude.rs:275`). Either is reachable by blackholing the
  relevant host/port during one fetch cycle.
- **Error** — the hardest to force cleanly (a response that's reachable but unparseable);
  may need a stub server in front of Codex's `USAGE_URL`, or should be honestly reported as
  the one sub-condition left unexercised if that's out of proportion.

---

## F-SET-12 — half-proven

**Needs: reclassify** — evidence is stale.
**Files (for the re-drive, not a build):** `rust/crates/tiller_ui/src/settings.rs`,
`rust/crates/tiller_usage/src/opencode_go.rs`, `rust/crates/tiller_usage/src/credentials.rs`.

The manifest's recorded evidence ("no cookie UI or state", pass 10) predates commit `7ae343d`
("Ollama Cloud provider card, cookie store, bar segment", which also carries OpenCode Go's,
2026-08-14) by hours. The current tree has real, rendered, tested UI: `render_provider_card`
draws a masked cookie field (`provider-opencode-cookie-field`), Save/Clear buttons
(`save-opencode-cookie`/`clear-opencode-cookie`) wired to `save_opencode_cookie`/
`clear_opencode_cookie` (`settings.rs:1400`/`:1431`), which write through a real
`CredentialStore` and flip `ProviderAccountStatus` + usage-bar visibility on success, with an
error path rendered on failure. The workspace-ID override field is present too and flows into
the live fetch loop (`opencode_workspace_id_override`, read fresh each cycle by the refresh
loop). **Do not dispatch this as a build** — re-adjudicate by driving Save/Clear live (paste a
cookie, confirm the account state/bar segment change, Clear, confirm it reverts).

---

## F-SET-13 — FAILED — absent

**Needs: reclassify** — evidence is stale, same root cause as F-SET-12.
**Files (for the re-drive):** `rust/crates/tiller_ui/src/settings.rs`,
`rust/crates/tiller_ui/src/status_bar.rs`, `rust/crates/tiller_usage/src/ollama.rs`.

Same commit (`7ae343d`) built this too. `ProviderKind::OllamaCloud` exists (`settings.rs:649`);
a full Ollama provider card with masked cookie field + Save/Clear renders and is tested;
`ollama_show_in_bar` flows through the same `SettingsSnapshot`/`UsageBarPrefs` contract as the
other three providers; `status_bar.rs` renders a real Ollama Cloud segment (a documented
Globe-icon stand-in, not a silent leftover) fed by a real `OllamaCloudUsageFetcher::fetch()`
call in the production refresh loop. Not a build task — re-adjudicate with the same live drive
as F-SET-12.

---

## F-SET-14 — FAILED — defective

**Needs: reclassify** — "dead control" framing is false; a real, smaller gap remains.
**Files:** `rust/crates/tiller_ui/src/settings.rs`.

The row's central claim — "`on_manage_account` has exactly one caller (a test); `main.rs`
never installs it... the button is inert" — is false on the current tree, and independently
disproved *live*: `P120-report.md`'s `F-CORE-AUTH-01` section drove this exact button on
2026-08-14 and captured it spawning a real `x-terminal-emulator -e claude auth login`, which
opened a genuine PKCE OAuth URL in the system browser (their own screenshot set). The evidence
has a real reasoning gap, not a fabrication: `on_manage_account` genuinely has only a test
caller, but that field is only an optional *host override* — `settings.rs:2006`'s
`manage_account_handler` falls back to a fully self-contained
`settings.launch_account_login(provider, cx)` (`settings.rs:1244`) whenever no host callback is
set, and that path needs no `main.rs` wiring at all. The button is real and does what "Add
Account" should.

What's genuinely still missing, per the row's own clause (`01-inventory-app.md:212`,
"exercise browser-login waiting/cancel/retry"): there is no in-app waiting/cancel affordance
while the spawned terminal runs (the terminal window is the only feedback), and
re-authenticate/remove are deliberately collapsed into the one action — a documented,
defensible design choice given this app holds no isolated per-provider credentials, not a
defect. **Flag for re-adjudication, not a ground-up build** — what's left is a much smaller
waiting/cancel-UI gap than "dead control" implies.

---

## F-SET-15 — half-proven

**Needs: reclassify** — the row's clause may be structurally unanswerable as written.
**Files:** `rust/crates/tiller_ui/src/settings.rs`.

Shares F-SET-14's false premise about `on_manage_account` being the reason there's "no live
route to a second account" — the real reason is architectural, not a wiring gap: this app has
exactly one credential slot per provider by design (the "Active" badge on the System-default
row is hardwired `true` — the code's own comment: "this app has no isolated accounts").
Clicking Add Account re-runs the *same* CLI's login; it does not add a second,
independently-selectable account, so no code path could ever move the badge. That makes the
row's clause (`01-inventory-app.md:213`: "with multiple accounts, select System default and
another account, confirm the badge moves") unanswerable by construction today, not merely
unbuilt — the same shape as `F-SET-21`'s existing N/A-platform ruling (a menu with only one
real option on this platform).

**Flag for a ruling before any build is dispatched:** either re-scope this row to
N/A-by-design alongside `F-SET-21`, or treat isolated multi-account credential storage as a
real feature to build (size **L** — a new per-provider credential-isolation model). Building it
without that ruling first risks throwing the work away.

---

## F-SET-16 — half-proven

**Needs: both.** Size **S** (timestamp UI) **+ exercise**.
**Files:** `rust/crates/tiller_ui/src/settings.rs`.

Search and Refresh are both real: a live-filtering `agent-search-field`, and a wired
"↻ Refresh" button → `refresh_agent_availability` → `try_discover_availability()`, pinned by a
green test (`refresh_agents_re_runs_agent_discovery`). The row's "Refresh half proven absent"
evidence (byte-identical capture before/after click) is very likely a **false negative**: a
re-discovery that finds nothing different on disk correctly renders nothing different — this
is the same trap `P120-report.md` hit driving a neighboring status-bar row. What genuinely is,
structurally, absent is the clause's other conjunct (`01-inventory-app.md:214`: "confirm
matching registry rows and **updated timestamp**/status") — no "last refreshed at ..." state
or render exists anywhere (confirmed by grep).

**Approach:** add a last-refreshed timestamp to Settings' agent-discovery state, set it in
`refresh_agent_availability`, render it near the Refresh button. Then re-exercise Refresh
against an environment that actually changes between clicks (rename an agent binary off PATH)
rather than a byte-identical one, so a real before/after difference is even observable.

---

## F-SET-17 — FAILED — absent

**Needs: reclassify** — built and already live-photographed after this row's evidence.
**Files (for reference, no build needed):** `rust/crates/tiller_ui/src/settings.rs`,
`rust/crates/tiller_agents/src/lib.rs`.

Commit `2dcbc1d` ("surface an agent-registry error state with retry (F-SET-17)", 2026-08-14
17:40) made discovery fallible: `tiller_agents::try_discover_availability()` now returns
`Result<Vec<AgentAvailability>, DiscoveryError>` (`lib.rs:188–217`/`:345`), distinguishing "not
on PATH" from "could not check". `settings.rs` renders the registry-error banner over the
last-good rows (`agent_registry_error`, set at construction `:836` and by
`refresh_agent_availability` `:1296–1298`), with the same "↻ Refresh" button doubling as Retry,
pinned by a green test. Commit `ec53f40` (same day, 17:45) is a **docs commit that captured
this live**, in the Wayland lane: `PATH`'s first entry `chmod 000` fails construction and
renders the warning banner with zero false "Not found" rows; `chmod 755` + a real
virtual-pointer click on "↻ Refresh" clears the banner and shows five truthful rows
(`reference/linux-progress/p111-set17-registry-error.png`,
`p111-set17-registry-recovered-by-retry.png`). This is stronger than merely reachable — it is
already photographically proven. **Flag for immediate re-adjudication toward PASSED**; no
build work remains here.

---

## F-SET-18 — half-proven

**Needs: build.** Size **L** — flagging explicitly, do not fold into F-SET-09.
**Files:** `rust/crates/tiller_ui/src/settings.rs`, `rust/crates/tiller_agents/src/lib.rs`
(or a new provisioner module beside `tiller_project/src/skill.rs`'s pattern),
`rust/crates/tiller/src/main.rs`.

Confirmed still genuinely absent — this is not a staleness case like its `F-SET-1{2,3,7}`
neighbors. No install/update/retry affordance exists per agent row today, only the status-only
path + pill (`render_provider_status` `settings.rs:1771–1795`, ACP badge `:1803–1830`). Unlike
`F-SET-09`'s Install Skill, there is no per-agent-CLI install-command function anywhere at all
— grepping `install_command`/`InstallCommand` across `tiller_agents` and `tiller_project`
turns up only the unrelated skill provisioner.

**Approach:** needs real install/update commands for all 5 CLIs, plus install-in-progress/
update-available/failed-retry render states per agent row — most naturally reusing
F-SET-09's soon-to-be-proven pattern (spawn a terminal tab, watch `ChildExited`, report status
back), which is a reason to sequence this *after* F-SET-09's feedback plumbing lands rather
than duplicate it. Genuinely L: five different CLIs' real install/update commands is its own
research task before any UI work starts.

---

## F-SET-19 — half-proven

**Needs: exercise.** Size **S**.
**Files:** none.

The System-follows-desktop half is real, built code, just unexercised. `Theme::init`/
`set_mode` (`tiller_theme/src/lib.rs:802–844`) install a dark-biased System theme immediately,
then `follow_portal` (`:848`) asynchronously queries the XDG desktop portal's `color-scheme`
setting via `ashpd` and re-resolves once it answers — re-triggered every time System is
explicitly re-selected. The row's clause (`01-inventory-app.md:217`) only requires that
choosing System/Light/Dark changes the app's appearance, which this satisfies once the portal
answers.

**Approach:** set the desktop's color-scheme preference to Light, select System in Settings'
Appearance card, confirm the app resolves to light (not stuck on the dark first-frame
fallback); then flip the desktop preference to Dark with System still selected and relaunch to
confirm the next portal read follows.

**Worth flagging, not a blocker:** the portal read is one-shot per selection/launch
(`ashpd`'s `settings.color_scheme()`, not a subscription to the portal's `SettingChanged`
signal) — a desktop theme change made *while Tiller keeps running* in System mode, without
touching the picker again, is not followed live. If that stronger reading of "follows desktop"
is in scope, it's a real, separate build (subscribe to the D-Bus signal), distinct from what
this row's clause literally asks for.

---

## F-SET-20 — FAILED — defective

**Needs: build.** Size **S**. Evidence verified accurate, not stale.
**Files:** `rust/crates/tiller_ui/src/settings.rs`.

Verified directly against the current tree: `set_translucency` (`settings.rs:1097–1100`) sets
the field and `cx.notify()`s but — unlike every sibling setter (e.g. `set_interface_font_size`
immediately below it, which calls `self.changed()`) — never calls `self.changed()`, and
`SettingsSnapshot` carries no `translucency` field at all, so the value cannot leave the entity
even once the setter is fixed. Independently confirmed zero consumers of
`background_appearance`/`WindowBackgroundAppearance`/`Blurred` anywhere in `tiller`,
`tiller_ui`, or `tiller_terminal`.

**Approach:** either wire it for real — add the field to `SettingsSnapshot`, call
`self.changed()` from the setter, and have `main.rs` apply it to the window via GPUI's
`WindowBackgroundAppearance` — or, if translucency is intentionally out of scope for Linux,
remove the dead control and its clause rather than leave an unsatisfiable conjunct in place
(the same choice already flagged for `F-SET-15`).

---

## F-SET-22 — FAILED — defective

**Needs: build.** Size **M**. Evidence verified accurate, not stale.
**Files:** `rust/crates/tiller_ui/src/settings.rs`, `rust/crates/tiller/src/main.rs`.

Verified directly: `agent_colors` round-trips correctly *within* Settings (click →
`SettingsSnapshot.agent_colors` → settings.rs's own persistence path), with a green test
proving the choice sticks per-row. But `app_settings_from_snapshot` (`main.rs:8038–8040`)
explicitly discards it — a code comment documents the gap in place ("F-SET-22 has no
`AppSettings` field yet; do not pretend this UI-only picker is persisted") — and grep confirms
`agent_colors` has zero references outside `settings.rs`, so nothing downstream (agent tab
icons, chat headers, wherever the accent is meant to show) ever reads it.

**Approach:** add the field to `AppSettings`/the persistence model, stop discarding it in
`app_settings_from_snapshot`, and give it a real consumer. Persisting the value alone would
only close half the clause ("confirm its accent color changes") — a consumer has to exist too.

---

## F-SET-24 — half-proven

**Needs: exercise.** Size **S** (once on the X11 lane). Evidence verified accurate, not stale.
**Files:** none in this worktree — the route is the X11 lane, not a code change.

Confirmed: `WAYLAND-LANE.md` documents that the embedded browser needs a real X11 window
handle and gets a Wayland one under this lane, so webview content — and therefore the
permission doorhanger it would trigger — never renders (chrome-only). Grep confirms
`browser.rs`'s `request_permission` has no caller besides real page content driving it and its
own tests; there is no separate dead-code path hiding behind the Wayland limitation.

**Approach:** on `DISPLAY=:1`, navigate the embedded browser to a page that requests a
permission, grant it, confirm it appears in Settings' Permissions card, then Revoke and confirm
it clears — closing the empty-state half this row already proved live on the Wayland lane.

---

## Cross-cutting notes

1. **Stale-evidence cluster (`F-SET-12`, `F-SET-13`, `F-SET-14`, `F-SET-17`, and the framing of
   `F-SET-15`).** All carry evidence text that predates same-day builds — P111's cookie/Ollama
   UI (`7ae343d`), the registry-error-and-retry feature (`2dcbc1d` + the `ec53f40` live-capture
   doc commit), and the `launch_account_login` self-contained fallback in `settings.rs`. The
   `INVENTORY-LEDGER.md` copy of each row's evidence was never refreshed once the code landed.
   Re-driving these four (five, counting F-SET-15's framing) against the current tree before
   dispatching *any* build work against them would recover real ledger progress for free and
   avoid a fleet agent re-building something that already exists.
2. **`main.rs` bottleneck.** `F-SET-04`'s fix (restore-time agent relaunch), `F-SET-09`/`F-SET-10`'s
   fixes (install/refresh feedback wiring), and `F-SET-18`'s eventual install plumbing all touch
   `main.rs`. `F-SET-04` and `F-SET-09`/`F-SET-10` in particular touch overlapping regions
   (`restore_tabs`, `add_agent_tab`, and the `~8130–8280` settings/status-bar construction
   block) — schedule them with that in mind rather than as three independent parallel briefs.
3. **Design-ruling-needed pair (`F-SET-15`, `F-SET-20`).** Both rows' clauses may be
   structurally unanswerable as written — single-account-by-design, and a translucency toggle
   nothing in the tree consumes. Both would benefit from an explicit product ruling (build the
   real feature, or reclassify to N/A/removed-by-design like `F-SET-21`'s precedent) before a
   build agent spends time on either.
