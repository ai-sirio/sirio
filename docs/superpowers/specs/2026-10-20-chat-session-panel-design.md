# The chat's session panel: environment, context, connectors, change request

**Date:** 2026-10-20
**Status:** design
**Mockups:** `2026-10-20-chat-session-panel-mockups.html` (beside this file)

## §0 Intent

Claude Code's desktop app shows, beside a chat, a column with four cards:
*Environment*, *Context*, *Connectors* and *Changes*. The user wants the same in
Sirio's chat, behind a button in the composer.

The reason, in the user's words: what matters most is the context, and how it is
used. The total alone is not enough; the user wants to see **what** occupies it
(memory files, skills, plugins, agents) so they can **slim their configuration**.
Everything else in the panel is useful company for that.

Decided with the user, in order:

1. **Placement (mockup A).** A button beside the composer's attach button opens a
   column **docked inside the chat tab, on its right**, as in Claude Code. The
   transcript narrows to make room. Not a popover, not the existing ring, not a
   view of Sirio's right panel.
2. **Context is grouped by origin, not by category.** Two groups: *you can
   reduce it* (memory files, each plugin with its skills and agents together, the
   conversation) and *fixed* (Claude Code's own prompt and tools, the autocompact
   reserve, free space). **Every origin opens and closes on its own** — the
   groups, each plugin, each "N more".
3. **A row that names a file opens it in a Sirio tab** (the project's
   `CLAUDE.md`, `MEMORY.md`, an agent's or a skill's `.md`).
4. **All four sections from the start**, in Claude Code's order: Environment,
   Context, Connectors, Change request.
5. **Change request: only the PR/MR of this chat's branch**, never the project's
   open list.
6. The rest of the play-back (§3–§7) was accepted as written ("va bene").

## §1 Goals, non-goals, anti-goals

**Goals**

- At a glance, which origin dominates the context; one click deeper, which item.
- One click from a heavy file to that file open in a tab.
- See which MCP servers the Claude session has and which need attention, and act
  on them (sign in, reconnect).
- See this branch's PR/MR state without leaving the chat.

**Non-goals**

- Editing configuration from the panel (uninstalling a plugin, deleting a skill,
  disabling an MCP server). The panel shows and opens; the user edits.
- A per-category view as Claude Code's `/context` draws it. The origin view
  replaces it.
- The project's other open change requests (the right panel's list has them).
- Breakdown for agents that do not report one (Codex, OpenCode, Pi over ACP).

**Anti-goals** — it fails even if it works when:

- A number in the panel is one nobody computed: a zero shown for "not reported",
  a sum that does not add up to the total the CLI reported, a stale status shown
  as current.
- A change in Claude Code's undocumented protocol breaks the chat, or the whole
  panel, instead of one row (see §8).
- Opening the panel doubles forge traffic or bypasses the rate-limit pause.
- The panel shows the sidebar's selected worktree instead of the chat's.

## §2 The button and the column

- **Button:** an icon button in the composer row, immediately after the attach
  button (`attach-image`, `sirio_ui/src/chat/composer_view.rs:1027`), drawn in
  its "on" state while the column is open. Tooltip names the column.
- **Shown for every agent.** Each section hides or degrades on its own (§8);
  Environment always has something to say.
- **The column** is docked inside the chat tab, to the right of the transcript
  and composer, full height of the tab. Cards stacked vertically, scrolling as a
  whole when taller than the tab. Each card opens and closes from its title.
- **Open/closed is per chat and survives a relaunch** of the chat tab. The
  sections' and groups' expansion state is per chat, in memory only.
- The existing context ring (`context-ring`, `chat/controls.rs:849`) and its
  popover stay as they are.

## §3 Environment

Where **this chat** runs: project name, the working directory (home-relative,
middle-elided), and the branch, labelled *worktree* when the directory is a linked
worktree rather than the project's main checkout. On a detached HEAD the line
shows the short hash and says so; it never names it a branch.

Source: the chat's `agent_cwd` (`chat/mod.rs:1895`), resolved against the
workspace's projects and worktrees (`sirio_project::Worktree` has `path` and
`is_primary`; the branch itself is read live, see §6). **Not** the sidebar's
selected worktree, and not
`UsageBarData` (`sirio/src/main.rs:11877`), which describes the selection. A chat
whose cwd belongs to no known worktree shows the directory alone.

## §4 Context

### Data

Sirio already sends `get_context_usage` at the end of every turn
(`sirio_acp/src/claude/worker.rs:846`), but `ContextUsageReport::parse`
(`sirio_claude/src/usage.rs:154`) keeps only the total, the window and the model.
The payload (Claude Code 2.1.304, probed live) carries:

| Field | Content |
|---|---|
| `categories[]` | `id`, `name`, `tokens`, `color`, `kind` (`used` / `deferred` / `buffer` / `free`), `isDeferred` |
| `memoryFiles[]` | `path`, `type` (`User` / `Project` / `AutoMem` …), `tokens` |
| `skills` | `totalSkills`, `tokens`, `skillFrontmatter[]` with `name`, `source` (`userSettings`, `plugin`, `syncedSkills` …), `pluginName`, `pluginId`, `path`, `tokens` |
| `agents[]` | `agentType`, `source`, `pluginId`, `path`, `tokens` |
| `mcpTools[]`, `mcpInstructions[]` | per-tool entries (empty in the probe) |
| `slashCommands` | totals only |
| `messageBreakdown` | `toolCallTokens`, `toolResultTokens`, `attachmentTokens`, …, `attachmentsByType[]` (e.g. hook output) |
| `totalTokens`, `rawMaxTokens`, `maxTokens`, `percentage`, `autoCompactThreshold`, `isAutoCompactEnabled`, `model` | totals |

**All of it comes with `detail: "summary"`**, the variant Sirio already sends
(`sirio_claude/src/control.rs:124`): re-probed with exactly that request, the
answer carried 9 categories, 3 memory files, 78 skills and 28 agents, in 1.0 s,
with no per-category API calls. So the breakdown is a **parser change**, not a
new request or a costlier one; `full` is not needed. (The test fixture answers
with `categories: []` today and must grow.)

`get_context_usage` also answers outside a turn (the probe sent it before any
prompt). When the column opens on a chat with no report yet, ask once.

**During a turn** the section keeps the last report, labelled as of the end of
the previous turn; it is replaced when the turn's report arrives. Nothing is
asked mid-turn.

### Presentation (mockup "panel-content-v2")

- Header: percentage and `used / window`. A stacked bar, one segment per
  top-level origin.
- **You can reduce it**
  - *Memory files* → each file, with its type tag and tokens. Each opens.
  - *Plugins and skills* → one row per plugin (its skills and agents summed,
    labelled "N skills · M agents"), plus rows for skill sources that are not
    plugins (the user's own, synced). Each plugin opens to its skills and agents.
    An item with a `path` opens that file.
  - *MCP tools* → per server, when the CLI reports any.
  - *Conversation* → messages, with the breakdown the CLI gives (tool calls,
    tool results, attachments such as hook output).
- **Fixed**: Claude Code's system prompt and tools, deferred tools (muted, not
  counted), the autocompact reserve, free space.
- Every level is sorted by weight, heaviest first. Beyond about ten items the rest
  folds into "N more" with its total, which opens too.

### Invariants

- **The rows add up to `totalTokens`.** Whatever the CLI reports in a category
  but does not itemise (e.g. `slashCommands`, which comes as a total) is a row of
  its own; an unexplained remainder is an explicit "other" row, never dropped.
- A field the CLI stops sending removes its rows, not the section; a payload with
  no window (`rawMaxTokens` absent or 0) keeps today's behaviour — the section
  says the size is unknown rather than showing 0%.
- Agents without a breakdown (ACP): the section shows the total they report
  (`ContextUsage { used, size }`) and one line saying this agent does not report
  what is in it.

## §5 Connectors (Claude only)

### Data

Probed against Claude Code 2.1.304:

- `system/init` carries `mcp_servers` (name + status); Sirio parses it into
  `McpServerStatus` (`sirio_claude/src/message.rs:82`) and then discards it
  (`sirio_acp/src/claude/events.rs:309-343`).
- **`mcp_status`** is a control request that answers on demand with
  `mcpServers[]`: `name`, `status`, `scope`, `source`, `config`. On the probe
  machine: 23 servers, statuses `needs-auth` (18) and `pending`; scopes/sources
  `claudeai`, `plugin` / `dynamic`, `user`.
- **`mcp_reconnect`**, **`mcp_authenticate`** and **`mcp_toggle`** exist: given
  an unknown server each answers `Server not found: <name>`, where an unsupported
  subtype answers `Unsupported control request subtype`. Their argument key in
  the probe was `serverName`. **`mcp_reconnect` verified on a real server**
  (`codegraph`, a local stdio server): `success` in 1.4 s, and the next
  `mcp_status` reported it `connected`. **`mcp_authenticate` verified** on
  `plugin:context7:context7` (`needs-auth`), with the user's consent: in 3 s it
  answers `success` with `{ authUrl, requiresUserAction: true,
  callbackExpected: true, redirectScheme: "localhost", callbackPort, state }`.
  The CLI opens nothing itself; **`authUrl` is an OAuth authorisation URL whose
  `redirect_uri` is `http://localhost:<callbackPort>/callback`, a listener the
  CLI holds on the machine it runs on.** So the URL must be opened in a browser
  on that same machine (Sirio's normal case: open it with the system browser),
  and the CLI process must stay alive until the callback lands. Whether the
  status then turns `connected` on its own, without a reconnect, was not
  observed (the authorisation was not completed); after the browser opens, poll
  `mcp_status` (every few seconds, for a bounded time) and, if it stays
  `needs-auth` once the user returns, offer Reconnect.

### Presentation

- Servers grouped by origin (claude.ai, plugins, user / project), each group
  opening and closing, with counts in the header ("23 · 18 need sign-in").
- Per server, its status: connected, starting, needs sign-in (**Sign in**),
  failed (**Reconnect**). An unknown status string is shown as the CLI sent it.
- Refreshed with `mcp_status` when the column opens, after each action, and
  after `system/init`. A status is never shown as current once it is known to be
  stale (an action in flight shows as such).
- **No enable/disable switch** in this version (see §10).
- Hidden for ACP agents: nothing reports their servers today
  (`sirio_acp/src/mcp_config.rs` only sends `<cwd>/.mcp.json`).

## §6 Change request

- Only the PR (GitHub) or MR (GitLab) of **the chat's branch**: number, title,
  state (open / draft / merged / closed), checks summary, review state. A click
  opens Sirio's existing change-request tab for it.
- No change request for the branch: one line saying so. No forge configured, or a
  detached HEAD: the section is hidden.
- Resolution must match the right panel's card: the persisted
  `change_request_link` first when it matches the current branch, then
  `ForgeClient::for_branch`. That logic is today private to the right panel
  (`load_card`, `sirio_ui/src/right_panel/change_requests.rs:407-470`); lift it
  into a shared function both use.
- **The branch is read live**, as the right panel's card does (`current_branch`
  off the GPUI thread, because the worktree may have switched since), never from
  the cached `Worktree.branch` label, which refreshes only on window focus and
  worktree selection. The Environment line uses the same live read, so the two
  sections never disagree.
- **The rate-limit pause must be shared.** Today it exists per widget:
  `ChangeRequestList::note_rate_limited` (`right_panel/change_requests.rs:260`:
  a minute when the forge names no reset, capped at an hour, re-arming a timer)
  and `ChangeRequestTab::note_rate_limited` (`change_request_tab.rs:500`: copies
  a present reset only). Lift it into **one pause per forge host**, held where
  every reader reaches it (beside `ChangeRequestSource`, set by `ForgeHub`),
  with the list's rules. The list and the chat column read and set it; the
  change-request tab keeps its own behaviour in this version. While it holds,
  the section shows the last known state, labelled as paused until the reset.
- Read while the column is visible and the section is open, at most once every
  60 s; opening the column reads only if the last read is older than that.
  `ChangeRequestSource::connect` runs off the GPUI thread (it runs git).
- Section title is **"Change request"**, not "Changes": Sirio already calls the
  git-status surface *Changes*.

## §7 Per agent

| Section | Claude (native) | Claude via ACP wrapper | Codex / OpenCode / Pi |
|---|---|---|---|
| Environment | ✓ | ✓ | ✓ |
| Context | breakdown | total + "not reported" line | total + "not reported" line, or hidden if no total |
| Connectors | ✓ | hidden | hidden |
| Change request | ✓ | ✓ | ✓ |

## §8 Constraints

- **Claude's stdio protocol is not a published contract** (CLAUDE.md, "A Claude
  chat talks to Claude Code"). Parse every new field defensively in
  `sirio_claude`: unknown fields ignored, a missing one degrades its rows, a
  refused control request degrades its section. Extend the conformance coverage
  (`claude_answers_the_native_handshake_it_claims`, live, SKIPs without the
  binary) so the day `mcp_status` or the context breakdown changes shape is
  noticed.
- **Chat presentation is Ely.** Read `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`
  and `docs/superpowers/specs/2026-09-30-ely-agent-chat-design.md`; keep Sirio's
  theme (`sirio_theme`, `ThemeColors { ely, sirio }`) and its chat controllers.
- **Text selection:** names and paths that are read, not clicked, use
  `selectable_text`; never inside a clickable row.
- **`sirio_perf`:** any trace label is a content-free `&'static str` — never a
  path, a server name or a title.
- **Transport-agnostic UI:** new data reaches `sirio_ui::chat` as `AcpEvent`s (or
  an accessor on the client), so the chat never knows which transport it has.
- **Version:** `rust/Cargo.toml` is already at 0.34.0 for this cycle; a `feat:`
  here does not move it.

## §9 Proof

End to end, per the repo's conventions: a real, isolated Sirio driven over the
control socket, ending in an artefact anyone can re-run (a transcript plus
PID-matched captures, dark and light). The Claude side runs against a stub CLI
that answers `get_context_usage` and `mcp_status` with fixtures shaped like the
live payloads (`sirio_acp/tests/fixtures/claude_fixture.py` already answers
`get_context_usage`); the change request against the fake forge the forge E2Es
use. A read-only `surface.chat.*` state verb exposing what the column shows
(sections, rows, totals) is the expected way for the test to assert without
reading pixels.

Unit tests, if any, are for the pure parts (the payload parsers and the
origin-grouping fold), written failure-first: missing fields, zero window,
rows that do not add up, unknown statuses, unknown sources.

## §10 Calls made by the designer (not by the user)

- **No enable/disable switch for MCP servers.** The user did not answer the
  question; `mcp_toggle` exists, so this is a cheap follow-up.
- English section titles (Sirio's UI is English): *Environment*, *Context*,
  *Connectors*, *Change request*.
- The column's open state persists per chat; section and group expansion do not.
- ACP agents get the total-only Context section rather than nothing.
- `get_context_usage` asked once on open when no report exists yet.
- Fixed column width (about 300 px), not user-resizable in this version.
- The context breakdown rides the existing `summary` request (verified to carry
  it); during a turn the last report stays, labelled.
- The branch is read live, not from the cached label.
- One rate-limit pause per forge host, with the right panel list's rules; the
  change-request tab's own pause is left alone in this version.

## §11 Left to the builder

- Icon, exact width, colours (from the theme), spacing, the "N more" threshold.
- Where the open state is persisted (the chat's existing persistence, or a new
  column with a migration).
- How the column behaves when the chat tab is too narrow to fit it and a
  readable transcript (overlay, or refuse to open).
- The exact strings, and how statuses map to icons.
- How a failed `mcp_authenticate` (an `error` answer) shows its reason on the
  row, and how long the post-sign-in status poll lasts.
