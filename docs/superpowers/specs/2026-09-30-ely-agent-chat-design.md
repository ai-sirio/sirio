# Ely components for the agent chat — design

**Date:** 2026-09-30

**Status:** proposed; layout approved, written spec awaiting review

**Scope:** the entire AI chat surface in Sirio.

## §0 Intent and agreed decisions

The user wants to replace the chat's presentation with Ely's Chat and Agent
components, using the Gallery as the component reference. A conversation
should make the agent's response, tools, progress and requests easy to read,
while leaving the composer available for the next prompt.

Decisions confirmed in the conversation:

| Item | Decision |
|---|---|
| Coverage | All of the AI chat: transcript, reasoning, tools, plans, subagents, requests, errors, attachments, queue and composer. |
| Appearance | Ely components with Sirio's existing theme, fonts and UI scale. |
| Integration | Vendor a pinned Ely revision and adapt it to Sirio's existing Bezel GPUI fork. |
| Layout | The interactive mockup's centered conversation, compact activity cards and fixed composer. |
| Agent image | The actual agent's logo, in white: Claude Code's pixel mark, Codex's own mark, and the corresponding marks for other agents. |

Success means that a user can perform today's chat interactions through the
new presentation, with no lost draft, transcript, selection, tool detail or
agent capability. The shell, sidebar, Settings, terminals and other tabs
keep their current components. Updating the app's GPUI foundation is outside
this design.

The portable [layout reference](assets/2026-09-30-ely-agent-chat-layout.html)
includes light/dark themes, wide/narrow panes, agent logo examples, tools,
questions, permissions, queue, completion, interruption and errors. It uses
HTML and synthetic data; it demonstrates layout, not native compatibility.
Its illustrative model names, durations, strings and picker options do not
define the production catalogues or introduce a localization change. Where
the prototype simplifies behavior, the requirements below govern.

## §1 Existing contracts

`sirio_ui::chat::Chat` owns the live UI state, receives `AcpEvent`s through
`ChatClient`, and renders both ACP sessions and native Claude sessions. The
transport adapters and persistence already implement the workflows being
presented. They remain the source of truth.

The view also owns durable `gpui::ListState`, transcript selection and its
document-wide offsets, turn navigation, folds, nested scroll handles,
reasoning takeover, pickers, accepted mentions, attachments, drafts,
permission responses and the FIFO prompt queue. Preserve their behavioral
contracts across the component migration.

The migration keeps the public `Chat` interface, `ChatEvent` integration,
protocol messages, control socket behavior and persistence schema. Shared
Markdown helpers used by file and change request views keep their existing
API and rendering behavior. No new headless session model replaces `Chat`.

## §2 Dependency and compatibility boundary

The reference is `ely-gpui-component` 0.1.0 at commit
`e17e31a6890c09ebcfa8b61133d7bc7c625edf69`, under MIT OR Apache-2.0.
The reviewed upstream manifest requires Rust 1.95 and upstream Zed GPUI
revision `1a28cff4b409169bac058bca40dfbfeb7621d19b`. Sirio uses
`bezel-gpui` and `bezel-gpui-platform` at `=0.3.8`. These types cannot be
assumed interchangeable.

Vendor Ely under `rust/vendor/ely-gpui-component`, keeping its licenses and
recording the upstream URL, revision and local changes in the vendor
documentation. Its GPUI dependencies must resolve to Sirio's existing
packages and patches, producing one GPUI type graph. Cargo lockfile changes
must preserve the Linux and Windows GPUI patches and Ghostty's CPU baseline
patch. Keep Sirio's platform features, including Linux Wayland/X11.

Build the chat and agent modules plus the primitives they require. Gate
unrelated Ely modules and dependencies if they otherwise pull in a second
terminal, webview or platform stack. The app gains no Ely Gallery runtime,
browser frontend or alternate terminal renderer.

The first implementation milestone must prove a real native Ely chat
component and input can build and render against the existing fork. Check
the workspace toolchain against Ely's actual API requirements. If this needs
a broad GPUI/platform migration or unrelated subsystem changes, report the
concrete incompatibility and return to design review before extending scope.
The current exploration has not established native binary compatibility.

### Initialization and assets

Provide chat-focused initialization for the required Ely theme, components
and scoped actions. Upstream's general initialization loads its fonts and
registers unrelated editor, document and terminal actions; those registrations
must not override Sirio's focus or shortcuts.

Compose Ely's embedded assets with Sirio's asset source. Give Ely assets a
distinct path namespace and update their references consistently so an Ely
icon cannot replace an existing Sirio asset with the same path. Keep asset
loading offline and deterministic, and preserve license attribution.

## §3 Sirio theme and agent identity

A single adapter maps Sirio's resolved appearance, base colors, semantic
colors, typography, radii, spacing, UI scale and motion preferences to Ely.
It updates when those inputs change, including System appearance changes;
rendering a message does not repeatedly mutate a global theme. Ely's theme
type can coexist with Bezel's theme without changing Bezel's palette.

Use Sirio's resolved UI and code font families and scaled sizes. Do not
install Ely's default Inter/JetBrains Mono appearance as a second preference.
The chat surface follows Sirio's existing opaque surface behavior. Warning,
success and danger indicate reported state using Sirio's semantic colors.

### White agent logos

Resolve identity from the chat's stable agent ID, including existing ACP
aliases; changing a model does not change the agent logo. The header,
assistant avatar and any agent identity control inside the chat use the same
white monochrome mark. The mark sits on a compact dark neutral plate in both
appearances, so it remains visible in the light theme. The adjacent agent
name and accessible label identify it without relying on its shape.

| Agent ID | Mark |
|---|---|
| `claude` / `claude-acp` | Claude Code's pixel mark, white. |
| `codex` / `codex-acp` | Codex's dedicated mark, white. |
| `opencode` | OpenCode's mark, white. |
| `pi` | Pi's monogram, white. |
| `omp` | Oh My Pi's existing silhouette, rendered monochrome white. |

Reuse the existing icon geometry and provenance where available. Claude
Code's pixel geometry is available in the same LobeHub package version
already vendored by Sirio: `@lobehub/icons-static-svg` 1.95.0,
`claudecode.svg`. Add its asset with attribution during implementation.
The chat-specific mark renderer must also support a monochrome treatment of
currently chromatic assets without changing their global use.

For an agent with no known mark, show its name with a neutral placeholder;
never borrow another agent's logo. For a subagent with no reported provider
identity, show its task label without assigning an invented provider logo.
Activity colors remain on status indicators rather than tinting the logo.
Existing marks outside the chat retain their current appearance.

## §4 Layout and component mapping

The chat fills its pane. A compact context header names the actual agent and
shows its connection/activity state. The transcript scrolls in a centered
reading column; the bottom request/queue/composer region stays visible.
Widths, padding and typography derive from Sirio tokens, with the mockup's
approximately 760 px reading column as the visual reference at default scale.

User messages form quiet bubbles aligned right. Assistant responses are
left-aligned readable prose, with the agent avatar and header. Related
reasoning and activity follow in conversation order. Assistant prose does
not gain a heavy card background. Message metadata appears only when known.

| Surface | Ely basis | Sirio integration |
|---|---|---|
| Conversation | `ChatContainer`, `MessageBubble`, `MessageAvatar`, `MessageHeader`, `MessageFooter` | Existing transcript content, identity and actions. |
| Scrolling | `ScrollToBottomButton`; message-list presentation | Existing virtualized list, navigation and selection state. |
| Reasoning | `ThinkingBlock`, `ThinkingIndicator`, `ThinkingDuration` | Existing takeover, timing and bounded selectable body. |
| Tools | `ToolCallCard`, `ToolCallGroup` | Current statuses, arguments, output, locations and rich content. |
| Plans and subagents | `AgentPlan`, `AgentStatus`, progress/step primitives | Only progress and relationships present in existing entries. |
| Requests | `PermissionPrompt`, `HumanInputRequest` | Exact protocol options, IDs and supported answer forms. |
| Draft and context | `PromptInput`, `ContextChips`, `AttachmentChip`, `AttachmentButton`, `DragDropOverlay` | Existing editing controller, mentions and attachments. |
| Submission | `SendButton`, `InputHint` | Existing eligibility, queue and cancel operations. |
| Failures | `ErrorMessage` and compatible notice surfaces | Existing error kind, retryability and authentication flow. |

Chat-specific menus and history popovers adopt Ely's matching primitives,
retaining their existing actions and focus handling. The chat menu preserves
following edited files, starting a new conversation and opening past chats.
History keeps current open/delete operations, deletion confirmation and empty
states. File following retains the current reported-location handoff and
throttling. Existing dated turns keep their day separators.

Ely's streaming and code presentation can frame content, but Sirio's parsed
Markdown and selectable renderers remain responsible for text where replacing
them would lose selection, syntax, diagrams or link behavior. Do not parse an
unchanged Markdown document again on every redraw.

### Responsive and keyboard behavior

At narrow pane widths the reading column uses the available width, controls
wrap, long labels truncate with access to their full text, and secondary
metadata yields space. Send/stop and request actions remain reachable. Tool
outputs and code scroll within their own wells rather than widening the pane.
Long request content can scroll without pushing the composer out of view.

Preserve existing tab order, focus restoration, picker navigation, composer
shortcuts and request keyboard handling. Interactive controls have names and
visible focus. IME composition must not submit a prompt; clipboard commands
must act on the focused selection/input, including platform Cmd/Ctrl behavior.
No new app-wide shortcut follows from Ely initialization.

## §5 Transcript and agent activity

Every current `Entry` variant has a presentation:

- **User/Assistant:** full selectable content, Markdown, links, code and
  existing copy actions. Preserve plain-text selection copy and source-copy
  semantics as separate existing operations.
- **Thought:** opens while streaming and settles closed only when the user
  has not taken control. Manual expansion survives redraws and virtualization.
  Preserve measured duration, auto-follow and bounded nested scrolling.
- **ToolCall:** name, status, summary and measured duration when available;
  expandable raw arguments/results, rich output, file locations, inline diffs
  and existing editor handoffs. Failure is visible without opening the body.
- **SubagentTask:** current task label, status and grouped child tool calls.
  Keep the existing conservative grouping; the current protocol does not
  supply a complete parent/child graph, so do not invent one.
- **Plan:** reported step order/status and current approval flow. Progress
  counts come from the entries rather than a cosmetic animation.
- **Permission:** pending, resolved, expired and dismissed states, including
  structured questions. The active request is in the bottom dock (§6).
- **TurnFooter/Notice:** existing completion, compaction, background-task and
  warning information, without fabricated statistics.
- **RewindPreview/RewindReport:** dry-run file counts, errors, confirmation,
  restoration results and skipped-link information.
- **Error:** existing connection/authentication and retryable failures (§8).

Consecutive compatible tool calls may share a collapsible group, preserving
their order. Grouping never consumes a permission, plan, thought or prose
entry and never hides a running, failed or awaiting-input state. Each call
still has its own expandable detail and stable identity.

Keep the existing turn rail, older-turn folding, jump-to-turn previews and
scroll-to-bottom behavior. A user reading earlier content is not dragged to
the bottom by an incoming chunk. Expanding a card or changing the pane width
remeasures affected rows without resetting the user's scroll position.

Tool bodies, thoughts and code use the existing edge-aware scroll chaining:
their well consumes scrolling until its edge, then the transcript can scroll.
Selection across rendered rows uses the existing document-wide offset space,
including Select All and copy; a component's private selection cannot split
the transcript into unrelated documents.

File rewind remains the existing capability-gated operation: live eligible
turn, no active stream, dry run, explicit confirmation and result. This UI
migration does not widen checkpoint eligibility or restore conversation state.

## §6 Requests, queue and composer

The bottom region is ordered as active request, queued prompts, composer and
input hint. Requests use the full reading width. An unanswered request stays
visible as the transcript grows; its eventual outcome remains in history.

Permission buttons are generated from the agent's advertised options and
send their exact IDs through the current handlers. Ely's fixed Once/Always/
Deny enum is not a replacement for the protocol catalogue. Do not offer
session-wide approval if the agent did not provide it. Question choices,
multi-step questions, free-text availability, validation, answer drafts,
timeouts and dismissal preserve the existing behavior. A raw prompt/tool
description remains readable and selectable.

The composer has context/attachment chips above the editor, with agent mode,
model, effort and advertised optional controls below it. It grows within
bounded height and scrolls thereafter. Preserve text, caret, accepted mentions,
slash commands, model search, image paste/drop, attachment removal and current
capability checks. Send eligibility includes supported attachment-only prompts.

`Enter` submits when idle and appends a valid prompt to the existing FIFO queue
while running; `Shift+Enter` inserts a newline. Queue controls keep removal,
clear-all, expansion and send-now operations, including cancellation and
ordering when sending a queued item immediately. The running action cancels through the existing
client; it does not clear the draft. Request and connection states apply the
current submission rules, and a draft remains editable wherever it is today.
The mockup's simplified disabled fields do not introduce new restrictions.

Model/mode/effort menus use reported catalogues and selected values. Fast mode,
thinking display and background-task options appear only for supported agents.
Do not replace agent-specific modes with the prototype's Ask/Plan/Auto list.
The context meter distinguishes unavailable usage from actual zero and shows
only reported or currently computed usage. No model, duration or percentage is
invented to fill a component's empty state.

## §7 Component seams and state ownership

Extract the rendering affected by this migration from the large `chat/mod.rs`
into focused chat modules, extending the existing `composer_view`,
`transcript`, `list_scroll`, `thought`, `tool_calls`, `turn_rail` and
`question_dock` boundaries. Keep the current event ingestion and behavioral
model; these modules continue to use its state.

| Unit | Responsibility | Inputs and effects |
|---|---|---|
| Ely adapter | Theme/asset mapping and component compatibility helpers. | Resolved Sirio theme and existing component state; no transport logic. |
| Transcript renderer | Layout, virtualized rows and Ely message/activity surfaces. | Existing entries, list/selection/fold handles and chat actions. |
| Request renderer | Active request and historical outcomes. | Existing request state; delegates answers to current handlers. |
| Composer renderer | Ely input shell, context, queue and controls. | Existing editing/attachment/catalogue state; delegates submit/cancel. |
| `Chat` | Connection, event processing, state changes and host integration. | Existing protocol and persistence contracts. |

Make minimal documented extensions to the vendored components where upstream
does not expose the required seam:

1. **Tool/thought/group bodies and expansion:** custom selectable body slots
   and externally controlled expansion keyed to stable chat identities.
   Upstream's private keyed toggle must not override Sirio's takeover or
   lose expansion when a virtualized row remounts.
2. **Prompt editor and submission:** an editor slot plus host-supplied submit
   eligibility and callbacks. Keep Sirio's editing controller inside Ely's
   visual shell, avoiding a second input entity or doubled container. Ely's
   default nonempty-text/busy checks must not swallow queue or attachment-only
   submission. Required keyboard events reach Sirio's existing handlers.
3. **Message list:** use the existing `ListState` with Ely rows and scroll
   affordances. Do not introduce Ely's independent private feed/scroll state.
4. **Requests:** protocol-driven option labels/IDs, supported input slots and
   current validation. Existing outcome states remain representable.
5. **Theme and identity:** chat-focused initialization, namespaced assets and
   custom agent marks without implicit global brand tinting.

Document the reason for every local extension and which upstream file it
changes. These seams allow real Ely components to carry Sirio's behavior.

## §8 Failures and restored sessions

Connecting, authentication required, interrupted, complete and connection
failure remain distinct. Show the existing actionable authentication guidance
and retry/reconnect operation where currently supported. An error does not
erase the partially received response, the user's draft or the queue. A
transport failure follows current cancellation/permission resolution behavior.

Restore entries through the existing conversion and render the same Ely
surfaces. Persisted data currently omits some rich tool details; show what
exists and do not manufacture raw output. Historical permissions remain
resolved/expired, plans do not regain live approval, and restored thoughts
start in their established closed state. Ephemeral rewind previews/reports
stay ephemeral. No database migration is needed for visual expansion or logos.

## §9 Validation and acceptance

Validation accompanies implementation after the written spec and plan gates.
This design-only change requires document/link and prototype checks, not a
product build.

### Compatibility evidence

- Record the toolchain and resolved GPUI packages for the first native build.
  Prove no incompatible second GPUI dependency is introduced.
- Build and exercise the required Ely components on the development platform;
  retain a native screenshot or reproducible capture from the actual app.
- macOS is Sirio's reference platform. Check macOS and Windows compilation
  through appropriate hosts/runners, and native input/rendering on available
  platforms. A missing platform is explicitly unverified, not claimed green.
  Ely's upstream macOS Gallery does not establish Sirio's Linux/Windows support.

### Behavior and visual evidence

Run the relevant existing `sirio_ui` chat tests and affected integration tests
with the real fixture subprocesses. Add behavioral regressions only for the
new adapter risks: queue while busy, attachment-only submit, dynamic permission
IDs, focus/selection, controlled expansion and row remeasurement. Tests drive
actual actions and observable outcomes.

Capture the new native UI with reproducible isolated app state and fixture
traffic, using the existing control socket/capture facilities where suitable:

| Coverage | Required observations |
|---|---|
| Empty/connecting/authentication | Honest agent identity, actionable status and retained draft. |
| Streaming/completed/interrupted/error | Selectable response, cancel/retry and stable state transitions. |
| Tools/reasoning/plans/subagents | Controlled folds, nested scrolling, rich diff/location handoff and reported progress. |
| Questions/permissions | Actual option IDs, supported answer forms, expiry and historical outcome. |
| Composer | Mentions/slash completion, attachments, queue FIFO/removal/clear/send-now and catalogue-driven controls. |
| Long/restored history | Virtualization, selection/copy, turn rail/folds, scroll anchoring and inactive historical requests. |
| Chat menu/history | File following, new conversation, reopening history and confirmed deletion. |
| Appearance/layout | Dark/light/System, UI scaling, wide/narrow panes and white logos on readable plates. |
| Agent variations | Correct Claude Code/Codex/OpenCode/Pi/omp marks and absent unsupported controls. |
| Rewind | Existing eligibility, preview, confirmation, success and failure. |

Compare long-transcript behavior against the current app: streaming should
update affected rows, not reparse/rerender the entire history. Keep current
animation and timer lifecycles; collapsed or inactive content should not gain
perpetual repaint timers. Any performance traces use static content-free names,
never prompts, tool titles or paths.

Use targeted crate build/test commands during iteration. `Scripts/ci.sh` and
`Scripts/ci-linux.sh` run only on the user's explicit request. Never describe
an HTML capture as native verification or a compile check as an interactive
platform test.

## §10 Review boundaries

This document specifies one chat UI migration with an initial compatibility
gate and incremental native presentation work. It adds no new transport,
conversation features, ratings, voice, regeneration, memory dashboard or
invented agent hierarchy. It does not alter the application's release version
as part of writing the design.

The next artifact is a written implementation plan, after the user reviews
and approves this spec. The plan must be reviewed and its execution method
selected before product code or dependencies change.

## References

- [Pinned Ely manifest](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/blob/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/Cargo.toml)
- [Ely Chat exports](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/blob/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/src/chat/mod.rs)
- [Ely Agent exports](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/blob/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/src/agent/mod.rs)
- [Ely chat Gallery](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/tree/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/examples/gallery/pages/chat)
- [Ely agent Gallery](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components/tree/e17e31a6890c09ebcfa8b61133d7bc7c625edf69/examples/gallery/pages/agent)
- [Native Claude chat design](2026-09-18-claude-native-chat-design.md)
- [Existing agent icon provenance](../../../rust/assets/icons/lobehub/ATTRIBUTION.md)
- [Claude Code pixel logo, pinned package](https://unpkg.com/@lobehub/icons-static-svg@1.95.0/icons/claudecode.svg)
