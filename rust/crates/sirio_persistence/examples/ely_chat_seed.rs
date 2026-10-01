//! Seeds a scratch database for the real-app chat verification
//! (`Scripts/Tests/test-ely-chat-ui-e2e.py`): one project, its selected
//! worktree and one active chat tab, so a fresh Sirio launched against it
//! restores straight into a rendered chat.
//!
//! A restored chat needs a recorded agent: without one the app refuses to
//! reopen it ("saved before Sirio recorded which agent it belonged to"), and
//! `SIRIO_ACP_PROGRAM` is read only when a *new* chat is opened with no agent
//! picked. So `--agent` names the adapter the tab records, and the runner puts
//! a stand-in for that adapter's binary on the instance's PATH. Appearance,
//! interface size and "updates off" go through `AppSettings`; nothing here
//! touches the schema.
//!
//! Usage: `ely_chat_seed --database PATH --worktree PATH --appearance light|dark --ui-size SIZE [--agent ID]`

use std::path::{Path, PathBuf};

use sirio_persistence::{
    AgentRef, AppDatabase, AppSettings, AppearanceMode, PersistenceError, ProjectRecord,
    SidebarState, TabRecord, WorktreeRecord, settings_ranges, stable_worktree_id,
};

/// The persisted tab the control socket addresses as `surfaceId`.
const CHAT_TAB_ID: &str = "ely-chat-tab";

/// The id the app gives the project at `path`: `p-` and the FNV-1a hash of the
/// canonical path, the same hash `stable_worktree_id` ends in. The app
/// re-registers a project it finds under another id, which replaces its
/// worktree and, by cascade, the chat tab seeded under it.
fn project_id_for(path: &Path) -> String {
    let id = stable_worktree_id("", path);
    let hash = id.rsplit("-wt-").next().unwrap_or_default();
    format!("p-{hash}")
}

/// Writes the project, worktree, chat tab, sidebar selection and settings.
fn seed_chat(
    database: &Path,
    worktree: &Path,
    appearance: AppearanceMode,
    ui_size: i64,
    agent: Option<&str>,
) -> Result<(), PersistenceError> {
    let db = AppDatabase::open(database)?;
    let project_id = project_id_for(worktree);
    let name = worktree
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    db.save_project(&ProjectRecord {
        id: project_id.clone(),
        name,
        root_path: worktree.to_string_lossy().into_owned(),
        order_idx: 0,
        color_hex: None,
        display_name: None,
        icon_kind: "icon".into(),
        icon_value: None,
        avatar_image: None,
        default_worktree_base: None,
        worktree_location_override: None,
    })?;
    let worktree_id = stable_worktree_id(&project_id, worktree);
    db.save_worktree(&WorktreeRecord {
        id: worktree_id.clone(),
        project_id: project_id.clone(),
        branch: "main".into(),
        path: worktree.to_string_lossy().into_owned(),
        order_idx: 0,
        is_primary: true,
        comment: None,
        created_at: None,
        updated_at: None,
        secondary_pane_hidden: false,
    })?;
    db.save_tabs(
        &worktree_id,
        &[TabRecord {
            id: CHAT_TAB_ID.into(),
            worktree_id: worktree_id.clone(),
            title: "Ely chat".into(),
            kind: "chat".into(),
            agent_id: agent.map(AgentRef::adapter),
            agent_session_id: None,
            order_idx: 0,
            is_active: true,
            last_event_at: None,
            closed_at: None,
        }],
    )?;
    db.save_sidebar_state(&SidebarState {
        expanded_project_ids: vec![project_id],
        selected_worktree_id: Some(worktree_id),
    })?;
    db.save_settings(&AppSettings {
        appearance,
        ui_font_size: ui_size,
        updates_enabled: false,
        ..AppSettings::default()
    })?;
    Ok(())
}

struct Arguments {
    database: PathBuf,
    worktree: PathBuf,
    appearance: AppearanceMode,
    ui_size: i64,
    agent: Option<String>,
}

fn parse(args: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let (mut database, mut worktree, mut appearance, mut ui_size) = (None, None, None, None);
    let mut agent = None;
    let mut args = args;
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--database" => database = Some(PathBuf::from(value("--database")?)),
            "--worktree" => worktree = Some(PathBuf::from(value("--worktree")?)),
            "--appearance" => {
                appearance = Some(match value("--appearance")?.as_str() {
                    "light" => AppearanceMode::Light,
                    "dark" => AppearanceMode::Dark,
                    other => return Err(format!("--appearance is light or dark, not {other:?}")),
                })
            }
            "--ui-size" => {
                let size = value("--ui-size")?
                    .parse::<i64>()
                    .map_err(|error| format!("--ui-size is a whole number: {error}"))?;
                if !settings_ranges::UI_FONT_SIZE.contains(&size) {
                    return Err(format!(
                        "--ui-size is {}..={}, not {size}",
                        settings_ranges::UI_FONT_SIZE.start(),
                        settings_ranges::UI_FONT_SIZE.end()
                    ));
                }
                ui_size = Some(size);
            }
            "--agent" => agent = Some(value("--agent")?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Arguments {
        database: database.ok_or("--database is required")?,
        worktree: worktree.ok_or("--worktree is required")?,
        appearance: appearance.ok_or("--appearance is required")?,
        ui_size: ui_size.ok_or("--ui-size is required")?,
        agent,
    })
}

fn main() {
    let arguments = match parse(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("ely_chat_seed: {message}");
            std::process::exit(2);
        }
    };
    // A scratch database is created, never reused: seeding over one that
    // exists would mix this run's rows with another's.
    if arguments.database.exists() {
        eprintln!(
            "ely_chat_seed: {} already exists",
            arguments.database.display()
        );
        std::process::exit(2);
    }
    if let Err(error) = seed_chat(
        &arguments.database,
        &arguments.worktree,
        arguments.appearance,
        arguments.ui_size,
        arguments.agent.as_deref(),
    ) {
        eprintln!("ely_chat_seed: {error}");
        std::process::exit(1);
    }
    println!("seeded {}", arguments.database.display());
}
