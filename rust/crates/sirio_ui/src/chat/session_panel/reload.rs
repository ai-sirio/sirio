//! The reload row, the last line of the Connectors card: `Plugins and skills`, with a
//! `Reload` that runs `reload_plugins` and then, once that applied, `reload_skills`.
//! Native Claude chats only.
//!
//! The first `reload_plugins` asks the CLI to hold when the reload would reset its prompt
//! cache: a held answer reloads nothing, shows the cost under the row and offers
//! `Reload anyway`, which sends the request without the hold. Nothing is sent while a turn
//! runs, and the gate is read again before the skills. The calls wait for the CLI (30 s
//! at most), so they run off the GPUI thread. A refusal of either verb retires the row
//! until the next connection.

use super::Row;
use crate::chat::Chat;
use crate::chat::visible_text::visible_text;
use gpui::{AppContext as _, Context as GpuiContext, Task};
use sirio_chat::{CacheImpact, CliState, ReloadError};

pub(super) const RELOAD_KEY: &str = "connectors.reload";

const BUSY_NOTE: &str = "Claude is busy with a turn. Try again when it ends.";
const HELD_LEAD: &str =
    "Reloading now changes Claude's tools, which resets the prompt cache: the next turn costs more.";

#[derive(Default)]
pub(super) struct Reload {
    /// A reload is open: the row reads `reloading…` and takes no click.
    running: bool,
    /// What the CLI said it would change, while its answer holds the reload.
    held: Option<CacheImpact>,
    /// A turn was running when the click came, or the CLI said it was busy.
    busy: bool,
    /// What the last reload said for itself: plugins that failed to load, or why it failed.
    message: String,
    /// The CLI does not have a reload verb: the row is gone until a new client.
    retired: bool,
    /// The open reload. Dropping it cancels it, which is how a new connection never
    /// receives the old one's answer.
    task: Option<Task<()>>,
}

fn names(list: &[String]) -> String {
    list.iter()
        .map(|name| visible_text(name, false))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The cost warning for a held reload, from the CLI's own account of what would change.
fn held_note(impact: &CacheImpact) -> String {
    let mut note = HELD_LEAD.to_string();
    if !impact.servers_added.is_empty() {
        note.push_str(&format!(" Adds: {}.", names(&impact.servers_added)));
    }
    if !impact.servers_removed.is_empty() {
        note.push_str(&format!(" Removes: {}.", names(&impact.servers_removed)));
    }
    match impact.lsp_tool_change.as_deref() {
        Some("adds") => note.push_str(" Adds the LSP tool."),
        Some("may-add") => note.push_str(" May add the LSP tool."),
        Some("removes") => note.push_str(" Removes the LSP tool."),
        Some("may-remove") => note.push_str(" May remove the LSP tool."),
        _ => {}
    }
    note
}

/// The row, or `None` once the CLI has refused a reload verb.
pub(super) fn row(reload: &Reload) -> Option<Row> {
    if reload.retired {
        return None;
    }
    let mut row = Row::new(RELOAD_KEY, "Plugins and skills".to_string(), "");
    row.show_action = true;
    if reload.running {
        row.detail = "reloading…".to_string();
        row.short = Some("reloading…".to_string());
    } else {
        let action = if reload.held.is_some() { "Reload anyway" } else { "Reload" };
        row.action = Some(action.to_string());
    }
    let mut parts = Vec::new();
    if let Some(impact) = &reload.held {
        parts.push(held_note(impact));
    }
    if reload.busy {
        parts.push(BUSY_NOTE.to_string());
    }
    if !reload.message.is_empty() {
        parts.push(reload.message.clone());
    }
    row.note = parts.join(" ");
    Some(row)
}

fn plugin_errors(count: u64) -> String {
    if count == 1 {
        "1 plugin failed to load.".to_string()
    } else {
        format!("{count} plugins failed to load.")
    }
}

/// What the open reload found, read back on the UI thread.
enum Outcome {
    Held(CacheImpact),
    Applied {
        error_count: u64,
        skills: Result<(), ReloadError>,
    },
    Failed(ReloadError),
}

impl Chat {
    /// Whether a turn runs: this chat's own, or one the CLI started.
    fn reload_is_busy(&self) -> bool {
        self.is_streaming()
            || matches!(
                self.cli_state(),
                Some(CliState::Running | CliState::RequiresAction)
            )
    }

    /// Clears the busy note: a turn ended, or the CLI went idle.
    pub(in crate::chat) fn reload_idle(&mut self, cx: &mut GpuiContext<Self>) {
        // A turn still runs (the CLI idle beside a streaming chat, or a queued turn
        // starting at once): the note stays true.
        if self.reload_is_busy() {
            return;
        }
        if self.session_panel.reload.busy {
            self.session_panel.reload.busy = false;
            cx.notify();
        }
    }

    /// Runs the reload row's action: Reload, or Reload anyway while one is held.
    pub(in crate::chat) fn activate_reload(&mut self, cx: &mut GpuiContext<Self>) -> Result<(), String> {
        let reload = &self.session_panel.reload;
        if reload.retired {
            return Err("this Claude Code does not reload plugins".to_string());
        }
        if reload.running {
            return Err("a reload is already running".to_string());
        }
        // The row is drawn only once the servers were read: the debug verb follows it.
        if !self.session_panel.connectors.is_read() {
            return Err("the reload row is not shown yet".to_string());
        }
        let Some(control) = self.client.as_ref().and_then(|client| client.reload_control()) else {
            return Err("this session has no reload".to_string());
        };
        if self.reload_is_busy() {
            self.session_panel.reload.busy = true;
            cx.notify();
            return Ok(());
        }
        let reload = &mut self.session_panel.reload;
        let hold = reload.held.is_none();
        reload.running = true;
        reload.busy = false;
        reload.message.clear();
        reload.task = Some(cx.spawn(async move |this, cx| {
            let first = {
                let control = control.clone();
                cx.background_spawn(async move { control.plugins(hold) }).await
            };
            let mut refresh = false;
            let outcome = match first {
                Ok(done) if done.held => Outcome::Held(done.impact),
                Ok(done) => {
                    refresh = true;
                    // A turn can begin while the plugins reload, and the skills change
                    // the tools a running turn is using.
                    // The note is set here, in the update that reads the gate, so a turn
                    // ending during the awaits below finds it and clears it.
                    let Ok(busy) = this.update(cx, |chat, _| {
                        let busy = chat.reload_is_busy();
                        if busy {
                            chat.session_panel.reload.busy = true;
                        }
                        busy
                    }) else {
                        return;
                    };
                    let skills = if busy {
                        Ok(())
                    } else {
                        let control = control.clone();
                        cx.background_spawn(async move { control.skills() }).await
                    };
                    Outcome::Applied { error_count: done.error_count, skills }
                }
                Err(error) => Outcome::Failed(error),
            };
            let answer = if refresh {
                let Ok(mcp) = this.update(cx, |chat, _| {
                    chat.client.as_ref().and_then(|client| client.mcp_control())
                }) else {
                    return;
                };
                match mcp {
                    Some(mcp) => Some(cx.background_spawn(async move { mcp.refresh_status() }).await),
                    None => None,
                }
            } else {
                None
            };
            let _ = this.update(cx, |chat, cx| {
                let reload = &mut chat.session_panel.reload;
                reload.running = false;
                match outcome {
                    Outcome::Held(impact) => reload.held = Some(impact),
                    Outcome::Applied { error_count, skills } => {
                        reload.held = None;
                        let mut parts = Vec::new();
                        if error_count > 0 {
                            parts.push(plugin_errors(error_count));
                        }
                        match skills {
                            Ok(()) => {}
                            Err(ReloadError::Busy) => reload.busy = true,
                            Err(ReloadError::Refused) => reload.retired = true,
                            Err(ReloadError::Failed(text)) => {
                                parts.push(format!("Reload failed: {}", visible_text(&text, false)));
                            }
                        }
                        reload.message = parts.join(" ");
                    }
                    Outcome::Failed(ReloadError::Busy) => reload.busy = true,
                    Outcome::Failed(ReloadError::Refused) => reload.retired = true,
                    Outcome::Failed(ReloadError::Failed(text)) => {
                        reload.message = format!("Reload failed: {}", visible_text(&text, false));
                    }
                }
                match answer {
                    Some(list) => chat.connectors_changed(list, cx),
                    None => cx.notify(),
                }
            });
        }));
        cx.notify();
        Ok(())
    }
}
