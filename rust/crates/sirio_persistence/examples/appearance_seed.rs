//! Seeds a scratch database with an appearance and nothing else, so an
//! isolated Sirio launched against it draws in that mode
//! (`Scripts/Tests/test-forge-ui-e2e.sh --appearance light`).
//!
//! Usage: `appearance_seed --database PATH [--appearance light|dark] [--auto-naming on]`
//! (at least one of the two; `--auto-naming on` turns the summarizer on, which
//! the rename scenarios of `test-chat-session-panel-e2e.sh` read.)
use sirio_persistence::{AppDatabase, AppSettings, AppearanceMode};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (mut database, mut appearance, mut auto_naming) = (None, None, false);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--database" => database = Some(PathBuf::from(value)),
            "--appearance" => {
                appearance = Some(match value.as_str() {
                    "light" => AppearanceMode::Light,
                    "dark" => AppearanceMode::Dark,
                    other => {
                        return Err(format!("--appearance is light or dark, not {other:?}").into());
                    }
                })
            }
            "--auto-naming" => match value.as_str() {
                "on" => auto_naming = true,
                other => return Err(format!("--auto-naming is on, not {other:?}").into()),
            },
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let database = database.ok_or("--database is required")?;
    if appearance.is_none() && !auto_naming {
        return Err("--appearance or --auto-naming is required".into());
    }
    AppDatabase::open(&database)?.save_settings(&AppSettings {
        appearance: appearance.unwrap_or(AppSettings::default().appearance),
        auto_naming,
        updates_enabled: false,
        ..AppSettings::default()
    })?;
    Ok(())
}
