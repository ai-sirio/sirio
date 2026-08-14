# T2-chat build plan — F-CHAT, 19 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T2-chat.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

## Two cross-cutting findings before the rows

**A. The stale-evidence window around the `.h_full()` fix.** `F-CHAT-02`, `F-CHAT-16` and
`F-CHAT-18` were all driven in one P109 batch commit, `2f3a1b4` (2026-08-14 18:12:45), which is
**18 minutes before** the group-surfaces height fix, `23af26c` (2026-08-14 18:30:16, the P117
`.h_full()` fix the parent brief cites). That fix's own report names "blank chat transcript" as
one of the two symptoms it closed. `F-CHAT-02`'s defining finding *is* a blank transcript pane
on a confirmed-sent turn — the textbook symptom of the bug that got fixed 18 minutes later. Later
same-day drives *after* the fix (`sweep E03-chat`/`E04-chat`, 22:19+) show transcript content
rendering normally on live sends. This does not clear the row (its own clause is about an
auth-required banner, which is separately and independently absent from source — see below), but
the "transcript renders nothing" finding should not be read as still-live without a re-drive.

**B. Four rows were marked `FAILED — absent` before the feature was built, and the ledger was
never re-swept.** Confirmed by `git log -S` against `tiller_ui/src/chat.rs`:

| row | verdict's evidence dated | feature landed |
|---|---|---|
| `F-CHAT-28` (subagent task cards) | pass 8 (project-early) | `9c286975` 2026-08-14 14:19 "feat(chat): surface subagent tasks and dismiss unrenderable permissions" |
| `F-CHAT-29` (copy an assistant response) | pass 17, 2026-08-14 02:40 | `398c0aea` 2026-08-14 17:25 "feat(chat): copy assistant responses" |
| `F-CHAT-30` (copy a code block) | pass 17, 2026-08-14 02:40 | `0ecbd525` 2026-08-14 17:26 "feat(chat): copy code blocks" |
| `F-CHAT-32` (edit summary open/revert) | pass 8 (project-early) | `c23da365` 2026-08-14 17:33 "feat(chat): add edit summaries" |

All four now have full render paths, live click handlers, and a passing `#[gpui::test]` each
(`a_subagent_task_card_expands_nested_tool_calls`,
`edit_summary_opens_and_reports_revert_success_or_error`, and the copy controls are exercised
inline by hover-reveal tests near them). `docs/linux-rewrite/INVENTORY-LEDGER.md`'s text for all
four rows has not been touched since the 2026-08-13 20:32 checkpoint commit — i.e. before every
one of the four feature commits above. These are `reclassify` flags, not `build` rows: the
verdict text describes a state of the code that stopped being true mid-project.

---

## `F-CHAT-02` — FAILED — absent

**Needs: build.** The row's actual clause (`01-inventory-app.md:160`) is narrower than its
current evidence: "an authentication-required state with Retry — banner, CLI guidance, Retry."
That specific behavior is genuinely unbuilt, confirmed independently of the stale-transcript
finding above:

- `tiller_acp::AcpError` (`rust/crates/tiller_acp/src/lib.rs:59-71`) has exactly three variants
  — `Timeout`, `Transport`, `Agent(String)` — none auth-specific.
- `tiller_acp::InitializeInfo` (`lib.rs:432-438`) carries only `protocol_version` and
  `agent_capabilities`; no `auth_methods` field, and there is no `authenticate` RPC call
  anywhere in the crate (`grep -rn "authenticate\|authMethods" crates/tiller_acp` — zero hits
  outside a test fixture's inert `"authMethods":[]`).
- `tiller_ui::chat::ErrorKind` has exactly one variant, `Connection` (confirmed independently at
  HEAD by `F-CHAT-33`'s own audit); every `Entry::Error` push site uses it.

So there is no signal to detect ("this session needs auth") and no variant to carry it if there
were. This is a small protocol feature, not a UI tweak: negotiate/observe the ACP auth
requirement, add a typed representation, and only then render a distinct banner + Retry (the
Retry *mechanism* — `retry_pending_send` / `start_connection`, chat.rs — already exists and can
be reused as-is; only the trigger condition and copy are missing). `tiller_ui/src/settings.rs`
already has per-agent CLI login argv literals (`vec!["auth", "login"]` for Claude,
`["auth", "login"]` for others, `settings.rs:503,511,3997,3999`) — reuse those strings for the
banner's CLI guidance rather than inventing new ones.

- **files**: `rust/crates/tiller_acp/src/lib.rs` (detect+classify the auth-required condition),
  `rust/crates/tiller_ui/src/chat.rs` (`ErrorKind` variant, render, wire existing Retry),
  `rust/crates/tiller_ui/src/settings.rs` (reference only, for the existing per-agent login argv)
- **size**: L — a new protocol-level concept spanning two crates, and it depends on how (or
  whether) the installed CLIs actually surface an auth-required condition over ACP at all, which
  is itself unresearched.

---

## `F-CHAT-05` — half-proven

**Needs: both.** The row's real clause (`01-inventory-app.md:163`) is "disable composer input
while waiting for permission or when the agent cannot interact — confirm the editor is disabled
with the corresponding placeholder." Direct code reading resolves the ambiguity the evidence
leaves open:

- `fn can_send(&self)` (chat.rs) is `!self.streaming && !self.connecting && !self.composer.is_empty()`
  — it never inspects `pending_question()`. Whatever blocks sending during a pending permission
  does so only because a permission request necessarily arrives while `streaming` is still true,
  not because of a dedicated permission-wait gate.
- The composer's empty-state placeholder branches only on `self.streaming` ("Type to queue for
  the next turn…" vs "Message...") — there is no distinct placeholder for "waiting on your
  permission answer" vs. ordinary mid-turn streaming, and typing is never actually disabled in
  either state (by design — this is the already-`PASSED` F-CHAT-06 queueing feature). The
  clause's literal "disabled" is not what this build does anywhere.

So the composer's queueable-not-disabled behavior during a permission wait is real and by design,
not a bug masked by bad evidence — but it does mean the clause as written is not satisfied. The
live drives (pass 17 offline, `sweep E03-chat`) also never reached a genuine unresolved
`Entry::Permission` — this build's installed Claude Code CLI auto-applies file edits with a
Pending/Revert card instead of raising `permission_request`. Two independent things are needed:
a decision + possibly a small change on whether "queueable" should count as the clause's
"disabled" (or the clause should be read as satisfied by `can_send()` already being false), and a
live drive that reaches a genuine unresolved permission card to observe it, since none has yet.
Untried lead for reaching a genuine permission card: a Bash/exec tool call outside any
pre-approved allow-list, or a different agent's ACP session (Codex's adapter has not been tried
for this specific clause).

- **files**: `rust/crates/tiller_ui/src/chat.rs` (`can_send`, composer placeholder branch) if the
  decision is to build a distinct permission-wait state
- **size**: M

---

## `F-CHAT-12` — FAILED — defective

**Needs: build.** Confirmed live and by code correlation. The removal itself is correct
(`fn remove_composer_chip`, chat.rs, calls `Composer::remove_chip` + `refresh_token_popups` +
`cx.notify()` — no bug there). The composer's outer container has an `.on_mouse_down` that grabs
`composer_focus` (`this.composer_focus.focus(window, cx)`), and the `×` control is a nested
`.on_click` (not `.on_mouse_down`) inside `composer-chip-{kind}` → `chip-remove-{index}`. Neither
`remove_composer_chip` nor the `×` click handler ever calls `window.focus(&self.composer_focus,
cx)` directly — the only place that happens is the ancestor container's own `on_mouse_down`,
which the live drive's five recovery attempts (including re-clicking that exact container) show
is **not** being reached or is not taking effect after a chip removal. This is a real
focus/routing defect, not a rendering artifact: the same window's terminal pane took keyboard
input seconds later in the same drive, and only leaving and re-entering the chat tab restored it.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (`remove_composer_chip`, the `chip-remove-*`
  `.on_click` handler, and the composer container's `.on_mouse_down`), possibly
  `rust/crates/tiller_ui/src/composer.rs` (`Composer::remove_chip`) if the model mutation itself
  is found to interact with cursor/selection state during investigation
- **approach**: make chip removal explicitly re-request focus (`window.focus(&self.composer_focus,
  cx)` inside `remove_composer_chip`, or on the `×` handler itself) rather than relying on the
  ancestor's `on_mouse_down` to have already fired/still be valid by the time the click completes
- **size**: M

---

## `F-CHAT-13` — NOT EXERCISED

**Needs: reclassify.** The current verdict's framing ("Wayland-lane control reached, produced no
popup — instrument-unreachable, cross-checked against `wayland-virtual-pointer.c`") undersells
what is independently confirmable straight from the app source: `grep -rn "ExternalPaths\|on_drop"
crates/` finds **zero** hits in `tiller_ui/src/chat.rs`, and zero uses of GPUI's `ExternalPaths`
type anywhere in the whole app (the only `.on_drop` sites in the codebase are `sidebar.rs`'s
internal `RowDrag`, `right_panel.rs`'s internal `PathBuf` drag, and `main.rs`'s pane-divider
drag — none of them the OS-level "a file manager dropped a file on this window" gesture the row
names). `ADJUDICATION-BACKLOG.md`'s own Unclear-rows section reached the same conclusion by the
same grep ("no `ExternalPaths`/`on_drop`/drag-file path in chat.rs... Likely absent"). This is not
an instrument problem the tooling will ever solve — no gesture proves a control that isn't wired
to anything. The verdict should read `FAILED — absent`, matching its sibling `F-CHAT-35`'s
phrasing for the same kind of finding, not `NOT EXERCISED`.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (needs an `ExternalPaths` drop target on the
  chat pane, the "Drop files to attach" overlay, chip/message wiring on drop, and a transient
  rejection state for unsupported file types — none of it exists today)
- **size**: M

---

## `F-CHAT-14` — FAILED — defective

**Needs: build.** The manifest's own evidence already fully diagnoses this one — confirmed by an
independent re-grep. `following_edited_files` (chat.rs) has exactly six references: its
declaration, its `false` init, its own menu label, its own toggle, and two test assertions that
only check the bool flips. `FileSystemEventMonitor` (`tiller_markdown/src/file_events.rs`), the
watcher that would make "following" do anything, has zero references anywhere in `tiller/src` or
`tiller_ui/src` — its only mentions are its own module declaration and `pub use` in
`tiller_markdown/src/lib.rs:34,41`. Toggle and engine are each other's missing half; this is the
`SEAMS.md`-shaped defect, not a UI bug. New Conversation (the clause's other half) is real and
already live-proven (P92) — the fix is scoped entirely to the follow half.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (consume the watcher when
  `following_edited_files` is true and a tool call reports an edited-file location),
  `rust/crates/tiller_markdown/src/file_events.rs` (`FileSystemEventMonitor` — currently
  unconsumed, needs a caller), `rust/crates/tiller_markdown/src/lib.rs` (already exports it; no
  change expected)
- **size**: M

---

## `F-CHAT-15` — FAILED — defective

**Needs: build.** The row's clause is "choose a supported permission mode from the composer pill
— open it, choose each available mode, confirm label and behavior change." Confirmed by direct
reading that there is no domain concept to choose from at all, not merely a missing click
handler: `grep -n "AgentMode\|SessionMode\|available_modes\|mode_options"
crates/tiller_ui/src/chat.rs crates/tiller_acp/src/lib.rs` finds nothing, and `AcpEvent`'s full
variant list (`tiller_acp/src/lib.rs`) has no mode-related member alongside its
`ModelCatalog`/`Effort`/`ContextUsage` siblings. The `"Ask"`/`"idle"`/`"working"`/`"offline"`
labels the status pill shows (chat.rs, the `status_pill` build) are a hardcoded 4-way match on
internal UI state (`connecting`/`streaming`/`has_completed_turn`/`client.is_some()`), not a value
read from the agent, and the "Opus Plan Mode" pre-turn text is the *model* picker's degraded
plain-badge fallback (`F-CHAT-36`'s territory), a different control entirely. There is nowhere
for a mode-chooser click handler to even attach, because there is no mode state and no ACP call
to change one — this mirrors `F-CHAT-02`'s auth gap in that the missing piece is a protocol
concept, not UI wiring.

- **files**: `rust/crates/tiller_acp/src/lib.rs` (would need a mode-report/set-mode surface if
  the underlying agent CLIs expose one over ACP — unresearched), `rust/crates/tiller_ui/src/chat.rs`
  (status pill click handler + a new mode-picker popover, modeled on the existing model/effort
  popover as a template)
- **size**: L — genuinely unmodeled protocol concept; whether any installed agent even offers a
  settable session mode over ACP needs research before this can be scoped tighter.

---

## `F-CHAT-16` — FAILED — absent

**Needs: build (narrower than the verdict text suggests).** The popover the pill opens (chat.rs's
`model_picker`, `self.model_picker_open`) is the *same* popover `F-CHAT-17`'s already-`PASSED`
effort selector uses — it renders `self.available_models` as a clickable list when non-empty
(with a "The connected agent did not report any models." fallback when empty) followed by an
effort section when `self.effort` is present. Model selection itself is proven live elsewhere
(`F-CHAT-36`: "both agents upgraded to a working picker (claude 6 models + effort)"). What is
genuinely and completely absent from this same block, confirmed by reading the whole popover
render arm: a search input, and any "Recommended" label or no-match state — none of those three
things exist anywhere in the popover's code, only the effort/model list. The row's "opens an
Effort selector, not a model picker" framing was likely a timing artifact (`available_models` was
still empty at that click, showing only the effort section) rather than a structural wrong-control
finding, but the search/Recommended gap is real and independent of that.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (`model_picker` render block — add a search
  field filtering `self.available_models`, and a "Recommended" designation if the ACP model
  catalog carries one; check `tiller_acp::ModelCatalog`'s fields for a recommended flag before
  assuming it needs adding upstream too)
- **size**: M

---

## `F-CHAT-18` — FAILED — absent

**Needs: reclassify.** The current verdict ("context-indicator click opens nothing... no
percent/token/cost popover ever appears") directly contradicts the code at HEAD. A full,
click-wired, `Escape`-dismissable context popover exists: `context_popover_open` (state field),
`fn toggle_context_popover` (wired to the `context-ring` div's `.on_click`), and a `context_popover`
render block (chat.rs) showing `"{percent}% of context used"`, a `"{used} / {size} tokens"` line,
and a cost line when `ContextUsage.cost` is present — plus a passing drawn test,
`context_ring_shows_reported_usage_and_escape_dismisses_popover`. This evidence was captured in
the same pre-`.h_full()`-fix P109 batch as `F-CHAT-02` (see cross-cutting finding A above); given
the code's own click handler and dismissal path are both present and tested, this needs a
re-drive before the "opens nothing" claim should stand.

That said, a real, narrower gap survives even a successful re-drive: `tiller_acp::ContextUsage`
(`lib.rs:164-171`) carries only `used`, `size`, and `cost` — there is no separate input/output/
cache breakdown anywhere in the type, so even a working popover cannot show the clause's "full
input/output/cache/cost breakdown," only an aggregate used/total figure plus cost. That part is a
genuine, small `build` gap layered under the reclassify.

- **files**: `rust/crates/tiller_ui/src/chat.rs` (context popover — reference only, re-verify by
  driving), `rust/crates/tiller_acp/src/lib.rs` (`ContextUsage` — add input/output/cache fields
  if the ACP wire protocol reports them; check the raw `session/update` payload before assuming
  it needs a new upstream capability)
- **size**: S to re-verify the popover opens at all; S–M for the input/output/cache breakdown once confirmed still missing

---

## `F-CHAT-20` — half-proven

**Needs: exercise.** The follow half is proven live. The manual-scroll-ownership half's mechanism
already exists in code, confirmed by reading: `push_entry` (chat.rs) checks
`self.list_state.is_following_tail()` *before* deciding whether to re-pin
(`self.list_state.set_follow_mode(FollowMode::Tail)`), which is exactly "stop forcing the view to
the bottom once the user has scrolled away, resume once they're back" — `FollowMode`/
`is_following_tail` are GPUI `ListState` primitives, not something this app has to hand-roll. The
gap is purely instrumental: `wayland-drive.sh`'s DSL and `wayland-virtual-pointer.c` expose no
scroll/axis primitive at all (independently re-grepped, matching the manifest). This is the
Wayland lane's limitation specifically — the X11 lane (`DISPLAY=:1`, `xdotool`) has a real wheel
primitive (`xdotool click 4`/`5`) that has not been tried for this row and is the natural next
attempt, since drag/scroll are the two gesture classes `WAYLAND-LANE.md` names as still unproven
on X11 too, not proven-impossible there.

- **files**: none — mechanism already exists (`rust/crates/tiller_ui/src/chat.rs`, `push_entry`)
- **approach**: drive a long stream, then `xdotool click 4` (or a real mouse-wheel event) inside
  the transcript region on the X11 lane; confirm the view stops tracking the tail, then scroll
  back to bottom and confirm it re-pins
- **size**: S

---

## `F-CHAT-25` — NOT EXERCISED

**Needs: exercise.** The row's clause is general — answer a pending question by text, by option,
or by Cancel — and the option-answer and cancel/dismiss halves of the exact same mechanism
(`Entry::Permission`, `respond_permission`, `dismiss_permission`) are already live-proven
elsewhere (`F-CHAT-24`'s Plan-card options, PASSED). What's specifically unproven is the
free-text sub-case: `Entry::Permission.text_input: Option<AnswerTextInput>` and its own render
arm, `render_question_answer_row`, both exist in chat.rs and are drawn-tested, but no live drive
has ever reached a turn where the agent asks a free-text question rather than offering buttons —
because this build's installed Claude Code CLI reports "no AskUserQuestion tool available" over
ACP and falls back to plain chat text instead of a real `Entry::Permission` with `text_input`.
This is confirmed live and directly, not inferred, and it is an environment/CLI limitation, not a
code gap — the render path is built and only the live proof is missing. Untried lead already
flagged in the manifest: Pi's `ui/select`. Also untried: Codex's ACP adapter, which has not been
tested against this specific clause anywhere in the ledger.

- **files**: none — `render_question_answer_row` / `respond_permission` (`rust/crates/tiller_ui/src/chat.rs`) already implement it
- **size**: S once an agent that actually raises a free-text ACP question is found

---

## `F-CHAT-26` — half-proven

**Needs: exercise.** The backend half is proven live. The visual half's code is fully present and
correct, confirmed by reading: the `pending-question-bar` block (chat.rs, gated on
`self.pending_question()`) renders `"Question waiting · {title}"` with a `"Show"` control whose
`.on_click` calls `self.list_state.scroll_to_reveal_item(index)` — exactly the clause's "click
Show, confirm the transcript scrolls to the question card." The manifest's own evidence already
identifies why this was never actually seen: every capture in that drive shows the **Terminal**
tab foregrounded (confirmed by a tab-bar crop check), not Chat, so the stddev-pixel-band evidence
cited measured Terminal scrollback noise. This is a drive-sequencing bug (foreground the wrong
tab, then measure), not a rendering or wiring defect — no code appears to need to change.

- **files**: none — reference only, `rust/crates/tiller_ui/src/chat.rs` (`pending-question-bar` block)
- **approach**: re-drive; explicitly select/foreground the **Chat** tab immediately before
  triggering the plan-mode turn and before every capture (e.g. `tab.select` to the chat tab's own
  index, or click the Chat tab label, right before each screenshot — not once at the start of the
  session)
- **size**: S

---

## `F-CHAT-27` — half-proven

**Needs: exercise.** Same shape as `F-CHAT-26`, same file, same root cause. The data-model half
is proven live and matches source exactly (`expire_unanswered()`/`control_entry_row`,
cross-checked against chat.rs's render arm and its mirror). The claimed "post-expiry frame" is
again the Terminal tab foregrounded, not Chat — no capture ever actually points a camera at the
`Entry::Permission { expired: true, .. }` render arm, which does contain the exact clause text
`"No answer — the turn ended"` (confirmed present in source, both the plain-permission and the
Plan-approval render arms carry it). Nothing to build; the drive needs to look at the right tab.

- **files**: none — reference only, `rust/crates/tiller_ui/src/chat.rs` (the `expired` branch of `Entry::Permission`'s render arm)
- **approach**: re-drive; foreground the Chat tab, let a permission turn expire (or call
  `surface.chat.stop`), then capture with Chat still foregrounded and read the card text directly
- **size**: S

---

## `F-CHAT-28` — FAILED — absent

**Needs: reclassify.** See cross-cutting finding B above. `Entry::SubagentTask` is constructed in
production (`is_subagent_tool_call`, matching titles/raw-input containing "task"/"subagent"/
"dispatch"/"spawn" or a `subagent_type`/`agent_type` JSON key, then `push_entry(Entry::SubagentTask
{..})`), rendered (`render_subagent_task_card`, with its own nested tool-call expand/collapse),
and covered by a full drawn test, `a_subagent_task_card_expands_nested_tool_calls`, which drives
exactly the clause: send a prompt, get a completed subagent card, click to expand it, click a
nested tool call to expand that independently. This landed in commit `9c286975` (2026-08-14
14:19), well after the "pass 8" evidence the current verdict cites. The row is not absent; it is
built and unit-proven, and needs a live drive (ask a real agent to dispatch a subagent/Task tool
call) to close for good — but the verdict text describing it as absent is simply wrong today.

- **files**: none — `rust/crates/tiller_ui/src/chat.rs` (`Entry::SubagentTask`, `is_subagent_tool_call`, `render_subagent_task_card`) already implement and test it
- **size**: S to live-drive once a subagent-dispatching prompt is found for an installed agent

---

## `F-CHAT-29` — FAILED — absent

**Needs: reclassify.** See cross-cutting finding B above. A per-message hover-reveal Copy control
exists on assistant responses: `CopyTarget::Assistant(entry_index)`, an `assistant-copy` div that
is `.invisible()` by default and `.group_hover(hover_group, |style| style.visible())` — i.e. it
only appears on hover, exactly the clause's "hover an assistant response, click Copy" — wired to
`copy_local_text`, which writes the clipboard and shows a "Copied ✓" confirmation for 2 seconds
(`chat.copied_target`, cleared by a background timer) — exactly the clause's "transient
checkmark." This landed in `398c0aea` (2026-08-14 17:25), **14.75 hours after** the pass-17
evidence (02:40 the same day) that the current verdict cites as "the ONLY copy path is
CopyTranscript." That finding was true when written and has been false since. The separate
`ctrl-a`/`ctrl-c` chord-dead finding in the same row's evidence is real but is about a different
control (`CopyTranscript`, the whole-transcript keyboard shortcut) than this clause names.

- **files**: none — `rust/crates/tiller_ui/src/chat.rs` (`CopyTarget::Assistant`, `copy_local_text`, the `assistant-copy` hover control) already implement it
- **approach**: re-drive — hover an assistant message, click the revealed Copy control, paste, confirm text and the 2s "Copied ✓" state
- **size**: S

---

## `F-CHAT-30` — FAILED — absent

**Needs: reclassify.** Same shape and same root cause as `F-CHAT-29`. A per-code-block Copy
control exists: `CopyTarget::CodeBlock { entry, block: id }`, a `code-block-copy-{entry}-{id}`
button in the code-block render arm, wired to the same `copy_local_text` with the same 2s
"Copied ✓" confirmation. This landed in `0ecbd525` (2026-08-14 17:26), also well after the
pass-17 evidence (02:40) the current verdict cites ("neither chat.rs nor tiller_markdown contains
a block-copy control"). That was true at 02:40 and stopped being true at 17:26 the same day.

- **files**: none — `rust/crates/tiller_ui/src/chat.rs` (`CopyTarget::CodeBlock`, the code-block copy button) already implements it
- **approach**: re-drive — click a code block's Copy control (it is not hover-gated the way the
  assistant-message one is; check the code before assuming a hover is required), paste, confirm
  the code text and the confirmation state
- **size**: S

---

## `F-CHAT-32` — FAILED — absent

**Needs: reclassify.** See cross-cutting finding B above. `EditSummaryState` (chat.rs), `fn
render_edit_summary`, `fn request_edit_revert`/`cancel_edit_revert`/`confirm_edit_revert`
(discard-via-git, async, tracks `reverting_path`/`reverted_paths`/`revert_error`) all exist and
are covered by a drawn test, `edit_summary_opens_and_reports_revert_success_or_error`, which
exercises exactly the clause: trigger an edit summary, Open, Revert, confirm the confirmation,
reverted, and error states (the test explicitly checks `edit-summary-error-2` renders on a failed
git discard). This landed in `c23da365` (2026-08-14 17:33), long after the "pass 8" evidence the
current verdict cites. Not absent; built and unit-proven; needs a live drive.

- **files**: none — `rust/crates/tiller_ui/src/chat.rs` (`EditSummaryState`, `render_edit_summary`, the revert flow) already implements and tests it
- **size**: S to live-drive (send a prompt that edits a file, then Open/Revert from the resulting card)

---

## `F-CHAT-33` — half-proven

**Needs: build (MCP half only).** The turn-error half already works and is unaffected. The
MCP-configuration-warning half is genuinely and completely absent, independently reconfirmed:
`grep -rin mcp crates/tiller_acp/src crates/tiller_ui/src/chat.rs`, excluding tests and comments,
returns zero hits. There is no MCP concept anywhere in the ACP client layer to warn about — this
would need the ACP session layer to first recognize an MCP-server configuration problem (if the
protocol/agent surfaces one at all — unresearched, same caveat as `F-CHAT-02`/`F-CHAT-15`) before
a new `ErrorKind` variant and banner could render it.

- **files**: `rust/crates/tiller_acp/src/lib.rs` (detect/surface an MCP config problem, if the
  protocol exposes one), `rust/crates/tiller_ui/src/chat.rs` (new `ErrorKind` variant + banner,
  reusing the existing `Entry::Error` render/OK-dismiss machinery)
- **size**: M–L depending on what the ACP wire protocol actually reports for MCP server failures

---

## `F-CHAT-34` — NOT EXERCISED

**Needs: build; also flag for reclassify.** `chat_sessions()` (`rust/crates/tiller_persistence/src/db.rs:523`)
exists with zero production callers anywhere (`grep -rln chat_sessions crates/` finds only the
persistence crate itself and its own test file) — matching `ADJUDICATION-BACKLOG.md`'s standing
finding verbatim. There is no browse/open/delete UI anywhere in `tiller_ui` — `grep -rn
"ChatHistoryMenu\|chat_sessions\|past chats" crates/tiller_ui/src crates/tiller/src/main.rs`
finds nothing. The current `NOT EXERCISED` verdict reads as "nobody drove it yet," but the
correct read is the same as `F-CHAT-35`'s own `FAILED — absent`: there is no route to drive at
all, only a resume-most-recent path (`ChatSession::restore`, main.rs). Worth a reclassify flag
alongside the build, since this and `F-CHAT-35` are one missing feature (see next row).

- **files**: `rust/crates/tiller_ui/src/chat.rs` (new history list/menu — likely off the overflow
  menu next to Follow Edited Files / New Conversation, per `F-CHAT-14`), `rust/crates/tiller_persistence/src/db.rs`
  (`chat_sessions()` exists; a delete-session query does not and would be needed), `rust/crates/tiller/src/main.rs`
  (wiring — the overflow menu and its actions are owned there for other chat controls; check
  before assuming chat.rs alone can own this)
- **size**: L — new UI surface, a new persistence query, and app-level wiring; build once for
  this row and `F-CHAT-35` together (see shared cause below)

---

## `F-CHAT-35` — FAILED — absent

**Needs: build. Shared cause with `F-CHAT-34`.** These are the same missing feature: `F-CHAT-34`
is "browse/open/delete past chats," `F-CHAT-35` is "the empty state when there are none to
browse." Both require the same new history-list surface described above — there is nothing to
build separately for the empty state once that surface exists (it is a single conditional branch
inside it: render "No past chats" when `chat_sessions()` returns empty instead of the list).
Building `F-CHAT-34`'s surface and closing `F-CHAT-35` should be one brief, not two.

- **files**: same as `F-CHAT-34` — `rust/crates/tiller_ui/src/chat.rs`,
  `rust/crates/tiller_persistence/src/db.rs`, `rust/crates/tiller/src/main.rs`
- **size**: folded into `F-CHAT-34`'s L; no separate size if built together
