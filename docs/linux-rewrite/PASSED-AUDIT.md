# PASSED-AUDIT — fable, FABLE-03 (2026-08-13 ~15:55 CEST)

Audit of the 146-row PASSED bucket in `INVENTORY-LEDGER.md` (pass-10 body; the file's
Totals block still says 88 and is stale). Hunting **verdict by adjacency**: conjunction
VERIFY clauses passed on evidence about a different (or partial) artefact. Method: all
146 rows triaged against their inventory VERIFY clauses; all 48 app-target rows
clause-checked; ~20 deep code checks with cheap disproofs; package tier sampled
(13/13 named tests exist), not re-derived. **I edited nothing; the critic applies
overturns.** Nothing here is an appearance claim — display is still dead; all
disproofs are grep/read of the live tree at `rust/crates/`.

## False PASSED — 10 (feature does not do what the clause says)

Each: id · clause's demand · what exists · disproof.

1. **F-CHAT-08** (known, re-confirmed) — connecting/send/stop control states. One `↑`
   control varying only by `can_send` (disabled while streaming, never stop).
   `tiller_ui/src/chat.rs` composer control region (~2120–2160). The founding case.
2. **F-CHAT-02** — "Authentication required banner and CLI guidance, then click Retry."
   No auth state exists anywhere: case-insensitive `auth` over all of `chat.rs` matches
   once — `authMethods:[]` inside a test's fake-agent script (chat.rs:3688); `tiller_acp/src`
   has zero matches. What exists is a generic connection-error banner with Retry
   (`Entry::Error{retryable}`, chat.rs:87; tests :3300, :3654). Retry is real, the banner
   is real, the *auth* banner was assembled from them.
3. **F-CHAT-16** — model picker: type search, matching/no-match, "Recommended" labels.
   No search input, no no-match state, no "Recommended" string in `chat.rs`/`composer.rs`
   (grep, zero hits). The drawn test proves select+escape only:
   `model_picker_selects_an_agent_advertised_model_and_escape_dismisses` (chat.rs:3456) —
   exactly what the ledger evidence says, which is not what the clause asks.
4. **F-CHAT-18** — context popover with "full input/output/cache/cost breakdown."
   Popover renders exactly: percent-used, `used/size tokens`, optional `Cost:` line
   (chat.rs:2404–2438). Input, output, and cache rows have no code. Drawn test
   (chat.rs:3504) checks usage shows + escape dismisses — never the breakdown.
5. **F-CHAT-23** — click tool card → expand → output/diff/location links → Dismiss.
   `Entry::ToolCall` carries only `{id,title,status}` (chat.rs:71–76); render is a
   title+status row (chat.rs:1881–1905) with no click handler, no links, no Dismiss.
   Ledger evidence ("Write card Pending→Completed live") is true and proves a
   different claim. Corroborated by F-CHAT-21/22 FAILED (no expansion machinery).
6. **F-SID-15** — right-click worktree → **Remove Worktree** → confirm prompt → gone.
   The worktree context menu exists but ends at "New Chat" with no Remove item
   (sidebar.rs:493–553); removal is the hover ×, which calls `remove_worktree` with
   **no confirmation** (sidebar.rs:1055–1080, wired at :1648). Route absent AND the
   clause's safety prompt absent (git's dirty-refusal is the only backstop).
7. **F-SET-16** — agents registry: type in Search, click Refresh, updated timestamp.
   "Search agents" is a **static text child in a pill-shaped div**, not an input
   (settings.rs:1183); Refresh's handler is the literal no-op `|_, _, _| {}`
   (settings.rs:1187–1189); no timestamp exists. Discovery rows render (evidence true);
   every interactive conjunct is dead chrome. Also contradicts F-SET-17 FAILED
   ("no agent registry") — the two rows describe the same absent registry.
8. **F-SET-21** — "choose each Files icon theme option and confirm the file-tree icons
   change." On Linux `file_icon_choices()` = `[Material]` — one option (settings.rs:30,
   :161–168); the drawn test (settings.rs:2109) clicks segment 0 and asserts the
   snapshot holds the only possible value. Nothing can change, and per F-CORE-FILE-08
   (FAILED) the tree has only generic File/FolderFill icons to change anyway.
9. **F-USE-02** — configure provider visibility in settings; segments + "unavailable
   tooltips" match. `tooltip` has **zero** case-insensitive matches in status_bar.rs —
   the evidence's own words ("tooltips render") name a thing not in the tree. No
   visibility coupling exists: `StatusBar::new(UsageBarData{branch,path})` with three
   hardcoded providers (status_bar.rs:47–66).
10. **F-USE-03** — configured/loading/stale/logged-out/failed each display. `segment_text`
    renders every `Unavailable(reason)` as the same "—" (status_bar.rs:151–165);
    `UsageReason::{NotInstalled,LoggedOut,Error}` are indistinguishable on the surface,
    and status_bar.rs contains no tests. Only Loaded was ever observed (Codex 67%).

## Doubts — downgrade candidates, not counted false

- **F-TAB-15** — ✕-close proven (pass 2); the context-menu Close Tab conjunct: no tab
  context menu exists; "Close Tab" appears only as a command-palette label asserted
  present in a test (command_palette.rs:36) — invocation unproven. D2-reconciliation may
  cover it; the verdict should still be half-proven until the palette route is exercised.
- **F-AUTO-06** — "confirm a notification is **delivered**": create/list/clear round-trip is
  real, but F-USE-06 (FAILED) establishes zero callers of should_notify/build_payload —
  no delivery path. The conjunct is unproven by the critic's own adjacent row.
- **F-WIN-01** — the `⌘,` chord: no `"ctrl-,"` binding found in any crate (exact grep);
  gear route and socket `surface.settings.*` are real. Chord conjunct unproven.
- **F-TAB-10 / F-WIN-07** — clause routes (Split button/pane menu; History menu) absent at
  judgment but D2-reconciled and disclosed in-row; P48's terminal context menu now
  carries SplitRight/SplitDown (main.rs:1828, :5474) — unverified (F-TAB-26). No action.

## Unreplayable PASSED — 4

`F-SID-01`, `F-SID-02`, `F-SID-04`, `F-USE-01` — pass-1 display observations whose
evidence paraphrases the clause and names no test, shot, or transcript. Plausibly true
(probe-2 era screenshots partially archive them); debt under the replayable-proof rule.
(F-CHAT-01/37, also pass-1 display, since acquired replayable cover: chat.rs:3012, :3110;
F-CHAT-01's timing conjunct is met by `Entry::TurnFooter` — now_hhmm footer, chat.rs:640–648.)

## What held up under attack (worth trusting)

F-TAB-20's ctrl-9→last is exactly tested (`numeric_tab_selection(9,3)==Some(2)`,
tiller_project/src/domain.rs:166); F-CHG-10/14 name real drawn tests mutating real
checkouts (changes.rs:1950, :1996); all 13 sampled package-tier test names exist where
claimed. The F-SET-03 test's doc comment (settings.rs:2141–2146) is the model verdict:
it records the dead half explicitly.

## Totals and the estimate

- **Examined**: 146/146 triaged against clauses; 48/48 app-target rows clause-audited;
  ~20 deep disproof checks. **Not reached**: per-row audit of the 98 package/machine
  rows (F-CORE-ACT, F-CORE, F-CTRL, F-AGENT, F-GIT, F-PERSIST, F-TERM-pkg) beyond the
  13-test existence sample and cross-row consistency; no test suite or socket
  transcript was re-run.
- **Fell**: 10 false PASSED (9 new + F-CHAT-08). **Unreplayable**: 4. **Doubts**: 4.
- **J1 impact**: 5 of the spine's chat-segment PASSED rows fell (F-CHAT-02/08/16/18/23).
  The spine's chat leg is materially softer than the ledger claims; D1/D3 briefs are
  aimed at the right holes.
- **Estimated false rate in the unexamined 98**: **2–5% (≈2–5 rows)**. Reasoning: the
  adjacency mechanism needs a display-blocked critic reasoning from code presence — it
  operated on UI rows (observed false rate there: 10/48 ≈ 21%). The machine rows were
  verified by executing the protocol or named tests (13/13 sample real), where a verdict
  cannot outrun its transcript; residual risk sits in rows whose clause smuggles a
  UI-side conjunct (the F-AUTO-06 pattern). **Whole-bucket estimate: ≈12–15 of 146
  (~8–10%) false as written** — that is the discount the completion claim carries.

## FABLE-04 sweep of the machine tier (fable, 2026-08-13 ~19:45 CEST)

The targeted pass the estimate above promised: every currently-PASSED machine-family row
(120 now — the set grew from 98 via the P41/P53 flips) scanned for VERIFY clauses that
smuggle a UI-side or delivery-side conjunct; 42 flagged by clause text; triaged against
ledger evidence to 8 code-checked conjuncts. Every zero below was validated (the pattern
proven to hit its definition site first). **I edited nothing; the critic applies.**

### False as written — 4, plus one partial (all in two conjunct classes)

1. **F-CORE-ACT-19** — "inspect the **emitted** notification title and body." `build_payload`
   (tiller_activity/src/model.rs:420) is referenced only by its own crate and tests — zero
   app call sites; nothing is ever emitted. The payload/field tests are real. Built ≠ emitted:
   F-AUTO-06's overturn, one row over.
2. **F-CORE-ACT-20** — "compare notification **delivery**" across foreground/hidden/not-running.
   `should_notify` (tiller_activity/src/notification.rs:19) has the same validated zero callers —
   there is no delivery to compare. The suppression-rule tests prove policy, not delivery.
3. **F-CTRL-NOTIFY-03** — "confirm both the **system notification** and list state." The evidence
   discloses "system posting platform N/A" — but no posting path exists in the workspace (zero
   `notify-send`/`zbus`/`org.freedesktop.Notifications` outside tiller_theme's unrelated portal
   code), and Linux has org.freedesktop.Notifications, so N/A is absorption, not fact. The
   create/list/clear half is live and real.
4. **F-CORE-FILE-04** — "**Cmd-click** links … confirm the resolved path and line/column target."
   `resolve_file_link` (tiller_project/src/file_link.rs:12) has zero callers outside its own
   crate (5 in-file references, 0 elsewhere), and no click-to-open-link machinery exists in
   tiller_terminal/tiller/tiller_ui (`cmd/ctrl-click|open_link|link_click|hovered_link`: zero;
   `on_click` as pattern validation: 17 hits in chat.rs alone). Resolution and scheme-rejection
   tests are real; nothing wires a click to them.
5. **F-CORE-ACT-02 (partial)** — "observe the sidebar state **and transition-driven notification
   behavior**." The sidebar half is exonerated below; the notification half falls with rows 1–2
   (zero callers). Half-proven, name which part.

### Exonerated by the same checks (the sweep's other half)

- **F-AUTO-04 / F-CORE-ACT-02 (sidebar half)** — notify does reach the drawn surface:
  `tab_status` reads `self.activity.status("pane-{id}")` for Terminal panes (main.rs:3019–3030),
  `worktree_status` folds it (main.rs:3058), `set_worktree_status` re-derives every render
  (main.rs:3444). The chain socket→model→sidebar dot exists in code.
- **F-AUTO-05 (select branch)** — control select confirms the sidebar highlight:
  `sidebar.set_selected_worktree` inside the control path (main.rs:3208).
- **F-CTRL-WORK-02** — CLI rows and sidebar render one shared collection; the comparison is
  structural.

### Doubts — not counted

- **F-CTRL-PANEL-08** — "confirm … **visible** pane change": focus wiring to real
  `FocusHandle`s exists (main.rs:4128–4134) but the visible conjunct was judged while every
  display was dead; replayable-proof debt, not falsity.
- **F-CORE-ACT-01** — the four observable states are three at the UI boundary:
  `AgentStatus::NeedsInput` maps to `ActivityStatus::Idle` (main.rs:3026) and `ActivityStatus`
  has no needs-input variant (right_panel.rs:31–39; its `Idle` doc-comment does say "waiting
  for input"). Package tests are honest; whether the observable contract and its priority
  ordering survive the collapse is a critic call on the sidebar/activity ground.

### Reconciliation

Found 4 full + 1 partial against the estimated 2–5 — the top of the range, and **every one
sits in the two conjunct classes the estimate named** (delivery-side, UI-side); no false rows
surfaced in the socket/persistence/git transcript families. The whole-bucket discount stands;
the unexamined remainder is now the 78 machine rows whose clauses carried no UI/delivery
conjunct, where the transcript rule holds.
