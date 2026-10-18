# Sirio as an agent development environment — programme, and project 1

**Date:** 2026-10-17
**Status:** proposed
**Programme:** Sirio becomes an ADE: one agent — the *director* — drives
Sirio on its own through the control socket and `sirioctl`, opening
terminals and chats, starting other agents in them, reading what they say
and deciding the next step, while the user watches it happen in Sirio's own
tabs. Seven projects (§3), each with its own design, plan and pull request.
This document fixes the programme and designs **project 1** in full;
projects 2–7 each get their own design document, written from §3's
boundaries and §4's cross-cutting rules.
**Related:** the detached host (`2026-10-05-host-foundation-design.md`,
`2026-10-06-host-terminal-sessions-design.md`), the change-request hand-off
(`2026-10-07-change-request-handoff-design.md`), the file editor on Ely
(`2026-10-17-ely-code-editor-design.md`, on `worktree/rapid-meadow-b32e`,
whose debug-only `surface.editor.*` verbs project 6 builds on).

## §0 Intent

The user has the Linux Nightly installed. From their own shell `sirioctl`
was "command not found"; when it was found, an agent could not use the
terminals in Sirio's tabs, could not read what the chats said and could not
create new terminals. The user wants Sirio to be an AI editor / ADE in
earnest, with Sirio's skill teaching agents how to use it that way.

Decided with the user, in order:

1. **One agent directs.** An agent takes command: it creates terminals,
   starts agents in them, and reads the context of chats and terminals to
   decide what to do next.
2. **Everything on its own.** The director needs no confirmation from the
   user for anything it does in Sirio — including answering other agents'
   permission requests.
3. **All eight proposed capabilities are in:** the director's core, an event
   stream, Sirio as an MCP server, a worktree per worker with checkpoints, a
   drivable shared editor, language-server answers for agents, an action log,
   and a task board. Ordered as §3.
4. **`sirioctl` reaches the user's PATH both at startup and from an explicit
   button** in Settings.
5. **The hooks are verified to work**, not assumed.
6. **The action log and task board are built from Ely GPUI Components'**
   gallery components where they fit (§3, project 7).

What the user does not want, inferred from the request and marked as such:
an ADE that works only in debug builds, a skill that documents commands the
installed build does not serve, and a director that has to poll in a loop.

## §1 What is there today

Read from the tree at `f8c143f9` and from the user's machine on 2026-10-17.

**Reaching `sirioctl`.**

- The Linux AppImage ships `usr/bin/sirioctl` (`Scripts/build-appimage.sh:125`),
  which exists only while the AppImage is mounted. Windows ships
  `{app}\sirioctl.exe` beside `sirio.exe` (`Scripts/build-inno.sh:81`) and
  touches no PATH. **The macOS bundle does not contain it**: the release job
  builds it (`build-release.yml:296-307`) but `Scripts/build-app-bundle.sh`
  copies only `sirio` and `sirio-host` (lines 76-84). On macOS every agent
  launch therefore reports "hooks not written" unless `sirioctl` happens to
  be on PATH.
- `resolve_sirioctl_path` (`sirio/src/main.rs:24125`) installs one app-owned
  copy at `$XDG_DATA_HOME/Sirio/bin/sirioctl` (`%LOCALAPPDATA%` on Windows),
  and every hook embeds that absolute path. **On unix it returns early when
  the copy exists** (line 24131), so the copy is never refreshed; the
  staleness check (`installed_copy_is_stale`) runs on Windows only. On the
  user's machine the copy is dated 2026-09-12, the AppImage 2026-09-29, and
  the copy has no `browser` family and no `panel show` — commands the
  bundled skill tells agents to run.
- Nothing puts `sirioctl` on the user's PATH. `install.sh` writes only
  `~/.local/bin/sirio`; the Settings card shows "Bundled binary: sirioctl"
  and says no install mechanism exists (`sirio_ui/src/settings.rs:3646`).
- A debug build installs a symlink to `target/debug/sirioctl` at the same
  path when none exists, and otherwise uses whatever is there — so debug and
  release builds share, and fight over, one copy.

**The environment a terminal inherits.** `CreateParams.env` is
`std::env::vars()` of the app (`sirio_terminal/src/link.rs:528`), and the
host's own fallback environment (`sirio_host/src/sessions.rs:153`) is the
environment the app launched it with. Under the AppImage that is AppRun's
environment: `PATH`, `LD_LIBRARY_PATH` and `XDG_DATA_DIRS` prefixed with the
mount, plus the gtk hooks' `GTK_THEME`, `GSETTINGS_SCHEMA_DIR`,
`GIO_EXTRA_MODULES`, `GDK_PIXBUF_MODULE_FILE`
(`Scripts/build-appimage.sh:229-245`). Every user program in a Sirio
terminal runs with the AppImage's libraries first in its search path while
Sirio is open, and with dead entries once it is not — and terminals outlive
the app by design ("Quit detaches"). A pane opened before a quit, a crash or
a Nightly self-update looks for `sirioctl` in a mount that no longer exists.

**The hooks.** On the user's machine the user-global hooks are installed and
all name the stale copy: five in `~/.claude/settings.json`, `notify` in
`~/.codex/config.toml`, `sirio-session.js` in OpenCode's plugins. The stale
copy still knows `notify --stdin-json`, and outside a pane it exits 0
silently, as designed. The round trip hook → socket → sidebar status was not
observable (Sirio was not running); nothing in the tree proves it end to end.

**The skill.** One skill, `skills/sirio/SKILL.md` (171 lines), compiled in
with `include_str!` and written by `prepare` for Claude Code
(`.claude/skills/sirio/`), OpenCode and Oh-My-Pi (`.agents/skills/sirio/`).
Codex and Pi get nothing from `prepare`. Settings → *Install Skill* runs
`npx skills add ai-sirio/sirio …` (`sirio_project/src/skill.rs:21`), which
fetches GitHub `main` — a copy that can be ahead of or behind the installed
binary. It covers raw terminal panels and the browser only, and is wrong in
four places:

- `worktree-set --worktree "$SIRIO_WORKTREE_ID"`: nothing exports
  `SIRIO_WORKTREE_ID`; panes get `SIRIO_PANE_ID` and `TILLER_PANE_ID` only
  (`sirio_term_session/src/lib.rs:176-178`).
- `browser screenshot` is documented as working; the app refuses it
  (`BROWSER_CAPABILITIES`, `main.rs:729-737`).
- Its `not_supported` table lists browser actions `sirioctl` answers with
  `unknown browser action` (`sirioctl.rs:1353-1364`).
- It says `panel state` returns base64 in `data`; `panel.state` returns
  `workingDirectory`, `survives`, `exitStatus`, and `panel scrollback`
  returns `data` and `bytes` (`sirio_control/src/host_panels.rs:188-200`).

**What a director can do over the socket today (release builds).**

| Need | Today |
|---|---|
| List terminal tabs | `surface.tabs.read`: kind and title, no ids, selected worktree only. `panel.list` lists only sessions the socket created; a terminal the user opened has no id a client can learn. |
| Open a terminal with a command or agent | `surface.terminal.open` is debug-only. Release: `panel.create` then `panel.show`. |
| Read / write a terminal | `panel.read`, `panel.scrollback`, `panel.write`, `panel.key` by id — for sessions whose id is known. |
| Open a chat with a chosen agent | Not possible outside a change-request hand-off. `surface.chat.open` reuses the first chat tab. |
| Read / write a chat | `surface.chat.read`, `.send`, `.stop`, `.permission` — raw socket only, no `sirioctl` command. |
| Answer a permission | Option ids are not in the transcript; the client must guess. |
| Know an agent is done or waiting | No activity state over the socket; `panel.wait` waits for exit only. No subscription. |
| Drive the editor | No open-file-at-line for an arbitrary path; no selection or cursor. |

## §2 Vocabulary

- **Director** — an agent running inside Sirio (in a terminal or a chat, any
  of the five adapters) that drives Sirio through `sirioctl`, the MCP server
  of project 4, or the socket.
- **Worker** — an agent the director started or reads from.
- **Stable copy** — the one app-owned `sirioctl` at
  `<data>/Sirio/bin/sirioctl` that hooks name and PATH links point to.
- **User environment** — the environment Sirio's own launcher received,
  before AppRun or any packaging wrapper changed it.

## §3 The programme

Each project ships on its own, ends in an E2E with a re-runnable artifact,
and updates the skill in the same pull request (§4.4).

1. **A `sirioctl` that is always there, hooks that are proven, a skill that
   is true** — designed in §5.
2. **The director's core.** Stable ids for every tab of every worktree,
   terminal or chat, and the terminal tab ↔ host session mapping;
   `sirioctl` commands for every verb below; open a terminal with a command
   or an agent (release); open a chat with a chosen agent (release); read and
   write terminals and chats, including terminals the user opened; the
   activity state of terminals and chats (working / idle / needs input /
   exited) from `sirio_activity` and the chat; a server-side *wait until*
   (state reached, or timeout) so the director does not poll; permission
   requests with their option ids, answerable by the director; close.
   Projects 1 and 2 together answer everything the user hit.
3. **The event stream.** `sirioctl events --follow` (and a socket
   subscription): tab opened or closed, activity changed, needs-input,
   chat turn completed, file changed, worktree changed. Bounded, lossy with
   a reported drop count, like `sirio_perf`'s trace.
4. **Sirio as an MCP server.** `sirioctl mcp` over stdio, one tool per
   release verb, generated from the same description `sirioctl` and the skill
   use, so the three cannot disagree. Registered for each adapter by
   `prepare` where the CLI supports worktree-local MCP config.
5. **A worktree per worker, and checkpoints.** The director creates a
   worktree, starts a worker in it, reads its diff, compares two workers'
   diffs, and brings a result back into a branch; before a worker starts, a
   checkpoint of the worktree is taken (a hidden ref, never a stash), and the
   director or the user can return to it. Builds on `sirio_git` and the
   Changes surface.
6. **A drivable, shared editor, and language answers for agents.** Open a
   file at a line, reveal and highlight a range, read the user's selection
   and cursor, send a selection to a chat; diagnostics, definition and
   references from `sirio_lsp`. Builds on the file editor on Ely and its
   `surface.editor.*` verbs, promoted to release.
7. **The action log and the task board.** The log: every director action in
   order, with time, target, outcome and details, filters, and a guarded
   *Stop the director*. The board: the director's plan and each worker's
   state. Built from Ely components already vendored — `Timeline`,
   `ToolCallCard`, `Badge`/`Tag`, `ChoiceChips`, `SearchInput`,
   `RelativeTime`, `AnimatePresence`, `ConfirmButton` (hold) for the log;
   `AgentPlan`, `AgentStepList`, `AgentStatus`, `AgentProgress`,
   `ProgressBar` for the board (`rust/vendor/ely-gpui-component/src/agent/`,
   `data_display/`). Upstream's `ActivityFeed` and `VirtualList` are not
   vendored; whether to vendor them is decided in project 7's design.
   *Claude's call:* not upstream's `KanbanBoard`, which is built for
   dragging by hand, not for a board the director writes.

## §4 Rules every project follows

### §4.1 Release builds serve the director

The director must work in the build the user runs. Verbs that are today
debug-only because they *write* — opening a terminal with a command, opening
a chat — are served by release builds from project 2 on. This reverses the
current policy for those verbs, and is the user's decision (§0, 2).

*Claude's call:* **forge writes stay debug-only** (`surface.change_request.act`
and its siblings), and so do the test knobs (`surface.host.end_all`,
`surface.update.inject`, …). Acting on GitHub or GitLab in the user's name is
outside "everything in Sirio", and can be reconsidered in its own design.

### §4.2 Who may drive

No new authentication. The socket keeps today's protections: same-user peer
identity (`sirio_ipc`, `getpeereid` on macOS), a per-user socket path, and the
Settings toggle (`controlSocket.enabled` / `SIRIO_SOCKET_ENABLE`) that turns
it — and every director — off. A director inside a Sirio pane is the user.
**Anti-goal:** any listener reachable from another user or the network.

### §4.3 Every release verb has a `sirioctl` command

No release verb is raw-socket only (today the chat verbs are). The MCP tools
of project 4 and the skill are derived from the same table.

### §4.4 One skill, shipped with the binary, checked against it

- The skill's source is the copy compiled into the binary. Nothing fetches
  it from the network.
- A test reads every `sirioctl …` line in the skill's code blocks and fails
  when `sirioctl`'s parser rejects it or when the method it sends is not in
  a **release** build's `capabilities`. It goes red on a rename or removal —
  a behaviour change — and is indifferent to refactors.
- Each project extends the skill for what it adds, in the same pull request.
  From project 2 on, the skill teaches the director's workflow, not just the
  commands.

### §4.5 Ids, never the screen

A target is always an id a verb returned, never "the selected worktree" or
"the focused tab" — the user may click while the director works. The
current skill already says this; project 2 makes it possible for every tab.

## §5 Project 1 — a `sirioctl` that is always there

### §5.1 One stable copy, always current

- The stable copy stays at `<data>/Sirio/bin/sirioctl`
  (`$XDG_DATA_HOME` or `~/.local/share` on unix, `%LOCALAPPDATA%` on
  Windows): the user-global hooks already installed name exactly that path,
  and keep working without being rewritten.
- At every launch the app compares the bundled `sirioctl` with the stable
  copy **by content** (a hash of the bytes; size and modification time are
  not evidence — an AppImage's files carry build-time mtimes, which can be
  older than the copy) and replaces it when they differ. The unix early
  return goes.
- Replacement is atomic: write a temporary file in the same directory, make
  it executable, `rename` over the copy. A hook running the old copy keeps
  running it; a second Sirio racing the first leaves one whole file. On
  Windows, where a running `.exe` cannot be overwritten, the old file is
  renamed aside first and removed on a later launch.
- On unix the stable copy is a **copy** under the AppImage and a symlink
  only when the source is itself a stable file (a package install, a
  checkout's `target/`). It never points into a mount.
- *Claude's call:* **the stable path belongs to Sirio.** It lies inside
  Sirio's own data directory, so whatever is there is replaced when its
  content differs from the bundled binary — including the user's unmarked
  2026-09-12 copy, which is exactly the copy that must be fixed. (Nothing
  checks ownership there today, contrary to what an earlier draft said; no
  marker is introduced.) The "never replace a foreign file" rule of §5.2 and
  §6 applies to the PATH link's place, which is the user's directory.
- *Claude's call:* **a debug build uses its own copy**,
  `<data>/Sirio/bin-debug/sirioctl`, mirroring `host-debug`, and never
  touches the release copy or the user's PATH. Worktree-local hooks a debug
  build writes name the debug copy; **user-global hooks always name the
  release stable path**, from any build, so a debug build's *Install Hooks*
  never leaves the user's global hooks pointing into a checkout.

### §5.2 On the user's PATH — at startup and from a button

The user decided both (§0, 4). This **overrides, for the CLI link only**,
the project rule that only an explicit action writes outside Sirio's own
data (CLAUDE.md, *Agent adapters*). Hooks and the user-level skill keep that
rule: they change only through their buttons, except that a user-level skill
copy Sirio already installed (it carries the Sirio marker) is refreshed at
startup so it never falls behind the binary — *Claude's call*.

What "on PATH" means per platform — always a link to, or the directory of,
the stable copy, never the bundle or the mount:

| Platform | At startup | The button |
|---|---|---|
| Linux | `~/.local/bin/sirioctl` → symlink to the stable copy. | Same, and re-creates it if it was removed. |
| macOS | A symlink in `/usr/local/bin` **only when that directory exists and is writable without a password** (Homebrew on Intel makes it so); otherwise in `~/.local/bin` if that directory is on the login PATH; otherwise nothing. Never a password prompt at startup. | `/usr/local/bin/sirioctl`, asking for the administrator password once, as VS Code's *Install 'code' command in PATH* does. |
| Windows | The stable copy's directory appended to the user's `Path` (HKCU, no admin), with the settings-change broadcast so new shells see it. | Same. The Inno uninstaller removes the entry. |

Rules common to all three:

- An existing `sirioctl` at the link's place that is not Sirio's link is
  never replaced; the card shows it and where it points.
- Whether the directory is on the user's **login** PATH is checked (the app
  already reads the login PATH, `login_path.rs`). A link in a directory the
  login shell does not search is reported as such on the card, never as
  success.
- A startup install that fails is logged and shown on the card; it never
  blocks the launch or shows a dialog.

### §5.3 The environment a terminal gets

Every process Sirio starts for the user — terminals (host sessions and the
`LocalLink` fallback), the host itself, chat agents — runs in the **user
environment** (§2), with:

- the stable copy's directory first on `PATH` (debug: the debug copy's), so
  a bare `sirioctl` in any pane is always the current one and survives the
  app quitting;
- `SIRIO_PANE_ID` and `TILLER_PANE_ID` as today;
- `SIRIO_WORKTREE_ID` — the value `worktree-set --worktree` accepts — for a
  terminal that belongs to a worktree (*Claude's call*: export it rather
  than remove it from the skill; a director needs to name its own worktree).

How the user environment is known: the packaging wrapper records it. AppRun
saves, before it changes anything, the whole environment as one hex snapshot in
`SIRIO_LAUNCH_ENV`, taken only when it fits in 131000 characters. The app takes
the user environment from that snapshot, less the launcher's own variables
(`APPIMAGE`, `APPDIR`, `ARGV0`, `OWD`, `SIRIO_LAUNCH_ENV`). The snapshot is
`env -0 | od -An -tx1 -v`, which needs only `env -0` and `od`, which every Linux
the AppImage runs on has, so the crate that reads it has no dependency
(decided during implementation). The app does not
guess by matching `/tmp/.mount_*` paths. Without a wrapper (macOS, Windows, a
checkout), or with a snapshot that does not decode, the app's environment is
the user environment.

*Claude's call:* **the restore is for children only.** The app's own process
keeps AppRun's environment: its browser pane (wry/WebKit) dlopens the WPE
backend through AppRun's `LD_LIBRARY_PATH` and resolves its helpers through
`././` paths (`Scripts/build-appimage.sh:199-208`), and gdk-pixbuf and GIO
load modules lazily from the gtk variables. The user environment is computed
once and lives in a leaf crate, `sirio_env`, with a process-wide setter like
`sirio_perf`, rather than as a parameter threaded through every API, because
every crate that spawns a process needs it. A site applies it through
`sirio_env::command`, which clears the child's environment and applies the
user's, so the site's own `.env(..)` calls come after the clear and survive it;
a site that starts its child another way takes `sirio_env::for_child()`. The
sites include the host launch, `CreateParams.env` from the terminal link and
from `panel.create` (`sirio_control/src/host_panels.rs`), the `LocalLink`
fallback, chat agents, and whatever the audit below moves (decided during
implementation).

**The host must be able to honour it.** Today a session's environment is the
host's own `std::env::vars()` *extended* by `CreateParams.env`
(`sirio_host/src/router.rs:238`, `sessions.rs:153-155`), so a client cannot
make a variable absent, and an adopted host launched by an older app still
carries the dead mount's variables. *Claude's call:* `CreateParams` gains an
additive, capability-advertised way to say "this map is the whole
environment" (the exact field and its ledger entry in `protocol/host-v1/`
are the builder's); a host that advertises it builds the session from the
client's map alone; with a host that does not — one adopted from before this
change — the client sends the user environment with an empty value for each
variable the app has and the user environment lacks, except `SHELL`. That is
best effort: a launcher variable the session would inherit is blank, not
present. That gap lasts until the host is next started, which the Settings host
status says (decided during implementation). The host itself is
launched in the user environment, so its fallback is clean too.

*Left to the builder:* the full list of other processes Sirio starts (git,
language servers, `plantuml`, `gh`/`glab`) — each is audited and either moved
to the user environment or listed in the plan with the reason it keeps the
app's.

Sessions already running keep the environment they were born with; nothing
reaches into a live process. A new pane is clean.

The stable copy of `sirioctl` is always a copy, never a symlink (§5.1); the PATH
link of §5.2 is the only link (decided during implementation).

**A login shell can drop the stable directory.** The default pane shell is a
login shell (`$SHELL -il`). A login profile that reassigns `PATH` drops the
stable directory from it, so `sirioctl` then resolves only through the
`~/.local/bin` link, and only when the profile keeps `~/.local/bin` on `PATH`,
as distro skeleton profiles do. In that case the link is what keeps `sirioctl`
in panes. A `cmd=` pane runs its command as `$SHELL -c`, which is not a login
shell, and keeps the stable directory (found by the E2E, decided during
implementation).

**A chat agent starts in its worktree (R31).** The agent's process directory and
`PWD` are the directory its session was created for, when that is absolute, and
otherwise the user's home. The launcher's own directory is never used
(decided during implementation).

**The default workspace never starts in the AppImage mount (R33).** When `APPDIR`
is set and the app's working directory is inside it, the default workspace starts at
`OWD`, the directory the AppImage was launched from, when that is an absolute existing
directory outside the mount; otherwise it starts at the user's home. The nearest
repository above that starting point is then used, as before (decided during
implementation).

**Links open in the user environment (R35).** gpui starts `xdg-open` for every link it
opens, and those children run in the user environment too: the vendored `gpui_linux`
clears each one's environment and applies the map `sirio` gives it at startup
(`set_child_environment`, recorded in `rust/vendor/README.md`), so a browser a link opens
never inherits the mount's variables (decided during implementation).

**A chat agent's PATH keeps only absolute entries (R32).** The agent's `PATH`
keeps its absolute entries, in order. On unix a `PATH` left with none becomes
`/usr/bin:/bin`, because an empty `PATH` is the current directory to `execvp`.
An empty or relative entry would name a directory inside a worktree that may be
an untrusted clone (CWE-427). On Windows the agent's environment sets
`NoDefaultCurrentDirectoryInExePath=1`, so `cmd` does not search the working
directory first and a cloned repository cannot supply the agent's program
(decided during implementation).

### §5.4 macOS ships `sirioctl`

`Scripts/build-app-bundle.sh` takes the `sirioctl` binary the release job
already builds and puts it in `Contents/MacOS/`, beside `sirio`, so the
existing sibling lookup finds it; it is signed with the hardened runtime like
`sirio-host` before notarization. `Scripts/Tests/test-release-workflow.sh`
and `test-nightly-workflow.sh` guard the workflow's shape and change with it.

### §5.5 The Settings card

The card that shows "Bundled binary: sirioctl" becomes the CLI's status:

- the stable copy's path, its version, and whether it matches the app;
- where the PATH link is, whether the login shell finds it, or why not
  (foreign file, directory not on PATH, permission);
- an **Install in PATH** button (§5.2), which says what it did;
- the hooks' state per adapter (§5.6), with today's *Install Hooks*.

**Follow-up (R36):** the per-adapter hook state on this card is deferred. This project
shows the copy and the link; the hooks stay with *Install Hooks* until a later project
puts their state here.

### §5.6 The hooks, proven

The hooks are what feed the sidebar's working / idle / needs-input (Layer A),
and project 2's activity state builds on them. Project 1 proves, for each of
the five adapters:

- the **worktree-local** form `prepare` writes: the hook command, exactly as
  written in the agent's config, run inside a Sirio pane with the payload the
  CLI sends, changes that pane's status as `sirioctl` reports it;
- the **user-global** form *Install Hooks* writes (no `--session`; the pane
  id comes from `SIRIO_PANE_ID`): the same, plus: outside a Sirio pane it
  exits 0 and changes nothing;
- the hook still works after the stable copy is replaced under it (§5.1),
  and in a pane whose app has quit and relaunched.

What a stub CLI cannot prove is that the real CLI fires its hooks. A live
test does — one real `claude`/`codex`/`opencode` turn each in a scratch
worktree, observed through `sirioctl` — and SKIPs without the binary or its
credentials, like `sirio_agents/tests/initial_prompt_live.rs`; it sits off
both gates. Pi has no hooks and is exercised through Layer B as today.

*Claude's call:* the **new** files project 1 has `prepare` write (the Codex
and Pi skills) are excluded through the common directory's `info/exclude`
when untracked, as the hand-off's `.sirio/handoff` already is. A file a
project already **tracks** (a committed `.claude/settings.local.json`, say)
keeps today's behaviour — merged and shown as modified — which this project
does not change.

### §5.7 The skill, true and everywhere

- **Fix the four errors of §1**: export `SIRIO_WORKTREE_ID` (§5.3); remove
  `browser screenshot` from what is documented as working; make the
  `not_supported` table match what `sirioctl` answers (either `sirioctl`
  answers `not_supported` for them — preferred, it is the honest error — or
  the table goes); correct the `panel state` / `panel scrollback` fields.
- **All five adapters get it from `prepare`.** Codex and Pi gain a
  `skill_markdown` and a worktree-local skill path. Where each CLI looks is
  not guessed: the `skills` CLI's own per-agent table is the reference, and a
  live probe (SKIP without the binary) confirms each CLI loads the file.
- ***Install Skill* installs the bundled copy** at the user level for each
  agent, at the same per-agent places, instead of running `npx` against
  GitHub `main`. It refuses to overwrite a file without the Sirio marker, as
  `install_skill` does today.
- **The check of §4.4** lands here, against today's skill.
- The skill's content stays what it covers today, corrected. Teaching the
  director's workflow starts in project 2, when the verbs exist.

### §5.8 Proof

`Scripts/Tests/test-sirioctl-e2e.sh` → `SIRIOCTL E2E OK`, on the shape of the
other E2Es (`--out-dir DIR` keeps the transcript, the pane environments
captured and the copies' hashes; an isolated `HOME`, `XDG_*`, `SIRIO_DB`;
stub agent CLIs on PATH). Cases:

1. First launch installs the stable copy and the PATH link; a shell with the
   login PATH finds `sirioctl` and `sirioctl version` matches the app.
2. A launch with a different bundled `sirioctl` replaces the copy, while a
   process holds the old one open; a launch with the same one leaves it.
3. A foreign `sirioctl` at the link's place and at the stable path is left
   alone and reported.
4. A pane under a simulated AppImage launcher: `PATH` starts with the stable
   directory, no recorded-mount entry in `PATH`, `LD_LIBRARY_PATH`,
   `XDG_DATA_DIRS` or the gtk variables, `SIRIO_WORKTREE_ID` set — for a
   terminal tab, for `panel.create`, and for a chat agent's process; and the
   app's own process still carries the launcher's variables. Run against a
   fresh host, and against an adopted host that predates the capability
   (old behaviour, reported).
5. Quit, remove the simulated mount, relaunch: a pane opened before the quit
   still runs `sirioctl` successfully.
6. Hooks, worktree-local and user-global, for each adapter (§5.6), including
   after the copy was replaced.
7. The skill: written by `prepare` for all five adapters, identical to the
   bundled one; *Install Skill* writes the user-level copies in the private
   `HOME`; every command it names is accepted (§4.4).
8. A debug build never touches the release copy or the PATH link, and its
   *Install Hooks* writes user-global hooks naming the release stable path.

Linux runs the whole script. macOS and Windows run the cases that do not need
a display in `macos-check.yml` and `windows-check.yml`, and the macOS bundle
is checked to contain a signed `Contents/MacOS/sirioctl`. The administrator
prompt of the macOS button is a manual row, recorded with the date it was
done, like the host's detach probe.

## §6 Non-goals and anti-goals

- **Not in project 1:** any new verb for tabs, chats or the editor
  (project 2 on); rewriting the skill around the director (project 2).
- **No repair of live processes**: a session born with a dirty environment
  keeps it.
- **Anti-goal:** a startup step that asks for a password, shows a dialog or
  blocks the launch.
- **Anti-goal:** replacing a file Sirio did not create in the user's own
  places — at a PATH link's place, or a skill without the marker. (The
  stable path, inside Sirio's data directory, is Sirio's, §5.1.)
- **Anti-goal:** a skill whose commands the installed build does not serve.

## §7 Left to the builder

- The variable names AppRun uses to record the user environment, and how
  "absent" is recorded.
- How the release `capabilities` list is made available to the skill check
  without building a release binary (one function taking the build kind is
  the obvious shape).
- The Windows rename-aside and cleanup details, and the uninstaller's PATH
  removal.
- Whether the macOS password prompt goes through `osascript` or an
  authorization API.
- The audit of other child processes (§5.3) and its outcome, in the plan.
- The live hook test's exact prompts, kept cheap (one turn, smallest model
  the CLI allows).

## §8 Calls Claude made, for the user to check

- Forge writes stay debug-only even for the director (§4.1).
- The stable path is Sirio's: its content is replaced whenever it differs,
  with no ownership marker (§5.1).
- A debug build has its own `sirioctl` copy and never touches PATH; user-global
  hooks always name the release copy (§5.1).
- The environment restore applies to child processes only; the app keeps
  AppRun's for its webview (§5.3).
- The host gains an additive "whole environment" form of `term.create`; an
  adopted older host keeps today's behaviour until restarted (§5.3).
- Files a project already tracks keep today's behaviour under `prepare`
  (§5.6).
- macOS at startup never asks for a password; it links only where it can
  without one (§5.2).
- Only the CLI link is installed at startup; hooks and first-time skill
  installs stay behind their buttons, while an already-installed skill copy
  is refreshed (§5.2).
- `SIRIO_WORKTREE_ID` is exported rather than dropped from the skill (§5.3).
- The task board does not use upstream's `KanbanBoard` (§3, 7).
