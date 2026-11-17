//! The Permissions card: the rules Claude Code is running under, as its
//! `/permissions` lists them, grouped by what they do. Read-only: it shows
//! the CLI's answer and edits nothing. Native Claude chats only.
//!
//! The card reads a cached answer. The ask waits for the CLI, so it runs
//! through a `RulesControl` handle inside `background_spawn`, one at a time:
//! a trigger during an ask leaves one follow-up for when it lands.

use super::{Row, Section};
use crate::chat::Chat;
use crate::chat::visible_text::visible_text;
use gpui::{AppContext as _, Context as GpuiContext, Task};
use sirio_chat::{PermissionRule, PermissionRules, is_refusal, source_words};
use std::collections::HashSet;

/// The groups in the order the card draws them, with their labels.
const GROUPS: [(&str, &str); 3] = [("allow", "Allow"), ("ask", "Ask"), ("deny", "Deny")];

#[derive(Default)]
pub(super) struct Permissions {
    /// The last good answer; `None` until one has been read.
    rules: Option<PermissionRules>,
    /// The CLI's sentence for the last ask that failed for any reason but a
    /// refusal. The last good list stays beside it.
    error: Option<String>,
    /// The CLI refused the request (the 2.1.257 floor): the card is gone
    /// until the next connection.
    retired: bool,
    in_flight: bool,
    /// A trigger came while an ask was open.
    again: bool,
    /// The open ask. Dropping it cancels it, which is how a new connection
    /// never receives the old one's answer.
    task: Option<Task<()>>,
}

impl Permissions {
    /// A new client is a new session: nothing of the last one is current.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

fn tag(rule: &PermissionRule) -> String {
    let mut tag = source_words(&rule.source);
    match rule.editability.as_str() {
        "readonly" => tag.push_str(" · read-only"),
        // The source `session` already says so.
        "session" if rule.source != "session" => tag.push_str(" · this session"),
        _ => {}
    }
    tag
}

/// The card, or `None` for an agent that has no rules to list, before the
/// first answer, and after a refusal. `flipped` holds the groups the reader
/// closed; it is read, never changed.
pub(super) fn section(
    permissions: &Permissions,
    supported: bool,
    flipped: &HashSet<String>,
) -> Option<Section> {
    if !supported || permissions.retired {
        return None;
    }
    if permissions.rules.is_none() && permissions.error.is_none() {
        return None;
    }
    let mut rows = Vec::new();
    let mut counted = 0;
    let mut notes = Vec::new();
    if let Some(list) = &permissions.rules {
        for (behavior, label) in GROUPS {
            let group: Vec<&PermissionRule> = list
                .rules
                .iter()
                .filter(|rule| rule.behavior == behavior)
                .collect();
            if group.is_empty() {
                continue;
            }
            counted += group.len();
            let key = format!("permissions/{behavior}");
            let open = !flipped.contains(&key);
            rows.push(Row {
                expanded: Some(open),
                ..Row::new(&key, label.to_string(), &group.len().to_string())
            });
            if !open {
                continue;
            }
            for (index, rule) in group.into_iter().enumerate() {
                let detail = rule
                    .description
                    .as_deref()
                    .map(|text| visible_text(text, false))
                    .unwrap_or_default();
                rows.push(Row {
                    depth: 1,
                    tag: tag(rule),
                    detail_below: !detail.is_empty(),
                    note: if rule.not_in_effect {
                        "Not in effect: managed settings allow only their own rules".to_string()
                    } else {
                        String::new()
                    },
                    ..Row::new(
                        &format!("permissions/{behavior}/{index}"),
                        visible_text(&rule.rule, false),
                        &detail,
                    )
                });
            }
        }
        if !list.workspace_directories.is_empty() {
            let key = "permissions/directories";
            let open = !flipped.contains(key);
            rows.push(Row {
                expanded: Some(open),
                ..Row::new(
                    key,
                    "Directories".to_string(),
                    &list.workspace_directories.len().to_string(),
                )
            });
            if open {
                for (index, path) in list.workspace_directories.iter().enumerate() {
                    rows.push(Row {
                        depth: 1,
                        ..Row::new(&format!("{key}/{index}"), visible_text(path, false), "")
                    });
                }
            }
        }
        if list.managed_only {
            notes.push("Managed settings allow only their own rules.".to_string());
        }
        for error in &list.errors {
            notes.push(format!(
                "Skipped {}: {}",
                visible_text(&error.file, false),
                visible_text(&error.message, false)
            ));
        }
    }
    if let Some(error) = &permissions.error {
        notes.push(format!(
            "Claude Code's rules could not be read: {}",
            visible_text(error, false)
        ));
    }
    let summary = match (&permissions.rules, counted) {
        (None, _) => String::new(),
        (Some(_), 1) => "1 rule".to_string(),
        (Some(_), n) => format!("{n} rules"),
    };
    Some(Section {
        key: "permissions",
        title: "Permissions",
        summary,
        note: notes.join("\n"),
        bar: Vec::new(),
        rows,
        custom_body: false,
    })
}

impl Chat {
    /// Asks the CLI for its rules, when the panel is open. At most one ask
    /// is open: a trigger during it leaves a single follow-up.
    pub(in crate::chat) fn ask_session_permissions(&mut self, cx: &mut GpuiContext<Self>) {
        if !self.session_panel.open {
            return;
        }
        let Some(control) = self
            .client
            .as_ref()
            .and_then(|client| client.rules_control())
        else {
            return;
        };
        let permissions = &mut self.session_panel.permissions;
        if permissions.retired {
            return;
        }
        if permissions.in_flight {
            permissions.again = true;
            return;
        }
        permissions.in_flight = true;
        permissions.task = Some(cx.spawn(async move |this, cx| {
            let answer = cx.background_spawn(async move { control.list() }).await;
            let _ = this.update(cx, |chat, cx| {
                let permissions = &mut chat.session_panel.permissions;
                permissions.in_flight = false;
                match answer {
                    Ok(rules) => {
                        permissions.rules = Some(rules);
                        permissions.error = None;
                    }
                    Err(error) if is_refusal(&error) => {
                        permissions.retired = true;
                        permissions.rules = None;
                        permissions.error = None;
                        permissions.again = false;
                    }
                    Err(error) => permissions.error = Some(error),
                }
                let again = std::mem::take(&mut permissions.again);
                cx.notify();
                if again {
                    chat.ask_session_permissions(cx);
                }
            });
        }));
    }
}
