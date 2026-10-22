# Crate boundaries (`rust/crates/`)

```
sirio_perf       (below everything — no deps at all, not even gpui, so any
                    layer may instrument without inverting the graph. Imported
                    by sirio_git, sirio_persistence, sirio_terminal,
                    sirio_activity, sirio_forge, sirio_lsp, sirio_term_session,
                    sirio_ui, sirio, and the vendored gpui_windows and
                    ely-gpui-component)
sirio_env        (below everything, beside sirio_perf — no dependencies. It is
                    the user environment every child process starts in: the
                    launch snapshot minus the launcher's own variables (APPIMAGE,
                    APPDIR, ARGV0, OWD, SIRIO_LAUNCH_ENV), the stable `sirioctl`
                    directory first on `PATH`, the per-pane names stripped
                    (`SIRIO_WORKTREE_ID`, `SIRIO_PANE_ID`, `TILLER_PANE_ID`), and
                    `for_child()` for a site that cannot spawn through `command`.
                    Its dependents are sirio_acp, sirio_control, sirio_diagram,
                    sirio_forge, sirio_git, sirio_lsp, sirio_project,
                    sirio_registry, sirio_terminal, sirio_ui, sirio_usage and sirio.
                    With the host's `term.env_exact` (`sirio_host_protocol::term`)
                    a session's environment is exactly the client's map; a host
                    without it gets that map plus an empty value for each app
                    variable the user environment lacks, except `SHELL`
                    (`sirio_env::host_create_env`).
                    The panel resolver (`worktree_environment_resolver` in
                    `sirio/src/main.rs`) sets `SIRIO_WORKTREE_ID` from the deepest
                    worktree holding the pane's starting directory)
    ^
sirio_theme, sirio_project, sirio_git, sirio_persistence,
sirio_activity, sirio_markdown, sirio_registry, sirio_release,
sirio_lsp, sirio_syntax, sirio_claude, sirio_diagram,
sirio_forge, sirio_privacy, sirio_ipc, sirio_host_protocol,
sirio_term_session
                                (leaves — no local deps beyond sirio_perf and sirio_env;
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
                                 negotiation, the liveness verdict — no I/O;
                                 sirio_term_session is the terminal session
                                 engine — PTYs, one resident `libghostty-vt`
                                 terminal per session, the journal a resume
                                 reads — with no GPUI; it takes `sirio_perf`,
                                 `libghostty-vt` and `portable-pty`)
    ^
sirio_agents     (-> sirio_claude)
sirio_usage      (-> sirio_claude)
sirio_acp        (-> sirio_persistence, sirio_claude)
sirio_terminal    (-> sirio_project, sirio_theme, sirio_ui, sirio_host_client, sirio_host_protocol, sirio_term_session, and `bezel` directly for its scrollbar)
sirio_control    (-> sirio_acp, sirio_persistence, sirio_ipc, sirio_host_client, sirio_host_protocol)
sirio_host       (-> sirio_ipc, sirio_host_protocol, sirio_term_session; the `sirio-host` binary)
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
                    except sirio_host, which it never links (the boundary host.md,
                    beside this file, draws), including sirio_terminal, sirio_control,
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
