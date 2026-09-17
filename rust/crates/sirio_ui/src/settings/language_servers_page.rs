//! Settings › Language Servers, drawn on bezel's settings scaffolding.
//!
//! Modelled on `agents_page.rs`: one row per item, `InstallState` for live
//! state, the row unclickable while the installer holds its lock, and the
//! installer's own error text shown verbatim.
//!
//! The page owns its state, its row model, its renderer and its tests;
//! `settings.rs` keeps the category, the routing and the persistence seam.
//! [`ServerStates`] is plain data, so [`language_server_rows`] stays pure —
//! no window, no context, no filesystem — which is what makes the rows
//! testable without a window.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;

use super::*;
use bezel::ui::widgets::{
    ButtonStyle, Buttons as _, Scaffolding as _, Status as _, card_row_hover, status_dot,
};
use sirio_lsp::{LanguageTable, Recipe};

/// Where one language's server is right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ServerOrigin {
    /// Not on `PATH` and not in Sirio's store.
    #[default]
    Absent,
    /// The reader's own copy, found on `PATH`. At launch the name is always
    /// tried first, so this is what the row reports even when Sirio installed
    /// a second copy underneath it.
    OnPath,
    /// Sirio's own copy, in the language-server store. `version` is what the
    /// install manifest recorded.
    Installed { version: String },
}

impl ServerOrigin {
    /// The row's state, in the words the design uses for it.
    pub fn label(&self) -> String {
        match self {
            Self::Absent => "absent".to_owned(),
            Self::OnPath => "on PATH".to_owned(),
            Self::Installed { version } => format!("installed by Sirio v{version}"),
        }
    }

    /// Whether the server is here at all. A server that is here is not an
    /// offer: PATH wins at launch, and a second copy would never run.
    fn is_here(&self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// What one row's action does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// Sirio can fetch this server: the row carries an Install button.
    Install,
    /// Something has to come first: the row carries a link to the page that
    /// says how, and the reason is shown beside the state.
    OpenUrl {
        url: &'static str,
        needs: &'static str,
    },
    /// Nothing to do — the server is already here, or there is nothing
    /// Sirio can fetch.
    Nothing,
}

/// One language's server: the shipped table's entry, plus what this machine
/// has for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerState {
    pub language: String,
    pub command: String,
    pub origin: ServerOrigin,
    /// How to get `command`. `None` is an entry of the reader's own, which
    /// carries no recipe on purpose — Sirio must not offer a second copy of
    /// a server they already chose.
    pub install: Option<Recipe>,
}

/// The whole page's model: every language the shipped table names, what this
/// machine has for each, and the languages whose install offer was declined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerStates {
    pub servers: Vec<ServerState>,
    /// "Don't ask again": the languages whose install offer the reader
    /// declined. Persisted as `lsp.silencedLanguages`; this page is the one
    /// place one comes back.
    pub silenced: BTreeSet<String>,
}

impl Default for ServerStates {
    /// The shipped table with nothing discovered yet: every server reads
    /// `absent` until [`Self::discover`] has looked, because a default that
    /// claimed anything else would be a claim about the machine.
    fn default() -> Self {
        Self {
            servers: LanguageTable::defaults()
                .entries()
                .iter()
                .map(|entry| ServerState {
                    language: entry.name.clone(),
                    command: entry.command.clone(),
                    origin: ServerOrigin::Absent,
                    install: entry.install.clone(),
                })
                .collect(),
            silenced: BTreeSet::new(),
        }
    }
}

impl ServerStates {
    /// Fills in each row's origin: the reader's `PATH` first, then Sirio's
    /// store — the order a launch tries them in, so an installed copy never
    /// hides the reader's own.
    ///
    /// `path` is a parameter rather than a `std::env` read so the states can
    /// be tested against a fixture PATH; the surface passes the process's own.
    pub fn discover(store_root: &Path, path: &OsStr) -> Self {
        let store = sirio_registry::InstallStore::new(store_root.to_path_buf());
        let mut states = Self::default();
        for server in &mut states.servers {
            server.origin = origin_of(server, &store, path);
        }
        states
    }
}

fn origin_of(
    server: &ServerState,
    store: &sirio_registry::InstallStore,
    path: &OsStr,
) -> ServerOrigin {
    if sirio_agents::find_executable_in_path(&server.command, path).is_some() {
        return ServerOrigin::OnPath;
    }
    // `store_id` is the package or release id the install is keyed by; a
    // `Manual` recipe has none, so there is nothing Sirio ever installed
    // for it and nothing to look up.
    let Some(id) = server.install.as_ref().and_then(Recipe::store_id) else {
        return ServerOrigin::Absent;
    };
    // A manifest outliving its executable is not an install: handing back a
    // path that is not there would report a half-deleted install as present.
    // The same rule is written out in `sirio`'s `LspSupervisor::installed_binary_for`
    // for the launch path; this crate cannot call it (it takes a
    // `sirio_lsp::LanguageEntry` and lives in the app), so the two move
    // together. Consolidating it on `InstallStore` would collapse them.
    match store.manifest(id) {
        Some(installed) if installed.executable.exists() => ServerOrigin::Installed {
            version: installed.version,
        },
        _ => ServerOrigin::Absent,
    }
}

/// One row, pure data: everything the page draws, derived from
/// [`ServerStates`] alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRow {
    pub language: String,
    pub command: String,
    pub origin: ServerOrigin,
    pub action: RowAction,
    /// Whether the install offer for this language was declined.
    pub silenced: bool,
}

/// One row per language the shipped table names, in the table's own order.
pub fn language_server_rows(states: &ServerStates) -> Vec<ServerRow> {
    states
        .servers
        .iter()
        .map(|server| ServerRow {
            language: server.language.clone(),
            command: server.command.clone(),
            origin: server.origin.clone(),
            action: action_for(server),
            silenced: states.silenced.contains(&server.language),
        })
        .collect()
}

fn action_for(server: &ServerState) -> RowAction {
    if server.origin.is_here() {
        return RowAction::Nothing;
    }
    match &server.install {
        Some(Recipe::Manual { url, needs }) => RowAction::OpenUrl { url, needs },
        Some(_) => RowAction::Install,
        None => RowAction::Nothing,
    }
}

impl Settings {
    pub(crate) fn render_language_servers(&self, theme: Theme, entity: Entity<Self>) -> gpui::Div {
        let bezel_theme = theme.to_bezel_theme();
        let rows = language_server_rows(&self.lsp_servers);
        let mut card = bezel_theme
            .group_box()
            .debug_selector(|| "settings-language-servers-card".to_string());
        for (index, row) in rows.iter().enumerate() {
            let install_state = self.install_states.get(&row.language);

            // The quiet meta line under the name: the command, then what has
            // to come first, then the declined offer, then install progress.
            let mut fragments: Vec<AnyElement> = Vec::new();
            fragments.push(
                div()
                    .debug_selector(move || format!("settings-language-server-command-{index}"))
                    .font_family(bezel_theme.font_mono.clone())
                    .text_size(px(11.0))
                    .child(text!(row.command.clone()))
                    .into_any_element(),
            );
            if let RowAction::OpenUrl { needs, .. } = &row.action {
                let needs = *needs;
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-language-server-needs-{index}"))
                        .child(text!(format!("needs {needs}")))
                        .into_any_element(),
                );
            }
            if row.silenced {
                fragments.push(
                    div()
                        .debug_selector(move || {
                            format!("settings-language-server-silenced-{index}")
                        })
                        .child(text!("Install offers off"))
                        .into_any_element(),
                );
            }
            if matches!(install_state, Some(InstallState::InFlight)) {
                fragments.push(
                    div()
                        .debug_selector(move || {
                            format!("settings-language-server-install-status-{index}")
                        })
                        .child(text!("Installing… this can take up to ten minutes."))
                        .into_any_element(),
                );
            }
            let body = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    bezel_theme
                        .row_title(row.language.clone())
                        .debug_selector(move || format!("settings-language-server-name-{index}")),
                )
                .child(
                    bezel_theme
                        .meta_line(fragments)
                        .debug_selector(move || format!("settings-language-server-meta-{index}")),
                );

            let status_id = format!("settings-language-server-status-{}", row.language);
            let status = div()
                .id(status_id.clone())
                .debug_selector(move || status_id.clone())
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(status_dot(if row.origin.is_here() {
                    bezel_theme.success
                } else {
                    bezel_theme.danger
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(bezel_theme.text_muted)
                        .child(text!(row.origin.label())),
                );
            let mut tail = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(status);
            // While this row's install is in flight the action disappears:
            // the installer holds the lock and would refuse a second click,
            // and a dead-looking button invites exactly that click.
            if !matches!(install_state, Some(InstallState::InFlight)) {
                match &row.action {
                    RowAction::Install => {
                        let control_entity = entity.clone();
                        let language = row.language.clone();
                        tail = tail.child(
                            bezel_theme
                                .button("Install", ButtonStyle::Prominent, None)
                                .id(("settings-language-server-install", index))
                                .debug_selector(move || {
                                    format!("settings-language-server-install-{index}")
                                })
                                .on_click(move |_, _, cx| {
                                    let language = language.clone();
                                    control_entity.update(cx, |_, cx| {
                                        cx.emit(SettingsEvent::InstallLanguageServer(language));
                                    });
                                }),
                        );
                    }
                    RowAction::OpenUrl { url, .. } => {
                        let url = *url;
                        tail = tail.child(
                            bezel_theme
                                .button("Install guide", ButtonStyle::Ghost, None)
                                .border_1()
                                .border_color(bezel_theme.border)
                                .hover(|s| s.bg(bezel_theme.element_hover))
                                .id(("settings-language-server-link", index))
                                .debug_selector(move || {
                                    format!("settings-language-server-link-{index}")
                                })
                                .on_click(move |_, _, cx| cx.open_url(url)),
                        );
                    }
                    RowAction::Nothing => {}
                }
            }
            // The one place a declined offer is taken back. The card's "Don't
            // ask again" writes the store; this writes it back.
            if row.silenced {
                let control_entity = entity.clone();
                let language = row.language.clone();
                tail = tail.child(
                    bezel_theme
                        .button("Offer again", ButtonStyle::Ghost, None)
                        .border_1()
                        .border_color(bezel_theme.border)
                        .hover(|s| s.bg(bezel_theme.element_hover))
                        .id(("settings-language-server-unsilence", index))
                        .debug_selector(move || {
                            format!("settings-language-server-unsilence-{index}")
                        })
                        .on_click(move |_, _, cx| {
                            let language = language.clone();
                            control_entity.update(cx, |settings, cx| {
                                settings.set_language_silenced(&language, false, cx)
                            });
                        }),
                );
            }
            let mut row_el = bezel_theme
                .card_row(index == 0)
                .hover(card_row_hover)
                .id(("settings-language-server-row", index))
                .debug_selector(move || format!("settings-language-server-row-{index}"))
                .flex_wrap()
                .child(body)
                .child(tail);
            if let Some(InstallState::Failed(message)) = install_state {
                row_el = row_el.child(
                    bezel_theme
                        .error_strip(message.clone())
                        .mt(px(8.0))
                        .w_full()
                        .id(("settings-language-server-install-reason", index))
                        .debug_selector(move || {
                            format!("settings-language-server-install-reason-{index}")
                        }),
                );
            }
            card = card.child(row_el);
        }

        let count = rows.len();
        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(
                div()
                    .w_full()
                    .mb(px(14.0))
                    .flex()
                    .justify_between()
                    .items_start()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap(px(10.0))
                                    .child(
                                        bezel_theme
                                            .page_header("Language Servers", None)
                                            .id("language-servers-page-title")
                                            .debug_selector(|| {
                                                "language-servers-page-title".to_string()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("language-servers-page-count")
                                            .debug_selector(|| {
                                                "language-servers-page-count".to_string()
                                            })
                                            .text_size(px(13.0))
                                            .text_color(bezel_theme.text_muted.opacity(0.7))
                                            .child(text!(format!("{count}"))),
                                    ),
                            )
                            .child(
                                bezel_theme
                                    .page_subtitle(
                                        "The server Sirio names for every language the editor can open. Each row reads your PATH first, then Sirio's own installs.",
                                    )
                                    .id("language-servers-page-subtitle")
                                    .debug_selector(|| {
                                        "language-servers-page-subtitle".to_string()
                                    }),
                            ),
                    ),
            )
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The row's index, from the model the page draws.
    fn row_index(language: &str) -> usize {
        ServerStates::default()
            .servers
            .iter()
            .position(|server| server.language == language)
            .expect("the shipped table names this language")
    }

    /// The UI tests address rows by index, because a `debug_bounds` lookup
    /// wants a `'static` selector string. The order those indices name is
    /// therefore pinned here: a table reorder has to fail loudly rather than
    /// silently point a click at another language.
    #[test]
    fn the_row_order_follows_the_shipped_table() {
        let rows = language_server_rows(&ServerStates::default());
        assert_eq!(rows[row_index("rust")].language, "rust");
        assert_eq!(rows[row_index("java")].language, "java");
        assert_eq!(row_index("rust"), 0, "the first row is the first entry");
        assert_eq!(row_index("java"), 5, "the selectors below say so");
    }

    fn row_for<'a>(rows: &'a [ServerRow], language: &str) -> &'a ServerRow {
        rows.iter()
            .find(|row| row.language == language)
            .expect("every language has a row")
    }

    #[test]
    fn every_language_has_a_row() {
        // The number the design pinned, and the reason the page is not a
        // duplicate of the table: it is built from `LanguageTable::defaults()`
        // itself. A language added there and not drawn here is the defect
        // this test exists to catch.
        assert_eq!(language_server_rows(&ServerStates::default()).len(), 21);
    }

    #[test]
    fn a_language_that_cannot_be_installed_offers_a_link_rather_than_a_button() {
        // java names a JVM as the thing that has to come first: there is
        // nothing for Sirio to fetch, so the row must link to the page that
        // says how, not draw a button that could not be kept.
        let rows = language_server_rows(&ServerStates::default());
        let java = row_for(&rows, "java");
        assert!(
            matches!(java.action, RowAction::OpenUrl { .. }),
            "java's action is a link, got {:?}",
            java.action
        );
        match java.action {
            RowAction::OpenUrl { url, needs } => {
                assert!(
                    !needs.is_empty(),
                    "the row has to say what has to come first"
                );
                assert!(
                    url.starts_with("https://"),
                    "the link has to go somewhere: {url}"
                );
            }
            _ => unreachable!("the match above already proved the arm"),
        }
    }

    #[test]
    fn an_installable_language_offers_an_install_and_one_already_here_offers_nothing() {
        let rows = language_server_rows(&ServerStates::default());
        assert_eq!(
            row_for(&rows, "rust").action,
            RowAction::Install,
            "rust-analyzer is one Sirio can fetch"
        );

        // A server the reader already has is not an offer: PATH wins at
        // launch, so a second copy would never run.
        let mut states = ServerStates::default();
        for server in &mut states.servers {
            if server.language == "rust" {
                server.origin = ServerOrigin::Installed {
                    version: "2026-09-14".to_owned(),
                };
            }
        }
        let rows = language_server_rows(&states);
        assert_eq!(row_for(&rows, "rust").action, RowAction::Nothing);
        assert_eq!(
            row_for(&rows, "rust").origin.label(),
            "installed by Sirio v2026-09-14",
            "and the row says where it came from"
        );
    }

    #[test]
    fn a_silenced_language_can_be_unsilenced_here() {
        let mut states = ServerStates::default();
        states.silenced.insert("java".to_string());
        let rows = language_server_rows(&states);
        let java = row_for(&rows, "java");
        assert!(java.silenced, "and the row says so, so it can be undone");
        let rust = row_for(&rows, "rust");
        assert!(!rust.silenced, "a language nobody declined is not marked");
    }

    /// The states read the reader's `PATH` before Sirio's store, and only
    /// then report `absent` — the three words the design gives the column,
    /// pinned against a scratch PATH and a scratch store rather than against
    /// whatever this machine happens to have.
    #[test]
    fn the_states_read_the_readers_path_before_sirios_store() {
        let scratch = std::env::temp_dir().join(format!(
            "sirio-language-server-states-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let path_dir = scratch.join("bin");
        let store_root = scratch.join("language-servers");
        std::fs::create_dir_all(&path_dir).expect("create the fixture PATH");
        std::fs::create_dir_all(&store_root).expect("create the fixture store");

        let defaults = ServerStates::default();
        let store_id = |language: &str| {
            defaults
                .servers
                .iter()
                .find(|server| server.language == language)
                .and_then(|server| server.install.as_ref())
                .and_then(Recipe::store_id)
                .expect("the shipped table installs this language")
                .to_owned()
        };

        // rust: present in the fixture PATH *and* in the store. PATH wins.
        let on_path = path_dir.join("rust-analyzer");
        std::fs::write(&on_path, b"#!/bin/sh\n").expect("write the fixture binary");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&on_path)
                .expect("stat the fixture binary")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&on_path, permissions).expect("make it executable");
        }
        let store = sirio_registry::InstallStore::new(&store_root);
        let installed_binary = scratch.join("installed-rust-analyzer");
        std::fs::write(&installed_binary, b"#!/bin/sh\n").expect("write the store's binary");
        store
            .write(&sirio_registry::InstalledAgent {
                id: store_id("rust"),
                version: "from-the-store".to_owned(),
                executable: installed_binary.clone(),
                args: Vec::new(),
                integrity: sirio_registry::Integrity::None,
            })
            .expect("write the rust manifest");
        // typescript: only in the store, with its executable still there.
        store
            .write(&sirio_registry::InstalledAgent {
                id: store_id("typescript"),
                version: "6.0.0".to_owned(),
                executable: installed_binary.clone(),
                args: Vec::new(),
                integrity: sirio_registry::Integrity::None,
            })
            .expect("write the typescript manifest");
        // python: in the store, but the manifest outlives its executable.
        store
            .write(&sirio_registry::InstalledAgent {
                id: store_id("python"),
                version: "1.1.414".to_owned(),
                executable: scratch.join("deleted"),
                args: Vec::new(),
                integrity: sirio_registry::Integrity::None,
            })
            .expect("write the python manifest");

        let states = ServerStates::discover(&store_root, path_dir.as_os_str());
        let origin = |language: &str| {
            states
                .servers
                .iter()
                .find(|server| server.language == language)
                .expect("the shipped table names this language")
                .origin
                .clone()
        };
        assert_eq!(
            origin("rust"),
            ServerOrigin::OnPath,
            "the reader's own copy is what a launch uses, so it is what the row reports"
        );
        assert_eq!(
            origin("typescript"),
            ServerOrigin::Installed {
                version: "6.0.0".to_owned()
            }
        );
        assert_eq!(
            origin("python"),
            ServerOrigin::Absent,
            "a manifest whose executable is gone is not an install"
        );
        assert_eq!(origin("kotlin"), ServerOrigin::Absent, "nothing anywhere");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Entering the page re-reads the machine: an install that landed while
    /// the reader was on another screen has to show up without a restart, and
    /// the store is the only thing that knows it landed.
    #[gpui::test]
    async fn entering_the_language_servers_page_reads_the_machine_again(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let scratch = std::env::temp_dir().join(format!(
            "sirio-language-servers-refresh-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let store_root = scratch.join("language-servers");
        std::fs::create_dir_all(&store_root).expect("create the fixture store");

        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let settings =
            cx.update(|window, _| window.root::<Settings>().flatten().expect("settings root"));

        // The page is constructed against a store that has nothing in it…
        settings.update(&mut cx.cx, |settings, _| {
            settings.set_lsp_store_root(store_root.clone())
        });
        // …for a server this machine really does not have: whatever it has on
        // PATH is this machine's business, and the fixture must not depend on
        // it being bare. A language with `absent` and a store id is one Sirio
        // could install and nobody has.
        let (language, store_id) = settings.read_with(&cx.cx, |settings, _| {
            settings
                .lsp_servers
                .servers
                .iter()
                .find(|server| {
                    server.origin == ServerOrigin::Absent
                        && server
                            .install
                            .as_ref()
                            .is_some_and(|recipe| recipe.store_id().is_some())
                })
                .map(|server| {
                    let id = server
                        .install
                        .as_ref()
                        .and_then(Recipe::store_id)
                        .expect("the filter above proved a store id")
                        .to_owned();
                    (server.language.clone(), id)
                })
                .expect("no machine has all twenty-one servers")
        });
        let origin = |cx: &mut VisualTestContext, settings: &Entity<Settings>| {
            settings.read_with(&cx.cx, |settings, _| {
                settings
                    .lsp_servers
                    .servers
                    .iter()
                    .find(|server| server.language == language)
                    .expect("the shipped table names this language")
                    .origin
                    .clone()
            })
        };
        assert_eq!(origin(&mut cx, &settings), ServerOrigin::Absent);

        // …and an install lands while the reader is elsewhere.
        let installed_binary = scratch.join("installed-server");
        std::fs::write(&installed_binary, b"#!/bin/sh\n").expect("write the installed binary");
        sirio_registry::InstallStore::new(&store_root)
            .write(&sirio_registry::InstalledAgent {
                id: store_id,
                version: "2026-09-14".to_owned(),
                executable: installed_binary,
                args: Vec::new(),
                integrity: sirio_registry::Integrity::None,
            })
            .expect("write the manifest");

        let category = cx
            .debug_bounds("settings-category-LanguageServers")
            .expect("the Language Servers category is offered");
        cx.simulate_click(category.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            origin(&mut cx, &settings),
            ServerOrigin::Installed {
                version: "2026-09-14".to_owned()
            },
            "entering the page read the store again"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The behaviour behind the row mark: pressing "Offer again" routes a
    /// snapshot through `on_change` with the language no longer silenced, so
    /// the host's save carries the change rather than throwing it away.
    #[gpui::test]
    async fn unsilencing_a_language_routes_the_new_list_through_the_persisted_snapshot(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let recorded: Rc<RefCell<Vec<SettingsSnapshot>>> = Rc::new(RefCell::new(Vec::new()));
        let observed = recorded.clone();
        let window = cx.add_window(move |_window, cx| {
            Settings::with_snapshot(
                cx,
                SettingsSnapshot {
                    lsp_silenced_languages: vec!["java".to_owned(), "kotlin".to_owned()],
                    ..SettingsSnapshot::default()
                },
            )
            .on_change(move |snapshot| observed.borrow_mut().push(snapshot))
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let category = cx
            .debug_bounds("settings-category-LanguageServers")
            .expect("the Language Servers category is offered");
        cx.simulate_click(category.center(), Modifiers::none());
        cx.run_until_parked();

        let java = row_index("java");
        assert_eq!(java, 5, "the selectors below say so");
        assert!(
            cx.debug_bounds("settings-language-server-silenced-5")
                .is_some(),
            "the declined language is marked on its row"
        );
        let unsilence = cx
            .debug_bounds("settings-language-server-unsilence-5")
            .expect("a declined language can be asked for again");
        cx.simulate_click(unsilence.center(), Modifiers::none());
        cx.run_until_parked();

        let last = recorded
            .borrow()
            .last()
            .cloned()
            .expect("the undo routed a snapshot through on_change");
        assert_eq!(
            last.lsp_silenced_languages,
            vec!["kotlin".to_owned()],
            "java is out of the persisted list and the other decline is untouched"
        );
        assert!(
            cx.debug_bounds("settings-language-server-unsilence-5")
                .is_none(),
            "and the row stops claiming the offer is off"
        );
    }

    /// `InstallState` is the host's live state, reused rather than
    /// re-invented: an install in flight leaves the row with no action to
    /// press, and a failure shows the installer's own text verbatim.
    #[gpui::test]
    async fn an_install_in_flight_leaves_no_action_and_a_failure_shows_the_installer_text(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        let window =
            cx.add_window(|_window, cx| Settings::with_snapshot(cx, SettingsSnapshot::default()));
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let category = cx
            .debug_bounds("settings-category-LanguageServers")
            .expect("the Language Servers category is offered");
        cx.simulate_click(category.center(), Modifiers::none());
        cx.run_until_parked();

        let rust = row_index("rust");
        assert_eq!(rust, 0, "the selectors below say so");
        assert!(
            cx.debug_bounds("settings-language-server-install-0")
                .is_some(),
            "an absent, installable language starts with its Install button"
        );

        let settings =
            cx.update(|window, _| window.root::<Settings>().flatten().expect("settings root"));
        settings.update(&mut cx.cx, |settings, cx| {
            settings.set_install_state("rust", Some(InstallState::InFlight));
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("settings-language-server-install-0")
                .is_none(),
            "the installer holds the lock, so the row offers no second click"
        );

        settings.update(&mut cx.cx, |settings, cx| {
            settings.set_install_state(
                "rust",
                Some(InstallState::Failed("npm exited 1: EAI_AGAIN".to_owned())),
            );
            cx.notify();
        });
        cx.run_until_parked();
        let reason = cx
            .debug_bounds("settings-language-server-install-reason-0")
            .expect("the installer's failure is shown on its own row");
        assert!(
            reason.size.width > px(0.0),
            "the error strip is text-bearing, not an empty line"
        );
        assert!(
            cx.debug_bounds("settings-language-server-install-0")
                .is_some(),
            "a failure leaves the action offered, as the agents row does"
        );
    }
}
