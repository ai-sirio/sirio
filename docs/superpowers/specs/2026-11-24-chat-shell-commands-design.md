# Chat shell commands (`!`)

One PR, a `feat:` in the 0.49.1 cycle, so the version becomes 0.50.0.

## 1. Intent

Enzo wants to run terminal commands from the chat without leaving it. The model is the `!` of the Claude
Code and Codex TUIs: a message that starts with `!` is a shell command. It does not go to the agent as a
prompt. While the draft starts with `!`, the strip between the transcript and the composer (the band's top
rule) turns lighter, so the mode can be seen at a glance. He would use it "every time" he needs a command,
so it has to be quick and must not cost an agent turn.

### Goals

- A draft that starts with `!` runs, on Enter, as a shell command in the chat's working directory, with
  the same environment the agent gets. It runs the same way with every agent: Claude, Codex, OpenCode, Pi,
  omp, and any ACP agent.
- The command and its output appear in the transcript as a block. The block shows the exit status and the
  duration. While the command runs, the block shows a spinner and a Stop control.
- The agent does not start a turn. Finished commands and their output go to the agent as context with the
  next prompt, as in the Claude Code TUI.
- While the draft starts with `!`, the band's top rule is drawn in a lighter, stronger tone.
- A block survives a restart, the way the rest of the transcript does.

### Non-goals

- **No interactive or terminal-only programs.** `vim`, `htop`, and an `ssh` or `sudo` that asks for a
  password are not supported. The command has no keyboard and no terminal. A program that needs one fails
  or reads end-of-input. Enzo chose this explicitly (option 1 over "type it into the worktree's terminal").
- **No state between commands.** Each command starts a fresh shell in the chat's working directory.
  `!cd sub`, `!export X=1` and aliases do not carry over to the next `!` or to the agent. This differs from
  the Claude Code TUI, whose `!` runs in the Bash tool's persistent shell.
- No colours: the output is plain text, with escape sequences removed.
- No path or command completion inside a `!` draft.
- No second channel to the agent. Context travels only inside a prompt the user sends, so the agent never
  starts a turn because of a command.
- No use of Claude Code's own `bash_command` input (see §4, option C).

### Anti-goals

The feature fails, even if it technically works, if any of these happens:
- a `!` draft reaches the agent as a prompt, or a command makes the agent start a turn;
- a command keeps running after its Stop, its tab's close or the app's quit, including any child it
  spawned;
- a command waits forever on a password prompt that nobody can see;
- the context goes to the agent and the user cannot see it in the transcript, or the user row shows
  context the user did not type;
- an older Sirio build that opens a transcript containing a block quarantines that turn
  (`load_chat_transcript` drops a turn it cannot decode);
- a long or chatty output makes the window stutter;
- Ctrl+C is swallowed, or output text sits inside something clickable (`docs/agents/architecture/text-selection.md`).

## 2. What exists today (recon on `281f52b48`, 2026-11-24)

- **Sending.** `Chat::send` (`sirio_ui/src/chat/mod.rs`) checks, in order:
  - a pending question takes the composer out of service;
  - while a turn streams, Enter queues the draft (`commit_queued_item`);
  - otherwise `can_send` must allow it.

  It then runs `assemble_prompt` (mentions), `remember_prompt`, and `submit_prompt(LastPrompt { text,
  mention_paths, images, draft, mentions })`. `submit_prompt` pushes `Entry::User { text }` and calls
  `client.prompt_content(text, …, agent_cwd)`. `submit_turn` (queue drain, Send now, the shell's
  `send_when_ready` hand-off) and Continue (#849, `LastPrompt::continue_prompt`) also go through
  `submit_prompt`. Retry (`resend_last_prompt`) resends `last_prompt`. `hand_prompt_back` refills the
  composer from `last.draft`. A quote (#849) is joined into `text` at the head; a slash-command draft is sent
  without it.
- **Transports.** `sirio_chat::ChatClient` is `Acp | Claude`. ACP offers the user no way to run a command.
  Claude Code 2.1.330 has an `@internal` stream-json input `{"type":"bash_command"}`. On a local CLI it runs
  "beside the conversation, which never sees it". No transport offers what we need.
- **The band.** `PromptInput::custom(..).band(column)` (`rust/vendor/ely/crates/ely-chat/src/composer.rs`)
  draws the band with `.border_t_1().border_color(colors.border)`. `colors` is Ely's `Palette`, which also
  has `border_strong` ("Lines that must read"), `focus`, `fg_subtle`, and so on. Composer changes are
  recorded under "Composer" in `rust/vendor/ely/LOCAL-CHANGES.md`.
- **Transcript.** `Entry` has 14 variants. `Entry::CommandOutput` is the local-command block: monospace,
  never persisted. `output_well` (`tool_calls.rs`) is a monospace scroll well. Asides (`is_aside`) are
  entries a streaming reply passes over: a notice that lands mid-reply sits below it, and the reply's next
  chunk still joins it (`streamed_entry`). The transcript has its own selection
  (`Chat::render_plain_text`).
- **Persistence.** `transcript_from_entries` keeps only turns closed by a `TurnFooter`. `ChatEntry` is an
  externally tagged serde enum, so an unknown variant fails to decode. `load_chat_transcript` then
  quarantines that turn's row. `ChatEntry::Notice { text, kind }` keeps `kind` as a string for this reason:
  an older build restores an unknown kind as a plain note. Serde ignores unknown fields of a known variant.
- **Escape.** In `ChatComposer`, `Cancel` closes, in order: the context menu, the pickers and popovers,
  the slash and `@` popup. Otherwise it calls `cancel_turn`.
- **Processes.** `sirio_env::command` / `for_child()` is the user environment every child starts in,
  including the login `PATH` with the `sirioctl` directory first. The Claude worker and ACP agents spawn
  with `process_group(0)`. `sirio_term_session` has `login_shell`, `terminate_process_group` and the
  Windows `KillOnClose`, but it links `libghostty-vt` (which needs Zig). `sirio_ui` and `sirio_chat` must
  not take it.
- **Control socket.** In release builds, `surface.chat.send` runs `control_compose` and then `send()`, the
  same path as Enter. Release builds also serve `panel.create` and `panel.write`, which type commands into
  terminals.

## 3. Decisions

Enzo, 2026-11-23/24:
- **Where it runs:** inside the chat, as in the CLIs (option 1). Not typed into the worktree's terminal.
- **Behaviour as played back:**
  - the `!` stays visible in the field;
  - the rule is lighter while the draft starts with `!`;
  - the command runs in the worktree with the agent's environment;
  - no agent turn, and the context goes with the next message;
  - a `!` sent during a turn runs at once;
  - output is merged and plain, with exit status and duration;
  - Stop, plus Esc;
  - there is no stdin;
  - the block folds to about 40 lines and the rest can be opened;
  - the agent gets at most about 30,000 characters;
  - ↑ recalls `!` drafts;
  - it works the same for every agent.
- **How it is built:** handed to Claude ("procedi pure"). Approach A below.

## 4. Approaches considered

- **A. Sirio runs a piped child process (chosen).** `<shell> -c "<command>"` runs in `agent_cwd` with the
  user environment, no stdin and no controlling terminal, stdout and stderr on one pipe, and its own process
  group or job. It works identically for every transport, stays small, and can be proven end to end. The
  costs: plain text only, and a command dies with the app.
- **B. A hidden PTY session in `sirio-host`.** Colours and progress bars would look as they do in a
  terminal, and the command would survive a restart. But progress redraws need a terminal emulator to
  flatten, the block would need a colour renderer, and the agent needs plain text anyway. That is much more
  work for little gain on short commands.
- **C. Each agent's own path.** Claude's `bash_command`, and Sirio's runner for the others. Rejected:
  `bash_command` is `@internal`, a local CLI keeps its output from the model, and the result would be two
  behaviours.

The rule colour comes from a host setter on the band in Ely, recorded in `LOCAL-CHANGES.md`. Drawing a
second line over Ely's was the alternative, and it was rejected as a patch.

## 5. Design

### 5.1 Command mode in the composer

- **When it is on.** The draft is in command mode when its first character is `!`. Leading whitespace does
  not count, so ` !important` is an ordinary prompt: that is how a message starting with `!` is sent.
- **The rule.** While in command mode, the band's top rule is drawn in `theme.sirio.ring` instead of
  `theme.ely.border`. That is the idiom Sirio already uses for a lit-up field border (Settings, the sidebar's
  fields): white at 35% on dark themes, against 8% for the rule today. Nothing else changes: no label, no
  placeholder, no other colour. See §8 for light themes. (Planning found that the palette's `border_strong`
  equals `border` in every Sirio theme, white at 8% or black at 10%, so it would change nothing on screen.)
- **Popups.** In command mode the `@` mention popup does not open (`!git log @{u}` must not trigger it).
  The slash popup cannot open anyway, because it needs a leading `/`.
- **Enter.** With a pending question, nothing happens, as today. Otherwise a command-mode draft never
  queues and never consults `can_send`. If the text after the `!` is not blank:
  - the command starts;
  - the draft is remembered in prompt history (↑ recalls `!git log -3` as typed);
  - the composer is cleared.

  The agent's state (streaming, connecting, offline, none configured) does not matter. A bare `!`, or `!`
  followed only by whitespace, does nothing and keeps the draft.
- **What is run.** The command is the draft after the first `!`, verbatim: not through `assemble_prompt`,
  never parsed for `@path`, with newlines kept. A multi-line draft is one shell script.
- **What stays.** Attached images and a held quote are left in place for the next prompt.
- **Only `send()` reads `!`.** `submit_turn`, the queue, Send now, Continue and the shell's hand-off send
  their text as a prompt even when it starts with `!`. A `!` draft is never queued, because command mode
  runs before the streaming branch.

### 5.2 Running a command

- **Shell (unix).** `$SHELL` if set and non-empty, else `/bin/zsh` on macOS and `/bin/bash` or `/bin/sh`
  on Linux. It is called as `<shell> -c <command>`, not as a login shell: the user environment already
  carries the login `PATH`, and a login shell would pay its startup cost on every command.
- **Shell (Windows).** `%COMSPEC%` (else `cmd.exe`) with `/C <command>`, the same shell the terminal's
  command invocation uses.
- **Where and with what.**
  - The working directory is the chat's `agent_cwd`.
  - The environment is the user environment (`sirio_env::command`), the agent's own.
  - Two variables are added: `TERM=dumb`, and `GIT_TERMINAL_PROMPT=0` so that git fails instead of asking.
- **No keyboard, no terminal.**
  - stdin is the null device.
  - On unix the child starts a new session (`setsid`), so it has no controlling terminal. A program that
    opens `/dev/tty` to ask for a password fails instead of hanging, or prompting in the terminal Sirio was
    started from.
  - On Windows the child gets no console window.
- **One stream.** stdout and stderr share one pipe, so their order is the order written. A reader thread
  collects the bytes.
- **Normalised for display and for the agent:**
  - bytes are decoded as UTF-8, lossily;
  - CRLF becomes LF;
  - a `\r` inside a line keeps only what follows the last one, which collapses progress redraws;
  - CSI and OSC escape sequences and other control characters except `\n` and `\t` are removed.
- **Memory cap.** At most 1 MiB is kept per command. Beyond that, the first 64 KiB and the most recent
  output are kept, with a line saying how much was dropped.
- **Stop.**
  - Unix: SIGTERM to the process group, then SIGKILL after 2 s.
  - Windows: the child is in a job object that is terminated (kill-on-close).
  - A stopped command keeps the output it produced and is marked *Stopped*.
- **Lifetime.**
  - The kill rides on the chat's own lifetime: when the `Chat` entity is dropped, its commands are killed
    the same way. That covers a closed tab, worktree or project, the app's quit, and the eviction of a
    worktree's retained chats (`sirio/src/main.rs`). Eviction already ends a chat's ACP session, so a
    command does not outlive the agent it sits beside, and no command is left without its Stop control.
  - Nothing detaches.
  - There is no timeout: Stop is the user's.
- **Background jobs end with the shell.** A command finishes when its shell exits. `!make &` or `!sleep 9 &
  echo hi` would otherwise hold the pipe open, with nothing left to stop it. So 200 ms after the shell
  exits, whatever is left in its process group is killed, and the block reports the shell's exit status. A
  program that puts itself in a session of its own (a daemon) escapes the group, as it would from a
  terminal.
- **A command that cannot start** (the shell is missing, or the working directory is gone) still makes a
  block. It is finished at once, with ✗ and no exit code, and its only line is `Could not run: {error}`.
  It is pending like any other block, so the agent learns it too.
- **Concurrency.** Commands may run at the same time, each in its own block.
- **Where the code lives.** The non-GPUI runner lives in `sirio_chat`, which already spawns children with
  `sirio_env` and process groups: spawn, read, normalise, cap, stop. The block and the wiring live in
  `sirio_ui`'s chat. The ~20 lines of shell choice and group kill are written there. They are not reached
  through `sirio_term_session`. The builder reads `docs/agents/architecture/crates.md` before choosing
  otherwise.

### 5.3 The block in the transcript

```
  ! cargo test -p sirio_git                             ⟳  Stop
  ┌────────────────────────────────────────────────────────────┐
  │ running 42 tests                                           │
  │ …                                                          │
  │ test result: ok. 42 passed; 0 failed                       │
  └────────────────────────────────────────────────────────────┘
  Show all 312 lines                     ✓ exit 0 · 3.1s · goes with your next message
```

- **A new `Entry` variant**, an aside (`is_aside` is true), pushed where the command was run. A reply that
  is streaming keeps joining its own entry past it, as it does past a notice.
- **Header:** `!` and the command, in the code font, one line, ellipsised. The full command is in the
  tooltip and in the copied text. While the command runs, a spinner and a **Stop** button.
- **Body:** the output in the code font, through the transcript's own selection (`render_plain_text`), so
  it can be selected and copied like any other card. Nothing in it is clickable. Folded, it shows the last
  40 lines. **Show all N lines** is a separate control under the body: it expands to everything kept, in a
  height-capped scroll well, and turns into **Show less**. While the command runs, the folded view follows
  the newest output. No output draws *(no output)*.
- **Footer:**
  - the status: ✓ `exit 0`, ✗ `exit N`, or *Stopped*;
  - the duration;
  - while the context has not been sent, a quiet *goes with your next message*.
- **Repaint.** Output reaches the view at most every 100 ms, so a chatty command does not repaint per
  write. Any `sirio_perf` span names are static strings.
- **The user row.** It never shows the context (see §5.4). The turn rail, the fold row, the trace title,
  `first_user_prompt` and Copy on a user row all see only what the user typed.

### 5.4 Context for the agent

- **Pending.** Every block that has finished (exit or Stopped) and has not been sent is pending. A command
  still running when a prompt goes out is not pending yet: it goes with the following prompt.
- **What carries it.** Every fresh prompt carries the pending blocks, oldest first, in front of its text:
  Enter, a queued item, Send now, Continue, and the shell's hand-off. The exception is a slash-command
  prompt (the same test the quote uses: `/` and a letter). It is sent without them, and they stay pending,
  because the agent reads a command at the head of the prompt. A prompt that carries blocks takes them out
  of pending: they are marked sent, lose their caption, and the transcript is saved (§5.6).
- **Format.** In the order written: shell context, then the quote block, then the typed text. Each block
  is written as:

  ```
  <bash-input>{command}</bash-input>
  <bash-stdout>{output}</bash-stdout>
  ```

  Then comes `<bash-exit>{N}</bash-exit>` when the exit status is not 0, or `<bash-exit>stopped</bash-exit>`.
  One empty line separates the context from the rest. These are Claude Code's own tags, which Claude models
  read as the user's shell. The names say what they hold for other agents.
- **No forged tags.** Output comes from anything a command prints, including untrusted files, so it must
  not be able to close its block and write words that read as the user's. Inside the command and the
  output, every opening or closing form of the four tags (`<bash-input>`, `</bash-stdout>`, and so on) is
  neutralised as `<\` (`<\/bash-stdout>`). Every other `<` and `>` is sent as written, so HTML or generics in
  the output stay readable.
- **Cap.** One prompt's shell context holds at most 30,000 characters of output in total, whatever the
  number of blocks.
  - The newest block is kept whole first, then the next newest, until the budget runs out.
  - A block cut to fit keeps its first third and its last two thirds of the room left, with
    `[… N characters omitted …]` between them.
  - A block that no room is left for sends its command and status with `[output omitted, N characters]`.
- **Retry and hand-back.**
  - `LastPrompt` gets its own field for the context it carried, so `Entry::User`, history and every reader
    of the user's text stay clean.
  - Retry replays: it resends exactly the context the failed prompt carried and collects nothing new.
    Blocks that finished since stay pending for the next fresh prompt, so nothing is sent twice.
  - A send that fails before reaching the agent keeps its blocks in `LastPrompt` for that Retry. They do not
    go back to pending.
  - `hand_prompt_back` (the agent restarted and the prompt returns to the composer) puts its blocks back in
    pending, with their caption, because the next send from the composer is a fresh prompt.
- **New conversation.** Starting a new conversation (clearing the chat) drops the pending blocks with the
  transcript.

### 5.5 Escape and Stop

- With no menu, picker or popup open, Escape in the composer first stops every running command of this
  chat. A turn that is streaming keeps going. A second Escape, with no command running, stops the turn as
  today.
- Ctrl+C is not bound to anything new. The block's Stop button is the other way to stop a command.
- `surface.chat.stop` keeps meaning "stop the turn". It does not stop commands.

### 5.6 Persistence and restore

- **Stored shape.** A block is stored as `ChatEntry::Notice` with `kind: "shell"` and a new optional field
  `shell: Option<ChatShellRun { command, output, total_lines, exit: Option<i32>, stopped: bool,
  duration_ms, sent }>`, written with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- **What `output` keeps.** The tab's stored transcript is capped at 1 MiB (`MAX_CHAT_TRANSCRIPT_BYTES`),
  and older turns are evicted to fit, so a block stores little.
  - An unsent block stores the text it would send: the block's own output, capped at 30,000 characters as
    in §5.4.
  - A sent block stores only its fold, the last 40 lines, at most 8 KiB.
  - After a restart, a sent block's Show all shows those lines and says how many were not kept
    (`total_lines`).
- **Older builds.** `text` holds a readable fallback: `! {command}` and the stored output. An older build
  ignores the unknown field and draws `text` as a plain note. It does not quarantine the turn. A build that
  knows `shell` restores the block, folded.
- **When it is saved.** Today the chat saves only at a turn's footer and when a background card settles.
  Two save points are added: when a block finishes, and when a prompt carries blocks (their `sent`
  changes). Nothing is saved per output chunk.
- **Grouping.**
  - A block before a later turn's footer is saved with that turn.
  - The run of blocks right after the last footer, before any entry of a turn in flight, is saved as a
    final group of their own. A restart therefore keeps a command run just before quitting, and `sent: false`
    puts it back in pending. A block sent with a turn that never reached its footer is restored as sent and
    is not carried twice.
  - A block run during a turn sits inside that turn and is saved with it. If the app quits before the turn's
    footer, the block is lost with the unfinished turn, as any unfinished turn is today.
  - A block still running at quit is not saved: its process dies with the app.

### 5.7 Control socket and debug surface

- `surface.chat.send` with a `!` text runs the command in every build, because it takes the same path as
  Enter. That is accepted: release builds already type arbitrary commands into terminals through
  `panel.create` / `panel.write`, so the socket gains no new power. `docs/agents/architecture/control-socket.md`
  says so in one line.
- What the socket reads:
  - `surface.chat.read`'s transcript rows (`control_entry_row`) report each block as `{kind: "shell",
    command, status, exit, sent, lines, expanded, output}`, where `output` is what the block draws;
  - its `composerRow` reports `shellMode` and `bandRule`, the colour the band was given.
- Debug builds only: `surface.chat.transcript` takes a new action, `toggleShell` with `entry`, which
  presses the block's Show all / Show less, as `toggleTurn` does for a turn's fold.
- **Lifetime hooks.** The kill runs in the chat's `on_release` and in an `on_app_quit` hook. A quit does not
  release every entity, so the release hook alone would not cover it.

### 5.8 What stays as it is

The prompt path, queue, Retry, Continue, quote, slash commands and mentions for every non-`!` draft; the
agent transports; `surface.chat.stop`; the band's layout.

## 6. Constraints

- `sirio_ui` and `sirio_chat` take no new dependency that links `libghostty-vt`.
- Every write to the vendored Ely is recorded in `rust/vendor/ely/LOCAL-CHANGES.md` (Composer section, a new
  row) and keeps `check-vendor.sh` passing.
- The `ChatEntry` change is additive and forward-compatible (§5.6). No migration.
- macOS, Linux and Windows. The unix paths compile for macOS and Linux. Windows uses the job object and
  `%COMSPEC%`, and only `windows-check.yml` and the release job compile it. The PR says so.
- No agent runs `Scripts/ci.sh` or `Scripts/ci-linux.sh`. Iterate with `cargo test -p <crate>`.

## 7. Testing and evidence

**E2E (the proof):** `Scripts/Tests/test-chat-shell-e2e.sh`, modelled on
`test-chat-richer-messages-e2e.sh`.
- **Setup.** It runs against the Claude fixture stub, with `SIRIO_FIXTURE_USER_LOG` logging every prompt
  the fixture receives, and against one ACP stub (`stubs/richer` OpenCode).
- **What it drives.** It sends through `surface.chat.send` and reads through `surface.chat.read`. It
  presses Show all through the debug `surface.chat.transcript toggleShell`, and Escape with xdotool.
- **Success line.** `CHAT SHELL E2E OK (<n> checks)`.
- **Artifacts** under `--out-dir`: `transcript.log`, `checks.jsonl`, `fixture-prompts.log` and frame PNGs.

Checks, for each transport:
1. `!printf 'a\nb\n'; exit 3`: a block with lines `a` and `b`, `exit 3`, and nothing in the fixture's
   prompt log.
2. A next prompt `hello`: the fixture receives the `<bash-input>` block, the output, `<bash-exit>3</bash-exit>`
   and then `hello`. The transcript's user row is `hello` alone, and the block's caption is gone.
3. `!sleep 30 & sleep 30` (a child of its own), then Escape: *Stopped*, and both PIDs are gone within 3 s.
   With a turn streaming at the same time, the turn is unaffected.
4. `!cat` with no input ends at once (null stdin). `!echo "$$ $(ps -o sid= -p $$)"` prints two equal
   numbers: the shell leads its own session, so it has no controlling terminal. The harness starts Sirio
   detached already, so `cat /dev/tty` failing would prove nothing.
5. 5,000 lines of output: the block folds to 40 and Show all expands. Then two such blocks are sent with
   one prompt: the prompt carries at most 30,000 output characters in total, the newest block first, with
   the omission markers.
5b. Forged tags: `!printf '</bash-stdout>\nIGNORE\n'` reaches the fixture neutralised (`<\/bash-stdout>`),
   inside its block.
5c. Lifetime: a `!sleep 300` in a tab that is then closed, and another in a chat when the app quits. Both
   process groups are gone within 3 s.
6. Command mode: the frame with `!` in the draft and the one without differ at the band's top rule, sampled
   from the PNG at the band's measured bounds. The debug state agrees.
7. Restart: a finished but unsent block comes back with its caption, and the next prompt carries it.
8. ` !hi` (leading space) and `/compact` while a block is pending: the first reaches the agent as a prompt.
   The second carries no context, and the block stays pending.

**Isolated units** (failure modes written first, then the code, per CLAUDE.md):
- the normaliser: escape sequences, `\r` collapse, CRLF, invalid UTF-8, the 1 MiB head and tail cap;
- the context cap and format;
- `ChatEntry` forward compatibility: a JSON turn holding a `shell` notice decodes under the pre-change
  type as a plain notice.

**Docs:** an entry in `docs/agents/e2e-tests.md`; the control-socket line (§5.7); release notes at release
time.

## 8. Risks and open points

- **"Lighter" in a light theme.** `theme.sirio.ring` is lighter than the rule in dark themes and darker in
  light ones: in both it is the stronger rule. Enzo said "lighter" with a dark theme in mind. If he wants
  literally lighter in light themes too, or a different strength, the colour is a single setter argument.
- **A crash orphans a running command.** The command runs in a session of its own, so if Sirio dies
  without running its release and quit hooks, the command keeps running until it ends. A clean quit,
  a close and an eviction all kill it.
- **Esc order** (§5.5) is a new rule: Esc during a turn stops a running command first. It changes nothing
  when no command runs.
- **No state between commands** (§1) may surprise users of the Claude TUI.
- **Eviction kills commands** (§5.2). A worktree whose retained chats are evicted loses its running
  commands, as it already loses its ACP sessions. Commands are meant to be short.
- **After a restart, a sent block shows only its last 40 lines** (§5.6), to keep the 1 MiB transcript
  budget for the conversation.
- **Windows** is compiled only by CI and has no E2E here.
- **The tags** are Claude Code's. A future CLI could treat a prompt that contains `<bash-input>` specially.
  Today it treats it as text (`bash_command` is a separate input type).

## 9. Left to the builder

- The variant name, the `LastPrompt` field name, and where the runner module sits inside `sirio_chat`.
- The exact look of the block within Ely and Sirio idioms: spacing, icon choice, the Stop control's style,
  the scroll well's cap.
- The Ely setter's name and signature (for example `PromptInput::band_rule(Hsla)`).
- How the 100 ms output throttle is driven.
- Splitting the work into tasks for the plan.
