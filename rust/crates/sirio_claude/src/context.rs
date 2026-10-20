//! What is in the context window: the `get_context_usage` breakdown.
//!
//! [`crate::usage::ContextUsageReport`] keeps the three numbers a meter needs.
//! This module keeps the rest of the answer (categories, memory files, skills,
//! agents, MCP tools, the message breakdown) and folds it by *origin*, the
//! question a person asks of a full window: what can I take out?
//!
//! Like everything in this crate it degrades rather than rejects: a field the
//! CLI stops sending removes its rows, an entry that is not shaped as expected
//! is dropped, and only a payload with no window gives `None`.

use std::path::PathBuf;

use serde_json::Value;

/// The breakdown of the context a `get_context_usage` answer carried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextBreakdown {
    pub total_tokens: u64,
    pub raw_max_tokens: u64,
    pub categories: Vec<ContextCategory>,
    pub memory_files: Vec<MemoryFile>,
    /// From `skills.skillFrontmatter`.
    pub skills: Vec<ContextItem>,
    /// From `agents[]`.
    pub agents: Vec<ContextItem>,
    /// From `mcpTools[]`; the origin is the server's name when it gave one.
    pub mcp_tools: Vec<ContextItem>,
    /// `messageBreakdown`: tool calls, tool results, attachments by type, ...
    pub messages: Vec<(String, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextCategory {
    pub id: String,
    pub name: String,
    pub tokens: u64,
    pub kind: CategoryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CategoryKind {
    Used,
    Deferred,
    Buffer,
    Free,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryFile {
    pub path: PathBuf,
    /// The CLI's own word: `User`, `Project`, `AutoMem`, ...
    pub kind: String,
    pub tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextItem {
    pub name: String,
    pub origin: ItemOrigin,
    pub path: Option<PathBuf>,
    pub tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemOrigin {
    Plugin(String),
    User,
    Project,
    Synced,
    Other(String),
}

/// The breakdown folded by origin: what can be reduced, and what is fixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextOrigins {
    pub reducible: Vec<OriginNode>,
    pub fixed: Vec<OriginNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginNode {
    /// Stable and unique within the tree, e.g. `plugins/plugin-dev`.
    pub key: String,
    pub label: String,
    pub tokens: u64,
    pub path: Option<PathBuf>,
    pub tag: Option<String>,
    /// False for what does not take room in the window: deferred tools and
    /// free space.
    pub counted: bool,
    pub children: Vec<OriginNode>,
}

impl ContextBreakdown {
    /// Reads the `get_context_usage` answer. `None` when it carries no window
    /// to measure against, the same rule as [`crate::ContextUsageReport`];
    /// every other field degrades on its own.
    #[must_use]
    pub fn parse(payload: &Value) -> Option<Self> {
        let raw_max_tokens = payload.get("rawMaxTokens").and_then(Value::as_u64)?;
        if raw_max_tokens == 0 {
            return None;
        }
        Some(Self {
            total_tokens: payload
                .get("totalTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            raw_max_tokens,
            categories: entries(payload.get("categories"), parse_category),
            memory_files: entries(payload.get("memoryFiles"), parse_memory_file),
            skills: entries(
                payload
                    .get("skills")
                    .and_then(|skills| skills.get("skillFrontmatter")),
                |entry| parse_item(entry, "name"),
            ),
            agents: entries(payload.get("agents"), |entry| {
                parse_item(entry, "agentType")
            }),
            mcp_tools: entries(payload.get("mcpTools"), parse_mcp_tool),
            messages: parse_messages(payload.get("messageBreakdown")),
        })
    }

    /// Folds the breakdown by origin: what the user can reduce, and what is
    /// fixed. Each group is the CLI's own category total; what the items do
    /// not explain is an explicit `Other` child, and items that add up to more
    /// than their category raise the group instead.
    #[must_use]
    pub fn origins(&self) -> ContextOrigins {
        let mut reducible = Vec::new();
        let mut fixed = Vec::new();

        if let Some(category) = self.category("memory_files") {
            reducible.push(self.memory_group(category));
        }
        if let Some(group) = self.plugins_group() {
            reducible.push(group);
        }
        if let Some(category) = self.category("mcp_tools") {
            reducible.push(self.mcp_group(category));
        }
        if let Some(category) = self.category("messages") {
            reducible.push(self.conversation_group(category));
        }

        if let Some(group) = self.claude_code_group() {
            fixed.push(group);
        }
        let folded = [
            "system_prompt",
            "system_tools",
            "memory_files",
            "skills",
            "custom_agents",
            "mcp_tools",
            "messages",
        ];
        for category in &self.categories {
            let deferred = category.kind == CategoryKind::Deferred;
            if !deferred && folded.contains(&category.id.as_str()) {
                continue;
            }
            let counted = !matches!(category.kind, CategoryKind::Deferred | CategoryKind::Free);
            let key = match category.id.as_str() {
                id if !counted => id.to_string(),
                id @ "autocompact_buffer" => id.to_string(),
                id => format!("other:{id}"),
            };
            fixed.push(leaf(&key, &category.name, category.tokens, counted));
        }

        if self.categories.is_empty() {
            fixed.push(leaf("context", "Context", self.total_tokens, true));
        }
        sort_nodes(&mut reducible);
        sort_nodes(&mut fixed);
        ContextOrigins { reducible, fixed }
    }

    fn category(&self, id: &str) -> Option<&ContextCategory> {
        self.categories
            .iter()
            .find(|c| c.id == id && c.kind != CategoryKind::Deferred)
    }

    fn memory_group(&self, category: &ContextCategory) -> OriginNode {
        let children = self
            .memory_files
            .iter()
            .map(|file| OriginNode {
                key: format!("memory/{}", file.path.display()),
                label: file.path.file_name().map_or_else(
                    || file.path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                ),
                tokens: file.tokens,
                path: Some(file.path.clone()),
                tag: (!file.kind.is_empty()).then(|| file.kind.clone()),
                counted: true,
                children: Vec::new(),
            })
            .collect();
        group("memory", "Memory files", category.tokens, children)
    }

    fn plugins_group(&self) -> Option<OriginNode> {
        let skills = self.category("skills");
        let agents = self.category("custom_agents");
        if skills.is_none() && agents.is_none() {
            return None;
        }
        let tokens = skills.map_or(0, |c| c.tokens) + agents.map_or(0, |c| c.tokens);

        // One child per origin, in first-seen order; sorting happens last.
        let mut origins: Vec<OriginNode> = Vec::new();
        let mut counts: Vec<(usize, usize)> = Vec::new();
        let items = self
            .skills
            .iter()
            .map(|item| (item, "skill"))
            .chain(self.agents.iter().map(|item| (item, "agent")));
        for (item, kind) in items {
            let (origin_key, origin_label) = origin_key_label(&item.origin);
            let at = match origins.iter().position(|o| o.key == origin_key) {
                Some(at) => at,
                None => {
                    origins.push(OriginNode {
                        key: origin_key,
                        label: origin_label,
                        tokens: 0,
                        path: None,
                        tag: None,
                        counted: true,
                        children: Vec::new(),
                    });
                    counts.push((0, 0));
                    origins.len() - 1
                }
            };
            let name = match &item.origin {
                ItemOrigin::Plugin(plugin) => item
                    .name
                    .strip_prefix(&format!("{plugin}:"))
                    .unwrap_or(&item.name),
                _ => &item.name,
            };
            let origin = &mut origins[at];
            origin.tokens += item.tokens;
            if kind == "skill" {
                counts[at].0 += 1;
            } else {
                counts[at].1 += 1;
            }
            origin.children.push(OriginNode {
                key: format!("{}/{kind}:{name}", origin.key),
                label: name.to_string(),
                tokens: item.tokens,
                path: item.path.clone(),
                tag: Some(kind.to_string()),
                counted: true,
                children: Vec::new(),
            });
        }
        for (origin, (skills, agents)) in origins.iter_mut().zip(counts) {
            origin.tag = Some(count_tag(skills, agents));
            unique_keys(&mut origin.children);
        }
        Some(group("plugins", "Plugins and skills", tokens, origins))
    }

    fn mcp_group(&self, category: &ContextCategory) -> OriginNode {
        let mut servers: Vec<(String, Vec<OriginNode>)> = Vec::new();
        for tool in &self.mcp_tools {
            let server = match &tool.origin {
                ItemOrigin::Other(name) => name.clone(),
                _ => String::new(),
            };
            let at = servers.iter().position(|(name, _)| *name == server);
            let at = at.unwrap_or_else(|| {
                servers.push((server.clone(), Vec::new()));
                servers.len() - 1
            });
            let key = format!("mcp/{server}/{}", tool.name);
            servers[at]
                .1
                .push(leaf(&key, &tool.name, tool.tokens, true));
        }
        let children = servers
            .into_iter()
            .map(|(server, mut tools)| {
                unique_keys(&mut tools);
                OriginNode {
                    key: format!("mcp/{server}"),
                    label: if server.is_empty() {
                        "Other".to_string()
                    } else {
                        server
                    },
                    tokens: tools.iter().map(|t| t.tokens).sum(),
                    path: None,
                    tag: Some(match tools.len() {
                        1 => "1 tool".to_string(),
                        n => format!("{n} tools"),
                    }),
                    counted: true,
                    children: tools,
                }
            })
            .collect();
        group("mcp", "MCP tools", category.tokens, children)
    }

    fn conversation_group(&self, category: &ContextCategory) -> OriginNode {
        let children = self
            .messages
            .iter()
            .map(|(name, tokens)| leaf(&format!("conversation/{name}"), name, *tokens, true))
            .collect();
        group("conversation", "Conversation", category.tokens, children)
    }

    fn claude_code_group(&self) -> Option<OriginNode> {
        let children: Vec<OriginNode> = ["system_prompt", "system_tools"]
            .iter()
            .filter_map(|id| self.category(id))
            .map(|c| leaf(&format!("claude_code/{}", c.id), &c.name, c.tokens, true))
            .collect();
        if children.is_empty() {
            return None;
        }
        let tokens = children.iter().map(|c| c.tokens).sum();
        Some(group("claude_code", "Claude Code", tokens, children))
    }
}

/// Reads each element of an array with `parse`, dropping what it rejects.
fn entries<T>(value: Option<&Value>, parse: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    value
        .and_then(Value::as_array)
        .map(|array| array.iter().filter_map(parse).collect())
        .unwrap_or_default()
}

fn text(entry: &Value, field: &str) -> Option<String> {
    entry.get(field).and_then(Value::as_str).map(str::to_string)
}

fn parse_category(entry: &Value) -> Option<ContextCategory> {
    let id = text(entry, "id")?;
    let tokens = entry.get("tokens").and_then(Value::as_u64)?;
    let kind = match text(entry, "kind").as_deref() {
        _ if entry.get("isDeferred").and_then(Value::as_bool) == Some(true) => {
            CategoryKind::Deferred
        }
        Some("used") | None => CategoryKind::Used,
        Some("deferred") => CategoryKind::Deferred,
        Some("buffer") => CategoryKind::Buffer,
        Some("free") => CategoryKind::Free,
        Some(other) => CategoryKind::Other(other.to_string()),
    };
    Some(ContextCategory {
        name: text(entry, "name").unwrap_or_else(|| id.clone()),
        id,
        tokens,
        kind,
    })
}

fn parse_memory_file(entry: &Value) -> Option<MemoryFile> {
    Some(MemoryFile {
        path: PathBuf::from(text(entry, "path")?),
        kind: text(entry, "type").unwrap_or_default(),
        tokens: entry.get("tokens").and_then(Value::as_u64)?,
    })
}

/// A skill or an agent: the same fields, the name under a different key.
fn parse_item(entry: &Value, name_field: &str) -> Option<ContextItem> {
    Some(ContextItem {
        name: text(entry, name_field)?,
        origin: parse_origin(entry),
        path: text(entry, "path").map(PathBuf::from),
        tokens: entry.get("tokens").and_then(Value::as_u64)?,
    })
}

fn parse_origin(entry: &Value) -> ItemOrigin {
    match text(entry, "source").as_deref() {
        Some("userSettings") => ItemOrigin::User,
        Some("projectSettings") => ItemOrigin::Project,
        Some("syncedSkills") => ItemOrigin::Synced,
        Some("plugin") => {
            let name = text(entry, "pluginName").or_else(|| {
                text(entry, "pluginId")
                    .map(|id| id.split('@').next().unwrap_or_default().to_string())
            });
            match name.filter(|name| !name.is_empty()) {
                Some(name) => ItemOrigin::Plugin(name),
                None => ItemOrigin::Other("plugin".to_string()),
            }
        }
        Some(other) => ItemOrigin::Other(other.to_string()),
        None => ItemOrigin::Other(String::new()),
    }
}

fn parse_mcp_tool(entry: &Value) -> Option<ContextItem> {
    Some(ContextItem {
        name: text(entry, "name")?,
        origin: ItemOrigin::Other(text(entry, "serverName").unwrap_or_default()),
        path: None,
        tokens: entry.get("tokens").and_then(Value::as_u64)?,
    })
}

fn parse_messages(breakdown: Option<&Value>) -> Vec<(String, u64)> {
    let Some(breakdown) = breakdown else {
        return Vec::new();
    };
    let number = |field: &str| breakdown.get(field).and_then(Value::as_u64);
    let mut rows: Vec<(String, u64)> = [
        ("Tool calls", "toolCallTokens"),
        ("Tool results", "toolResultTokens"),
        ("Assistant messages", "assistantMessageTokens"),
        ("User messages", "userMessageTokens"),
    ]
    .iter()
    .filter_map(|(label, field)| Some((label.to_string(), number(field)?)))
    .collect();
    let by_type = entries(breakdown.get("attachmentsByType"), |entry| {
        Some((
            text(entry, "name")?,
            entry.get("tokens").and_then(Value::as_u64)?,
        ))
    });
    if by_type.is_empty() {
        rows.extend(number("attachmentTokens").map(|tokens| ("Attachments".to_string(), tokens)));
    } else {
        rows.extend(by_type);
    }
    rows.retain(|(_, tokens)| *tokens > 0);
    rows
}

fn origin_key_label(origin: &ItemOrigin) -> (String, String) {
    match origin {
        ItemOrigin::Plugin(name) => (format!("plugins/{name}"), name.clone()),
        ItemOrigin::User => ("plugins/source:user".to_string(), "User".to_string()),
        ItemOrigin::Project => ("plugins/source:project".to_string(), "Project".to_string()),
        ItemOrigin::Synced => ("plugins/source:synced".to_string(), "Synced".to_string()),
        ItemOrigin::Other(source) => (
            format!("plugins/source:other:{source}"),
            if source.is_empty() {
                "Other".to_string()
            } else {
                source.clone()
            },
        ),
    }
}

fn count_tag(skills: usize, agents: usize) -> String {
    let noun = |n: usize, one: &str, many: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    };
    match (skills, agents) {
        (0, agents) => noun(agents, "agent", "agents"),
        (skills, 0) => noun(skills, "skill", "skills"),
        (skills, agents) => format!(
            "{} · {}",
            noun(skills, "skill", "skills"),
            noun(agents, "agent", "agents")
        ),
    }
}

fn leaf(key: &str, label: &str, tokens: u64, counted: bool) -> OriginNode {
    OriginNode {
        key: key.to_string(),
        label: label.to_string(),
        tokens,
        path: None,
        tag: None,
        counted,
        children: Vec::new(),
    }
}

/// A group whose weight is its category's. Children that do not explain all of
/// it leave an `Other` row; children that explain more raise the group.
fn group(
    key: &str,
    label: &str,
    category_tokens: u64,
    mut children: Vec<OriginNode>,
) -> OriginNode {
    unique_keys(&mut children);
    let explained: u64 = children.iter().map(|c| c.tokens).sum();
    if category_tokens > explained {
        children.push(leaf(
            &format!("{key}/other"),
            "Other",
            category_tokens - explained,
            true,
        ));
    }
    OriginNode {
        key: key.to_string(),
        label: label.to_string(),
        tokens: category_tokens.max(explained),
        path: None,
        tag: None,
        counted: true,
        children,
    }
}

/// Two items can share a name; a key is still one row.
fn unique_keys(nodes: &mut [OriginNode]) {
    let mut seen = std::collections::HashSet::new();
    for node in nodes {
        let base = node.key.clone();
        let mut n = 1;
        while !seen.insert(node.key.clone()) {
            n += 1;
            node.key = format!("{base}#{n}");
        }
    }
}

/// Heaviest first, ties by label, at every level.
fn sort_nodes(nodes: &mut [OriginNode]) {
    nodes.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.label.cmp(&b.label)));
    for node in nodes {
        sort_nodes(&mut node.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node<'a>(nodes: &'a [OriginNode], key: &str) -> &'a OriginNode {
        nodes
            .iter()
            .find(|n| n.key == key)
            .unwrap_or_else(|| panic!("no node {key} in {:?}", keys(nodes)))
    }

    fn keys(nodes: &[OriginNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.key.as_str()).collect()
    }

    fn category(id: &str, name: &str, tokens: u64, kind: &str) -> serde_json::Value {
        json!({"id": id, "name": name, "tokens": tokens, "kind": kind})
    }

    /// The numbers Claude Code 2.1.304 answered with on the probe machine.
    fn probe_payload() -> serde_json::Value {
        let plugin_dev_skills: Vec<_> = [200, 200, 150, 150, 120, 100, 94, 80]
            .iter()
            .enumerate()
            .map(|(i, tokens)| {
                json!({
                    "name": format!("plugin-dev:skill-{i}"), "source": "plugin",
                    "pluginName": "plugin-dev",
                    "pluginId": "plugin-dev@claude-plugins-official",
                    "path": format!("/plugins/plugin-dev/skills/skill-{i}/SKILL.md"),
                    "tokens": tokens
                })
            })
            .collect();
        let mut skills = plugin_dev_skills;
        skills.push(json!({"name": "prompt-master", "source": "userSettings",
            "path": "/home/u/.claude/skills/prompt-master/SKILL.md", "tokens": 117}));
        skills.push(json!({"name": "synced-one", "source": "syncedSkills", "tokens": 1396}));
        json!({
            "totalTokens": 64923, "rawMaxTokens": 400000,
            "categories": [
                category("system_prompt", "System prompt", 2315, "used"),
                category("system_tools", "System tools", 9740, "used"),
                {"id": "system_tools_deferred", "name": "System tools (deferred)",
                 "tokens": 16426, "isDeferred": true, "kind": "deferred"},
                category("custom_agents", "Custom agents", 4947, "used"),
                category("memory_files", "Memory files", 34233, "used"),
                category("skills", "Skills", 7102, "used"),
                category("messages", "Messages", 6586, "used"),
                category("autocompact_buffer", "Autocompact buffer", 33000, "buffer"),
                category("free_space", "Free space", 302077, "free"),
            ],
            "memoryFiles": [
                {"path": "/home/u/.claude/CLAUDE.md", "type": "User", "tokens": 809},
                {"path": "/repo/CLAUDE.md", "type": "Project", "tokens": 28941},
                {"path": "/home/u/.claude/projects/p/memory/MEMORY.md",
                 "type": "AutoMem", "tokens": 4483},
            ],
            "skills": {"totalSkills": 10, "tokens": 7102, "skillFrontmatter": skills},
            "agents": [
                {"agentType": "plugin-dev:agent-creator", "source": "plugin",
                 "pluginId": "plugin-dev@claude-plugins-official", "tokens": 431},
                {"agentType": "plugin-dev:plugin-validator", "source": "plugin",
                 "pluginId": "plugin-dev@claude-plugins-official", "tokens": 410},
                {"agentType": "plugin-dev:skill-reviewer", "source": "plugin",
                 "pluginId": "plugin-dev@claude-plugins-official", "tokens": 408},
            ],
            "mcpTools": [],
        })
    }

    #[test]
    fn a_report_without_a_window_is_none() {
        assert_eq!(ContextBreakdown::parse(&json!({"totalTokens": 10})), None);
        assert_eq!(
            ContextBreakdown::parse(&json!({"totalTokens": 10, "rawMaxTokens": 0})),
            None
        );
    }

    #[test]
    fn a_probe_shaped_payload_folds_into_origins() {
        let origins = ContextBreakdown::parse(&probe_payload())
            .expect("a breakdown")
            .origins();

        assert_eq!(origins.reducible[0].key, "memory");
        assert_eq!(origins.reducible[0].tokens, 34233);
        let first = &origins.reducible[0].children[0];
        assert_eq!(first.tag.as_deref(), Some("Project"));
        assert_eq!(first.tokens, 28941);

        let plugins = node(&origins.reducible, "plugins");
        let plugin_dev = node(&plugins.children, "plugins/plugin-dev");
        assert_eq!(plugin_dev.tokens, 2343);
        assert_eq!(plugin_dev.tag.as_deref(), Some("8 skills · 3 agents"));
        let agent = node(
            &plugin_dev.children,
            "plugins/plugin-dev/agent:skill-reviewer",
        );
        assert_eq!(agent.tokens, 408);

        // The buffer is the one counted fixed node that is not in the total.
        let counted_used: u64 = origins
            .reducible
            .iter()
            .chain(&origins.fixed)
            .filter(|n| n.counted && n.key != "autocompact_buffer")
            .map(|n| n.tokens)
            .sum();
        assert_eq!(counted_used, 64923);
    }

    #[test]
    fn a_breakdown_with_unitemised_tokens_shows_them_as_other() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 1000, "rawMaxTokens": 10_000,
            "categories": [category("memory_files", "Memory files", 1000, "used")],
            "memoryFiles": [{"path": "/r/CLAUDE.md", "type": "Project", "tokens": 600}],
        }))
        .expect("a breakdown");
        let origins = breakdown.origins();
        let memory = node(&origins.reducible, "memory");
        let tokens: Vec<u64> = memory.children.iter().map(|c| c.tokens).collect();
        assert_eq!(tokens, [600, 400]);
        assert_eq!(memory.children[1].label, "Other");
    }

    #[test]
    fn children_larger_than_their_category_raise_the_group() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 100, "rawMaxTokens": 10_000,
            "categories": [category("memory_files", "Memory files", 100, "used")],
            "memoryFiles": [
                {"path": "/a.md", "type": "User", "tokens": 90},
                {"path": "/b.md", "type": "Project", "tokens": 60},
            ],
        }))
        .expect("a breakdown");
        let origins = breakdown.origins();
        let memory = node(&origins.reducible, "memory");
        assert_eq!(memory.tokens, 150);
        assert_eq!(memory.children.len(), 2, "no Other when nothing is left");
    }

    #[test]
    fn a_payload_without_categories_still_has_a_total() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 5000, "rawMaxTokens": 200_000,
        }))
        .expect("a breakdown");
        let origins = breakdown.origins();
        assert!(origins.reducible.is_empty());
        assert_eq!(origins.fixed.len(), 1);
        assert_eq!(origins.fixed[0].label, "Context");
        assert_eq!(origins.fixed[0].tokens, 5000);
        assert!(origins.fixed[0].counted);
    }

    #[test]
    fn an_unknown_category_is_shown_under_fixed_with_its_own_name() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 300, "rawMaxTokens": 10_000,
            "categories": [category("new_thing", "New thing", 300, "used")],
        }))
        .expect("a breakdown");
        let origins = breakdown.origins();
        let shown = node(&origins.fixed, "other:new_thing");
        assert_eq!(shown.label, "New thing");
        assert_eq!(shown.tokens, 300);
        assert!(shown.counted);
    }

    #[test]
    fn an_item_without_path_or_source_is_kept_as_text() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 50, "rawMaxTokens": 10_000,
            "categories": [category("skills", "Skills", 50, "used")],
            "skills": {"skillFrontmatter": [{"name": "lonely", "tokens": 50}]},
        }))
        .expect("a breakdown");
        assert_eq!(breakdown.skills[0].origin, ItemOrigin::Other(String::new()));
        assert_eq!(breakdown.skills[0].path, None);

        let origins = breakdown.origins();
        let plugins = node(&origins.reducible, "plugins");
        assert_eq!(plugins.children.len(), 1);
        assert_eq!(plugins.children[0].label, "Other");
        assert_eq!(plugins.children[0].children[0].label, "lonely");
        assert_eq!(plugins.children[0].children[0].path, None);
    }

    #[test]
    fn deferred_and_free_are_not_counted() {
        let origins = ContextBreakdown::parse(&probe_payload())
            .expect("a breakdown")
            .origins();
        assert!(!node(&origins.fixed, "system_tools_deferred").counted);
        assert!(!node(&origins.fixed, "free_space").counted);
        assert!(node(&origins.fixed, "claude_code").counted);
    }

    #[test]
    fn malformed_entries_are_skipped_not_fatal() {
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 30, "rawMaxTokens": 10_000,
            "categories": [category("skills", "Skills", 30, "used"), "junk", {"id": "x"}],
            "skills": {"skillFrontmatter": [
                {"name": "bad", "source": "userSettings", "tokens": "x"},
                {"name": "good", "source": "userSettings", "tokens": 30},
            ]},
        }))
        .expect("a breakdown");
        assert_eq!(breakdown.categories.len(), 1);
        assert_eq!(breakdown.skills.len(), 1);
        assert_eq!(breakdown.skills[0].name, "good");
        assert_eq!(breakdown.skills[0].origin, ItemOrigin::User);
    }
}
