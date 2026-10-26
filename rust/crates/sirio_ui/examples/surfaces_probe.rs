//! Native probe for the surfaces the app's own E2E tests do not reach: the eight
//! Settings pages, a tab context menu and a tooltip, drawn with Sirio's theme as the
//! app draws them today. `Scripts/Tests/test-surfaces-probe.sh` captures one frame per
//! process and one process per frame and appearance, so the frames are the "before" and
//! "after" of every PR that moves these surfaces off bezel (docs/testing/bezel-off.md).
//!
//! Arguments: `--frame NAME` (required), `--appearance dark|light` (default `dark`) and
//! `--list`, which prints the frame names. The window is 1470x833, the size of the app's
//! main window.
//!
//! The layout is a contract the script reads: the tooltip box is 120x28 at (200, 200) of
//! the window, and the script moves the pointer to its centre.
//!
//! Every Settings frame is seeded through the public builders only (`with_account_states`,
//! `with_availability`, `with_launch_sources`, `set_install_state`, `with_transport_notes`,
//! `set_permission_status`, `set_lsp_store_root` and the rest): no machine state reaches a
//! frame, and no API exists for the probe. A state those builders cannot reach prints
//! `GAP <frame> <state>` on stdout instead of being faked.
//!
//! `settings-git-hosting` is the page with no forge source (its empty state);
//! `settings-git-hosting-accounts` installs `ProbeForgeSource` through
//! `forge_source::set_source`, a source of three fixed, made-up hosts that reads no credential,
//! runs no CLI and opens no connection, so the host rows are drawn: the forge, the means
//! buttons, the token field, Save token and Forget token, and a host that asks for its forge.
//!
//! The Agents warning strip is the registry-error state: `Settings::with_snapshot` keeps
//! the discovery sweep's error, and with no `PATH` the sweep has none to search. The script
//! therefore runs every frame with `PATH` unset, which also keeps the Language Servers
//! rows and the PlantUML line from reading the machine.
//!
//! What unsetting `PATH` does not stop: `Settings::with_snapshot` still runs
//! `ProviderAccountStates::discovered()`, which reads the user's credential files and, for a
//! signed-in Claude, runs `claude auth status`. With `PATH` unset glibc's `execvp` falls back
//! to `/bin:/usr/bin`, so a `claude` installed there would still run. No pixel depends on it:
//! `with_account_states` replaces the result before the first paint, and the frames show the
//! fixed `probe@example.com`.
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Context, Entity, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use sirio_agents::AgentAvailability;
use sirio_forge::{ChangeRef, Forge, ForgeClient, ForgeError, Means, Revisions};
use sirio_lsp::{LanguageTable, Recipe};
use sirio_privacy::{PermissionKind, PermissionStatus};
use sirio_registry::{
    Distribution, InstallStore, InstalledAgent, Integrity, LaunchSource, RegistryAgent,
    UnavailableReason,
};
use sirio_theme::{Theme, ThemeMode};
use sirio_ui::controls;
use sirio_ui::forge_source::{
    self, ChangeRequestSource, Connection, HostRow, RevisionError, TokenWrite,
};
use sirio_ui::settings::{
    CliStatus, InstallState, ProviderAccountStates, ProviderAccountStatus, Settings,
    SettingsCategory, SettingsSnapshot,
};
use sirio_ui::status_bar::{UpdateState, UpdateStatus};
use sirio_ui::tab_bar::{TabContextAction, TabContextItem, render_tab_context_menu};
use sirio_usage::LocalAccountState;

const WINDOW_WIDTH: f32 = 1470.0;
const WINDOW_HEIGHT: f32 = 833.0;

#[derive(Clone, Copy, Debug)]
enum Frame {
    Settings(SettingsCategory),
    /// Settings > Git Hosting with a forge source installed, so the host rows are drawn.
    GitHostingAccounts,
    TabMenu,
    Tooltip,
}

/// The frame name of each Settings page. Fixed text, not derived from the page's title: a
/// retitled page must not turn into an added and a removed frame in a before/after diff. The
/// match has no wildcard arm, so a new category does not compile until it is named here.
fn category_slug(category: SettingsCategory) -> &'static str {
    match category {
        SettingsCategory::AiProviders => "ai-providers",
        SettingsCategory::Agents => "agents",
        SettingsCategory::LanguageServers => "language-servers",
        SettingsCategory::GitHosting => "git-hosting",
        SettingsCategory::General => "general",
        SettingsCategory::Permissions => "permissions",
        SettingsCategory::Appearance => "appearance",
        SettingsCategory::Editor => "editor",
    }
}

impl Frame {
    fn all() -> Vec<Self> {
        SettingsCategory::all()
            .into_iter()
            .map(Self::Settings)
            .chain([Self::GitHostingAccounts, Self::TabMenu, Self::Tooltip])
            .collect()
    }

    fn name(self) -> String {
        match self {
            Self::Settings(category) => format!("settings-{}", category_slug(category)),
            Self::GitHostingAccounts => "settings-git-hosting-accounts".to_owned(),
            Self::TabMenu => "tab-menu".to_owned(),
            Self::Tooltip => "tooltip".to_owned(),
        }
    }
}

struct Options {
    frame: Frame,
    mode: ThemeMode,
}

fn usage() -> String {
    let names: Vec<String> = Frame::all().into_iter().map(Frame::name).collect();
    format!(
        "usage: surfaces_probe --frame NAME [--appearance dark|light] | --list\nframes: {}",
        names.join(", ")
    )
}

/// Reads the arguments; `None` means `--list` was answered and there is nothing to draw.
fn parse_options() -> Option<Options> {
    let mut frame = None;
    let mut mode = ThemeMode::Dark;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--list" => {
                for frame in Frame::all() {
                    println!("{}", frame.name());
                }
                return None;
            }
            "--frame" => {
                let name = args.next().unwrap_or_default();
                frame = Frame::all().into_iter().find(|candidate| candidate.name() == name);
                if frame.is_none() {
                    eprintln!("unknown frame '{name}'\n{}", usage());
                    std::process::exit(2);
                }
            }
            "--appearance" => match args.next().as_deref() {
                Some("dark") => mode = ThemeMode::Dark,
                Some("light") => mode = ThemeMode::Light,
                other => {
                    eprintln!("--appearance takes dark or light, not {other:?}\n{}", usage());
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("unknown argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let Some(frame) = frame else {
        eprintln!("--frame is required\n{}", usage());
        std::process::exit(2);
    };
    Some(Options { frame, mode })
}

/// The four provider cards: signed in with an identity, signed out, signed in without one
/// (a cookie provider) and a provider with no local store.
fn account_states() -> ProviderAccountStates {
    ProviderAccountStates {
        claude: ProviderAccountStatus {
            label: LocalAccountState::SignedIn.label(),
            signed_in: true,
            identity: Some("probe@example.com \u{b7} Example Org".to_owned()),
        },
        codex: ProviderAccountStatus::from_account_state(LocalAccountState::SignedOut),
        opencode_go: ProviderAccountStatus::from_account_state(LocalAccountState::SignedIn),
        ollama_cloud: ProviderAccountStatus::from_account_state(LocalAccountState::NoLocalStore),
    }
}

/// One row per shipped adapter, in catalog order: Claude Code and Codex found on the
/// (fixed, fictional) PATH, the other three missing.
fn agent_availability() -> Vec<AgentAvailability> {
    sirio_agents::ALL
        .iter()
        .enumerate()
        .map(|(index, adapter)| AgentAvailability {
            id: adapter.id(),
            display_name: adapter.display_name(),
            executable: (index < 2)
                .then(|| PathBuf::from(format!("/usr/local/bin/{}", adapter.executable_name()))),
        })
        .collect()
}

fn registry_agent(id: &str, name: &str, version: &str) -> RegistryAgent {
    RegistryAgent {
        id: id.into(),
        name: name.into(),
        version: version.into(),
        description: None,
        repository: None,
        website: None,
        license: None,
        icon: None,
        distributions: vec![Distribution::Binary(Default::default())],
    }
}

fn installed(id: &str, version: &str, integrity: Integrity) -> LaunchSource {
    LaunchSource::Installed(InstalledAgent {
        id: id.into(),
        version: version.into(),
        executable: PathBuf::from(format!("/opt/sirio-probe/agents/{id}/{id}")),
        args: Vec::new(),
        integrity,
    })
}

/// The Agents rows' resolved sources, and the registry's current versions:
/// - Claude Code: installed, checksum verified (green dot, Installed badge, meta line);
/// - Codex: installed, no published checksum, and a newer version published (Update);
/// - OpenCode: installable (Install);
/// - Pi: installable, with a failed install (the error strip);
/// - Oh-My-Pi: not in the registry.
fn agent_sources() -> (
    Vec<(String, LaunchSource)>,
    std::collections::BTreeMap<String, String>,
) {
    let sources = vec![
        ("claude".to_owned(), installed("claude-acp", "0.4.2", Integrity::Sha256)),
        ("codex".to_owned(), installed("codex-acp", "0.3.1", Integrity::None)),
        (
            "opencode".to_owned(),
            LaunchSource::Installable {
                agent: registry_agent("opencode", "OpenCode", "1.18.21"),
            },
        ),
        (
            "pi".to_owned(),
            LaunchSource::Installable {
                agent: registry_agent("pi-acp", "Pi", "0.9.0"),
            },
        ),
        (
            "omp".to_owned(),
            LaunchSource::Unavailable(UnavailableReason::NotInRegistry),
        ),
    ];
    let versions = [("claude", "0.4.2"), ("codex", "0.3.4")]
        .into_iter()
        .map(|(id, version)| (id.to_owned(), version.to_owned()))
        .collect();
    (sources, versions)
}

/// A language-server store holding one install and the scratch directory it lives in. The
/// window's `Surface` holds it for as long as the process runs, so a page that re-reads the
/// store later (an asynchronous refresh) still finds it.
///
/// The directory is `$SURFACES_PROBE_SCRATCH` when the script names one (it removes the
/// directory itself at exit, which matters because the script ends a probe with SIGTERM and
/// no destructor runs then) and otherwise a directory of the temp dir named for the pid,
/// removed when the store is dropped, which a normal quit does.
struct LanguageStore {
    scratch: PathBuf,
}

impl LanguageStore {
    fn root(&self) -> PathBuf {
        self.scratch.join("store")
    }

    /// TypeScript's server, recorded as installed by Sirio.
    fn new() -> Self {
        let scratch = std::env::var_os("SURFACES_PROBE_SCRATCH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!("sirio-surfaces-probe-{}", std::process::id()))
            });
        let _ = std::fs::remove_dir_all(&scratch);
        let store = Self { scratch };
        std::fs::create_dir_all(store.root()).expect("create the probe's language store");
        let executable = store.scratch.join("typescript-language-server");
        std::fs::write(&executable, b"").expect("create the probe's server executable");
        let id = LanguageTable::defaults()
            .entries()
            .iter()
            .find(|entry| entry.name == "typescript")
            .and_then(|entry| entry.install.as_ref())
            .and_then(Recipe::store_id)
            .expect("the shipped table installs TypeScript's server from a package");
        InstallStore::new(store.root())
            .write(&InstalledAgent {
                id: id.to_owned(),
                version: "5.0.1".to_owned(),
                executable,
                args: Vec::new(),
                integrity: Integrity::Sha256,
            })
            .expect("record the probe's installed server");
        store
    }
}

impl Drop for LanguageStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

/// A forge source that answers from memory: three fixed, made-up hosts and nothing else. Every
/// method is fixed data; none reads a credential, runs `gh` or opens a socket. Settings >
/// Git Hosting asks it only for `hosts()`.
struct ProbeForgeSource;

impl ProbeForgeSource {
    fn host(host: &str, forge: Option<Forge>) -> HostRow {
        HostRow {
            host: host.to_owned(),
            forge,
            means: None,
            pinned_means: None,
            account: None,
            write: None,
            has_token: false,
            configured: false,
        }
    }
}

impl ChangeRequestSource for ProbeForgeSource {
    fn connect(&self, _worktree: &Path) -> Connection {
        Connection::NoForgeRemote
    }

    fn has_forge(&self, _worktree: &Path) -> bool {
        false
    }

    fn current_branch(&self, _worktree: &Path) -> Option<String> {
        None
    }

    fn client_for(&self, _reference: &ChangeRef) -> Result<Arc<ForgeClient>, Connection> {
        Err(Connection::NoForgeRemote)
    }

    fn forget(&self, _host: &str) {}

    fn set_forge(&self, _host: &str, _forge: Forge) {}

    fn set_means(&self, _host: &str, _forge: Forge, _means: Option<Means>) {}

    fn save_token(&self, host: &str, _forge: Forge, _token: &str) -> Result<String, ForgeError> {
        Err(ForgeError::NotAuthenticated {
            host: host.to_owned(),
        })
    }

    fn delete_token(&self, _host: &str) {}

    /// - github.com: a public host, signed in through `gh` and detected (Detect is the chosen
    ///   means);
    /// - git.example.corp: a self-hosted GitLab (so the forge buttons show), on a read-only token
    ///   the user pinned (Token is chosen, the token field, Save token and Forget token show);
    /// - code.unknown.test: a host nothing identifies, which asks the user for its forge.
    fn hosts(&self) -> Vec<HostRow> {
        vec![
            HostRow {
                means: Some(Means::Cli),
                account: Some("probe-user".to_owned()),
                ..Self::host("github.com", Some(Forge::GitHub))
            },
            HostRow {
                means: Some(Means::Token),
                pinned_means: Some(Means::Token),
                account: Some("probe-bot".to_owned()),
                write: Some(TokenWrite::No),
                has_token: true,
                configured: true,
                ..Self::host("git.example.corp", Some(Forge::GitLab))
            },
            HostRow {
                configured: true,
                ..Self::host("code.unknown.test", None)
            },
        ]
    }

    fn ensure_revisions(
        &self,
        _worktree: &Path,
        _reference: &ChangeRef,
        _revisions: &Revisions,
        _target_branch: Option<&str>,
    ) -> Result<(), RevisionError> {
        Ok(())
    }

    fn release_revisions(&self, _worktree: &Path, _live: &[ChangeRef]) {}
}

/// The frame's `Settings`, seeded through the public builders and showing `category`, and the
/// language-server store behind it (Language Servers only), which the caller keeps alive.
fn settings_entity(
    category: SettingsCategory,
    mode: ThemeMode,
    cx: &mut App,
) -> (Entity<Settings>, Option<LanguageStore>) {
    let language_store = (category == SettingsCategory::LanguageServers).then(LanguageStore::new);
    let settings = cx.new(|cx| {
        let snapshot = SettingsSnapshot {
            theme: mode,
            socket_path: "/run/user/1000/sirio/control.sock".to_owned(),
            auto_naming: true,
            limit_mounted_worktrees: true,
            ..SettingsSnapshot::default()
        };
        let mut settings = Settings::with_snapshot(cx, snapshot)
            .with_version("0.0.0-probe")
            .with_channel("stable")
            .with_update_state(UpdateState {
                enabled: true,
                channel: "stable".to_owned(),
                status: UpdateStatus::Available {
                    version: "0.0.1".to_owned(),
                    notes: "Fixes a thing and adds another.".to_owned(),
                },
                last_checked: Some("14 minutes ago".to_owned()),
            })
            .on_apply_update(|| {})
            .with_account_states(account_states())
            .with_availability(agent_availability())
            .with_transport_notes(vec![(
                "claude".to_owned(),
                "Native \u{b7} claude 2.1.306".to_owned(),
            )])
            .with_browser_origins([
                "https://example.com".to_owned(),
                "https://docs.example.org".to_owned(),
            ]);
        let (sources, versions) = agent_sources();
        settings.apply_launch_sources(sources, versions);
        settings.set_install_state(
            "pi",
            Some(InstallState::Failed(
                "Could not unpack pi-acp: unsupported archive format".to_owned(),
            )),
        );
        // The Language Servers page shares the install-state map, keyed by language.
        settings.set_install_state(
            "c",
            Some(InstallState::Failed("clangd: the download timed out".to_owned())),
        );
        settings.set_host_status(Some("Live \u{b7} 3 terminal sessions".to_owned()), cx);
        settings.set_cli_status(
            Some(CliStatus {
                stable_path: "/home/probe/.local/share/sirio/bin/sirioctl".to_owned(),
                version_line: "Same build as this app".to_owned(),
                path_line: "On your PATH".to_owned(),
            }),
            cx,
        );
        for (kind, status) in PermissionKind::ALL.into_iter().zip([
            PermissionStatus::Granted,
            PermissionStatus::Denied,
            PermissionStatus::NotRequested,
            PermissionStatus::CheckManually,
            PermissionStatus::Granted,
            PermissionStatus::CheckManually,
        ]) {
            settings.set_permission_status(kind, status);
        }
        settings.set_lsp_silenced_languages(["python".to_owned()]);
        if let Some(store) = &language_store {
            settings.set_lsp_store_root(store.root());
        }
        settings
    });
    settings.update(cx, |settings, cx| settings.select_category(category, cx));
    (settings, language_store)
}

/// What the public API cannot reach, one line each.
fn print_gaps(frame: Frame) {
    let gap = |state: &str| println!("GAP {} {state}", frame.name());
    match frame {
        Frame::Settings(SettingsCategory::Agents) => {
            gap("search-no-match (the search field's text is only typed in)");
            if std::env::var_os("PATH").is_some() {
                gap("registry-warning-strip (needs PATH unset when the page is built)");
            }
        }
        Frame::Settings(SettingsCategory::General) => {
            gap("summarizer-menu-open (the popover opens only from a click)");
        }
        Frame::Settings(SettingsCategory::Permissions) if !cfg!(target_os = "macos") => {
            gap("macos-permission-rows (drawn on macOS only)");
        }
        _ => {}
    }
}

struct Surface {
    frame: Frame,
    settings: Option<Entity<Settings>>,
    /// Kept for its `Drop` and for any later read of the store; never drawn.
    _language_store: Option<LanguageStore>,
}

impl Render for Surface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::get(cx);
        let root = div()
            .size_full()
            .relative()
            .bg(theme.ely.bg)
            .font_family(theme.typography.ui_family);
        match self.frame {
            Frame::Settings(_) | Frame::GitHostingAccounts => {
                root.child(div().size_full().children(self.settings.clone()))
            }
            Frame::TabMenu => {
                // The rows of a terminal tab's menu in the middle of a group: every kind of
                // row the real menu has.
                let items = vec![
                    TabContextItem::enabled("Open File", "open-file", TabContextAction::OpenFile),
                    TabContextItem::separator(),
                    TabContextItem::enabled("Rename", "rename", TabContextAction::Rename),
                    TabContextItem::separator(),
                    TabContextItem::enabled("Close", "close", TabContextAction::Close),
                    TabContextItem::enabled(
                        "Close Others",
                        "close-others",
                        TabContextAction::CloseOthers,
                    ),
                    TabContextItem::disabled(
                        "Close Tabs to the Right",
                        "close-right",
                        TabContextAction::CloseTabsToRight,
                    ),
                    TabContextItem::separator(),
                    TabContextItem::disabled(
                        "Move Earlier",
                        "move-earlier",
                        TabContextAction::MoveEarlier,
                    ),
                    TabContextItem::enabled(
                        "Move Later",
                        "move-later",
                        TabContextAction::MoveLater,
                    ),
                ];
                root.child(
                    div().absolute().left(px(200.0)).top(px(160.0)).child(
                        render_tab_context_menu(items, Rc::new(|_, _, _| {}), theme, cx),
                    ),
                )
            }
            Frame::Tooltip => root.child(
                div()
                    .id("probe-tooltip-box")
                    .absolute()
                    .left(px(200.0))
                    .top(px(200.0))
                    .w(px(120.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme.ely.border)
                    .bg(theme.ely.surface)
                    .text_size(theme.typography.footnote)
                    .text_color(theme.ely.fg_muted)
                    .tooltip(controls::tooltip("Probe tooltip"))
                    .child("Hover target"),
            ),
        }
    }
}

fn main() {
    let Some(options) = parse_options() else {
        return;
    };
    print_gaps(options.frame);
    gpui_platform::application()
        .with_assets(sirio_ui::ely::AppAssets)
        .run(move |cx: &mut App| {
            // The app's motion setup (`sirio`'s `init_motion`): bezel parks its animation clock
            // while no window of the app is active, and a display with no window manager never
            // activates one, so a row's hover fade would stop half-way and stay drawn that way
            // after the pointer left it (what a wheel over a Settings page does to the row at the
            // window's centre).
            {
                use bezel::motion::AppExt as _;
                cx.set_pause_when_inactive(false);
            }
            // The debug switch `Scripts/bezel-off-captures.sh` exports: snap every animation to
            // its rest state. It lands before `ely::init`, which copies the flag into Ely's theme
            // (a no-op in release builds), as in `sirio`'s `main`.
            if cfg!(debug_assertions) && std::env::var_os("SIRIO_REDUCED_MOTION").is_some() {
                cx.set_reduce_motion(true);
            }
            // Fonts before `Theme::init`: it resolves and caches the families once.
            sirio_theme::register_ui_fonts(cx).expect("Sirio fonts");
            sirio_theme::register_bundled_terminal_font(cx).expect("Sirio terminal font");
            Theme::init(cx);
            Theme::set_mode(options.mode, cx);
            // Theme observers run when this closure returns; Ely's palette must already
            // follow the mode chosen above when the window draws.
            sirio_ui::ely::init(cx);
            sirio_ui::ely::init(cx);
            sirio_ui::chat::init(cx);
            sirio_ui::text_selection::init(cx);
            let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |window, cx| {
                    window.set_window_title("Surfaces Probe");
                    let (settings, language_store) = match options.frame {
                        Frame::Settings(category) => {
                            let (settings, store) = settings_entity(category, options.mode, cx);
                            (Some(settings), store)
                        }
                        Frame::GitHostingAccounts => {
                            forge_source::set_source(Arc::new(ProbeForgeSource), cx);
                            let (settings, store) =
                                settings_entity(SettingsCategory::GitHosting, options.mode, cx);
                            (Some(settings), store)
                        }
                        Frame::TabMenu | Frame::Tooltip => (None, None),
                    };
                    cx.new(|_| Surface {
                        frame: options.frame,
                        settings,
                        _language_store: language_store,
                    })
                },
            )
            .expect("open the surfaces probe window");
            cx.activate(true);
        });
}
