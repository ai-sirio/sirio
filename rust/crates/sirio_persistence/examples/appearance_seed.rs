//! Seeds a scratch database with an appearance and nothing else, so an
//! isolated Sirio launched against it draws in that mode
//! (`Scripts/Tests/test-forge-ui-e2e.sh --appearance light`).
//!
//! Usage: `appearance_seed --database PATH --appearance light|dark`
use sirio_persistence::{AppDatabase, AppSettings, AppearanceMode};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (mut database, mut appearance) = (None, None);
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
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let database = database.ok_or("--database is required")?;
    let appearance = appearance.ok_or("--appearance is required")?;
    AppDatabase::open(&database)?.save_settings(&AppSettings {
        appearance,
        updates_enabled: false,
        ..AppSettings::default()
    })?;
    Ok(())
}
