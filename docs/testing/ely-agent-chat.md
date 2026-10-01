# Verifying the Ely agent chat

The AI chat is drawn with the vendored [Ely GPUI components](../../rust/vendor/ely-gpui-component/LOCAL-CHANGES.md)
over Sirio's own theme and state (`docs/superpowers/specs/2026-09-30-ely-agent-chat-design.md`).
This page says how that is checked, what each check does and does not prove,
and which platforms have been seen with real input and rendering.

Three lanes, each with a different strength:

| Lane | What it drives | Proves |
|---|---|---|
| `cargo test -p sirio_ui --lib chat::` | The real `Chat` in GPUI's test harness, real fixture subprocesses, real actions (clicks, keys, drags, clipboard) | Behaviour and layout contracts, fast, every platform that builds |
| `Scripts/Tests/test-ely-chat-ui-e2e.py` | **The real Sirio app**, isolated, over its control socket, with PID-matched X11 captures | The app wiring: restore, persistence, the control protocol, the chat in the shell's real layout |
| `ely_chat_probe` (an example) | The actual `Chat` in a bare window with an **explicit agent identity**, driven with real pointer and keys | Interaction the control socket cannot reach (menus, popups, selection) and the five agent marks |

Nothing is claimed for a platform that was not run. See [Platform evidence](#platform-evidence).

## The real-app runner

```bash
cargo build -p sirio --bin sirio -p sirio_persistence --example ely_chat_seed -p sirio_ui --example ely_chat_probe
python3 Scripts/Tests/test-ely-chat-ui-e2e.py --state-only --out-dir /tmp/sirio-ely-chat-state
python3 Scripts/Tests/test-ely-chat-ui-e2e.py --display :N      --out-dir /tmp/sirio-ely-chat-native
```

`--state-only` exercises state and omits frames. Without it a display is
required, with `xwininfo`, `xprop`, `import`, `identify` on `PATH` (and
`xdotool` for the identity lane, the scroll check and `--window-size`). Other
flags: `--no-build`, `--no-identity`, `--window-size WxH`, `--sirio-bin PATH`
(run the same scenarios against another build) and `--only NAME[,NAME]`.

A run ends with `ELY CHAT UI E2E OK` and the list of artifact paths, or with
`FAIL: <reason>` and a nonzero exit.

### Isolation

Every scenario is its own Sirio process with a scratch `SIRIO_DB`,
`SIRIO_SOCKET` and `SIRIO_CREDENTIALS`, in a scratch git repository, seeded by
`ely_chat_seed` (public persistence APIs only: project, worktree, one active
`chat` tab, sidebar selection and `AppSettings` for appearance, interface size
and updates off). `ely_chat_seed` refuses a database that already exists. The
user's database, credentials and running Sirio are never touched.

### The agent

The Python fixture (`rust/crates/sirio_ui/tests/fixtures/chat_fixture.py`) is the
agent, through a proxy that records its traffic (`agent-traffic.jsonl`). It
stands in for **the `opencode` binary**: the seeded tab records the OpenCode
adapter, an executable named `opencode` is first on the instance's `PATH`, and
Sirio's own restore path resolves OpenCode's built-in `opencode acp` and
launches it. So the fixture is reached the way a real agent is.

It cannot ride `SIRIO_ACP_PROGRAM`, as the plan first assumed: a persisted
chat with no recorded agent is refused on restore ("saved before Sirio recorded
which agent it belonged to"), and `SIRIO_ACP_PROGRAM` is read only when a *new*
chat is opened with no agent picked. The consequence: the app frames show
OpenCode's mark because the persisted identity says so; the fixture run
establishes **no** provider-logo identity. The five marks come from the probe.

### Control protocol

Existing methods only, newline JSON, every parameter a string:
`surface.chat.open` (selects an already rendered chat — it does **not** create
one; the seeded database does), `surface.chat.send` / `.compose`
(`surfaceId`, `text`), `surface.chat.permission` (`surfaceId`, `requestId`,
`optionId`), `surface.chat.stop` / `.read` (`surfaceId`), `system.quit`. The
helper is `request(socket_path, method, params)`; a non-string parameter raises.
Every request and reply goes to `responses.jsonl`.

One read-model change came out of writing the runner: a plan's pending
approval was not in `surface.chat.read`, so a client could not answer it
without guessing the request handle. The `plan` row now carries `id` and
`status` when it holds an approval, like a `permission` row.

### Scenarios

| Scenario | Asserted (state) | Frames |
|---|---|---|
| `staged_queue` | first turn streams; two prompts queue in order; after release the queue drains, each prompt exactly once, in order; an absent surface **fails** | streaming + queue, drained |
| `permission` | a request is pending; an unknown request id is refused; answering completes the turn and marks the row `selected` | pending, answered |
| `question` | listed-option question answered with its option id and echoed | pending, answered |
| `plan` | plan awaits approval; approving advances the plan entries | pending, approved |
| `cancel` | partial reply streams; stop keeps it; a later prompt completes | partial, after cancel |
| `death_recovery` | the agent dies and the chat shows an error; after a relaunch the same agent answers | died, recovered |
| `authentication` | the chat names the authentication it needs | banner |
| `restore` | light theme, interface size 15; the transcript survives a relaunch and reads `completed` | before, restored |
| `activity` | tool statuses `Completed`/`Failed`/`InProgress`; reasoning reaches the transcript | tools and reasoning |
| `subagent` | the delegating task completes | task card |
| `long_history` | 60 turns in order; the wheel scrolls the transcript; **a view scrolled back does not move when a turn arrives below** (pixel comparison of the transcript area, rail and scrollbar excluded) | tail, scrolled up, after a new turn, back at the tail |

Then the identity lane: the five agents (Claude Code, Codex, OpenCode, Pi,
Oh-My-Pi) × dark/light, as the probe with `ELY_PROBE_AGENT`.

### Failure discrimination

The runner was made to fail on purpose; each row below was run and its output
recorded.

| Condition | Result |
|---|---|
| An absent surface (`surface.chat.read`, `surfaceId=no-such-surface`) | `ok: false`, `unknown chat surface: no-such-surface` — asserted by `staged_queue` |
| An unknown permission request id | `ok: false` — asserted by `permission` |
| Capture lane with no `DISPLAY` | `FAIL: no display for the capture lane: pass --display :N, or --state-only`, exit 1 |
| Capture lane with an unreachable display | `FAIL: cannot open display ':99' for the capture lane: ... (use --state-only to skip frames)`, exit 1 |
| `--state-only` on the same machine | exercises all scenarios, exit 0 |
| `ELY_E2E_CORRUPT=1` (the first scenario's expected reply gets a `!`) | `FAIL: staged-queue: the first reply is not 'first streamed!'`, exit 1 |
| The same run without it | `ELY CHAT UI E2E OK`, exit 0 |
| A frame | rejected if it is blank (fewer than 200 colours) or if no window carries the launched PID in `_NET_WM_PID` |
| A wheel that moves nothing | the scroll check fails (`scrolling back changed N pixels` must be at least 500) |

## The probe lane

`rust/crates/sirio_ui/examples/ely_chat_probe.rs` opens the actual `Chat` in a
bare window. Run it on an isolated display, match the window to the process you
launched (`_NET_WM_PID`) before sending input or capturing, and never touch a
user's own Sirio:

```bash
cargo build -p sirio_ui --example ely_chat_probe
DISPLAY=:N ELY_PROBE_CHAT=1 ELY_PROBE_FIXTURE_MODE=ely-catalogue \
  ELY_PROBE_APPEARANCE=dark ELY_PROBE_UI_SIZE=13 ELY_PROBE_AGENT=codex \
  ELY_PROBE_FIXTURE_DIR=/tmp/scratch ELY_PROBE_HISTORY_DB=/tmp/scratch/history.db \
  target/debug/examples/ely_chat_probe
```

| Variable | Does |
|---|---|
| `ELY_PROBE_CHAT=1` | the real `Chat` (otherwise a component gallery) |
| `ELY_PROBE_FIXTURE_MODE` | the fixture mode: `plain`, `staged`, `permission`, `question-options`, `plan`, `cancel`, `auth-required`, `death-then-ok`, `echo-blocks`, `ely-permission` (opaque option ids), `ely-expiry`, `ely-activity` (tools, diff, reasoning), `ely-catalogue` (modes, long model names, four effort levels, 85% context) |
| `ELY_PROBE_FIXTURE_DIR` | the fixture's scratch directory (`go` gate, `wire.jsonl` of real replies) |
| `ELY_PROBE_AGENT` | `claude`, `codex`, `opencode`, `pi` or `omp`: the identity supplied to the chat |
| `ELY_PROBE_APPEARANCE`, `ELY_PROBE_UI_SIZE` | `light`/`dark`; interface size 12–18 |
| `ELY_PROBE_HISTORY_DB` | seeds a scratch database (long and empty titles) and runs the chat with persistence on, for the History popover |
| `ELY_PROBE_CLIPBOARD_IMAGE` | puts a PNG on the clipboard, so Ctrl+V attaches it (X11 has no portal file dialog here) |
| `ELY_PROBE_HISTORY` | restores a persisted transcript JSON |

Input comes from `xdotool` (`mousemove --window`, `click`, `type`, `key`) and
frames from ImageMagick `import`. The popups, menus, composer states, queue,
dock and the interface sizes in the matrix were exercised this way.

## The §9 matrix

Source of each observation: **app** = the runner against the real app,
**probe** = `ely_chat_probe` with real pointer/keys (see
[The probe lane](#the-probe-lane), which reproduces them), **test** = a
`sirio_ui` chat test.

| Coverage | Observed | Source |
|---|---|---|
| Empty / connecting / authentication | empty chat, `connecting` state, the authentication banner with its guidance, draft kept | app (`authentication`, `restore`), probe (Task 3) |
| Streaming / completed / interrupted / error | streaming, completed, stop, agent death with its error card, retry after death, stable transitions | app (`staged_queue`, `cancel`, `death_recovery`), probe (Task 3) |
| Tools / reasoning / plans / subagents | tool statuses, grouped reads, reasoning, plan entries advancing, a delegated task; expandable arguments, nested scrolling, rich diff with Unicode, selection and copy | app (`activity`, `plan`, `subagent`), probe (Task 4), tests |
| Questions / permissions | actual option ids (`allow`, `green`, `allow:this-call`), unknown ids refused, expiry and the historical card, a request never answered after it expired | app, probe (Task 5), tests |
| Composer | slash and `@` completion, attachment chips (wrapping), attachment-only send, IME-composition guard, queue FIFO / fold / clear / remove / send now, catalogue controls | probe (Tasks 6–7), tests; queue also app |
| Long / restored history | 60 turns, folded older turns, the "Latest" jump, scroll anchoring (the scrolled-back view did not change by one pixel when a turn arrived), restore after relaunch | app (`long_history`, `restore`), tests (turn rail, virtualization) |
| Chat menu / history | follow toggle, new conversation, history with a long title cut, open rebinding persistence, confirmed deletion, both secondary-click menus | probe (Task 7), test `ely_history_opens_the_chosen_session_and_confirms_deletion` |
| Appearance / layout | dark and light, interface size 12 / 13 / 15 / 18, wide and narrow panes (the app's chat column is ~360px), white marks on plates in light | app (`restore` light/15), probe (Tasks 3–7) |
| Agent variations | the five marks; controls absent when the agent reports none | probe (`identity/*.png`), tests |
| Rewind | eligibility only | test (`the_rewind_action_appears_only_where_it_can_work` and the rewind cards' tests). **Not seen natively:** it needs a native Claude chat with checkpoints, and no real agent was run |

### Not established

- A real **IME** (IBus, fcitx, macOS, Windows). Composition is exercised through
  the field's platform input handler in a test; the native event was not
  synthesised. Do not read the guard as proof for a platform that delivers Enter
  after the IME has committed.
- **Rewind** natively (above).
- Pointer **drag selection in the bare X11 lane**: native drag coordinates did not
  start a selection there (Select All + copy + paste worked); the drag/copy
  contract is proved by the chat tests that drive an actual GPUI drag.

## Performance against the baseline

Baseline = the commit the branch started from (`5f53b064`), built in a separate
worktree and target. Same machine, same Xvfb, same fixture.

`Scripts/perf/chat-stream-compare.py` runs the existing perf fixture
(`Scripts/perf/acp-fixture.py`: 500 seeded entries, then 1500 chunks at 20 ms)
through the real app's restore path with the opt-in trace (`SIRIO_PERF_TRACE`,
static content-free span names) and reports the process's CPU over the stream
and the span totals. Two runs each, alternating.

| Build | CPU s (of ~30.5 s) | `Chat.handle_event` | `Chat.parse_markdown` | `Chat.render` | `Chat.render_entry` |
|---|---|---|---|---|---|
| release, baseline | 27.5 / 27.6 | 1817 / 1861 ms | 1628 / 1666 ms | 540 / 539 calls, 120 / 127 ms | 556 / 550 calls, 4.8 / 4.7 ms |
| release, branch | 28.3 / 28.2 | 1893 / 1866 ms | 1689 / 1674 ms | 458 / 466 calls, 107 / 110 ms | 461 / 470 calls, 7.7 / 7.6 ms |
| debug, baseline | 31.5 / 31.6 | 14094 / 13952 ms | 13671 / 13534 ms | 126 / 121 calls | 136 / 129 calls, 5.2 / 5.4 ms |
| debug, branch | 31.8 / 32.5 | 13685 / 14740 ms | 13267 / 14307 ms | 87 / 84 calls | 91 / 86 calls, 7.1 / 7.2 ms |

Reading it:

- **Streaming still updates the one affected row.** Markdown is parsed once per
  chunk (1500), never for the 500-entry history, with the same cost as before.
  `Chat.remeasure_entry` ran 1500 times in both.
- **Fewer frames are drawn at the same CPU** — about 14% fewer in release
  (31% in debug). The instrumented spans do not move (`Chat.render` costs about
  the same per call); the difference is in what they do not cover, GPUI's
  layout, paint and the software rasteriser (the Xvfb has no GPU, ~50 ms a frame).
  A 700×500 window gave the same ratio as 1280×833, so it is not fill area.
  Nor is it the rows' own build: the list re-renders about one row a frame
  (the streaming one), and `Chat.render_entry` — which includes the agent
  mark, rebuilt per row — totals under 8 ms over the whole stream on either
  build, a few milliseconds more on the branch against seconds of missing frames.
  It was not localised further: stack sampling was not possible here
  (`ptrace_scope=1`, no `perf`), and the native GPUI journal needs a feature
  build and a driver this repository does not carry. **Read it as a measured,
  unexplained per-frame cost on a software-rendered display, not as a GPU
  result.**
- Scroll anchoring and the "Latest" affordance behave the same as the baseline
  (the same `long_history` run on both builds: 0 pixels moved).

## Platform evidence

| Host | Toolchain | Run | Result |
|---|---|---|---|
| Linux x86_64, Arch, kernel 7.2.3, isolated Xvfb (software Mesa), X11 | rustc 1.98.1, cargo 1.98.1, Zig 0.15.2 | the gate below, the runner, the probe | passed |
| macOS (reference platform), Windows, physical Wayland | — | not available in this environment | **unverified** |

An attempted `cargo check -p ely-gpui-component --target aarch64-apple-darwin`
stops in a dependency's C build (`cc-rs`, no macOS SDK): a cross-target source
check was not obtained, and would not be a native rendering or input result if
it were. On a macOS or Windows host, run `cargo check -p sirio --bin sirio` and
the chat tests, then capture light/dark and narrow/wide with the probe, and
record the host, toolchain and commands here.

One GPUI graph: `cargo metadata --locked` resolves 30 `gpui`/`zed` packages, none
with more than one version; `ely-gpui-component` depends on `bezel-gpui =0.3.8`
and `bezel-gpui-platform =0.3.8`, the same packages Sirio pins.

### The gate (Linux)

```bash
cargo build -p sirio --bin sirio -p sirio_control --bin sirioctl
cargo build -p sirio_persistence --example ely_chat_seed
cargo test -p sirio_ui --lib chat::                      # 205 passed
cargo test -p sirio_acp --test chat_integration          # 7 passed
cargo test -p sirio_control --test control_integration   # 44 passed
cargo test -p sirio --bin sirio restored_chat            # 2 passed
cargo test -p ely-gpui-component --features test-support # 79 passed
cargo check --workspace --all-targets                    # passes
```

`cargo fmt --check -p sirio_ui -p sirio -p sirio_persistence -p ely-gpui-component`
exits 1 and `cargo clippy -p sirio_ui -p sirio --all-targets -- -D warnings`
exits 101. Neither is the migration:

- **fmt:** 38 files differ. The branch touches two: `question_dock.rs` (6 diffs, the
  same 6 as at the baseline, left alone) and `sirio/src/main.rs` (120, the same
  as at the baseline; the two this branch added were fixed). The other 36 are
  untouched files.
- **clippy:** `-D warnings` stops at five existing sites the branch never
  touched: `sirio_theme/src/lib.rs:324`, `sirio_claude/src/message.rs:17`,
  `sirio_lsp/src/connection.rs:37`, `sirio_lsp/src/server.rs:96`,
  `sirio_forge/src/transport.rs:370`, before it reaches `sirio_ui`. With
  `--no-deps` the two packages' own lints were compared with the baseline file by
  file: the branch's three collapsible `if`s were fixed, and nothing else is new
  outside the vendored crate. The vendored crate's own warnings (unused
  `pub(crate)` re-exports of the parts of Ely this chat does not use, and similar)
  are Ely's, left as they are rather than suppressed.

`Scripts/ci.sh` and `Scripts/ci-linux.sh` were not run.
