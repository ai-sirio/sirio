# B2-chat — critic verdicts

Verified from `linux/gpui-waku` at the integration HEAD (`358e72c`), driven entirely on the
Wayland lane (`Scripts/wayland-drive.sh`, `TILLER_WL_KEEP=1` follow-up sessions), no lock taken.
I did not build this slice and did not read the builder's reasoning; only the structured report
and the live app were used.

Screenshots referenced below live under `reference/linux-progress/verify-B2-chat/`. Raw session
logs/transcripts (JSON reads via the control socket) were kept as tool output in this
transcript, not committed, per the project's own convention of committing only durable
proof-artifacts (images) alongside the verdicts doc.

## Static/test confirmation

`cargo build -p tiller` clean. The six tests the report names all exist and pass in isolation:

```
cargo test -p tiller_ui --lib -- dropping_external_files_attaches_chips_and_rejects_the_oversized_one \
  dropping_external_files_is_refused_during_permission_wait \
  permission_wait_disables_the_composer_and_shows_its_own_placeholder \
  following_edited_files_opens_the_tool_calls_reported_location \
  model_picker_search_filters_and_badges_the_recommended_model \
  attach_control_accepts_one_image_and_rejects_the_rest
-> test result: ok. 6 passed; 0 failed
```

A green test is not treated as proof below — every row was independently driven live against a
real running `tiller` instance and (where the row needed a real agent turn) a real
`npx @agentclientprotocol/claude-agent-acp` session, not a fixture double.

## F-CHAT-05 — composer disable during permission-wait

**half-proven, unchanged.** The offline half (composer inert with no placeholder from `●
offline`) is untouched by this wave and stands from pass 17; not re-driven here.

The permission-wait half remains unreached — but this pass went further than any prior sweep in
establishing *why*, definitively rather than empirically:

- **Claude is now provably, not just observedly, incapable of raising a `permission_request`
  here.** `~/.claude/settings.json` sets `"permissions":{"defaultMode":"auto"}` and
  `"skipDangerousModePermissionPrompt": true` — a global developer setting, not an
  environment quirk that a cleverer prompt could route around. Every prior sweep's "tried
  several prompts, none worked" is explained by this one line.
- **Codex was tried as the alternate route the report itself suggests** (`TILLER_ACP_PROGRAM`
  pointed at a wrapper invoking `npx -y @agentclientprotocol/codex-acp@latest`, the same
  package this codebase already drives successfully elsewhere per
  `CRITIC-findings-log.md:2211`). It fails before reaching a turn at all:
  `surface.chat.open` returns `{"kind":"error","text":"could not launch ACP agent: ACP agent
  requires authentication (API Key, ChatGPT)"}` — the installed `codex` CLI's own ChatGPT-session
  auth (`~/.codex/auth.json`) does not carry over to the `codex-acp` bridge, and no
  `OPENAI_API_KEY` is present on this machine. Not touched further (extracting/injecting a key
  is out of scope for a verifier).
- **Pi, OpenCode and Oh-My-Pi are not reachable at all through this code path**:
  `tiller_agents::{pi,opencode,omp}::acp_program()` all return `None` by design ("no ACP
  server... a chat tab must not be offered for it") — confirmed by reading all three adapters.
  Claude and Codex are the *only* two catalog entries with an ACP program, and both are now
  confirmed blocked for independent reasons.

New code verified structurally: `can_send()` (chat.rs:1539) explicitly gates on
`pending_question().is_none()`; `insert_text`/`backspace`/`delete`/`send` each early-return
under the same condition; the empty-composer placeholder branches on
`pending_question().is_some()` before falling through to the streaming/idle cases
(chat.rs:5444-5470). The row's own test drives a real (fixture) `Entry::Permission` with
`resolved: None` through the full app, not a mock, and passes. No live discriminating evidence
of the actual gated behaviour was obtained this pass either, so this does not move past
half-proven.

## F-CHAT-12 — chip removal keeps composer focus

**PASSED.** This is the exact regression the ledger documented from P92 ("five recovery
attempts... produced nothing... only leaving the chat tab and returning restores input"),
reproduced through the identical mechanism and now shown fixed, live, with text-based (not
just pixel) discrimination.

The "+" attach control opens a native file-portal dialog this harness cannot drive (confirmed:
`attach_test_paths` is a test-only injection point, matching `ENVIRONMENT.md`'s documented
portal-picker limitation) — but `remove_composer_chip` + the click handler at chat.rs:5525 is
generic over chip kind, so a `@`-mention file chip exercises the identical code path F-CHAT-12
fixed. Drove it through a real `@`-mention (typed `@`, clicked `src/main.rs` in the live mention
popup — `f12-01-chip-inserted.png`), then clicked the chip's `×` (`f12-02-chip-removed-still-
focused.png` — composer shows the empty placeholder again but keeps its focus ring), then typed
`STILLFOCUSEDPROBE` **with no intervening click**. `surface.chat.read` immediately after showed
`"composerText":"STILLFOCUSEDPROBE"` and the frame confirms it rendered
(`f12-03-probe-text-landed-no-click.png`). Before this fix that exact sequence left the field
dead until the tab was left and reopened.

## F-CHAT-13 — drag-and-drop file attach

**NOT EXERCISED** (was `FAILED — absent`; the basis for that verdict — structural code absence
— no longer holds, so it is not carried forward unchanged).

Code now genuinely exists: `chat-root` chains a real `.on_drop(cx.listener(Self::
drop_external_paths))` gated on `can_accept_drop()` (chat.rs:5863, reusing F-CHAT-05's own
`pending_question().is_none()` gate), a `drag_over::<ExternalPaths>` overlay
(`chat-drop-overlay`), and `drop_external_paths` (chat.rs:2083) classifies paths into image vs
`@`-style file chips with the oversized-image rejection the report describes — all confirmed by
direct reading, not taken on the report's prose. Its two new tests drive real
`gpui::FileDropEvent` objects (not a hand-rolled double) and both pass.

The live drag gesture itself is categorically unreachable by any harness available on this
machine, not just this lane: `ENVIRONMENT.md` states outright that "XDND drags are equally out
of reach: `xdotool` has no source window to negotiate the protocol, so file-drop rows are
unexercisable by this harness (a human hand can still do them — record NOT EXERCISED..., never
FAILED)" — and independently, `Scripts/wayland-virtual-pointer.c` (read, not compiled)
implements only `motion_absolute`/`button`/`frame`, no `wl_data_device` drag-offer call at all.
No file manager or other drag-source client runs in this headless environment either, so there
is nothing to originate a drop from even off-instrument. This is the same wall `E03-chat-
verdicts.md` hit on this exact row before the code existed; it now blocks the *gesture* rather
than proving the *code*.

## F-CHAT-14 — Follow Edited Files opens the tool call's location

**FAILED — defective.** Live-driven end to end and caught a real, replayable integration
defect the report's own claim does not survive contact with.

Toggled Follow Edited Files on through the real overflow menu (confirmed twice: once by the
click landing, once by reopening the menu afterward and seeing its own label flip to "Stop
Following" — `f14-01-follow-toggled-on.png`), then sent a real turn to a live
`claude-agent-acp` session asking it to edit `README.md`. It did: `Read README.md` then
`Edit README.md` both completed (confirmed independently by reading the file off disk — the new
line landed), and the installed `claude-agent-acp` package's own source
(`tools.js:123`, read directly) shows its Edit tool call reports
`locations: input?.file_path ? [{path: input.file_path}] : []` — a real, non-empty location, not
a hypothetical one. `following_edited_files_opens_the_tool_calls_reported_location`'s own unit
test confirms `maybe_follow_location` correctly turns that into `ChatEvent::OpenFile` in
isolation.

No file tab opened. Two independent forced-repaint captures several seconds apart after the
edit completed both show only the pre-existing `Chat`/`Terminal` tabs
(`f14-02-edit-completed.png`, `f14-03-no-new-tab-final-check.png`).

Root cause, confirmed by reading `rust/crates/tiller/src/main.rs`: `TillerWorkspace::bind_chat`
— the only code anywhere that subscribes to a `Chat` entity's `ChatEvent` stream — has exactly
two call sites, `add_chat_tab` (:4184) and `resume_chat` (:4237). The two functions that build
the chat tab a real user actually lands on when opening or switching to any worktree —
`restore_tabs` (:7618, used at initial window construction) and `restore_tabs_in_workspace`
(:7740, used on worktree switch/session merge) — both construct `TabContent::Chat` directly and
never call it, unlike their sibling terminal tabs, which both call sites *do* wire via
`Self::bind_terminal_tabs(&tabs, cx)` immediately afterward. `surface.chat.open`'s control
handler only ever finds an existing `TabKind::AgentChat` tab (never creates one via
`add_chat_tab`), so it always resolves to one of the unwired restoration paths. The row's report
claims the seam is "already consumed live by the host... reusing that live seam needed no
foreign-file change at all" — that is true of the code in isolation, but false for the tab type
every real session actually uses, which is exactly what this row's own `howToExercise` walks
through. This is very likely also a live defect for F-CHAT-32's "open" link (same event, same
missing subscription), which is out of this slice's scope to verdict but is left as a note for
whoever owns it.

## F-CHAT-16 — model picker search, Recommended badge, no-match state

**PASSED.** Live-driven against a real Claude session's actual advertised model catalog
(`Default (recommended)`, `Opus (1M context)`, `Fable`, `Sonnet`, `Haiku`, `Opus Plan Mode`),
not a fixture:

- Clicked the model pill: search input + full list + `Recommended` badge on the first entry
  render (`f16-01-search-field-and-recommended-badge.png`).
- Typed `son`: list narrows to `Sonnet` + `Opus Plan Mode` (the latter matching on id/description,
  not name — legitimate per the ported filter's own semantics), `Recommended` badge gone since
  its row was filtered out (`f16-02-filtered-to-sonnet.png`).
- Typed further nonsense: `No models match` renders (`f16-03-no-models-match.png`).
- 20x Backspace: full six-row list returns, badge restored
  (`f16-04-backspace-restores-full-list.png`).
- `surface.chat.read`'s `composerText` was read after every keystroke throughout and stayed
  `""` the entire time — the search query never touched the message being composed underneath,
  the row's third conjunct, confirmed by socket-level state rather than inferred from the
  screenshot.

All conjuncts of the clause were driven live and each produced the expected, discriminating
result (none of these states — search box, filtered list, no-match text, badge presence/absence
— is what a fresh/default render would show).

## F-CHAT-18 — context usage token breakdown

**FAILED — absent, unchanged** (builder reported `not-built`; per the verification brief this
row is not promoted on intent, so this is confirmation, not a new drive).

Independently re-confirmed both of the report's structural claims: `tiller_acp::ContextUsage`
(`rust/crates/tiller_acp/src/lib.rs:210`) carries only `used`/`size`/`cost` — no
input/output/cache-token fields exist anywhere on it — and none of this wave's three code
commits (`82d4ec5`, `7c8c787`, `ce3c13e`) touch the context-usage render arm; only the doc
commit (`62904af`) mentions this row. The block is real and current.

## Verdicts

| id | verdict | why |
|---|---|---|
| F-CHAT-05 | half-proven | Offline half unchanged (pass 17). Permission-wait half still unreached, now with the root cause nailed down for both ACP-capable agents: Claude's own `~/.claude/settings.json` sets `defaultMode:"auto"` (global, not prompt-dependent); Codex's ACP bridge fails auth outright (`ACP agent requires authentication`), confirmed live. Pi/OpenCode/omp have no ACP program at all. New guard code verified structurally + its fixture-agent test passes, but no live discriminating evidence of the gated behaviour itself. |
| F-CHAT-12 | PASSED | Live: removed a chip via its × control, typed with zero intervening click, `surface.chat.read` showed the keystrokes landed in `composerText` immediately — the exact P92 regression, now fixed and reproduced with text-level (not just pixel) evidence. |
| F-CHAT-13 | NOT EXERCISED | Code now genuinely exists (real `on_drop`/`drag_over`, gated on the F-CHAT-05 rule) and its two new tests drive real `FileDropEvent`s and pass — moved off `FAILED — absent` since that verdict's basis (structural code absence) is gone. The live drag gesture is categorically unexercisable by any harness on this machine (`ENVIRONMENT.md`'s own documented XDND limitation; the Wayland virtual-pointer client implements no drag-offer protocol at all), so it cannot be promoted further. |
| F-CHAT-14 | FAILED — defective | Live-driven fully: toggled Follow on for real, a real Claude turn produced a real Edit tool call with a real reported location (confirmed via the installed `claude-agent-acp` package's own source and the file's actual on-disk change) — but no file tab opened, confirmed by two separate forced-repaint captures. Root cause: `TillerWorkspace::bind_chat` is only wired from `add_chat_tab`/`resume_chat`; the two functions that build the tab a real user actually uses (`restore_tabs`, `restore_tabs_in_workspace`) never call it, unlike their sibling terminal-tab wiring at the same sites. The report's "already consumed live by the host" claim does not hold for the tab this row's own exercise steps land on. |
| F-CHAT-16 | PASSED | Live against a real agent's real model catalog: search filters correctly (by name and by id/description), Recommended badge tracks the filtered set, no-match state renders, Backspace fully restores the list, and the composer's own draft was independently confirmed untouched via socket reads throughout. All conjuncts driven, all discriminating. |
| F-CHAT-18 | FAILED — absent | Unchanged; builder reported `not-built`. Independently re-confirmed `ContextUsage` still has no token-breakdown fields and no code for this row shipped this wave. |
