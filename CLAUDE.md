# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Sirio — a native app for macOS, Linux and Windows (Rust, [gpui](https://github.com/zed-industries/zed)) for running multiple AI coding agents (Claude Code, Codex, OpenCode, Pi, Oh-My-Pi) side by side, one sidebar per project, one terminal per git worktree. **macOS is the reference release platform** — it gates every release, with Linux and Windows released behind it; see `docs/superpowers/specs/2026-08-29-macos-release-platform-design.md`. The macOS release job runs on a GitHub-hosted `macos-15`, pinned rather than `macos-latest` because the Zig 0.15.2 tarball cannot link on macOS 26 (see the Zig step in `build-release.yml`), and publishes a signed, notarized DMG beside the Linux AppImage and the Windows installer, with the signed manifest served from `dl.sirioai.app/<channel>.json`. A *stable* `v*.*.*` tag needs a hand-written `docs/release-notes/<version>.md` and a tag matching `rust/Cargo.toml` (`Scripts/check-release-version.sh` refuses otherwise). The macOS code paths are deliberate and specific (the libproc process walk in `sirio_activity`, `getpeereid` peer identity in `sirio_control`, a native `NSStatusItem` tray in `sirio`, the TCC permission probe in `sirio_privacy`). Originally a macOS/Swift app (itself a fork of Orca with reduced scope); the Swift app was retired once this Rust/gpui port covered its inventory. Its final commit is `5430d7bfdb4a295be8ce072526ae5108259b80f8`; read any of its source with `git show 5430d7bfdb4a295be8ce072526ae5108259b80f8:<path>`, or check it out with `git worktree add <dir> 5430d7bfdb4a295be8ce072526ae5108259b80f8`. Terminal rendering is built on `libghostty-vt`, with `portable-pty` supplying the PTY.

### External references

- [bezel](https://github.com/crabtalk/bezel) — the UI library for the application shell and the non-chat surfaces not yet moved to Ely (the change-request detail tab is already on Ely: `change_request_tab/ely_ui.rs`). Build those surfaces' primitives from bezel (`bezel::ui`, `bezel::motion`, `bezel::theme`, `bezel::agent`); its `gallery` crate is the visual and usage reference.
- [Ely GPUI Components](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components) — the AI chat's presentation library, adapted under `rust/vendor/ely-gpui-component` to the same Bezel GPUI packages. `rust/vendor/ely-palette` holds its palette, the vocabulary of Sirio's colours. When changing chat presentation, read its `LOCAL-CHANGES.md` and `docs/superpowers/specs/2026-09-30-ely-agent-chat-design.md`; retain Sirio's theme and existing Chat controllers.
- [gpui-component](https://github.com/longbridge/gpui-component) — secondary structural reference when the surface's primary library has no equivalent.
- [waku](https://github.com/egoist/waku) — a comparable app (multiple coding agents, one pane each). Reference for prior art on the same problem, not a dependency.

## Commands

```bash
# Single verification gate for the whole repo.
# ONLY on the user's explicit request — an agent must NEVER launch this (or
# Scripts/ci-linux.sh) on its own. For iteration use `cargo build/test -p <crate>`.
Scripts/ci.sh    # -> prints "CI OK" if everything passes

# Fuller, stricter gate: the above plus fmt/clippy/cross-target checks and a headless
# smoke test that drives a real instance of the app. See its own header for what it
# additionally covers and why some of its stages are allowed to SKIP or report BLOCKED.
Scripts/ci-linux.sh

# Live end-to-end test of the update chain: signs a manifest, serves it over
# loopback and drives a probe compiled the way the release job compiles the app.
# Publishes nothing and touches no installed copy. SIRIO_UPDATE_E2E_VERBOSE=1
# prints each stage the probe reported.
Scripts/Tests/test-update-e2e.sh   # -> prints "UPDATE E2E OK"

# The host foundation: a real sirio-host started detached, adopted, killed
# around, idled out and drained across a protocol major; --out-dir DIR keeps
# every host's log and state; SIRIO_HOST_E2E_VERBOSE=1 prints each probe line.
Scripts/Tests/test-host-e2e.sh      # -> prints "HOST E2E OK (<n> cases, <s>s)"
# Whether a detached process survives its parent's world, row by row.
Scripts/Tests/probe-host-detach.sh  # -> prints "DETACH PROBE OK"

# Live end-to-end test of the forge layer (sirio_forge): the token transport
# and the real gh/glab against loopback fake forges. --out-dir DIR keeps the
# transcript and the fake forges' request logs; SIRIO_FORGE_E2E_VERBOSE=1
# prints every probe answer.
Scripts/Tests/test-forge-e2e.sh    # -> prints "FORGE E2E OK"

# The change request view and tab in a real, isolated Sirio against the fake
# forge, over the control socket; --state-only skips the window captures.
Scripts/Tests/test-forge-ui-e2e.sh  # -> prints "FORGE UI E2E OK"

# A change request's diff, the editor hand-off (local file or read-only
# snapshot, across a quit and relaunch) and the fetch with its failures,
# against a bare repository and the fake forge; includes unified/split review
# threads, resolved folds and outdated sections; same flags as above.
Scripts/Tests/test-forge-diff-e2e.sh   # -> prints "FORGE DIFF E2E OK"

# The Changes surface on Ely against a real repository: unified and split,
# git failing and recovering, the in-window Discard confirmation (confirmed
# through the debug-only `surface changes confirm`); same flags as above,
# plus --appearance light|dark.
Scripts/Tests/test-changes-e2e.sh   # -> prints "CHANGES E2E OK"

# Acting on a change request: every write of sirio_forge on the wire (the token
# transport and the real gh/glab), then the tab's actions in a real, isolated
# Sirio driven by the debug-only `surface change-request act` verb. --stage NAME
# runs one stage (wire-github, wire-gitlab, failures, merge, metadata, cli,
# scopes, ci, ui); ci covers re-runs and job logs. --state-only skips captures;
# --appearance light|dark seeds the isolated app's appearance.
Scripts/Tests/test-forge-actions-e2e.sh   # -> prints "FORGE ACTIONS E2E OK"

# Checking a change request out into a worktree (change requests C1): same
# repository, forks that accept and refuse pushes, a gone branch and a closed
# request, reuse with fast-forward, refusals and the pinned card, against bare
# repositories and the fake forge; same flags as the other forge E2Es.
Scripts/Tests/test-change-request-handoff-e2e.sh   # -> prints "HANDOFF E2E OK"

# Ely's Tabs and git badges with Sirio's theme, on a private Xvfb with
# Mesa's lavapipe: PID-matched captures, a click per tab, the badge tooltip,
# dark and light (docs/testing/ely-forge-probe.md).
Scripts/Tests/test-ely-forge-probe.sh --xvfb   # -> prints "ELY FORGE PROBE OK"

# Iterate on one crate only
cd rust && cargo test -p <crate>

# Run a single test
cd rust && cargo test -p <crate> <test_name>

# Build the sirioctl CLI standalone (control-socket client used by agent hooks)
cd rust && cargo build -p sirio_control --bin sirioctl

# Dev build+launch loop: cargo build -> kill any running instance -> relaunch.
Scripts/build-dev.sh

# Release (macOS). Each stage takes the previous stage's artifact, so any of
# them can be run by hand against a local build. CODESIGN_IDENTITY must name a
# Developer ID Application identity.
Scripts/check-release-version.sh v0.6.0          # tag vs rust/Cargo.toml
Scripts/set-workspace-version.sh <version>       # what the nightly job compiles in
Scripts/build-app-bundle.sh <binary> <version> build/Sirio.app <sirio-host-binary>
Scripts/build-dmg.sh build/Sirio.app Sirio build/Sirio-<version>.dmg
```

Both release channels go through `.github/workflows/build-release.yml`: `release.yml`
calls it for a `v*.*.*` tag (Stable — needs a hand-written `docs/release-notes/<version>.md`,
see its README) and `nightly.yml` from `main` on a schedule (Nightly — stamps
`<workspace>-nightly.<YYYYMMDDHHMM>`). Both need the two repository settings in
`docs/release-signing.md`; the manifest lands on the `gh-pages` branch, which #312 wires
to `dl.sirioai.app`.

`rust/target/` is gitignored, generated by `cargo`. `Scripts/ci.sh` and `Scripts/ci-linux.sh` are two tiers of the same gate, not duplicates that can drift silently: `Scripts/ci.sh` runs only what is verified clean on this tree today (`cargo build --workspace --all-targets`, then `cargo nextest run --workspace` — see the comment at its own top for exactly which checks it deliberately leaves out and why), `Scripts/ci-linux.sh` is the comprehensive one. The tests run under [nextest](https://nexte.st) rather than `cargo test` for two reasons: cargo runs this workspace's 45 test binaries strictly one after another, so the two slow ones dominate the wall clock while the other 43 wait at idle cores; and a test that owns its own process cannot be tripped by a neighbour sharing one, which is where several of this gate's macOS failures came from. `rust/.config/nextest.toml` holds the profiles, the PTY test group and the release gate's skip list, each written next to the reasoning for it.

Anything that builds `sirio_terminal` (both gates, `cargo build/test --workspace`, any `-p sirio_terminal` command) needs **Zig exactly 0.15.2** on PATH: `libghostty-vt-sys` shells out to `zig build` and upstream pins that version exactly — a *newer* Zig fails too, so the upgrade reflex makes it worse; install 0.15.2 alongside and put it first on PATH. Both CI scripts fail fast with a clear message when it is missing or mismatched (#63). Optional: point `GHOSTTY_SOURCE_DIR` at a local ghostty checkout to skip the build script's network clone. `libghostty-vt-sys` is served from `rust/vendor/` rather than crates.io, so that the Zig build targets a portable `baseline` CPU instead of the build machine's own — v0.9.6's Windows binary could not start on a Zen 3 desktop without it; `rust/vendor/README.md` has the whole story. Note for whoever next reaches for a version bump there: **every upstream release carrying that fix also requires Zig 0.16.0**, so taking it means moving this pin at the same time, not separately.

`Scripts/ci.sh` additionally needs **cargo-nextest** on PATH — `cargo install cargo-nextest --locked`, or `brew install cargo-nextest` — and fails fast with that install line when it is missing, the same shape as the Zig preflight.

On Windows, the MSVC toolchain is required; see `docs/prototypes/ghostty-pane-windows.md`.

## Architecture

### Crate boundaries (`rust/crates/`)

```
sirio_perf       (below everything — no deps at all, not even gpui, so any
                    layer may instrument without inverting the graph. Imported
                    by sirio_git, sirio_persistence, sirio_terminal,
                    sirio_activity, sirio_ui and sirio)
    ^
sirio_theme, sirio_project, sirio_git, sirio_persistence,
sirio_activity, sirio_markdown, sirio_registry, sirio_release,
sirio_lsp, sirio_syntax, sirio_claude, sirio_diagram,
sirio_forge, sirio_privacy, sirio_ipc, sirio_host_protocol
                                (leaves — no local deps beyond sirio_perf;
                                 sirio_theme takes `bezel-theme` (pinned
                                 `=0.1.4`) and the vendored `ely-palette`;
                                 sirio_ui takes the external `bezel` crate;
                                 sirio_syntax takes `bezel-syntax` plus the
                                 fifteen tree-sitter grammars bezel does not
                                 carry; sirio_claude is Claude Code's own
                                 stdio protocol as types and pure functions —
                                 no process, no channels — which is what lets
                                 sirio_agents, sirio_acp and sirio_usage all
                                 take it without inverting the graph;
                                 sirio_forge is GitHub and GitLab change
                                 requests over GraphQL, with the token
                                 handed in by its caller;
                                 sirio_privacy is the macOS TCC permissions
                                 as a pure model plus the one probe that asks
                                 macOS, on the objc2 0.6 family gpui links;
                                 sirio_ipc is the local transport — unix
                                 socket or Windows named pipe, peer identity,
                                 a process's start time — extracted from
                                 sirio_control so the host shares it;
                                 sirio_host_protocol is the host's wire as
                                 types and pure functions — frames, version
                                 negotiation, the liveness verdict — no I/O)
    ^
sirio_agents     (-> sirio_claude)
sirio_usage      (-> sirio_claude)
sirio_acp        (-> sirio_persistence, sirio_claude)
sirio_terminal    (-> sirio_project, sirio_theme, and `bezel` directly for its scrollbar)
sirio_control    (-> sirio_acp, sirio_persistence, sirio_ipc)
sirio_host       (-> sirio_ipc, sirio_host_protocol; the `sirio-host` binary)
sirio_host_client (-> sirio_ipc, sirio_host_protocol)
sirio_update     (-> sirio_control, sirio_registry, sirio_release)
sirio_apply      (-> sirio_update)
    ^
sirio_ui         (-> sirio_acp, sirio_agents, sirio_diagram, sirio_forge, sirio_git,
                      sirio_lsp, sirio_markdown, sirio_persistence, sirio_privacy,
                      sirio_project, sirio_registry, sirio_syntax, sirio_theme,
                      sirio_usage)
    ^
sirio            (the app: main.rs — the only crate that depends on everything above
                    except sirio_host, which it never links (the boundary of the host
                    section below), including sirio_terminal, sirio_control,
                    sirio_host_client and sirio_activity, which sirio_ui itself does
                    not touch)
```

`sirio_theme` holds Sirio's palette in Ely's vocabulary: `ThemeColors` is
`{ ely: ely_palette::Palette, sirio: SirioColors }`, with values frozen in
`presets.rs` from what bezel's `Theme::branded` and Sirio's ladders produced
(`docs/THEME-PROVENANCE.md`). `sirio_ui::ely` feeds Ely's theme and
bezel-theme's registry from the `Theme` global; `Theme::to_bezel_theme` still
derives bezel's own tokens for bezel widgets and bezel-markdown. A bezel bump
therefore changes bezel widgets and Markdown, not Sirio's tokens. `ThemeMode`
is bezel-theme's `AppearanceMode` re-exported; the only other appearance enum
is `sirio_persistence::AppearanceMode`, which carries the serde contract, and
`sirio`'s `main.rs` holds the single conversion between them.

There is no single crate every other crate funnels through the way Swift's `TillerCore` worked — each concern (git, persistence, agent adapters, activity detection, terminal, control socket, UI primitives) lives in its own largely-independent leaf or near-leaf crate, and `sirio`'s `main.rs` is the integration point that wires `PaneRegistry` (`sirio_control`), `AgentActivityModel` (`sirio_activity`), and the ACP/agent/git/persistence layers into the `sirio_ui` components it renders. Run `cargo build -p <crate>` to check one crate compiles in isolation before assuming a change is layered correctly.

### Performance trace (`sirio_perf`, `sirio/src/native_perf.rs`)

`sirio_perf` is off unless `SIRIO_PERF_TRACE` names a file that does not yet
exist. Unset, `event` and `span` are a `OnceLock` read and a branch, which is
why they sit directly in hot `render` bodies. Set, one writer thread drains a
bounded channel (32768) to a TSV; a full channel drops events and reports how
many in a once-a-second `health` row named `trace.dropped` (the count rides in
the `entity` column), so **a trace is allowed to lose events and says when it
did** — read those rows before trusting a count. Instrumentation is
deliberately shallow: a `span` at the top of a `Render::render`, an `event`
where something decides to `notify`.

**Every `name` is a content-free `&'static str`** — that is the crate's one
hard rule, pinned by a test that asserts a fixture's text never reaches the
trace (`acp_redraw_trace_names_notifications_without_recording_content`, in
`sirio_ui`'s `chat`). Never interpolate a title, a path, a prompt or scrollback
into a label; the `entity` u64 is how a row is told apart from its siblings.

Two separate switches, easily confused: the env var above turns the TSV on in
any build, while `sirio`'s `perf-native` **cargo feature** additionally compiles
`native_perf.rs`, which exports GPUI's bounded foreground journal (frames,
input, task polls) as JSON and pulls in `gpui/profiler` plus a pinned
`bezel-zed-scheduler`. A default build has no journal at all. `Scripts/perf/`
holds the harness and report scripts; `docs/perf/` holds the captures, each
recording its own commands, hashes and bounds.

### Release, update, apply — three crates, one hand-off each

`sirio_release` owns the signed-artifact format: the channel manifest, the accepted
Ed25519 key set, and `AcceptedKeys::verify` (recompute SHA-256, then `verify_strict`).
`sirio_update` reads the manifest for the compiled-in channel (`sirio_control::
ReleaseChannel::RELEASE_CHANNEL`), downloads, and verifies — and **stops there**,
handing back a `VerifiedUpdate { version, notes, path, platform }`. `sirio_apply` is the
only crate that touches the running installation. The boundary is deliberate: a `Dev`
build never reaches the network at all (`updates_enabled()` is false), and refusing to
apply is a real outcome — `sirio_apply` rejects an install it cannot self-locate rather
than guessing a path. Every packaging file reads its identity from `Scripts/identity.sh`;
`SIRIO_INNO_APP_ID` must never change, because Inno decides upgrade-in-place by comparing it.

Three things in that chain are `option_env!`, baked in at build time and therefore
invisible to any unit test: the manifest URL (`SIRIO_UPDATE_MANIFEST_URL`, honoured only
in debug builds so a shipped Stable binary has no test endpoint), the channel
(`SIRIO_RELEASE_CHANNEL`) and the accepted key set (`SIRIO_RELEASE_ACCEPTED_KEYS`). A
build that never received them behaves exactly like an updater with nothing to report,
which is the failure this chain is least able to notice on its own.
`Scripts/Tests/test-update-e2e.sh` is the answer: it signs a manifest with the real
`sirio-release` CLI, serves it over loopback, compiles
`rust/crates/sirio_apply/examples/update_probe.rs` the way the release job compiles the
app, and drives the real `Updater` through check → download → verify → apply — including,
on Windows, the real silent spawn of the staged extensionless PE. Run it after any change
to the three crates or to the compiled-in environment; `Scripts/ci-linux.sh` runs it as a
stage.

### Change requests: one GraphQL layer, two transports (`sirio_forge`)

`sirio_forge` reads GitHub pull requests and GitLab merge requests —
"change requests" — through GraphQL on both forges, and parses each forge's
answers once. How the request travels is the user's *means*: `CliTransport`
hands it to `gh api` / `glab api --include`, which own authentication;
`TokenTransport` sends it with `ureq` and the platform certificate verifier,
so a self-managed forge behind a corporate CA verifies. The token is handed
in by the caller and never stored or logged by the crate. GitLab queries
that carry a merge request summary have a baseline variant, used from the
first `Field '…' doesn't exist` onward, because GraphQL rejects a whole
query that names a field an older server lacks. `SIRIO_FORGE_TEST_ENDPOINTS`
points hosts at loopback in debug builds only, and `SIRIO_FORGE_FETCH_TIMEOUT_MS`
shortens the fetch's 120 s bound the same way. `Scripts/Tests/test-forge-e2e.sh`
proves the seam; `tests/forge_live.rs` notices the day a query stops
matching the live schema (it SKIPs without credentials and is off both
gates). `docs/superpowers/specs/2026-09-27-change-requests-design.md` has the
design.

The UI reads through one seam, `sirio_ui::forge_source::ChangeRequestSource`,
a GPUI global the host sets once (`sirio/src/forge.rs`, `ForgeHub`): the
right panel's fifth view and the `ChangeRequest` tab in the Secondary half
never see a token or a setting. `ForgeHub` owns the `forge.hosts` setting —
a Settings-screen save re-applies the stored value — and keeps tokens in the
credential store under `forge:<host>`. The list asks nothing before it is
shown and refreshes only while visible. A rate limit pauses every read until
its reset (a minute when the forge names none), and the list looks again by
itself when it ends. `surface.change_requests.search` sets the list's real
search field, including its debounce. A restored tab loads when shown.

The detail tab's *Files* is the diff, read by `sirio_git::range_*` between
the commits `sirio_forge::Revisions` names. When either is not local,
`ForgeHub` makes them local with one non-interactive `git fetch` into
`refs/sirio/change-requests/<remote>/<N>/{head,base}` — invisible to
History's `--branches`, visible to an external `git log --all`. The fetch
runs with git's own credentials (a helper or an ssh key), never with the
token Sirio holds; when the target branch is gone from the forge the head
ref is fetched alone. Those refs only anchor objects against `git gc`, and
are swept — at startup, on a worktree switch and when a tab that held them
closes — of whatever no tab, open or parked with another worktree, holds.
*Open in editor* opens the local file when the worktree is at the head, and
a read-only snapshot tab otherwise; a persisted snapshot's sha and path are
refused before they reach a label or a path. Its Discard asks inside the
window (an Ely `Dialog`), and a row's right-click offers *Copy path* and, on
a change request, *Open on the forge*.
`docs/superpowers/specs/2026-09-28-change-request-diff-design.md` has the
design; `Scripts/Tests/test-forge-diff-e2e.sh` proves it.

**Acting on a change request** goes through one door, `ForgeClient::act(number,
&Action)` (`sirio_forge::action`). It reads the change request's id, state and
permissions afresh, refuses in `check_action` — a pure function — whatever the
forge would refuse, and only then sends the mutation: GraphQL on both forges,
and GitLab's approval as REST (`Transport::request`; GitLab has no approve
mutation). The answer is read as a *write's*: GitHub refuses with HTTP 200 and
a `null` payload, GitLab with the reason in the payload's `errors`, and the
interpreter that reads queries would call either a success. Sirio never
retries a write; after any write the tab re-reads the forge instead of patching
its own state, and a connection dropped after the send reads as "could not
confirm". In the UI, `ChangeRequestTab::perform` is the one function the
buttons, the composer and the edit fields call, and the control socket's
`surface change-request act` verb calls it too — **in debug builds only**: a
release build answers "unknown method" and does not list it, so nothing that
writes to a forge is reachable over the socket. Two live tests, the
`…_accepts_every_document_sirio_writes_with` pair, notice a mutation that stops
matching the forge's schema: they send every document with an id that names
nothing.
`docs/superpowers/specs/2026-09-29-change-request-actions-design.md` has the
design; `Scripts/Tests/test-forge-actions-e2e.sh` proves it.

**CI** (slice B2c, spec §15): a check that is a GitHub Actions or GitLab CI
job carries a `CheckJob` (job and run ids, whether the forge would retry it).
*Re-run* and *Re-run failed* are `Action::Rerun` through `act` — GitHub REST,
GitLab `jobRetry`/`pipelineRetry`; GitLab retries per pipeline, so its *Re-run
failed* sits on the pipeline row, not on a stage. `ForgeClient::job_log` reads
the job's status, then its log: GitHub answers with a redirect to a signed
URL on another host, which the token means fetches itself with **no
credential** (`RestRequest.log` turns redirects off for it) and `gh` follows
the same way; `gh api` prints terminal escapes only with
`--allow-escape-sequences`. Only the last 4 MiB are kept, cut at a line, and
the tab says so. `sirio_ui::ansi_log` turns the text into styled lines and
folding groups without a terminal; `CiLogTab` (`TabKind::CiLog`, `"ci_log"`)
draws it in a `uniform_list` and reloads a running job's log every 5 s only
while it is drawn. The `ci` stage of `test-forge-actions-e2e.sh` proves the
re-runs and log state; drawing and visible reload require its framed run.

**Review threads** (B3) are read by `ForgeClient::review_threads` — GitHub
`reviewThreads`, GitLab diff `discussions`, a GitLab thread outdated when its
position names an older head — and drawn and written in the *Files* diff:
`ChangesTab` takes forge-neutral `Annotation`s (`sirio_ui::diff_annotations`)
and one view per key, splits a context band so an anchored line is never
hidden, and puts a thread whose line is not drawn at the top of its file.
`ChangeRequestTab` owns the `ThreadView`s and replaces the Conversation's
line-comment footnotes with one entry per thread. Threads are read on first
load, a manual refresh, entering *Files* when idle and after a write, never
by the CI timer. The `threads` scenarios of `test-forge-diff-e2e.sh` prove it.
B3b writes through the same door: `Reply`, `Resolve` and `LineComment`
(GitHub REST review comments, which publish at once; GitLab REST
`discussions`, a range with `line_range` and GitLab's `line_code`), each
reply or resolve preceded by a fresh read of that thread's permissions.
`ChangesTab` draws a gutter "+" only when its owner calls `set_commentable`,
on the lines within 3 of a change (`diff_annotations::commentable`), and
emits `CommentOn`; the change request tab owns the one composer and every
thread card's fields.
`surface change-request thread --compose / --cancel / --suggest` serve every
build; `act reply|resolve|unresolve|line-comment|edit-thread-comment|review-add|
review-submit|review-discard|review-discard-confirm|draft-edit|draft-delete`
are debug-only. The `threads` stage of `test-forge-actions-e2e.sh` proves the
wire and `scenario_thread_writes` of `test-forge-diff-e2e.sh` the UI.

B3c drafts a review **on the forge**. On GitHub that is the pending review:
`addPullRequestReview` starts it with its first line comment in the same
mutation. On GitLab it is the draft notes, over REST because GraphQL has none,
and `bulk_publish` followed by B2's verdict submits them; a verdict that fails
after the publish is a warning. `ChangeHeader.draft` is read with the header,
and `act` reads it afresh before every review write, so *Start a review* joins a
draft started elsewhere. The strip, the submit dialog and the discard
confirmation live in `change_request_tab/review.rs`, and B2's *Approve* /
*Request changes* submit the draft while one exists. A ```` ```suggestion ````
block draws as a small diff whose "before" lines are the end of the thread's
`diff_hunk` (`change_request_tab/suggestion.rs`); applying stays on the forge.
Outdated threads have a `ThreadView` each, hosted by their file's
`OutdatedView`, so they take writes and keep a reply across a re-read. `act
review-add|review-submit|review-discard|review-discard-confirm|draft-edit|draft-delete`
are debug-only; `thread --suggest` serves every build. The `review` stage of
`test-forge-actions-e2e.sh` proves the wire, and `scenario_review` of
`test-forge-diff-e2e.sh` the UI. The debug-only `review-open-submit` helper
opens the submit dialog without sending, for the framed test.

**C1** checks a change request out into a worktree. `sirio_ui::handoff`
decides, purely, the local branch (`<source_branch>`, `<owner>/<source_branch>`
for a fork, or `pr-<N>`/`mr-<N>` when the head repository is gone), the push
target (the listed remote, the viewer's own fork remote when a project remote
already names the head repository and the viewer may push to it (and the local
branch of the source name is free or tracks that remote), a `sirio-<owner>-<number>` remote when the
viewer may push to a fork, or none — read-only) and whether to reuse, create or
refuse. Each fork change request has its own remote, because a remote's push
mapping is per remote: two change requests sharing one would make a plain
`git push` from either worktree push both branches. The remote is removed with
the last linked worktree, and only when its single push mapping names that
worktree's branch. `ForgeHub::checkout`
(`sirio/src/forge/checkout.rs`) fetches the head through B1's
`RevisionFetcher` first, so nothing is created before a refusal, then runs the
`sirio_git` steps off the GPUI thread. A fork's local `<owner>/<branch>` gets
`remote.sirio-<owner>-<number>.push = refs/heads/<owner>/<branch>:refs/heads/<branch>`,
because a plain `git push` under `push.default=simple` would not reach
`<branch>`. A reused worktree is fast-forwarded only when it is clean and on
the change request's branch; one switched elsewhere, dirty or diverged is left
as it is and the status line says why. A worktree whose folder was deleted
outside git is unregistered (one entry, never a prune) and created again. The
link worktree → change request lives in `sirio_persistence` (`change_request_link`,
v22, keyed by the exact path the sidebar uses, with the branch it was made for;
the card honours it only while the worktree is on that branch), and the card
reads it with `ForgeClient::summary` before the branch match. *Open in a worktree* is in the tab's action bar and the list
row's menu (which opens the tab); the outcome is the tab's checkout status
line. `surface change-request checkout` acts on the open tab and serves every
build because it touches only local git. `test-change-request-handoff-e2e.sh`
proves it.

**Merge, reviewers and labels** (B2b) go through the same door.
`Action::Merge` carries the head the user saw when the confirmation opened, and
`act` refuses `HeadMoved` before sending, ahead of the forge's own
`expectedHeadOid` / `sha` guard. The strip is drawn from `MergeCapability`,
whose `Unreported` verdict draws nothing. GitHub deletes the branch with a REST
`DELETE` after a successful merge, never for a fork's head; if only that call
fails, the outcome is a merge with a warning. GitLab deletes it with a flag of
the accept, offers no rebase (its method is per project), and cancels an
auto-merge over REST, since GraphQL has no mutation for it. A reviewer or label
picker sends one `Set*` when its popover closes; that relies on two vendored
Ely hooks, `Popover::on_close` and `Popover::open` (`LOCAL-CHANGES.md`). The
`merge`, `metadata` and `ui` stages of `test-forge-actions-e2e.sh` prove it.

### Languages: one list, two independent answers (`sirio_syntax`, `sirio_lsp`)

`sirio_ui::editor::Language` is the list of what Sirio recognises — twenty-three
languages plus plain text, resolved from the extension, never from content. Two
separate things hang off it, and they failed independently for most of the
project's life:

- **Colour** is compiled in. `sirio_syntax` holds one tree-sitter grammar per
  language `bezel-syntax` does not carry, registered through bezel's own
  `Lang::new` extension point so the spans are ordinary `HighlightKind`s and
  reach `Theme::syntax_palette` with no second code path. A grammar crate
  qualifies only if it reaches tree-sitter through `tree-sitter-language`
  rather than naming a `tree-sitter` version of its own — two tree-sitters in
  the graph are two unrelated `Language` types with one name.
- **Navigation** needs a program on `PATH`. `sirio_lsp::LanguageTable::defaults`
  names one server per language; naming it is not shipping it, and a command
  that is not installed resolves to `LspError::NotInstalled`. Nothing is
  *volunteered* about that — no card, ever, or there would be one on almost
  every file. The context menu is the exception, because it is asked: it
  names the missing program (`jdtls is not on PATH`) rather than describing
  Sirio. The name reaches it through `lsp::Dead::NotInstalled` and
  `FileContextFacts::missing_language_server`, and the failure path pushes the
  facts a second time because the tab computed its own before anyone had
  looked.

The failure mode both halves share is silence: an unlisted language is detected,
named in the status line, and then rendered as plain text with no action in its
context menu — indistinguishable from a file with nothing to say. The three
tests that make the lists agree are
`sirio_ui`'s `every_language_the_editor_recognises_has_a_grammar`,
`sirio`'s `every_language_the_editor_recognises_has_a_server`, and
`sirio_syntax`'s `every_query_compiles_against_its_grammar`.
`docs/superpowers/specs/2026-09-16-language-coverage-design.md` carries the table
of which server each language names and why a missing one says nothing.

### Markdown Preview: HTML, Mermaid, PlantUML (`sirio_markdown::expand_html`, `sirio_diagram`)

The file view's Preview has no HTML engine. `expand_html` maps a GitHub-style
subset of raw HTML onto the existing `Block`/`Inline` model and reduces the rest
to text; `sirio_ui::markdown_preview` then turns lone images into picture blocks
and diagram fences into SVG files in a per-user cache, which bezel's
`BlockKind::Image` draws. gpui rasterises SVG images at 2× on its own, so the
SVG is cached as rendered. Mermaid renders in-process
(`mermaid-rs-renderer`, pinned `=0.3.1` because its version is part of the cache
key). PlantUML follows `sirio_lsp`'s "a program on PATH" model — `plantuml` is
found, never shipped — with one difference: a fence it cannot render gets a
one-line note, because the author asked for a picture. The local run is
sandboxed (`PLANTUML_SECURITY_PROFILE=SANDBOX`: no network, no local
includes — only the embedded stdlib); PlantUML older than 1.2023.9 is refused
with a note, because its sandbox profile is unsafe, and the optional server
(`markdown.plantumlServer`) is off by default because it sends the source off
the machine.
`docs/superpowers/specs/2026-09-23-markdown-rich-preview-design.md` has the design.

### Text you can select and copy (`sirio_ui::text_selection`, `selectable_markdown`)

gpui paints a `&str`/`String`/`SharedString` child and does nothing else with it —
no hitbox, no mouse handlers — and bezel has no selectable text, so **text drawn as a
plain child cannot be selected**. Only the chat transcript, the file editor and the
terminal had their own selection; everything else (Settings, change requests, banners,
dialogs, the diff) was inert. Use `selectable_text(..)` where a string is shown to be
read: it lays out exactly like the string it replaces, takes a drag, a double-click (one
segment of a path or branch: `/` and `\` separate, `-` `.` `_` do not) and a triple-click
(the whole line), and Ctrl/Cmd+C copies it. One
selection exists at a time, in a `Global`; selecting moves focus into a sink (the
workspace root's `root_focus`) so a terminal or composer is never handed a copy.

- **Never inside something clickable** — a row, a button, a tab, a palette entry: a
  press there fires the click on release even after a drag, and the I-beam would replace
  the pointer. The titlebar's drag region is out too.
- **Selection is per run of text.** A drag does not continue into the next label; the
  chat transcript alone has a document-wide offset space. In a loop, give each item an
  identity with `.id(..)` (the default is the call site), or two identical strings
  select together.
- **Ctrl+C must never be swallowed.** gpui dispatches a bound action *before* any raw
  `on_key_down` and stops there unless the handler propagates, so `on_copy` declines
  (`cx.propagate()`) whenever it has nothing to copy or the sink is not focused, and the
  binding is scoped `!Terminal`. A terminal's Ctrl+C stays SIGINT.
- Rendered Markdown outside the chat goes through
  `Chat::render_markdown_document_with_link_override` → `SelectableMarkdown`, which
  keeps a link's click when nothing was selected. The chat's own cards use the
  transcript selection (`Chat::render_plain_text`) so Select All highlights what it copies.
- The host wires three things: `text_selection::init`, `set_sink(root_focus)`, and
  `.on_action(text_selection::on_copy)` on the root of **each** render branch (Settings
  and the main shell). `text_selected_in_settings_is_copied_by_the_real_shell` and
  `text_selected_in_the_main_shell_…` in `sirio` fail if any of them is missing.
- Tests: host the view under `text_selection::testing::host`, then `copy_line` /
  `copy_span` — a real drag and a real Ctrl+C against the real clipboard.

### Agent adapters (`sirio_agents`)

Every supported CLI implements the `AgentAdapter` trait (`id`, `display_name`, `has_native_hooks`, `prepare`, `command`, `resume_command`). `sirio_agents::ALL` is the fixed list of 5 adapters, in display order. An adapter's ACP claim (`builtin_acp`) covers only its own binary's subcommand — verified, defaulting to `None`; everything reachable through a separate package resolves through `sirio_registry`. `prepare` writes only worktree-local hook config — **never** touches user-global config (`~/.claude/settings.json`, `~/.codex/config.toml`, etc.). The one writer of user-global config is the explicit Settings → General → **Install Hooks** action (`install_global_hooks`, a trait method defaulting to `NotSupported`): Claude merges the five hook arrays into `$CLAUDE_CONFIG_DIR`/`~/.claude/settings.json`, Codex sets `notify` in `$CODEX_HOME`/`~/.codex/config.toml` via `toml_edit` (format-preserving), OpenCode writes `~/.config/opencode/plugins/sirio-session.js`; Pi and omp honestly report no user-global mechanism. Those hooks carry **no** `--session`: `sirioctl notify`/`session-ref` fall back to the `SIRIO_PANE_ID` every Sirio pane exports and exit 0 silently outside one (no pane, or no socket), so a global hook never surfaces an error inside an agent Sirio did not launch. Adapters that generate command-line overrides embedding JSON (Codex's `-c notify=[...]`, omp's hook file) share `json_string_literal` in `sirio_agents/src/shell_quote.rs` — it must build a JSON string literal without escaping slashes, because Codex's `-c key=value` override is parsed as **TOML**, and `\/` (JSON's optional slash-escaping) is not a valid TOML escape. Getting this wrong makes Codex fail silently at config load, before it ever reaches its TUI.

### A Claude chat talks to Claude Code, not to a wrapper

A chat tab for Claude Code drives the `claude` already on the user's PATH
over the stdio protocol the Claude Agent SDK uses internally
(`claude -p --output-format stream-json --input-format stream-json`), not
the `claude-acp` registry wrapper. `sirio_claude` is that protocol as types
and pure functions; `sirio_acp::claude` owns the process and emits the same
`AcpEvent`s the ACP client does, so `sirio_ui::chat` cannot tell which
transport a tab uses. The resolution lives in `sirio`'s `claude_transport`:
native when `claude` is on PATH at or above `sirio_claude::MIN_CLAUDE_VERSION`
(2.1.257, the version the official wrapper's SDK bundles and therefore
certifies), the wrapper otherwise, with the reason shown in Settings →
Agents. `SIRIO_CLAUDE_TRANSPORT=acp` forces the fallback for diagnosis.

**That protocol is not a published contract.** It is versioned with the
Agent SDK, and `claude` self-updates, so everything in `sirio_claude` is
written to degrade rather than reject: an unknown `type` or `subtype`
becomes an `Other`, unknown fields are ignored, a malformed line is skipped,
and a control request the CLI refuses degrades that one feature rather than
the session. `claude_answers_the_native_handshake_it_claims`
(`sirio_agents/tests/acp_conformance.rs`) is what notices the day the claim
stops holding; like its two siblings it SKIPs without the binary and is on
the release gate's skip list. The terminal pane is unaffected — it has
always run `claude` interactively.

### Agent activity detection — layered evidence, not one signal (`sirio_activity`)

Whether a pane shows as running/idle/needs-input is resolved from four independent evidence layers, weakest overridden by strongest as it arrives — ported byte-for-byte in intent from the original Swift `AgentActivityModel`, and `sirio_activity` carries no GPUI dependency, so this stays pure state-machine logic testable without a window:

- **Layer A — `sirioctl notify` hooks.** Authoritative when present. Agents with `has_native_hooks() == true` call `sirioctl notify --session <paneId> --status <status>` themselves (user-global hooks omit `--session` and let sirioctl read `SIRIO_PANE_ID`); a recent Layer-A push suppresses Layer B for a debounce window (`should_apply_title_signal`, `sirio_activity::title`).
- **Layer B — OSC terminal title.** `identify_agent_from_title` assigns an unregistered pane's agent identity from its title text; `detect_status_from_title` reads status from the same title using each CLI's own convention. Every CLI has its own title format, captured empirically, not guessed: Claude idles as `✳ …`, works as `. …` or a braille spinner; Pi titles `π - <cwd>`; its omp fork titles `π: <cwd>` (the colon is the only distinguishing mark); Codex 0.144+ also writes a braille "dots" spinner into its title while working — a bare spinner alone is therefore ambiguous between Claude and Codex and must **not** be used to assign identity (`contains_braille_spinner`; both are caught by Layer D instead).
- **Layer C — content signal.** `detect_content_status` matches live pane scrollback content on output-settle; not debounced against Layer A, since a genuine content match is closer to ground truth than a stale title.
- **Layer D — foreground process.** `inspect_foreground_agent` (`sirio_activity::process`) walks the pane shell's direct child processes via `/proc` and matches comm names against the catalog. This is the only signal that catches agents with no usable title convention. Node/Bun-hosted CLIs (pi, omp) are invisible here and rely on Layer B instead.

Pane ownership determines who is allowed to clear a pane's status, and matters when changing this code: **spawn-owned** (Sirio launched it — cleared by watching process exit), **title-owned** (cleared only when the title stops matching that agent's conventions), **process-owned** (Layer D — cleared only by `AgentActivityModel::process_gone`, never by an unrelated title change). Mixing these up reintroduces bugs where one layer's signal (or absence of one) wipes state that another layer is still relying on.

### Control socket (`sirio_control`)

`ControlServer` listens on a unix socket (`$SIRIO_SOCKET` if set, otherwise `$XDG_RUNTIME_DIR/Sirio/control.sock`, falling back to the XDG state directory when no runtime directory exists) and dispatches line-delimited JSON `ControlRequest`s to a `ControlHandler` the app (`sirio`) implements. This is both the `sirioctl` CLI's transport and how agent lifecycle hooks talk back to Sirio (Layer A above). `PaneRegistry` (in `sirio_control`, not the terminal crate) is the shared source of truth for live panes that both the control handler and the foreground-process walk (Layer D) read from. Its method groups — panel.*, workspace.* (worktrees), project.*, pane.* and tab.*, surface.*, browser.*, notification.*, system.* (ping/capabilities/identify), session.* and a few singletons — are all dispatched from `rust/crates/sirio/src/main.rs`, and `sirioctl capabilities` lists the full set a running instance serves; the flat kebab sirioctl commands (`list-workspaces`, `new-workspace`, `select-workspace`, …) mirror cmux's CLI names. The socket can be disabled in Settings (`controlSocket.enabled` / `SIRIO_SOCKET_ENABLE`), which also disables Layer-A hooks.

### Pane hierarchy

`SirioWorkspace` (`rust/crates/sirio/src/main.rs`, the `gpui::Render` root) holds the open tabs/panes and a `Vec` of workspace rows (project/worktree/branch/path, each with a `mounted` bool) — the Rust equivalent of Swift's `Project -> [Worktree]` plus `openWorktreeIds`. A worktree's `mounted` flag tracks whether its terminal hosts stay mounted (PTYs alive) across sidebar selection changes; this is a live contract other code depends on; see the `tab_has_live_foreground_process` comment in `main.rs` for a case where an activity-model shortcut nearly broke it. On app quit (`cx.on_app_quit`), `PaneRegistry::shutdown` tears every pane down; a plain window close only flushes session state (`cx.on_window_closed`) and does not call `shutdown` — closing the window is not wired to kill agent PTYs, matching the original app's behavior of quitting only ending everything.

### A host behind the app (`sirio_host`, `sirio_host_client`)

`sirio-host` is a detached process that is to own everything that runs, so that agents survive a crash, a Force Quit, a Quit or an update of the app. It is a programme of seven sub-projects and this is **SP1**, the foundation (`docs/superpowers/specs/2026-10-05-host-foundation-design.md`; the programme is its §2). SP1 hosts no sessions: it serves `host.ping`, `host.info`, `host.shutdown` and the `host.state` topic, and `protocol/host-v1/` holds the capability ledger, the generated schema and the conformance cases the E2E replays against a real host. The app shows the host's status in Settings → General and nothing else depends on it: without a host the app runs as it always did.

The boundary is one dependency edge: `sirio` → `sirio_host_client`, **never** `sirio_host`. At startup `ensure_host` (off the GPUI thread) observes the host's state, adopts a live host or stages the packaged `sirio-host` into the data root and starts it from there, detached from the app's process group — a systemd scope or `setsid` on Linux, a launchd job on macOS, a breakaway process on Windows (`Scripts/Tests/probe-host-detach.sh` is how each arm is proven: its macOS and Windows rows run in CI and Force Quit by hand, and `docs/testing/host-detach-probe.md` records which rows are done). There is at most one host per protocol major per data root, and a client speaks majors N and N−1, so an older host drains instead of being replaced.

The data root is `$SIRIO_HOST_HOME` if absolute, else `<XDG_DATA_HOME | ~/.local/share | %LOCALAPPDATA%>/Sirio/host` — **no temp-dir fallback**, a host on tmpfs loses its state. Per major N it holds `host-v<N>.sock` (a named pipe on Windows), `host-v<N>.lock` (held for the host's whole life), `host-v<N>.json` (pid, start time, version, generation), `log/host-v<N>.log` (4 MiB, two generations; names and ids, never session content, the `sirio_perf` rule), `bin/<version>/sirio-host`, and on macOS `launchd/`.

**Nothing kills a host on the strength of a failed connection.** A host that holds its lock and does not answer reads `Unverifiable` and is waited for, never replaced or signalled; a recorded pid counts only with its start time. Only `host.shutdown` ends a host, and it refuses with `sessions_live` unless `force`. The idle exit (60 s with no client and no session, only in `on-demand` mode) is the host's own decision.

The debug-only knobs — `SIRIO_HOST_PROTOCOL_MAJOR`, `SIRIO_HOST_IDLE_GRACE_MS`, `SIRIO_HOST_BIN` and the method `host.debug.hold_session` — are compiled out of release builds with `cfg!(debug_assertions)`, which is why `Scripts/Tests/test-host-e2e.sh` drives debug builds only. **Known SP1 limitation:** `SIRIO_HOST_HOME` is the one variable that isolates a host's data root. `SIRIO_DB` and `SIRIO_SOCKET` do not move it, and a debug build does not either, so a debug build or an isolated Sirio run without it adopts or starts a host in the user's real root.

Quit is unchanged until SP2: `PaneRegistry::shutdown` still ends every pane (see Pane hierarchy), because there are no host sessions yet. SP2 turns Quit into detach.

## Conventions

- **Testing is end to end.** A feature is proven against the real binaries, and every E2E run ends in an artifact anyone can re-run and check — the shape `Scripts/Tests/test-update-e2e.sh` (`UPDATE E2E OK`) and `Scripts/visual-sweep.sh` (a socket transcript plus PID-matched window captures under `--out-dir`) already have. The existing `#[test]` suite stays; these rules govern what gets added:
  - A `#[test]` is for a unit that must be proven in isolation, and it comes first: write every way the unit can fail, then the code. A unit test written after its code does not land.
  - A test earns its place by going red on a behaviour change. Tautological tests (asserting what the code does by construction, a mock echoed back) and change-detector tests (pinning internals, so a behaviour-preserving refactor goes red) are harmful.
  - A bug fix gets a regression test only when it exposes a genuine gap in behaviour coverage.
  - `Scripts/ci-linux.sh`'s comment covers the two workspace-wide tests that must run per-crate rather than concurrently with every other test binary.
- **Commit messages**: [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`), lower-case imperative subject.
- **The version names the next release, not the last commit.** `[workspace.package] version` in `rust/Cargo.toml` carries the next stable release's number and moves once per cycle, in the commit that opens it, right after a release is tagged (`v0.13.3` → `0.13.4`). Nothing else in the cycle touches it: only its first `feat:` moves it again, patch to minor (`0.14.0`), later `feat:`s leave it, and the release-notes commit writes only `docs/release-notes/<version>.md`. `Scripts/set-workspace-version.sh <version>` stays its only writer, `Scripts/check-release-version.sh` still compares the release tag against it, and `Scripts/check-cycle-version.sh` refuses a version that is not above the last released tag, the guard against a forgotten opening bump. A nightly is therefore always the prerelease of the stable actually coming.
- `Scripts/ci.sh` must print `CI OK` before a PR is opened — but the *local* run happens **only on the user's explicit request**. An agent never launches `Scripts/ci.sh` or `Scripts/ci-linux.sh` autonomously; when the gate is needed, ask the user and wait. Iterate with `cargo build/test -p <crate>` instead. `.github/workflows/pr.yml` runs `Scripts/ci.sh` on Linux for every pull request and every push to `main`. That is a backstop, not a substitute — it reports after the fact, and the macOS-only paths (the libproc walk in `sirio_activity`, `getpeereid` in `sirio_control`, the `NSStatusItem` tray in `sirio`, the TCC probe in `sirio_privacy`) are `cfg`-gated away on that runner. `.github/workflows/macos-check.yml` builds and links the app on `macos-15` for every pull request and runs `sirio_privacy`'s `permissions` example, which calls every TCC read for real, then the host's detach probe and E2E; the test suite on macOS stays the release gate's business. `.github/workflows/windows-check.yml` is the only pre-merge Windows compile: it builds the host crates on `windows-latest` and runs their tests, the probe and the E2E (the app itself still builds on Windows only in the release job). `pr.yml` also runs the host E2E, which is deliberately not in `Scripts/ci.sh`. `Scripts/Tests/test-pr-workflow.sh` guards the workflow's shape, the way its siblings guard `release.yml` and `nightly.yml`.

## Agent skills

### Issue tracker

Issues live in this repo's GitHub Issues (`ai-sirio/sirio`), driven through the `gh`
CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their default label strings. See
`docs/agents/triage-labels.md`.

### Domain docs

Single-context — one `CONTEXT.md` plus `docs/adr/` at the repo root, both created
lazily by `/domain-modeling` rather than up front. See `docs/agents/domain.md`.
