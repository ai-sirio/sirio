//! The answer to `get_hooks_listing`: the hooks the CLI loaded, as its
//! read-only `/hooks` menu lists them.
//!
//! Read leniently like the rest of the wire: a field of the wrong JSON type
//! counts as absent. A row's `editable` (its raw configuration) is never
//! read.

use serde_json::Value;

use crate::permissions::{SettingsError, parse_settings_errors};

/// A lifecycle event that has at least one listed hook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookEvent {
    /// The event, such as `PreToolUse`.
    pub name: String,
    /// The CLI's one-line description of when it fires.
    pub summary: String,
}

/// One hook, as the CLI lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    /// The event it fires on.
    pub event: String,
    /// The tool or value pattern it is limited to; empty for all.
    pub matcher: String,
    /// The settings source: `userSettings`, `pluginHook`, ...
    pub source: String,
    /// The CLI's own label for the source.
    pub source_label: String,
    /// The plugin that registered it, on a plugin hook.
    pub plugin_name: Option<String>,
    /// What `command_text` is: `Command`, `Prompt`, `URL`, ...
    pub content_label: String,
    /// The one-line text, which may be the hook's status message.
    pub display_text: String,
    /// The command, prompt, URL or tool, with control characters revealed.
    /// No fallback: `display_text` may be a status message.
    pub command_text: Option<String>,
    /// The hook is listed but does not run.
    pub disabled: bool,
    /// The `if` condition.
    pub condition: Option<String>,
    /// The timeout in seconds, as the number's own text.
    pub timeout: Option<String>,
    /// The hook runs once.
    pub runs_once: bool,
    /// The hook runs in the background.
    pub runs_in_background: bool,
}

/// What the policy and settings say about hooks as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HooksPolicy {
    /// Managed settings turned hooks off.
    pub disabled_by_policy: bool,
    /// Only the organization's hooks run.
    pub managed_only: bool,
    /// The organization limits hooks to plugins: the CLI marks the settings rows disabled.
    pub plugin_only: bool,
    /// `disableAllHooks` is set in the merged settings.
    pub all_disabled: bool,
    /// A managed source exists but could not be read.
    pub unreadable: bool,
}

/// Safe mode or bare mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HooksMode {
    /// How to leave the mode.
    pub exit_hint: String,
    /// The organization's hooks still run (safe mode only).
    pub managed_hooks_still_apply: bool,
}

/// The session's hooks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HooksListing {
    /// Events with at least one hook, in the CLI's lifecycle order.
    pub events: Vec<HookEvent>,
    /// Every hook, in the CLI's order.
    pub hooks: Vec<Hook>,
    /// The organization and settings state.
    pub policy: HooksPolicy,
    /// Present in safe mode.
    pub safe_mode: Option<HooksMode>,
    /// Present in bare mode.
    pub bare_mode: Option<HooksMode>,
    /// Settings files that were skipped.
    pub errors: Vec<SettingsError>,
}

/// Reads a `get_hooks_listing` success payload. `None` unless `hooks` is an
/// array.
#[must_use]
pub fn parse_hooks_listing(payload: &Value) -> Option<HooksListing> {
    let rows = payload.get("hooks")?.as_array()?;
    let hooks = rows.iter().filter_map(parse_hook).collect();
    let events = payload
        .get("events")
        .and_then(Value::as_array)
        .map(|events| {
            events
                .iter()
                .filter_map(|event| {
                    Some(HookEvent {
                        name: event.get("name")?.as_str()?.to_string(),
                        summary: text(event, "summary").unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let policy = payload
        .get("policy")
        .filter(|policy| policy.is_object())
        .map(|policy| HooksPolicy {
            disabled_by_policy: flag(policy, "disabledByPolicy"),
            managed_only: flag(policy, "managedOnly"),
            plugin_only: flag(policy, "pluginOnly"),
            all_disabled: flag(policy, "allDisabled"),
            unreadable: flag(policy, "policyUnreadable"),
        })
        .unwrap_or_default();
    Some(HooksListing {
        events,
        hooks,
        policy,
        safe_mode: mode(payload, "safeMode"),
        bare_mode: mode(payload, "bareMode"),
        errors: parse_settings_errors(payload),
    })
}

fn parse_hook(row: &Value) -> Option<Hook> {
    let event = text(row, "event").filter(|event| !event.is_empty())?;
    let display = text(row, "displayText");
    let command = text(row, "commandText");
    if display.is_none() && command.is_none() {
        return None;
    }
    Some(Hook {
        event,
        matcher: text(row, "matcher").unwrap_or_default(),
        source: text(row, "source").unwrap_or_default(),
        source_label: text(row, "sourceLabel").unwrap_or_default(),
        plugin_name: text(row, "pluginName"),
        content_label: text(row, "contentLabel").unwrap_or_default(),
        display_text: display.or_else(|| command.clone()).unwrap_or_default(),
        command_text: command,
        disabled: flag(row, "disabled"),
        condition: text(row, "condition"),
        timeout: row
            .get("timeout")
            .and_then(Value::as_number)
            .filter(|number| {
                number
                    .as_f64()
                    .is_some_and(|seconds| seconds >= 0.0 && seconds.is_sign_positive())
            })
            .map(ToString::to_string),
        runs_once: flag(row, "runsOnce"),
        runs_in_background: flag(row, "runsInBackground"),
    })
}

fn mode(payload: &Value, key: &str) -> Option<HooksMode> {
    let mode = payload.get(key).filter(|mode| mode.is_object())?;
    Some(HooksMode {
        exit_hint: text(mode, "exitHint").unwrap_or_default(),
        managed_hooks_still_apply: flag(mode, "managedHooksStillApply"),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
