//! The answer to `reload_plugins`, reduced to what a surface shows.
use serde_json::Value;

/// What a held reload would change in the session's tool list. Every name
/// is plugin-authored text, so a surface treats it as untrusted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheImpact {
    pub servers_added: Vec<String>,
    pub servers_removed: Vec<String>,
    /// `adds`, `may-add`, `removes` or `may-remove`; `None` for no change.
    pub lsp_tool_change: Option<String>,
}

/// A successful `reload_plugins` answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginsReloaded {
    /// The CLI left the session alone because applying the reload would
    /// change its tool list. A CLI that does not know the hold sends no
    /// `held` key and has applied the reload.
    pub held: bool,
    pub impact: CacheImpact,
    /// Plugins that failed to load.
    pub error_count: u64,
}

/// Reads a `reload_plugins` success body. `None` when it is not an object.
#[must_use]
pub fn parse_plugins_reload(payload: &Value) -> Option<PluginsReloaded> {
    let fields = payload.as_object()?;
    let impact = fields.get("cache_impact");
    let names = |key: &str| -> Vec<String> {
        impact
            .and_then(|impact| impact.get(key))
            .and_then(|list| list.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|name| name.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(PluginsReloaded {
        held: fields.get("held").and_then(|held| held.as_bool()) == Some(true),
        impact: CacheImpact {
            servers_added: names("mcp_servers_added"),
            servers_removed: names("mcp_servers_removed"),
            lsp_tool_change: impact
                .and_then(|impact| impact.get("lsp_tool_change"))
                .and_then(|change| change.as_str())
                .map(str::to_string),
        },
        error_count: fields
            .get("error_count")
            .and_then(|count| count.as_u64())
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Failure list for `parse_plugins_reload`. It is wrong if:
    // - the floor's answer, which has no `held` key, reads as held;
    // - a held answer loses a name, or the LSP change;
    // - `lsp_tool_change: null` reads as a change;
    // - an answer with no `cache_impact`, or with a missing list in it,
    //   yields `None` instead of an empty impact;
    // - a payload that is not an object reads as a success;
    // - a missing `error_count` is not 0.

    #[test]
    fn the_floor_answer_without_held_is_applied() {
        let floor = json!({"commands": [], "agents": [], "plugins": [], "mcpServers": []});
        let parsed = parse_plugins_reload(&floor);
        assert_eq!(
            parsed,
            Some(PluginsReloaded {
                held: false,
                impact: CacheImpact::default(),
                error_count: 0,
            })
        );
    }

    #[test]
    fn a_held_answer_keeps_every_name_and_the_lsp_change() {
        let held = json!({
            "held": true,
            "error_count": 2,
            "cache_impact": {
                "mcp_servers_added": ["one", "two\u{202e}"],
                "mcp_servers_removed": ["gone"],
                "lsp_tool_change": "may-add"
            }
        });
        let parsed = parse_plugins_reload(&held).expect("a held answer parses");
        assert!(parsed.held);
        assert_eq!(parsed.error_count, 2);
        assert!(
            parsed.impact.servers_added == ["one", "two\u{202e}"],
            "every added name is kept"
        );
        assert!(
            parsed.impact.servers_removed == ["gone"],
            "the removed name is kept"
        );
        assert_eq!(parsed.impact.lsp_tool_change.as_deref(), Some("may-add"));
        let null_change = json!({"held": true, "cache_impact": {"lsp_tool_change": null}});
        let parsed = parse_plugins_reload(&null_change).expect("a null change parses");
        assert!(parsed.impact.lsp_tool_change.is_none());
    }

    #[test]
    fn a_partial_or_missing_impact_reads_as_empty() {
        let missing = parse_plugins_reload(&json!({"held": true})).expect("parses");
        assert!(missing.impact == CacheImpact::default(), "a missing impact is empty");
        let partial = json!({"held": true, "cache_impact": {"mcp_servers_added": ["a"]}});
        let partial = parse_plugins_reload(&partial).expect("parses");
        assert!(partial.impact.servers_added == ["a"], "the given list is kept");
        assert!(partial.impact.servers_removed.is_empty());
    }

    #[test]
    fn a_payload_that_is_not_an_object_is_unreadable() {
        assert!(parse_plugins_reload(&json!("ok")).is_none());
        assert!(parse_plugins_reload(&json!([1])).is_none());
        assert!(parse_plugins_reload(&json!(null)).is_none());
    }
}
