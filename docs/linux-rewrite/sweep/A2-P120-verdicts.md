# A2-P120 verdicts

Adjudicated against `docs/linux-rewrite/P120-report.md` and the captures it names under
`reference/linux-progress/p120/`. Every frame/log cited below was opened and read directly
(images via `Read`, several cropped/zoomed with `convert` for legibility), not taken from the
report's prose. VERIFY clauses quoted below are from `docs/linux-rewrite/02-inventory-packages.md`
/ `01-inventory-app.md`, independently re-read, not from the sweep table's paraphrase.

Cites the evidence standard: `docs/linux-rewrite/EVIDENCE-STANDARD.md`.

---

## F-AGENT-API-01 — half-proven

Clause: every adapter exposes ID/display/hook-flag/prepare/launch/resume/optional-summarizer;
fixed catalog is Claude Code, Codex, OpenCode, Pi, Oh-My-Pi in that order.

Proven live: `ctl system.capabilities` against the running `p120a` socket returned 53 real
methods (`api01-system-capabilities.json`) — no method name contains "summar" anywhere.
Independently, `new-tab-menu-try3.png` shows the live new-tab picker listing exactly
"Claude Code, Codex, OpenCode, Pi, Oh-My-Pi" as top-level items, matching the required order.

Not proven: per-adapter properties (native-hook flag, launch, resume) for Codex/OpenCode/Pi/
Oh-My-Pi individually — only Claude Code's launch + hook-flag were separately driven this pass
(via the ACT-19 and SAFE-02 evidence below). The control-socket method list is a reasonable but
indirect proxy for "no summarizer command exists," not a direct per-adapter enumeration.

`evidenceDiscriminates: true`

---

## F-AGENT-SAFE-02 — NOT EXERCISED (unchanged) — report evidence is for the wrong row

Clause: **Claude hook migration** (`ClaudeHookMigrator`) rewrites only a stale quoted leading
`tillerctl` path in existing hook commands, preserving pane IDs/arguments/unrelated
hooks/other JSON keys; unchanged/unparseable files are no-ops. VERIFY: run migration against
matching/stale/malformed/unrelated hook files and diff every non-path field.

The report's evidence (`safe02-sha256-before-after.txt`, `safe02-worktree-local-hooks.json`) is
real and clean, but it proves a different row's clause: that `prepare()` writes only
worktree-local hook config and never touches `~/.claude/settings.json` /
`~/.codex/config.toml` (SHA-256 identical before/after a real Claude Code launch) while writing a
correct, fresh `.claude/settings.local.json` in the worktree. That is **F-AGENT-SAFE-01**'s
clause verbatim ("Adapter preparation writes only worktree-local hook/plugin/skill files"), not
F-AGENT-SAFE-02's. No stale hook file was ever constructed, and `ClaudeHookMigrator` was not
invoked — directly or through any exposed API — at any point in this pass. Independently
re-confirmed by grep: `ClaudeHookMigrator` has zero references outside its own module in the
current tree, same as the ledger's existing note. Verdict stays NOT EXERCISED; the row's actual
subject was not touched.

`evidenceDiscriminates: false`

---

## F-CORE-ACT-19 — FAILED — defective

Clause: notification title is formatted as `<agent display name> — <human worktree label>`;
body contains branch plus optional project/comment.

Live D-Bus transcript (`act19-act20-notify-background-fired.log`, captured under a running
`dbus-monitor` filter, with a positive control in `act19-positive-control-dbus.log` proving the
monitor itself is not blind): a real `Notify` call fired with
`title="Claude Code — finished"`, `body="linux/gpui-waku · tiller"`.

Body matches the clause (branch `linux/gpui-waku` + project `tiller`). Title does not: I traced
the title's construction in `rust/crates/tiller_activity/src/model.rs:448` —
`format!("{agent_display_name} — {}", status.human_label())`, and `status.human_label()`
(`tiller_activity/src/status.rs:29`) returns `"running"`/`"needs input"`/`"finished"`/`"failed"` —
a **status** word, not the worktree label the clause requires. The live-captured title is
literally `<agent> — <status>`, confirmed both by the wire capture and by the source line that
produces it. This is a real, reached, observed defect, not an absence.

`evidenceDiscriminates: true`

---

## F-CORE-ACT-20 — half-proven

Clause: notification suppressed when no agent running, when status unchanged, or when app
active + pane visible; otherwise allowed.

Proven live, both directions of the specific gate the ledger cited as the blocker
(`app_active` hardcoded `true`, "only a non-visible pane can ever notify"): pane made the
active tab (`visible=true`) → `tillerctl notify --status error` produced **no** `Notify` call in
5s of monitoring (`act20-notify-foreground-suppressed.log`); pane backgrounded (`visible=false`)
→ `tillerctl notify --status done` produced a real `Notify` call
(`act19-act20-notify-background-fired.log`).

Not proven: the "no agent running" and "status did not change" (`old == Some(new)`) suppression
branches — neither transition was driven this pass.

`evidenceDiscriminates: true`

---

## F-CORE-ACT-24 — FAILED — absent

Clause: session refs distinguish a stable content ID from the live pane ID; restore planning
classifies captured refs as resumable or prunable based on which content IDs are present.

Live, reproduced twice (`act24-25-26-multi-worktree-restart.txt`, screenshots
`1786728159491413189-act2526-post-restart.png` /
`1786728218394535892-act24-25-26-second-restart.png` confirm the "Claude Code" pane/tab shell
reappearing across 2 real kill+relaunch cycles against the same on-disk DB): each restart spawns
a **brand-new** `claude` process for the restored pane with a fresh random `--session-id`
(`e8e3cb45-...` then `d5cdcc17-...`), with no `--resume`/`--continue` flag present either time.
This directly disproves the resumable/prunable distinction: the app always spawns fresh,
unconditionally, regardless of whether a prior content ID is present.

`evidenceDiscriminates: true`

---

## F-CORE-ACT-25 — NOT EXERCISED (unchanged)

Clause: launch restoration remounts selected/open worktrees before deferred ones, preserving a
priority order.

The drive could only take a single `workspace.list` snapshot ~3-4s after each relaunch, by which
point all 4 worktrees already showed `mounted:true` — it cannot distinguish "no priority/deferred
split exists" from "the split resolves faster than sampled." The one clean, reproducible finding
(`ctl workspace.select`'s runtime change does not survive a restart; the persisted DB selection
wins) is a different mechanism than the clause's mount-ordering claim. The report itself declines
to claim a disproof here, and I concur — no discriminating evidence was produced for this row's
actual subject.

`evidenceDiscriminates: false`

---

## F-CORE-ACT-26 — FAILED — absent

Clause: mount eviction fires only above a positive cap, evicts eligible nonselected worktrees in
open order, never evicts a running/needs-input/unsaved worktree.

Independently re-confirmed by grep (`ids_to_evict` in `tiller_activity/src/mount.rs:9`): its only
caller anywhere in the tree is its own integration test (`activity_domain_integration.rs:211`) —
zero references in `tiller`/`tiller_ui`. Live: 4 mounted worktrees (including one with a running
agent, `pane-3`) survived 2 consecutive full process restarts with zero evictions
(`act24-25-26-multi-worktree-restart.txt`, same screenshots as ACT-24). No cap-enforcement
mechanism exists in the running app to observe protecting anything against.

`evidenceDiscriminates: true`

---

## F-CORE-AUTH-01 — half-proven

Clause: account identity parsing reports Claude logged-in/email/organization fields from JSON,
and uses the first nonempty Codex credential line as identity.

Proven live: Settings → AI Providers → Claude Code → "Add Account" spawns a real
`x-terminal-emulator -e claude auth login` process and opens a genuine
`claude.ai/oauth/authorize?...` PKCE URL in the real system browser — both windows visible
side-by-side in `1786725460554010974-auth01-add-account-clicked.png`. This disproves the prior
"dead button" premise the row's UNREACHABLE reasoning rested on.

Not proven: `parse_claude_json` itself. The flow was deliberately aborted before any code was
pasted (real-credential-risk reasoning, `1786725547285656214-after-auth01-abort.png` shows clean
abort), so no JSON was ever parsed and no identity fields were ever extracted or observed. The
clause's actual subject remains untouched.

`evidenceDiscriminates: true`

---

## F-CORE-DOM-03 — NOT EXERCISED (unchanged) — report's "UNREACHABLE" framing not adopted

Clause: default project base is deterministic under a Linux replacement root.

Clicked "+ Add Project" on the isolated nested Wayland lane starting from a project-free sidebar
(`1786727207869944678-dom03-baseline.png` confirms empty sidebar). No dialog, and no visible
change other than cursor position, appears in `1786727242693155517-dom03-add-project-clicked.png`
— sidebar stays empty. But the report itself documents that a portal file-chooser call from the
isolated nested compositor could plausibly be routed to and rendered by the **real host
desktop's** portal backend instead, invisible to a capture scoped to the nested instance — so
this is not a clean disproof. That is a genuine ambiguity, but it is an instrument-isolation
limit, not evidence that no route can exist on Linux; I decline to promote the report's own lean
toward `UNREACHABLE`.

`evidenceDiscriminates: false`

---

## F-CORE-FILE-03 — NOT EXERCISED (unchanged)

Report explicitly did not attempt this row this pass — cited risk (a mis-aimed drag on the
shared real `:1` desktop could drop a file on another live pane's window) and a missing
press/motion/release primitive in the Wayland virtual pointer. No new evidence produced.

`evidenceDiscriminates: false`

---

## F-CORE-FILE-06 — NOT EXERCISED (unchanged) — report evidence exercises a different component

Clause: an open **markdown document** loads UTF-8, tracks dirty/conflict/deleted state, saves
atomically, reloads, and auto-reloads external changes unless local edits create a conflict;
external deletion/rename is surfaced. SRC: `MarkdownDocument.swift`.

The three cited captures
(`1786725326938102356-file06-external-modify-retry.png`,
`1786725371276811386-file06-external-delete.png`,
`1786725386644971816-file06-restored-clean.png`) all show the **"Changes" tab** (the git
diff/status panel, "Local changes (N)") picking up an external `echo >>`/`mv` on `README.md` from
outside the app. That is a real, live behavior, but it is not the row's subject: I read
`tiller_ui/src/file_view.rs` directly — `FileSystemEventMonitor`, `poll_file_system_events` and
`check_external` are defined and used exclusively inside `FileView` ("a tab showing one path,"
the file-backed editor with dirty/conflict tracking — the actual Rust counterpart of
`MarkdownDocument`), and `FileSystemEventMonitor` has zero other call sites in the tree. The
"Changes" panel shown in the captures is a different component (a multi-file diff list, not "a
tab showing one path") and is refreshed by some other mechanism. No markdown file was ever opened
in an editor tab this pass, no in-app edit/save occurred, and no conflict (external change +
unsaved local edit) was ever created — the row's actual clause was not exercised.

`evidenceDiscriminates: false`

---

## F-CORE-FILE-08 — PASSED

Clause: file icon lookup selects distinct icons by exact filename/extension for files, named
icons for directories.

Live, rendered Files panel across several captures this session (e.g.
`act19-claude-code-clicked.png`) shows visually distinct glyphs for representative types in the
same tree: a plain document glyph for `.md` files (`AGENTS.md`, `CLAUDE.md`, ...), a
git-branch glyph for `.gitignore`/`.git-checkpoint-msg`, a distinct icon for `Tiller.xcodeproj`,
and a generic folder glyph for directories (`App`, `docs`, `rust`, `Scripts`, ...) — all rendered
simultaneously in one live drive, not a static read. Settings → Appearance
(`1786727242693155517-dom03-add-project-clicked.png`'s sibling capture,
`1786727207869944678-dom03-baseline.png` area — confirmed directly in the Appearance screenshot)
shows "File icons: Material," the platform's icon-theme replacement for the reference's
SF-Symbol mapping, matching the row's own PLATFORM note.

`evidenceDiscriminates: true`

---

## F-CORE-SET-01 — half-proven

Clause: settings expose refresh interval (60-3600s clamp), control-socket enablement
(`TILLER_SOCKET_ENABLE`), resume/autoname/translucency, session retention, mount cap, font
sizes, sidebar/right-panel widths — set at accepted/rejected boundaries, restart, inspect
effective values.

Proven live (`set01-malformed-values-resolved.txt`): 5 malformed values
(`appearance.uiFontSize="not-a-number"`, `appearance.terminalFontSize="99999"`,
`appearance.theme="totally-bogus-theme"`, `controlSocket.enabled="maybe-ish"`,
`usage.refreshIntervalMin="-500"`) written directly into the on-disk SQLite `setting` table,
process killed and relaunched fresh against the same DB, then read back live via
`ctl surface.settings.read`: each fell back to its default or clamped to its range boundary
(`13`, `24`, `"system"`, `"true"`, `"1"` respectively). No crash; the control socket itself
stayed live and responsive throughout, which is itself evidence for the socket-enablement
fallback.

Not proven live: `general.summarizerAgent` (the report reasoned from a code read of the DB
allow-list, explicitly flagged as not independently re-read via `ctl`), and the clause's other
named settings — resume/autoname/translucency toggles, session retention, mount cap,
sidebar/right-panel widths, and the `TILLER_SOCKET_ENABLE` **environment-variable** override
specifically (as opposed to the DB-value fallback that was tested).

`evidenceDiscriminates: true`

---

## F-CORE-USG-05 — half-proven

Clause: Codex credentials load from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`, require
access+refresh tokens, merge-save tokens and last-refresh time, and are refresh-needed after 8
days.

Proven live (`usg05-06-07-synthetic-credentials.txt`): a well-formed-but-bogus `auth.json` under
`$CODEX_HOME` (the real code's real precedence order) rules out the "missing credentials" branch
by construction; the real fetch → 401 → `refresh_token()` failure → `LoggedOut` chain executes
end to end against the real `chatgpt.com`/`auth.openai.com` endpoints, observed live in the
status bar ("Codex logged out",
`1786728406096566027-usg-fake-codex-statusbar.png`, confirmed by direct read).

Not proven: `needs_refresh()`'s 8-day time-based gate (re-grepped by the report, still 0
non-test callers — untouched by this drive, a real 401 triggered the refresh, not the time gate)
and the successful-refresh merge-save path (`save_credentials`) — deliberately not exercised
(would need a real, even if short-lived, OAuth grant).

`evidenceDiscriminates: true`

---

## F-CORE-USG-06 — half-proven

Clause: token refresh posts to `auth.openai.com` and classifies HTTP 401 as reused, revoked, or
expired according to the response.

Proven live: a real POST to the real token endpoint with a garbage refresh token produced a real
non-200; `classify_token_refresh_failure()` executes for real inside `refresh_token()` on that
real response.

Not proven, and currently **not provable through any live/observable channel**: the report traced
that `classify_token_refresh_failure()`'s return variant (Reused/Revoked/Expired/Other) is
computed but then discarded by its caller — every variant maps identically to `LoggedOut` in the
status bar. So while the classification code path runs for real, the clause's actual promise
("classifies... according to the response," inspectable via outcome) has no external signal to
distinguish by right now, through this or any other current instrument. Worth flagging as a
standalone product finding, not just an unexercised row.

`evidenceDiscriminates: true` for the proven half; the unproven half is currently
undiscriminable by any observation, which is itself the finding

---

## F-CORE-USG-07 — half-proven

Clause: usage fetching loads credentials, refreshes as needed, calls the backend, parses usage
windows, exposes mapped logged-out/error outcomes, across valid/refresh-needed/missing/rejected
credential states.

Proven live: "valid credentials" state shown throughout the session (real "Codex 100% 5h" usage
data with parsed windows, visible in essentially every capture this pass under the real
`~/.codex` credentials) and "rejected credentials" state via the fake-`$CODEX_HOME` experiment
("Codex logged out," live in the status bar, same capture cited above).

Not proven: "missing credentials" (no `auth.json` at all) and "refresh-needed" (stale >8-day
credentials) states were not driven this pass.

`evidenceDiscriminates: true`

---

## F-GIT-RUN-02 — NOT EXERCISED (unchanged) — report's "UNREACHABLE" framing not adopted

Clause: `GitRunner` streaming emits stderr lines incrementally while running, final result after
completion.

Live: "New Worktree..." opens a real inline form (`1786727295942274649-gitrun02-new-worktree-clicked.png`),
disproving "dead affordance"; text injection lands in the focused field
(`1786727572981819453-gitrun02-refocused.png` confirms "120-gitrun02-test" landed, focus ring
visible). But `wtype -k Return`/`Escape`, an explicit press/release pair, and a literal embedded
newline all failed to submit the form across 4 independent attempts
(`gitrun02-return-key-diagnostic.txt`), and the identical failure reproduced against the
unrelated Chat message box, ruling out a form-specific handler bug. This points at the
injection tool's non-printable-keysym delivery on this compositor/GPUI stack, not at the app.
That is an instrument limitation — a different driver or a real keypress could plausibly submit
it — not proof that no Linux route exists; I decline the report's own lean toward
`UNREACHABLE`. Progress-streaming itself was never reached.

`evidenceDiscriminates: false`

---

## F-TERM-UI-02 — NOT EXERCISED (unchanged) — report's "UNREACHABLE" framing not adopted

Clause: Cmd-click (Linux: Super-click) terminal URLs route through the owning pane's own router.

A URL was typed and appears in the terminal (`term02-url-typed.png`) — though cropped/zoomed
inspection shows it renders with **no distinct hyperlink styling** (same plain color as
surrounding shell text; no underline), which itself leaves open whether a link region was even
registered under the cursor, independent of the modifier question below. `Super` held + click at
the verified position twice (fresh `TILLER_DB` each time) produced no `xdg-open` call either time
(`term02-superclick-fresh.png`). The report itself documents that COSMIC's own compositor very
plausibly intercepts `Super` as a global shortcut before the XWayland client ever sees it, which
would produce this exact negative for reasons unrelated to Tiller's code — an instrument/WM
ambiguity, not a platform impossibility. Only one pane/URL was tested, not the two the clause
asks for. I decline the report's lean toward `UNREACHABLE`.

`evidenceDiscriminates: false`

---

## F-USE-01 — FAILED — absent

Clause: bottom usage bar shows the settings gear, a refresh action, and current worktree info.

Live, across many captures this session: the bar consistently shows a settings-gear icon
(bottom-left) and current worktree/branch info (bottom-right, e.g. "linux/gpui-waku ·
~/Scrivania/Progetti/tiller-linux"). No refresh affordance appears anywhere in the bar under
hover or click in any capture, directly contrasted against the separately-located, functioning
"Refresh now" text control inside Settings → AI Providers
(`1786725407070998351-settings-ai-providers.png`). The clause's "refresh action" component is
confirmed absent from the required location, live — not merely by code read (the ledger's own
prior note, `on_refresh exists at :148 but no refresh button is rendered or wired`, is now
live-confirmed rather than just code-read).

`evidenceDiscriminates: true`

---

## F-USE-02 — half-proven

Clause: enabled provider segments are visible; unavailable segments carry a tooltip.

Proven live: hovering the enabled Codex usage segment with the persistent virtual pointer
produces no tooltip (`use02-hover-codex-nostretch.png`).

Not proven: the clause's actual "unavailable tooltip" case. The PATH-strip meant to force an
unavailable segment state did not actually do so (see F-USE-03 finding below — the segment stayed
"signed in"/live), so `use02-hover-nopath-nostretch.png` is effectively a second hover of the
same enabled-state segment, not a genuinely different condition. No unavailable-state hover was
captured by any instrument this pass.

`evidenceDiscriminates: true` for the proven half only

---

## F-USE-03 — half-proven — one sub-claim in the report is an overclaim

Clause: usage bar shows loading, stale, logged-out, and error display states (plus normal
loaded values).

Proven live, in the actual bar: "loaded" successful values (Codex 100% 5h, essentially every
capture this session) and "logged-out" (fake-`$CODEX_HOME` experiment, "Codex logged out" shown
live, `1786728406096566027-usg-fake-codex-statusbar.png`, confirmed by direct read).

Not proven: "loading" and "stale" states.

**Overclaim found and independently checked.** The report states: "Settings → AI Providers
correctly reflected this: `"available":"false"`, `"status":"Not found on PATH"` for the affected
providers (`02-use03-providers-nopath.png`)." I opened that exact frame, cropped and 2x-upscaled
the Claude Code/Codex cards, and compared it pixel-for-pixel against the pre-strip baseline
(`1786725407070998351-settings-ai-providers.png`, same crop). Both show Claude Code and Codex as
**"Signed in"** with a green status dot — not "Not found on PATH," not "available: false" in any
visible form. The only actual difference between the two frames is that the account email
(`e.palmisano@reply.it`) shown in the baseline's Status row is absent after the PATH strip; the
overall signed-in state is unchanged. No such string or unavailable state appears anywhere in the
cited screenshot. The report's separately-and-honestly-reported finding — that the status bar
itself stays fully live and successful, decoupled from the PATH strip
(`1786725734612025838-use03-statusbar-nopath.png`, confirmed) — stands on its own merits and is
real; the specific "Settings correctly reflected unavailability" claim does not match the
evidence cited for it.

`evidenceDiscriminates: true` for the parts independently confirmed

---

# Summary of report-quality findings (beyond the per-row verdicts)

1. **F-AGENT-SAFE-02**: the report's entire evidence block is for a different row
   (F-AGENT-SAFE-01's clause — "prepare() touches only worktree-local files"), not the row it is
   filed under (F-CORE-ACT... no, F-AGENT-SAFE-02 — `ClaudeHookMigrator` rewriting stale paths).
   `ClaudeHookMigrator` was never invoked. This is the largest miss in the batch: a full page of
   solid live evidence, filed against the wrong ledger row.
2. **F-CORE-FILE-06**: the report's evidence exercises the git "Changes" (diff/status) panel's
   external-change refresh, not the `FileView`/`MarkdownDocument`-equivalent open-editor
   dirty/conflict/deleted tracking the row's SRC names. Source-confirmed: `FileSystemEventMonitor`
   lives exclusively inside `file_view.rs`, and the "Changes" panel is a structurally different,
   multi-file component.
3. **F-USE-03**: the report quotes specific JSON-shaped values (`"available":"false"`,
   `"status":"Not found on PATH"`) as what a cited screenshot shows; the screenshot does not show
   them — both providers still read "Signed in" after the PATH strip, with only the account email
   missing. The bar-decoupling half of the same finding is accurate and independently confirmed.
4. Three rows the report itself leans toward marking `UNREACHABLE` (F-CORE-DOM-03, F-GIT-RUN-02,
   F-TERM-UI-02) are, on the evidence actually gathered, instrument/tooling/compositor
   ambiguities (portal routing to the real desktop; `wtype` not delivering non-printable keysyms,
   reproduced on an unrelated field; possible COSMIC `Super` interception) rather than proof that
   no route can exist on Linux. `UNREACHABLE` is reserved for the latter; I kept all three at
   `NOT EXERCISED`. Had this framing been copied into the ledger as written, it would have closed
   an avenue (per `ENVIRONMENT.md`'s own "the platform forbids it is the most expensive kind of
   wrong answer") that a better driver could still open.
5. Positive: the D-Bus, socket, and DB-restart evidence throughout (ACT-19/20/24/26, API-01,
   SET-01, USG-05/06/07) is genuinely live, real, and well-controlled (positive control for the
   D-Bus monitor, synthetic-but-real Codex credential path, validated grep before trusting a
   zero). This is the strongest evidence quality seen in this slice even where the specific verdict
   lands short of `PASSED`.
