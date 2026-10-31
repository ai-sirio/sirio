//! The Context card: what fills the agent's context window, by origin. The
//! CLI's breakdown is folded into an origin tree once, when it arrives, and
//! cached; a render only reads that tree and flattens the part the reader
//! has opened.

use super::{BarSegment, Row, Section};
use crate::chat::Chat;
use gpui::Context as GpuiContext;
use sirio_chat::{ContextBreakdown, ContextOrigins, ContextUsage, OriginNode, Standing};
use std::collections::HashSet;

/// A level with more children than this shows the heaviest and folds the
/// rest into one `N more` row.
const MAX_CHILDREN: usize = 10;

/// What an agent that reports no breakdown says in place of one.
const NOT_REPORTED: &str = "This agent does not report what is in its context";

/// The last breakdown the agent reported, folded.
struct Report {
    used: u64,
    window: u64,
    origins: ContextOrigins,
}

#[derive(Default)]
pub(super) struct Context {
    report: Option<Report>,
    /// Whether the report was asked for since the client came up: it is
    /// asked once, never again on every open.
    asked: bool,
}

impl Context {
    fn set(&mut self, breakdown: &ContextBreakdown) {
        self.report = Some(Report {
            used: breakdown.total_tokens,
            window: breakdown.raw_max_tokens,
            origins: breakdown.origins(),
        });
    }

    /// A new client is a new session: its context is not the last one's.
    pub(super) fn clear(&mut self) {
        self.report = None;
        self.asked = false;
    }
}

impl Chat {
    /// Takes the client's latest breakdown into the card, folding it once.
    pub(in crate::chat) fn refresh_session_context(&mut self, cx: &mut GpuiContext<Self>) {
        if let Some(breakdown) = self.client.as_ref().and_then(|c| c.context_breakdown()) {
            self.session_panel.context.set(&breakdown);
            cx.notify();
        }
    }

    /// When the column is open and nothing has been reported yet, asks the
    /// agent once. Never mid-turn: a turn's end brings its own report.
    pub(in crate::chat) fn ensure_session_context(&mut self) {
        if !self.session_panel.open || self.streaming || self.session_panel.context.report.is_some()
        {
            return;
        }
        let Some(client) = &self.client else {
            return;
        };
        if let Some(breakdown) = client.context_breakdown() {
            self.session_panel.context.set(&breakdown);
        } else if !self.session_panel.context.asked {
            self.session_panel.context.asked = true;
            let _ = client.refresh_context();
        }
    }
}

/// The card, or `None` when the chat has neither a breakdown nor a total.
/// `flipped` holds the nodes whose fold state is the opposite of their
/// default; it is read, never changed, here.
pub(super) fn section(
    context: &Context,
    usage: Option<&ContextUsage>,
    reports_breakdown: bool,
    streaming: bool,
    flipped: &HashSet<String>,
) -> Option<Section> {
    let mut section = Section {
        key: "context",
        title: "Context",
        summary: String::new(),
        note: String::new(),
        bar: Vec::new(),
        rows: Vec::new(),
        custom_body: false,
    };
    if let Some(report) = &context.report {
        section.summary = header(report.used, report.window);
        if streaming {
            section.note = "as of the last turn".to_string();
        }
        let mut build = Build {
            window: report.window,
            flipped,
            rows: Vec::new(),
            bar: Vec::new(),
        };
        build.groups(&report.origins);
        section.rows = build.rows;
        section.bar = build.bar;
    } else if let Some(usage) = usage {
        section.summary = header(usage.used, usage.size);
        if !reports_breakdown {
            section.rows.push(Row::new("context/unreported", NOT_REPORTED.to_string(), ""));
        }
    } else {
        return None;
    }
    Some(section)
}

/// `16% · 64.9k / 400.0k`; a window of unknown size is said, not drawn as 0%.
fn header(used: u64, window: u64) -> String {
    if window == 0 {
        return format!("{} · window unknown", tokens(used));
    }
    let percent = (used as f64 * 100.0 / window as f64).round();
    format!("{percent}% · {} / {}", tokens(used), tokens(window))
}

pub(super) use crate::trace_style::compact_count as tokens;

/// The rows and the bar of one card, built from the folded tree.
struct Build<'a> {
    window: u64,
    flipped: &'a HashSet<String>,
    rows: Vec<Row>,
    bar: Vec<BarSegment>,
}

impl Build<'_> {
    fn groups(&mut self, origins: &ContextOrigins) {
        for (key, title, nodes) in [
            ("reducible", "You can reduce it", &origins.reducible),
            ("fixed", "Fixed", &origins.fixed),
        ] {
            if nodes.is_empty() {
                continue;
            }
            self.rows.push(Row {
                heading: true,
                ..Row::new(&format!("context/group:{key}"), title.to_string(), "")
            });
            for node in nodes {
                self.top_level(node);
            }
        }
    }

    /// A top-level node is one segment of the bar, sized against the window,
    /// and wears that segment's colour; what takes no room has neither.
    fn top_level(&mut self, node: &OriginNode) {
        let swatch = (node.counted && self.window > 0).then(|| {
            let hue = self.bar.len();
            self.bar.push(BarSegment {
                fraction: (node.tokens as f64 / self.window as f64).min(1.0) as f32,
                hue,
            });
            hue
        });
        self.node(node, 0, swatch);
    }

    fn node(&mut self, node: &OriginNode, depth: u8, swatch: Option<usize>) {
        let key = format!("context/{}", node.key);
        // Top-level groups start open, deeper nodes closed.
        let expanded = (!node.children.is_empty())
            .then(|| (depth == 0) != self.flipped.contains(&key));
        self.rows.push(Row {
            depth,
            tag: node.tag.clone().unwrap_or_default(),
            expanded,
            action: node.path.as_ref().map(|_| "open".to_string()),
            path: node.path.clone(),
            muted: !node.counted,
            swatch,
            ..Row::new(&key, node.label.clone(), &tokens(node.tokens))
        });
        if expanded == Some(true) {
            self.children(&node.key, &node.children, depth + 1, !node.counted);
        }
    }

    /// The heaviest ten, then one row that holds the rest, then the
    /// remainder row: it is not an item to fold away whatever it weighs.
    /// Children arrive sorted heaviest first, the remainder after them.
    fn children(&mut self, parent: &str, children: &[OriginNode], depth: u8, muted: bool) {
        let start = self.rows.len();
        let items = children
            .iter()
            .take_while(|child| child.standing != Standing::Remainder)
            .count();
        let (items, remainders) = children.split_at(items);
        let (shown, rest) = items.split_at(items.len().min(MAX_CHILDREN));
        for child in shown {
            self.node(child, depth, None);
        }
        if !rest.is_empty() {
            let key = format!("context/{parent}/more");
            let open = self.flipped.contains(&key);
            let remainder = rest
                .iter()
                .fold(0u64, |sum, node| sum.saturating_add(node.tokens));
            self.rows.push(Row {
                depth,
                expanded: Some(open),
                muted,
                ..Row::new(&key, format!("{} more", rest.len()), &tokens(remainder))
            });
            if open {
                for child in rest {
                    self.node(child, depth + 1, None);
                }
            }
        }
        for child in remainders {
            self.node(child, depth, None);
        }
        if muted {
            for row in &mut self.rows[start..] {
                row.muted = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SectionSnapshot;
    use crate::chat::tests::{offline_chat_view, refresh_frame};
    use crate::chat::{Chat, ChatEvent, SessionPanelAction};
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use serde_json::json;
    use sirio_chat::{ContextBreakdown, ContextUsage};
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    fn category(id: &str, name: &str, tokens: u64, kind: &str) -> serde_json::Value {
        json!({"id": id, "name": name, "tokens": tokens, "kind": kind})
    }

    /// The shape Claude Code answers with, small: three memory files, one
    /// plugin with `skills` skills and three agents, a user skill, hook
    /// output as the conversation.
    fn breakdown(skills: usize) -> ContextBreakdown {
        let mut skill_list: Vec<_> = (0..skills)
            .map(|i| {
                json!({
                    "name": format!("plugin-dev:skill-{i:02}"), "source": "plugin",
                    "pluginName": "plugin-dev", "pluginId": "plugin-dev@official",
                    "path": format!("/plugins/plugin-dev/skills/skill-{i:02}/SKILL.md"),
                    "tokens": 100 + i
                })
            })
            .collect();
        skill_list.push(json!({"name": "prompt-master", "source": "userSettings",
            "path": "/home/u/.claude/skills/prompt-master/SKILL.md", "tokens": 117}));
        let skill_tokens: u64 = 117 + (0..skills as u64).map(|i| 100 + i).sum::<u64>();
        ContextBreakdown::parse(&json!({
            "totalTokens": 64923, "rawMaxTokens": 400000,
            "categories": [
                category("system_prompt", "System prompt", 2315, "used"),
                category("system_tools", "System tools", 9740, "used"),
                {"id": "system_tools_deferred", "name": "System tools (deferred)",
                 "tokens": 16426, "isDeferred": true, "kind": "deferred"},
                category("custom_agents", "Custom agents", 1249, "used"),
                category("memory_files", "Memory files", 34233, "used"),
                category("skills", "Skills", skill_tokens, "used"),
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
            "skills": {"totalSkills": skills + 1, "tokens": skill_tokens,
                       "skillFrontmatter": skill_list},
            "agents": [
                {"agentType": "plugin-dev:agent-creator", "source": "plugin",
                 "pluginId": "plugin-dev@official", "tokens": 431},
                {"agentType": "plugin-dev:plugin-validator", "source": "plugin",
                 "pluginId": "plugin-dev@official", "tokens": 410},
                {"agentType": "plugin-dev:skill-reviewer", "source": "plugin",
                 "pluginId": "plugin-dev@official", "tokens": 408},
            ],
            "mcpTools": [],
            "messageBreakdown": {
                "toolCallTokens": 0, "toolResultTokens": 0, "attachmentTokens": 8731,
                "attachmentsByType": [
                    {"name": "hook_success", "tokens": 6788},
                    {"name": "hook_additional_context", "tokens": 1943},
                ],
            },
        }))
        .expect("a breakdown")
    }

    fn open_with(
        cx: &mut TestAppContext,
        breakdown: Option<ContextBreakdown>,
    ) -> (Entity<Chat>, &mut VisualTestContext) {
        let (chat, cx) = offline_chat_view(cx);
        chat.update(cx, |chat, cx| {
            chat.set_session_panel_open(true, cx);
            if let Some(breakdown) = &breakdown {
                chat.session_panel.context.set(breakdown);
            }
        });
        refresh_frame(cx);
        (chat, cx)
    }

    fn context_section(chat: &Entity<Chat>, cx: &VisualTestContext) -> SectionSnapshot {
        chat.read_with(&cx.cx, |chat, _| chat.control_snapshot())
            .session_panel
            .sections
            .into_iter()
            .find(|section| section.key == "context")
            .expect("a context section")
    }

    fn act(chat: &Entity<Chat>, cx: &mut VisualTestContext, action: SessionPanelAction) {
        chat.update_in(cx, |chat, window, cx| chat.session_panel_act(action, window, cx))
            .expect("the action is accepted");
        refresh_frame(cx);
    }

    fn labels(section: &SectionSnapshot) -> Vec<(u8, &str, &str)> {
        section
            .rows
            .iter()
            .map(|row| (row.depth, row.label.as_str(), row.detail.as_str()))
            .collect()
    }

    #[gpui::test]
    async fn the_context_section_lists_origins_heaviest_first(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, Some(breakdown(5)));
        let section = context_section(&chat, cx);
        assert_eq!(section.summary, "16% · 64.9k / 400.0k");
        let rows = labels(&section);
        assert_eq!(rows[0], (0, "You can reduce it", ""));
        assert_eq!(rows[1], (0, "Memory files", "34.2k"));
        // Memory files are open at the top level, heaviest file first.
        assert_eq!(rows[2].1, "CLAUDE.md");
        assert_eq!(rows[2].2, "28.9k");
        let heading = rows.iter().position(|row| row.1 == "Fixed").expect("a Fixed group");
        let reducible: Vec<&str> = rows[1..heading]
            .iter()
            .filter(|row| row.0 == 0)
            .map(|row| row.1)
            .collect();
        assert_eq!(
            reducible,
            ["Memory files", "Conversation", "Plugins and skills"]
        );
        // Fixed keeps the remainder after Claude Code and free space last,
        // whatever they weigh.
        let fixed: Vec<&str> = rows[heading + 1..]
            .iter()
            .filter(|row| row.0 == 0)
            .map(|row| row.1)
            .collect();
        assert_eq!(
            fixed,
            [
                "Claude Code",
                "Other",
                "System tools (deferred)",
                "Autocompact buffer",
                "Free space"
            ]
        );
        // What takes no room is drawn muted and stays out of the bar.
        let muted = chat.read_with(&cx.cx, |chat, _| {
            chat.session_panel_sections()
                .into_iter()
                .find(|section| section.key == "context")
                .unwrap()
                .rows
                .into_iter()
                .filter(|row| row.muted)
                .map(|row| row.label)
                .collect::<Vec<_>>()
        });
        assert!(muted.iter().any(|label| label == "Autocompact buffer"), "{muted:?}");
    }

    #[gpui::test]
    async fn a_plugin_opens_to_its_skills_and_agents(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, Some(breakdown(5)));
        let closed = context_section(&chat, cx);
        let plugin = closed
            .rows
            .iter()
            .find(|row| row.key == "context/plugins/plugin-dev")
            .expect("the plugin row");
        assert_eq!(plugin.expanded, Some(false), "a plugin starts closed");
        assert_eq!(plugin.tag, "5 skills · 3 agents");
        assert!(
            closed.rows.iter().all(|row| row.depth < 2),
            "nothing below a plugin is drawn while it is closed"
        );

        act(
            &chat,
            cx,
            SessionPanelAction::Toggle("context/plugins/plugin-dev".into()),
        );
        let open = context_section(&chat, cx);
        let inside: Vec<&str> = open
            .rows
            .iter()
            .filter(|row| row.depth == 2)
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(
            inside,
            [
                "agent-creator",
                "plugin-validator",
                "skill-reviewer",
                "skill-04",
                "skill-03",
                "skill-02",
                "skill-01",
                "skill-00"
            ]
        );

        act(
            &chat,
            cx,
            SessionPanelAction::Toggle("context/plugins/plugin-dev".into()),
        );
        let again = context_section(&chat, cx);
        assert!(again.rows.iter().all(|row| row.depth < 2), "the toggle is symmetric");
    }

    #[gpui::test]
    async fn activating_a_memory_file_opens_it(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, Some(breakdown(2)));
        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                if let ChatEvent::OpenFile(path) = event {
                    seen.borrow_mut().push(path.clone());
                }
            })
            .detach();
        });
        let section = context_section(&chat, cx);
        let file = section
            .rows
            .iter()
            .find(|row| row.label == "CLAUDE.md" && row.tag == "Project")
            .expect("the project memory file");
        assert_eq!(file.action.as_deref(), Some("open"));
        act(&chat, cx, SessionPanelAction::Activate(file.key.clone()));
        assert_eq!(*opened.borrow(), [PathBuf::from("/repo/CLAUDE.md")]);

        let group = context_section(&chat, cx)
            .rows
            .into_iter()
            .find(|row| row.label == "Memory files")
            .expect("the group row");
        let refused = chat.update_in(cx, |chat, window, cx| {
            chat.session_panel_act(SessionPanelAction::Activate(group.key), window, cx)
        });
        assert!(refused.is_err(), "a group has nothing to open");
    }

    #[gpui::test]
    async fn more_than_ten_children_fold_into_n_more(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, Some(breakdown(12)));
        act(
            &chat,
            cx,
            SessionPanelAction::Toggle("context/plugins/plugin-dev".into()),
        );
        let section = context_section(&chat, cx);
        let inside: Vec<_> = section.rows.iter().filter(|row| row.depth == 2).collect();
        assert_eq!(inside.len(), 11, "ten heaviest and one fold: {inside:?}");
        let more = inside[10];
        // 3 agents + 7 skills fill the ten; the 5 lightest skills fold.
        assert_eq!(more.label, "5 more");
        assert_eq!(more.key, "context/plugins/plugin-dev/more");
        assert_eq!(more.expanded, Some(false));
        let rest: u64 = (0..5).map(|i| 100 + i).sum();
        assert_eq!(more.detail, format!("{rest}"));

        act(&chat, cx, SessionPanelAction::Toggle(more.key.clone()));
        let section = context_section(&chat, cx);
        let deeper: Vec<&str> = section
            .rows
            .iter()
            .filter(|row| row.depth == 3)
            .map(|row| row.label.as_str())
            .collect();
        assert_eq!(deeper, ["skill-04", "skill-03", "skill-02", "skill-01", "skill-00"]);
    }

    #[gpui::test]
    async fn the_remainder_is_drawn_after_n_more_not_inside_it(cx: &mut TestAppContext) {
        let files: Vec<_> = (0..12)
            .map(|i| json!({"path": format!("/r/f{i:02}.md"), "type": "Project", "tokens": 100 + i}))
            .collect();
        let breakdown = ContextBreakdown::parse(&json!({
            "totalTokens": 40000, "rawMaxTokens": 400000,
            "categories": [category("memory_files", "Memory files", 40000, "used")],
            "memoryFiles": files,
        }))
        .expect("a breakdown");
        let (chat, cx) = open_with(cx, Some(breakdown));
        let section = context_section(&chat, cx);
        let inside: Vec<&str> = section
            .rows
            .iter()
            .filter(|row| row.depth == 1)
            .map(|row| row.label.as_str())
            .collect();
        // The remainder weighs far more than any file and is not one of the
        // ten shown, nor of what "2 more" holds.
        assert_eq!(inside.len(), 12, "{inside:?}");
        assert_eq!(&inside[10..], ["2 more", "Other"]);
        let more = section
            .rows
            .iter()
            .find(|row| row.label == "2 more")
            .expect("the fold");
        assert_eq!(more.detail, "201", "100 + 101, the two lightest files");
    }

    #[gpui::test]
    async fn an_acp_chat_shows_its_total_and_says_why_nothing_else(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, None);
        assert!(
            chat.read_with(&cx.cx, |chat, _| chat.control_snapshot())
                .session_panel
                .sections
                .iter()
                .all(|section| section.key != "context"),
            "with no total and no breakdown the card is not there"
        );
        chat.update(cx, |chat, cx| {
            chat.context_usage = Some(ContextUsage {
                used: 10_000,
                size: 100_000,
                ..ContextUsage::default()
            });
            cx.notify();
        });
        refresh_frame(cx);
        let section = context_section(&chat, cx);
        assert_eq!(section.summary, "10% · 10.0k / 100.0k");
        assert_eq!(
            labels(&section),
            [(0, "This agent does not report what is in its context", "")]
        );
    }

    #[gpui::test]
    async fn a_click_folds_a_node_and_opens_a_file(cx: &mut TestAppContext) {
        let (chat, cx) = open_with(cx, Some(breakdown(3)));
        cx.simulate_resize(gpui::size(gpui::px(1100.0), gpui::px(900.0)));
        // The card starts folded, and a folded card draws no rows to click.
        act(&chat, cx, SessionPanelAction::Toggle("context".into()));
        refresh_frame(cx);
        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = opened.clone();
        cx.update(|_, cx| {
            cx.subscribe(&chat, move |_, event: &ChatEvent, _| {
                if let ChatEvent::OpenFile(path) = event {
                    seen.borrow_mut().push(path.clone());
                }
            })
            .detach();
        });
        let click = |cx: &mut VisualTestContext, key: &str| {
            let selector: &'static str = Box::leak(format!("session-row-{key}").into_boxed_str());
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} is drawn"));
            cx.simulate_click(bounds.center(), gpui::Modifiers::none());
            cx.run_until_parked();
            refresh_frame(cx);
        };
        click(cx, "context/plugins/plugin-dev");
        let section = context_section(&chat, cx);
        assert!(section.rows.iter().any(|row| row.depth == 2), "the click opened the plugin");
        click(cx, "context/memory//repo/CLAUDE.md");
        assert_eq!(*opened.borrow(), [PathBuf::from("/repo/CLAUDE.md")]);
    }
}
