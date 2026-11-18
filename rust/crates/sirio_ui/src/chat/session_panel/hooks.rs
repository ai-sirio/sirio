//! The Hooks card: the hooks Claude Code loaded for the session, as its
//! `/hooks` lists them, grouped by event. Read-only: it shows the CLI's
//! answer and edits nothing. Native Claude chats only.
//!
//! The card reads a cached answer. The ask waits for the CLI, so it runs
//! through a `HooksControl` handle inside `background_spawn`, one at a time:
//! a trigger during an ask leaves one follow-up for when it lands. The card
//! never reads a row's raw configuration, so nothing of it can reach the
//! snapshot.

use super::{Row, Section};
use crate::chat::Chat;
use crate::chat::visible_text::visible_text;
use gpui::{AppContext as _, Context as GpuiContext, Task};
use sirio_chat::{Hook, HooksListing, is_refusal};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Hooks {
    /// The last good answer; `None` until one has been read.
    listing: Option<HooksListing>,
    /// The CLI's sentence for the last ask that failed for any reason but a
    /// refusal. The last good listing stays beside it.
    error: Option<String>,
    /// The CLI refused the request: the card is gone until the next
    /// connection.
    retired: bool,
    in_flight: bool,
    /// A trigger came while an ask was open.
    again: bool,
    /// The open ask. Dropping it cancels it, which is how a new connection
    /// never receives the old one's answer.
    task: Option<Task<()>>,
}

impl Hooks {
    /// A new client is a new session: nothing of the last one is current.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// The word a hook's source goes by in the tag.
fn source_word(hook: &Hook) -> String {
    let word = match hook.source.as_str() {
        "userSettings" => "User".to_string(),
        "projectSettings" => "Project".to_string(),
        "localSettings" => "Local".to_string(),
        "flagSettings" => "Startup".to_string(),
        "policySettings" => "Managed".to_string(),
        "pluginHook" => match hook.plugin_name.as_deref() {
            Some(name) if !name.is_empty() => format!("Plugin {name}"),
            _ => "Plugin".to_string(),
        },
        "sessionHook" => "Session".to_string(),
        "builtinHook" => "Built-in".to_string(),
        _ if !hook.source_label.is_empty() => hook.source_label.clone(),
        other => other.to_string(),
    };
    visible_text(&word, false)
}

/// The text under an open hook row. The command, which the CLI's user
/// controls, comes last so it cannot pose as the lines above it.
fn body(hook: &Hook) -> String {
    let mut lines = Vec::new();
    if !hook.source_label.is_empty() {
        lines.push(format!("Source: {}", visible_text(&hook.source_label, false)));
    }
    let mut options = Vec::new();
    if let Some(condition) = &hook.condition {
        options.push(format!("if {}", visible_text(condition, false)));
    }
    if let Some(timeout) = &hook.timeout {
        options.push(format!("timeout {} s", visible_text(timeout, false)));
    }
    if hook.runs_once {
        options.push("runs once".to_string());
    }
    if hook.runs_in_background {
        options.push("runs in the background".to_string());
    }
    if !options.is_empty() {
        lines.push(options.join(" · "));
    }
    if let Some(command) = &hook.command_text {
        let command = visible_text(command, true);
        if hook.content_label.is_empty() {
            lines.push(command);
        } else {
            lines.push(format!("{}: {command}", visible_text(&hook.content_label, false)));
        }
    }
    lines.join("\n")
}

/// The events that have hooks: those `events[]` lists, in its order, then
/// the others in first-seen order.
fn event_order(listing: &HooksListing) -> Vec<&str> {
    let mut order: Vec<&str> = Vec::new();
    for event in &listing.events {
        if listing.hooks.iter().any(|hook| hook.event == event.name)
            && !order.contains(&event.name.as_str())
        {
            order.push(&event.name);
        }
    }
    for hook in &listing.hooks {
        if !order.contains(&hook.event.as_str()) {
            order.push(&hook.event);
        }
    }
    order
}

fn notes(listing: &HooksListing) -> Vec<String> {
    let mut notes = Vec::new();
    let policy = &listing.policy;
    if policy.disabled_by_policy {
        notes.push(
            "Your organization turned hooks off: the hooks marked disabled do not run."
                .to_string(),
        );
    } else if policy.all_disabled {
        notes.push("disableAllHooks is set: the hooks marked disabled do not run.".to_string());
    }
    if policy.managed_only {
        notes.push(
            "Only your organization's hooks run; hooks from settings files are not listed."
                .to_string(),
        );
    }
    if policy.plugin_only {
        notes.push(
            "Your organization limits hooks to plugins: the hooks marked disabled do not run."
                .to_string(),
        );
    }
    if policy.unreadable {
        notes.push("Your organization's settings could not be read.".to_string());
    }
    if let Some(mode) = &listing.safe_mode {
        let hint = visible_text(&mode.exit_hint, false);
        notes.push(if mode.managed_hooks_still_apply {
            format!(
                "Safe mode: the hooks marked disabled do not run; your organization's still \
                 do. To leave it, {hint}."
            )
        } else {
            format!("Safe mode: the hooks marked disabled do not run. To leave it, {hint}.")
        });
    }
    if let Some(mode) = &listing.bare_mode {
        notes.push(format!(
            "Bare mode: the hooks marked disabled do not run. To leave it, {}.",
            visible_text(&mode.exit_hint, false)
        ));
    }
    for error in &listing.errors {
        notes.push(format!(
            "Skipped {}: {}",
            visible_text(&error.file, false),
            visible_text(&error.message, false)
        ));
    }
    notes
}

/// The card, or `None` for an agent that has no hooks to list, before the
/// first answer, and after a refusal. `flipped` holds the nodes whose fold
/// is the opposite of their default; it is read, never changed.
pub(super) fn section(
    hooks: &Hooks,
    supported: bool,
    flipped: &HashSet<String>,
) -> Option<Section> {
    if !supported || hooks.retired {
        return None;
    }
    if hooks.listing.is_none() && hooks.error.is_none() {
        return None;
    }
    let mut rows = Vec::new();
    let mut notes_out = Vec::new();
    if let Some(listing) = &hooks.listing {
        for event in event_order(listing) {
            let group: Vec<&Hook> =
                listing.hooks.iter().filter(|hook| hook.event == event).collect();
            let shown = visible_text(event, false);
            let key = format!("hooks/{}", shown.replace('/', "%2F"));
            let summary = listing
                .events
                .iter()
                .find(|known| known.name == event)
                .map(|known| visible_text(&known.summary, false))
                .unwrap_or_default();
            let open = !flipped.contains(&key);
            rows.push(Row {
                expanded: Some(open),
                tag: group.len().to_string(),
                detail_below: true,
                ..Row::new(&key, shown, &summary)
            });
            if !open {
                continue;
            }
            for (index, hook) in group.into_iter().enumerate() {
                let row_key = format!("{key}/{index}");
                let unfolded = flipped.contains(&row_key);
                let mut tag = source_word(hook);
                if hook.disabled {
                    tag.push_str(" · disabled");
                }
                rows.push(Row {
                    depth: 1,
                    tag,
                    tag_below: true,
                    detail_below: true,
                    expanded: Some(unfolded),
                    note: if unfolded { body(hook) } else { String::new() },
                    ..Row::new(
                        &row_key,
                        visible_text(&hook.display_text, false),
                        &visible_text(&hook.matcher, false),
                    )
                });
            }
        }
        notes_out = notes(listing);
    }
    if let Some(error) = &hooks.error {
        notes_out.push(format!(
            "Claude Code's hooks could not be read: {}",
            visible_text(error, false)
        ));
    }
    let summary = match &hooks.listing {
        None => String::new(),
        Some(listing) => match listing.hooks.len() {
            0 => "No hooks".to_string(),
            1 => "1 hook".to_string(),
            n => format!("{n} hooks"),
        },
    };
    Some(Section {
        key: "hooks",
        title: "Hooks",
        summary,
        note: notes_out.join("\n"),
        bar: Vec::new(),
        rows,
        custom_body: false,
    })
}

impl Chat {
    /// Asks the CLI for its hooks, when the panel is open. At most one ask
    /// is open: a trigger during it leaves a single follow-up.
    pub(in crate::chat) fn ask_session_hooks(&mut self, cx: &mut GpuiContext<Self>) {
        if !self.session_panel.open {
            return;
        }
        let Some(control) = self
            .client
            .as_ref()
            .and_then(|client| client.hooks_control())
        else {
            return;
        };
        let hooks = &mut self.session_panel.hooks;
        if hooks.retired {
            return;
        }
        if hooks.in_flight {
            hooks.again = true;
            return;
        }
        hooks.in_flight = true;
        hooks.task = Some(cx.spawn(async move |this, cx| {
            let answer = cx.background_spawn(async move { control.list() }).await;
            let _ = this.update(cx, |chat, cx| {
                let hooks = &mut chat.session_panel.hooks;
                hooks.in_flight = false;
                match answer {
                    Ok(listing) => {
                        hooks.listing = Some(listing);
                        hooks.error = None;
                    }
                    Err(error) if is_refusal(&error) => {
                        hooks.retired = true;
                        hooks.listing = None;
                        hooks.error = None;
                        hooks.again = false;
                    }
                    Err(error) => hooks.error = Some(error),
                }
                let again = std::mem::take(&mut hooks.again);
                cx.notify();
                if again {
                    chat.ask_session_hooks(cx);
                }
            });
        }));
    }
}
