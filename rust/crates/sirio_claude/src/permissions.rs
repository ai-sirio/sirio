//! The answer to `list_permission_rules`: the rules the session is
//! running under, as the terminal's `/permissions` lists them.
//!
//! Read leniently like the rest of the wire: a row without the keys a
//! card needs is skipped, an optional key of the wrong type is absent.

use serde_json::Value;

/// One permission rule, as the CLI reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRule {
    /// `allow`, `deny` or `ask`.
    pub behavior: String,
    /// The settings source: `userSettings`, `policySettings`, ...
    pub source: String,
    /// The rule text, verbatim. May carry invisible or control characters.
    pub rule: String,
    /// The terminal's subtitle for the rule: prefix, emphasis and suffix
    /// joined. `None` where the terminal shows none.
    pub description: Option<String>,
    /// `persistent`, `session` or `readonly`.
    pub editability: String,
    /// Managed settings pin the rules, so this one is ignored.
    pub not_in_effect: bool,
}

/// A settings file the CLI skipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    /// The file.
    pub file: String,
    /// The key inside it, empty for a syntax error.
    pub path: String,
    /// What was wrong.
    pub message: String,
}

/// The session's permission rules and workspace directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionRules {
    /// Every rule, in the CLI's order.
    pub rules: Vec<PermissionRule>,
    /// The directories the session may work in. The CLI's `source` label
    /// for a directory is unreliable, so only the path is kept.
    pub workspace_directories: Vec<String>,
    /// Managed settings allow only their own rules.
    pub managed_only: bool,
    /// Settings files that were skipped.
    pub errors: Vec<SettingsError>,
    /// The answer was cut: it holds placeholders, not the real rules.
    pub withheld: bool,
}

/// Reads a `list_permission_rules` success payload. `None` unless
/// `state.rules` is an array.
#[must_use]
pub fn parse_permission_rules(payload: &Value) -> Option<PermissionRules> {
    let state = payload.get("state")?;
    let rows = state.get("rules")?.as_array()?;
    let rules = rows.iter().filter_map(parse_rule).collect();
    let workspace_directories = state
        .get("workspaceDirectories")
        .and_then(Value::as_array)
        .map(|dirs| {
            dirs.iter()
                .filter_map(|dir| dir.get("path")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let errors = state
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| {
                    Some(SettingsError {
                        file: error.get("file")?.as_str()?.to_string(),
                        path: error
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        message: error.get("message")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(PermissionRules {
        rules,
        workspace_directories,
        managed_only: flag(state, "managedOnly"),
        errors,
        withheld: flag(state, "withheld"),
    })
}

fn parse_rule(row: &Value) -> Option<PermissionRule> {
    let field = |key: &str| row.get(key)?.as_str().map(str::to_string);
    Some(PermissionRule {
        behavior: field("behavior")?,
        source: field("source")?,
        rule: field("rule")?,
        description: row.get("description").and_then(description),
        editability: field("editability")?,
        not_in_effect: flag(row, "notInEffect"),
    })
}

fn description(value: &Value) -> Option<String> {
    let part = |key: &str| value.get(key).and_then(Value::as_str);
    let parts = [part("prefix"), part("emphasis"), part("suffix")];
    if parts.iter().all(Option::is_none) {
        return None;
    }
    Some(parts.into_iter().flatten().collect())
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
