//! The Sessions view's list: the sidebar's own rows and pills, flattened
//! into one card per agent session and ordered by its last agent event,
//! plus the archived chats the host pushes in. Pure — no gpui — so the
//! ordering and filtering rules are tested without a window.

use std::path::PathBuf;

use chrono::TimeZone;

use super::*;

/// What activating a session row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionTarget {
    /// A live tab of the selected worktree (`SidebarEvent::SelectTab`).
    Open(usize),
    /// A tab of another worktree's strip (`SidebarEvent::SelectParkedTab`).
    Parked { path: PathBuf, index: usize },
    /// An archived chat, by tab id (`SidebarEvent::ReopenClosedChat`).
    Closed(String),
}

/// One card of the Sessions view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    /// Stable identity: the tab's persistence id. Keys the cached row view.
    pub key: String,
    pub target: SessionTarget,
    pub title: String,
    pub icon: Icon,
    pub brand: Option<AgentBrandColor>,
    pub project: String,
    pub branch: String,
    pub status: Option<ActivityStatus>,
    /// Last agent event (open) or archive time (closed), Unix ms.
    pub at: Option<i64>,
    pub selected: bool,
}

/// An archived chat as the host pushes it (`Sidebar::set_closed_sessions`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedSession {
    pub tab_id: String,
    /// The worktree's checkout path, spelled as the sidebar's worktree row
    /// spells it (the host resolves it against the catalog).
    pub worktree_path: PathBuf,
    pub title: String,
    pub agent: Option<AgentMark>,
    pub closed_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionList {
    pub open: Vec<SessionRow>,
    pub closed: Vec<SessionRow>,
}

/// An agent chat always is a session; a terminal is one once an agent has
/// been identified in it (its pill carries a brand).
fn is_session(pill: &SidebarPill) -> bool {
    match pill.kind {
        TabKind::AgentChat => true,
        TabKind::Terminal => pill.brand.is_some(),
        _ => false,
    }
}

fn newest_first(a: Option<i64>, b: Option<i64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn matches(row: &SessionRow, query: &str) -> bool {
    query.is_empty()
        || [
            row.title.as_str(),
            row.brand.map_or("", agent_name),
            row.project.as_str(),
            row.branch.as_str(),
        ]
        .iter()
        .any(|field| field.to_lowercase().contains(query))
}

/// Flattens the sidebar's rows into sessions. Walks every row, not only the
/// visible ones, so a collapsed project still lists its sessions.
pub fn session_list(rows: &[SidebarRow], closed: &[ClosedSession], filter: &str) -> SessionList {
    let query = filter.trim().to_lowercase();
    let mut project = String::new();
    let mut open = Vec::new();
    for row in rows {
        match row.kind {
            RowKind::Project => project = row.title.clone(),
            RowKind::Worktree => {
                for pill in row.pills.iter().filter(|pill| is_session(pill)) {
                    let target = match (pill.tab_id, pill.parked_tab, row.path.as_ref()) {
                        (Some(id), _, _) => SessionTarget::Open(id),
                        (None, Some(index), Some(path)) => SessionTarget::Parked {
                            path: path.clone(),
                            index,
                        },
                        _ => continue,
                    };
                    open.push(SessionRow {
                        key: pill.persistence_id.clone(),
                        target,
                        title: pill.title.clone(),
                        icon: pill.icon,
                        brand: pill.brand,
                        project: project.clone(),
                        branch: row.title.clone(),
                        status: pill.status,
                        at: pill.last_event_at,
                        selected: pill.selected,
                    });
                }
            }
        }
    }
    // `sort_by` is stable: equal times keep tree order.
    open.sort_by(|a, b| newest_first(a.at, b.at));
    let closed = closed
        .iter()
        .filter_map(|session| {
            let index = rows.iter().position(|row| {
                row.kind == RowKind::Worktree
                    && row.path.as_deref() == Some(session.worktree_path.as_path())
            })?;
            let project = rows[..index]
                .iter()
                .rev()
                .find(|row| row.kind == RowKind::Project)
                .map(|row| row.title.clone())
                .unwrap_or_default();
            Some(SessionRow {
                key: session.tab_id.clone(),
                target: SessionTarget::Closed(session.tab_id.clone()),
                title: session.title.clone(),
                icon: session
                    .agent
                    .map_or(Icon::MessageSquare, |agent| agent.icon),
                brand: session.agent.map(|agent| agent.brand),
                project,
                branch: rows[index].title.clone(),
                status: None,
                at: Some(session.closed_at),
                selected: false,
            })
        })
        .collect::<Vec<_>>();
    SessionList {
        open: open
            .into_iter()
            .filter(|row| matches(row, &query))
            .collect(),
        closed: closed
            .into_iter()
            .filter(|row| matches(row, &query))
            .collect(),
    }
}

/// The badge the Sessions segment wears: error outranks needs-input, the
/// rule `RightPanel::activity_badge` had.
pub fn attention(open: &[SessionRow]) -> Option<ActivityStatus> {
    [ActivityStatus::Error, ActivityStatus::NeedsInput]
        .into_iter()
        .find(|wanted| open.iter().any(|row| row.status == Some(*wanted)))
}

/// A card's time label, from elapsed time (not calendar days).
pub fn relative_time(now_ms: i64, at_ms: i64) -> String {
    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let elapsed = now_ms.saturating_sub(at_ms);
    if elapsed < MINUTE {
        "now".to_owned()
    } else if elapsed < HOUR {
        format!("{}m", elapsed / MINUTE)
    } else if elapsed < DAY {
        format!("{}h", elapsed / HOUR)
    } else if elapsed < 2 * DAY {
        "yesterday".to_owned()
    } else if elapsed < 7 * DAY {
        format!("{}d", elapsed / DAY)
    } else {
        chrono::Local
            .timestamp_millis_opt(at_ms)
            .single()
            .map(|date| date.format("%-d %b").to_string())
            .unwrap_or_default()
    }
}

/// The agent's display name, matched by the filter.
pub fn agent_name(brand: AgentBrandColor) -> &'static str {
    match brand {
        AgentBrandColor::Claude => "Claude",
        AgentBrandColor::Codex => "Codex",
        AgentBrandColor::OpenCode => "OpenCode",
        AgentBrandColor::Pi => "Pi",
        AgentBrandColor::Omp => "omp",
        AgentBrandColor::Unknown => "",
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn project(id: usize, title: &str) -> SidebarRow {
        SidebarRow {
            id,
            kind: RowKind::Project,
            depth: 0,
            title: title.into(),
            selected: false,
            expanded: false,
            is_primary: false,
            agent_status: None,
            is_git: true,
            path: None,
            tab_id: None,
            parked_tab: None,
            tab_kind: None,
            agent_icon: None,
            agent_brand: None,
            comment: None,
            pills: Vec::new(),
        }
    }

    fn worktree(id: usize, branch: &str, path: &str, pills: Vec<SidebarPill>) -> SidebarRow {
        SidebarRow {
            kind: RowKind::Worktree,
            depth: 1,
            title: branch.into(),
            path: Some(PathBuf::from(path)),
            pills,
            ..project(id, branch)
        }
    }

    fn pill(key: &str, kind: TabKind, agent: Option<&str>, at: Option<i64>) -> SidebarPill {
        let mark = agent.and_then(AgentMark::for_agent_id);
        SidebarPill {
            tab_id: Some(key.len()),
            parked_tab: None,
            title: format!("{key} title"),
            icon: mark.map_or(Icon::SquareTerminal, |mark| mark.icon),
            brand: mark.map(|mark| mark.brand),
            status: None,
            selected: false,
            kind,
            persistence_id: key.into(),
            last_event_at: at,
        }
    }

    fn keys(rows: &[SessionRow]) -> Vec<&str> {
        rows.iter().map(|row| row.key.as_str()).collect()
    }

    #[test]
    fn only_agent_tabs_become_sessions() {
        let rows = vec![
            project(0, "sirio"),
            worktree(
                1,
                "main",
                "/r/main",
                vec![
                    pill("chat", TabKind::AgentChat, Some("claude"), Some(3)),
                    pill("shell", TabKind::Terminal, None, Some(9)),
                    pill("agent-term", TabKind::Terminal, Some("codex"), Some(5)),
                ],
            ),
        ];
        let list = session_list(&rows, &[], "");
        assert_eq!(keys(&list.open), ["agent-term", "chat"]);
    }

    #[test]
    fn a_collapsed_project_still_contributes_its_sessions() {
        let mut sirio = project(0, "sirio");
        sirio.expanded = false;
        let rows = vec![
            sirio,
            worktree(
                1,
                "main",
                "/r/main",
                vec![pill("a", TabKind::AgentChat, Some("claude"), Some(1))],
            ),
        ];
        let list = session_list(&rows, &[], "");
        assert_eq!(keys(&list.open), ["a"]);
        assert_eq!(list.open[0].project, "sirio");
        assert_eq!(list.open[0].branch, "main");
    }

    #[test]
    fn newest_event_first_unknown_last_ties_in_tree_order() {
        let rows = vec![
            project(0, "p"),
            worktree(
                1,
                "one",
                "/r/one",
                vec![
                    pill("unknown-1", TabKind::AgentChat, Some("claude"), None),
                    pill("old", TabKind::AgentChat, Some("claude"), Some(10)),
                ],
            ),
            worktree(
                2,
                "two",
                "/r/two",
                vec![
                    pill("tie-a", TabKind::AgentChat, Some("pi"), Some(20)),
                    pill("unknown-2", TabKind::AgentChat, Some("pi"), None),
                    pill("tie-b", TabKind::AgentChat, Some("pi"), Some(20)),
                ],
            ),
        ];
        let list = session_list(&rows, &[], "");
        assert_eq!(
            keys(&list.open),
            ["tie-a", "tie-b", "old", "unknown-1", "unknown-2"]
        );
    }

    #[test]
    fn a_parked_pill_targets_its_worktree_path_and_strip_index() {
        let mut parked = pill("p", TabKind::AgentChat, Some("claude"), Some(1));
        parked.tab_id = None;
        parked.parked_tab = Some(2);
        let rows = vec![
            project(0, "p"),
            worktree(1, "main", "/r/main", vec![parked]),
        ];
        let list = session_list(&rows, &[], "");
        assert_eq!(
            list.open[0].target,
            SessionTarget::Parked {
                path: PathBuf::from("/r/main"),
                index: 2
            }
        );
    }

    #[test]
    fn closed_sessions_resolve_project_and_branch_by_path_and_drop_unknown_ones() {
        let rows = vec![
            project(0, "sirio"),
            worktree(1, "feat/x", "/r/x", Vec::new()),
        ];
        let closed = vec![
            ClosedSession {
                tab_id: "gone".into(),
                worktree_path: PathBuf::from("/r/removed"),
                title: "Gone".into(),
                agent: AgentMark::for_agent_id("claude"),
                closed_at: 5,
            },
            ClosedSession {
                tab_id: "kept".into(),
                worktree_path: PathBuf::from("/r/x"),
                title: "Kept".into(),
                agent: AgentMark::for_agent_id("codex"),
                closed_at: 4,
            },
        ];
        let list = session_list(&rows, &closed, "");
        assert_eq!(keys(&list.closed), ["kept"]);
        assert_eq!(list.closed[0].project, "sirio");
        assert_eq!(list.closed[0].branch, "feat/x");
        assert_eq!(list.closed[0].target, SessionTarget::Closed("kept".into()));
        assert_eq!(list.closed[0].at, Some(4));
    }

    #[test]
    fn the_filter_matches_title_agent_project_and_branch_case_insensitively() {
        let rows = vec![
            project(0, "Orbit"),
            worktree(
                1,
                "fix/login",
                "/r/login",
                vec![pill("a", TabKind::AgentChat, Some("codex"), Some(1))],
            ),
            worktree(
                2,
                "main",
                "/r/main",
                vec![pill("b", TabKind::AgentChat, Some("claude"), Some(2))],
            ),
        ];
        assert_eq!(keys(&session_list(&rows, &[], "ORBIT").open), ["b", "a"]);
        assert_eq!(keys(&session_list(&rows, &[], "login").open), ["a"]);
        assert_eq!(keys(&session_list(&rows, &[], "Codex").open), ["a"]);
        assert_eq!(keys(&session_list(&rows, &[], "b TITLE").open), ["b"]);
        assert!(session_list(&rows, &[], "nothing").open.is_empty());
    }

    #[test]
    fn attention_prefers_error_over_needs_input() {
        let row = |status| SessionRow {
            key: String::new(),
            target: SessionTarget::Open(0),
            title: String::new(),
            icon: Icon::MessageSquare,
            brand: None,
            project: String::new(),
            branch: String::new(),
            status,
            at: None,
            selected: false,
        };
        assert_eq!(attention(&[row(Some(ActivityStatus::Running))]), None);
        assert_eq!(
            attention(&[row(Some(ActivityStatus::NeedsInput))]),
            Some(ActivityStatus::NeedsInput)
        );
        assert_eq!(
            attention(&[
                row(Some(ActivityStatus::NeedsInput)),
                row(Some(ActivityStatus::Error))
            ]),
            Some(ActivityStatus::Error)
        );
    }

    #[test]
    fn relative_time_buckets() {
        const S: i64 = 1_000;
        const M: i64 = 60 * S;
        const H: i64 = 60 * M;
        const D: i64 = 24 * H;
        let now = 1_000 * D;
        assert_eq!(relative_time(now, now - 59 * S), "now");
        assert_eq!(relative_time(now, now - 60 * S), "1m");
        assert_eq!(relative_time(now, now - 59 * M), "59m");
        assert_eq!(relative_time(now, now - 60 * M), "1h");
        assert_eq!(relative_time(now, now - 23 * H), "23h");
        assert_eq!(relative_time(now, now - 24 * H), "yesterday");
        assert_eq!(relative_time(now, now - 47 * H), "yesterday");
        assert_eq!(relative_time(now, now - 48 * H), "2d");
        assert_eq!(relative_time(now, now - 6 * D), "6d");
        assert_eq!(
            relative_time(now, now + 5 * M),
            "now",
            "a future time reads now"
        );
        let dated = relative_time(now, now - 7 * D);
        let expected = chrono::Local
            .timestamp_millis_opt(now - 7 * D)
            .single()
            .expect("valid time")
            .format("%-d %b")
            .to_string();
        assert_eq!(dated, expected);
    }

    #[test]
    fn every_brand_has_a_name() {
        assert_eq!(agent_name(AgentBrandColor::Claude), "Claude");
        assert_eq!(agent_name(AgentBrandColor::Codex), "Codex");
        assert_eq!(agent_name(AgentBrandColor::OpenCode), "OpenCode");
        assert_eq!(agent_name(AgentBrandColor::Pi), "Pi");
        assert_eq!(agent_name(AgentBrandColor::Omp), "omp");
        assert_eq!(agent_name(AgentBrandColor::Unknown), "");
    }
}
