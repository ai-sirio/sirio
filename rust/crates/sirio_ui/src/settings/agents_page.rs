//! Settings › Agents, drawn on bezel's settings scaffolding.
//!
//! Moved out of `settings.rs`: the screen's renderer, its private helpers
//! and its tests live here. State, events, `provider_row` and
//! `installed_integrity_note` stay in `settings.rs`.

use super::*;
use bezel::ui::icons;
use bezel::ui::tooltip::Tooltip;
use bezel::ui::widgets::{
    ButtonStyle, Buttons as _, Content as _, Scaffolding as _, Status as _, card_row_hover,
    status_dot,
};
use gpui::Focusable;

/// #334: the old path pill capped the resolved path at 240 px so version plus checksum still fit one meta line.
const PATH_FRAGMENT_MAX_WIDTH: f32 = 240.0;

impl Settings {
    /// Ids with an Update to offer, in row order: Installed rows whose
    /// registry version moved ahead, skipping rows with an install in
    /// flight (their action is hidden too, so they must not count).
    fn outdated_agent_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for availability in &self.provider_availability {
            if matches!(
                self.install_states.get(availability.id),
                Some(InstallState::InFlight)
            ) {
                continue;
            }
            if let sirio_registry::LaunchSource::Installed(installed) =
                self.launch_source_for_row(availability.id)
                && self
                    .registry_versions
                    .get(availability.id)
                    .is_some_and(|latest| latest != &installed.version)
            {
                ids.push(availability.id.to_string());
            }
        }
        ids
    }

    pub(crate) fn render_agents(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // F-SET-16: filtering hides non-matching rows but must not renumber
        // the ones that stay — `settings-agent-row-{index}` ids are keyed to
        // the original `provider_availability` position, and an existing
        // test asserts against those exact indices.
        let bezel_theme = theme.to_bezel_theme();
        let query = self
            .agent_search_field
            .read(cx)
            .content()
            .trim()
            .to_lowercase();
        let first_load = self.provider_availability.is_empty()
            && self.agent_registry_error.is_none()
            && query.is_empty();
        let outdated = self.outdated_agent_ids();
        let mut card = bezel_theme
            .group_box()
            .debug_selector(|| "settings-agents-card".to_string());
        let mut first_visible_row = true;
        for (index, availability) in self.provider_availability.iter().enumerate() {
            let source = self.launch_source_for_row(availability.id);
            let row = provider_row(availability, Some(&source));
            if !query.is_empty()
                && !row.name.to_lowercase().contains(&query)
                && !row.description.to_lowercase().contains(&query)
            {
                continue;
            }
            if !first_visible_row {
                // `card_row` draws its own top border past the first row.
            }
            let first = first_visible_row;
            first_visible_row = false;
            let install_state = self.install_states.get(availability.id);

            // The quiet meta line under the name: the CLI path (or the
            // install hint), the ACP version, the checksum note, the
            // unavailable reason, install progress — in that order.
            let mut fragments: Vec<AnyElement> = Vec::new();
            if let Some(path) = &availability.executable {
                let shown = sirio_project::display_path(path);
                let full = path.to_string_lossy().into_owned();
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-agent-description-{index}"))
                        .min_w_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .id(("settings-agent-path", index))
                                .debug_selector(move || format!("settings-agent-path-{index}"))
                                .tooltip(move |window, cx| Tooltip::text(full.clone(), window, cx))
                                .font_family(bezel_theme.font_mono.clone())
                                .text_size(px(11.0))
                                .max_w(px(PATH_FRAGMENT_MAX_WIDTH))
                                .overflow_hidden()
                                .child(caret::field_value(text!(shown))),
                        )
                        .into_any_element(),
                );
            } else {
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-agent-description-{index}"))
                        .min_w_0()
                        .overflow_hidden()
                        .child(text!(
                            id = ("settings-agent-description", index),
                            format!("Install the {} CLI to use it", row.name)
                        ))
                        .into_any_element(),
                );
            }
            if let Some(version) = row.version.as_ref() {
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-agent-version-{index}"))
                        .child(text!(
                            id = ("settings-agent-version", index),
                            // #197: name the subject. This is the
                            // ACP server package's version, from
                            // `sirio_registry` — never the CLI binary's.
                            format!("ACP v{version}")
                        ))
                        .into_any_element(),
                );
            }
            if let sirio_registry::LaunchSource::Installed(installed) = &source
                && let Some(latest) = self.registry_versions.get(availability.id)
                && latest != &installed.version
            {
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-agent-latest-{index}"))
                        .child(text!(format!("v{latest} available")))
                        .into_any_element(),
                );
            }
            if let Some(note) = installed_integrity_note(&source) {
                fragments.push(
                    div()
                        .id(("settings-agent-integrity", index))
                        .debug_selector(move || format!("settings-agent-integrity-{index}"))
                        .child(text!(note))
                        .into_any_element(),
                );
            }
            if let sirio_registry::LaunchSource::Unavailable(reason) = &source {
                let label = match reason {
                    sirio_registry::UnavailableReason::NotInRegistry => "No ACP server",
                    sirio_registry::UnavailableReason::NoArtifactForPlatform => {
                        "Not available for this platform"
                    }
                    sirio_registry::UnavailableReason::UnsupportedDistribution => {
                        "Unsupported install format"
                    }
                };
                fragments.push(
                    div()
                        .debug_selector(move || format!("settings-agent-acp-reason-{index}"))
                        .child(text!(label))
                        .into_any_element(),
                );
            }
            if matches!(install_state, Some(InstallState::InFlight)) {
                fragments.push(
                    div()
                        .id(("settings-agent-install-status", index))
                        .debug_selector(move || format!("settings-agent-install-status-{index}"))
                        .child(text!("Installing… this can take up to ten minutes."))
                        .into_any_element(),
                );
            }
            let tile = div()
                .debug_selector(move || format!("settings-agent-tile-{index}"))
                .flex_none()
                .size(px(36.0))
                .rounded(px(BezelTheme::panel_radius()))
                .border_1()
                .border_color(bezel_theme.border)
                .bg(bezel_theme.ink(0.03))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    IconElement::new(row.icon, IconSize::Small)
                        .text_color(provider_glyph_color(theme, row.id)),
                );
            let body = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    bezel_theme
                        .row_title(row.name)
                        .debug_selector(move || format!("settings-agent-name-{index}")),
                )
                .child(
                    bezel_theme
                        .meta_line(fragments)
                        .debug_selector(move || format!("settings-agent-meta-{index}")),
                );
            // F-SET-18: a not-installed agent with a known install command
            // gets a real Install control, not just a red status pill —
            // clicking it hands the command to the host (a spawned
            // terminal, mirroring the Agent Skill card) and leaves a
            // confirmation line under the row so the click's effect is
            // visible on this screen even though this crate cannot watch
            // the spawned install finish.
            // F-SET-18 (closed): Install/Update are renderings of the row's
            // resolved source. The button emits only — `sirio` owns the
            // installer, and this crate never runs installs itself. While
            // this row's install is in flight the action disappears: the
            // per-agent lock would refuse a second click anyway, and a
            // dead-looking button invites exactly that click.
            let action = if matches!(install_state, Some(InstallState::InFlight)) {
                None
            } else {
                match &source {
                    sirio_registry::LaunchSource::Installable { .. } => Some((
                        SettingsEvent::InstallAgent(availability.id.to_string()),
                        "Install",
                    )),
                    sirio_registry::LaunchSource::Installed(_) => {
                        if outdated.iter().any(|id| id == availability.id) {
                            Some((
                                SettingsEvent::UpdateAgent(availability.id.to_string()),
                                "Update",
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            let install_control: Option<AnyElement> =
                if matches!(install_state, Some(InstallState::InFlight)) {
                    Some(
                        div()
                            .id(("settings-agent-install-spinner", index))
                            .debug_selector(move || {
                                format!("settings-agent-install-spinner-{index}")
                            })
                            .w(px(28.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(loading::compact("settings-install-spinner", window, cx))
                            .into_any_element(),
                    )
                } else {
                    action.map(|(event, label)| {
                        if label == "Update" {
                            let control_entity = entity.clone();
                            bezel_theme
                                .button("Update", ButtonStyle::Ghost, None)
                                .border_1()
                                .border_color(bezel_theme.border)
                                .hover(|s| s.bg(bezel_theme.element_hover))
                                .id(("settings-agent-update", index))
                                .debug_selector(move || format!("settings-agent-update-{index}"))
                                .on_click(move |_, _, cx| {
                                    control_entity.update(cx, |_, cx| {
                                        cx.emit(event.clone());
                                    });
                                })
                                .into_any_element()
                        } else {
                            let control_entity = entity.clone();
                            bezel_theme
                                .button("Install", ButtonStyle::Prominent, None)
                                .id(("settings-agent-install", index))
                                .debug_selector(move || format!("settings-agent-install-{index}"))
                                .on_click(move |_, _, cx| {
                                    control_entity.update(cx, |_, cx| {
                                        cx.emit(event.clone());
                                    });
                                })
                                .into_any_element()
                        }
                    })
                };
            let status_id = format!("settings-agent-status-{}", availability.id);
            let status = div()
                .id(status_id.clone())
                .debug_selector(move || status_id.clone())
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(status_dot(if availability.is_available() {
                    bezel_theme.success
                } else {
                    bezel_theme.danger
                }))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(bezel_theme.text_muted)
                        .child(text!(availability.status_label())),
                );
            let mut tail = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(status);
            if let Some(control) = install_control {
                tail = tail.child(control);
            } else if matches!(
                &source,
                sirio_registry::LaunchSource::Builtin { .. }
                    | sirio_registry::LaunchSource::Installed(_)
            ) {
                let badge_id = format!("settings-agent-acp-{}", availability.id);
                tail = tail.child(
                    bezel_theme
                        .badge_active("Installed")
                        .id(badge_id.clone())
                        .debug_selector(move || badge_id.clone()),
                );
            }
            let mut row_el = bezel_theme
                .card_row(first)
                .hover(card_row_hover)
                .id(("settings-agent-row", index))
                .debug_selector(move || format!("settings-agent-row-{index}"))
                .flex_wrap()
                .child(tile)
                .child(body)
                .child(tail);
            if let Some(InstallState::Failed(message)) = install_state {
                row_el = row_el.child(
                    bezel_theme
                        .error_strip(message.clone())
                        .mt(px(8.0))
                        .w_full()
                        .id(("settings-agent-install-reason", index))
                        .debug_selector(move || format!("settings-agent-install-reason-{index}")),
                );
            }
            card = card.child(row_el);
        }

        let agent_count = self.provider_availability.len();
        let focus_search_field = self.agent_search_field.clone();
        let search_field = self.agent_search_field.clone();
        let update_all_entity = entity.clone();
        let refresh_entity = entity;

        let mut surface = div()
                .w(px(CONTENT_WIDTH))
                .pt(px(DETAIL_TOP_PADDING))
                .pb(px(DETAIL_BOTTOM_PADDING))
                .child(
                    div()
                        .w_full()
                        .mb(px(14.0))
                        .flex()
                        .justify_between()
                        .items_start()
                        .gap(px(16.0))
                        .child(
                            // Left: bezel page headline + subtitle.
                            // `page_header` carries no per-part selectors,
                            // so the count is a sibling with the headline's
                            // own tokens (13px, muted at 0.7) rather than its
                            // `Some(n)` arm.
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(
                                    div()
                                        .flex()
                                        .items_baseline()
                                        .gap(px(10.0))
                                        .child(
                                            bezel_theme
                                                .page_header("Agents", None)
                                                .id("agents-page-title")
                                                .debug_selector(|| {
                                                    "agents-page-title".to_string()
                                                }),
                                        )
                                        .child(
                                            div()
                                                .id("agents-page-count")
                                                .debug_selector(|| {
                                                    "agents-page-count".to_string()
                                                })
                                                .text_size(px(13.0))
                                                .text_color(
                                                    bezel_theme.text_muted.opacity(0.7),
                                                )
                                                .child(text!(format!("{agent_count}"))),
                                        ),
                                )
                                .child(
                                    bezel_theme
                                        .page_subtitle(
                                            "The coding agents a pane can run. Each row reads its CLI from your PATH and its ACP server from the registry.",
                                        )
                                        .id("agents-page-subtitle")
                                        .debug_selector(|| {
                                            "agents-page-subtitle".to_string()
                                        }),
                                ),
                        )
                        .child({
                            let mut toolbar = div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .flex_none();
                            toolbar = toolbar.child(
                                div()
                                    .id("agent-search-field")
                                    .debug_selector(|| {
                                        "agent-search-field".to_string()
                                    })
                                    .w(px(220.0))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        move |_, window, cx| {
                                            focus_search_field
                                                .read(cx)
                                                .focus_handle(cx)
                                                .focus(window, cx);
                                        },
                                    )
                                    .child(search_field.clone()),
                            );
                            // F-SET-16: a "Refreshed …" stamp next to the
                            // button — the conjunct a byte-identical
                            // before/after capture read as absent, since
                            // Search and Refresh were themselves already wired
                            // and tested.
                            if let Some(stamp) = self.agent_last_refreshed.as_ref() {
                                toolbar = toolbar.child(
                                    div()
                                        .id("agents-last-refreshed")
                                        .debug_selector(|| {
                                            "agents-last-refreshed".to_string()
                                        })
                                        .text_size(px(11.5))
                                        .text_color(
                                            bezel_theme.text_muted.opacity(0.65),
                                        )
                                        .child(text!(format!("Refreshed {stamp}"))),
                                );
                            }
                            // One Update per outdated row, in row order —
                            // the host serialises per agent with its
                            // in-flight lock, so no new event is needed.
                            if !outdated.is_empty() {
                                toolbar = toolbar.child(
                                    bezel_theme
                                        .button("Update All", ButtonStyle::Ghost, None)
                                        .border_1()
                                        .border_color(bezel_theme.border)
                                        .hover(|s| s.bg(bezel_theme.element_hover))
                                        .id("agents-update-all")
                                        .debug_selector(|| "agents-update-all".to_string())
                                        .on_click(move |_, _, cx| {
                                            update_all_entity.update(cx, |this, cx| {
                                                for id in this.outdated_agent_ids() {
                                                    cx.emit(SettingsEvent::UpdateAgent(id));
                                                }
                                            });
                                        }),
                                );
                            }
                            toolbar.child(
                                bezel_theme
                                    .button("Refresh", ButtonStyle::Ghost, None)
                                    .hover(|s| s.bg(bezel_theme.element_hover))
                                    .id("refresh-agents")
                                    .debug_selector(|| "refresh-agents".to_string())
                                    .on_click(move |_, _, cx| {
                                        refresh_entity.update(cx, |this, cx| {
                                            this.refresh_agent_availability(cx)
                                        });
                                    }),
                            )
                        }),
                );
        // F-SET-17: the registry error state, rendered over the rows from
        // the last successful sweep, with the Refresh button above as
        // the retry.
        if let Some(error) = self.agent_registry_error.clone() {
            surface = surface.child(
                bezel_theme
                    .warning_strip(error)
                    .id("settings-agents-registry-error")
                    .debug_selector(|| "settings-agents-registry-error".to_string()),
            );
        }
        if first_load {
            surface.child(
                div()
                    .id("settings-agents-loading")
                    .debug_selector(|| "settings-agents-loading".to_string())
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(theme.spacing.card_gap)
                    .text_size(theme.typography.headline)
                    .text_color(theme.text_muted)
                    .child(loading::indeterminate(
                        "settings-agents-loading-orb",
                        loading::GENERIC_ORB,
                        &theme,
                        window,
                        cx,
                    ))
                    .child("Loading agents…"),
            )
        } else if !query.is_empty() && first_visible_row {
            surface.child(
                card.child(
                    bezel_theme
                        .empty_state(
                            icons::MAGNIFER,
                            format!("No agents match \u{201c}{query}\u{201d}"),
                            format!("Clear the search to see all {agent_count} agents."),
                        )
                        .debug_selector(|| "settings-agents-empty".to_string()),
                ),
            )
        } else {
            surface.child(card)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn open_agents_category(
        window: &gpui::WindowHandle<Settings>,
        cx: &mut gpui::VisualTestContext,
    ) {
        let _ = window;
        if let Some(bounds) = cx.debug_bounds("settings-category-Agents") {
            cx.simulate_click(bounds.center(), Modifiers::none());
            cx.run_until_parked();
        }
    }

    /// F-SET-16: typing in the Agents screen's bezel search field narrows
    /// the drawn rows to the matching agent, and clearing it restores every
    /// row — the same contract `sidebar.rs`'s project filter already has.
    /// Filtering must not renumber surviving rows: ids stay keyed to the
    /// original discovery position, so a filtered-out row 1 does not
    /// become the new row 0.
    #[gpui::test]
    async fn agent_search_narrows_rows_and_clearing_restores_them(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
            AgentAvailability {
                id: "omp",
                display_name: "Oh-My-Pi",
                executable: None,
            },
        ];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        // Focus the search field the way a user does: click it.
        let search = cx
            .debug_bounds("agent-search-field")
            .expect("the search field is drawn");
        cx.simulate_click(search.center(), Modifiers::none());
        cx.run_until_parked();

        cx.simulate_input("claude");
        cx.run_until_parked();

        let search_state = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .agent_search_field
                .read(cx)
                .content()
                .to_string()
        });
        assert_eq!(
            search_state, "claude",
            "the keystrokes reached the search field"
        );

        assert!(
            cx.debug_bounds("settings-agent-row-0").is_some(),
            "the matching row stays drawn"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-1").is_none(),
            "row 1 is filtered out while the search reads 'claude'"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-2").is_none(),
            "row 2 is filtered out while the search reads 'claude'"
        );

        // Clearing the search restores every row.
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |settings, cx| {
                settings.agent_search_field.update(cx, |field, cx| {
                    field.set_content("", cx);
                });
                cx.notify();
            });
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("settings-agent-row-0").is_some());
        assert!(
            cx.debug_bounds("settings-agent-row-1").is_some(),
            "row 1 returns after the search is cleared"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-2").is_some(),
            "row 2 returns after the search is cleared"
        );
    }

    #[gpui::test]
    async fn the_agents_page_opens_with_a_bezel_header(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
        ];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let title = cx
            .debug_bounds("agents-page-title")
            .expect("the bezel page header renders a titled line");
        assert!(
            title.size.width > px(0.0) && title.size.height > px(0.0),
            "the title is text-bearing"
        );
        let count = cx
            .debug_bounds("agents-page-count")
            .expect("the header counts the agents");
        assert!(
            cx.debug_bounds("agents-page-subtitle").is_some(),
            "the subtitle renders under the title"
        );
        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("rows render below the header");
        assert!(
            title.origin.y + title.size.height <= row.origin.y,
            "the title sits above the rows: title={title:?} row={row:?}"
        );
        assert!(
            count.origin.x >= title.origin.x + title.size.width,
            "the count sits right of the title: title={title:?} count={count:?}"
        );
        let dy = count.origin.y - title.origin.y;
        assert!(
            dy <= px(8.0) && dy >= px(-8.0),
            "the count shares the title's line: title={title:?} count={count:?}"
        );
        let field = cx
            .debug_bounds("agent-search-field")
            .expect("the bezel search field renders");
        assert!(
            field.origin.x > title.origin.x + title.size.width,
            "the field sits right of the title: title={title:?} field={field:?}"
        );
        let dw = field.size.width - px(220.0);
        assert!(
            dw <= px(2.0) && dw >= px(-2.0),
            "the field is 220 wide: field={field:?}"
        );
        let refresh = cx
            .debug_bounds("refresh-agents")
            .expect("Refresh renders in the toolbar");
        assert!(
            refresh.origin.x >= field.origin.x + field.size.width,
            "Refresh sits right of the field: field={field:?} refresh={refresh:?}"
        );
    }

    #[gpui::test]
    async fn an_available_agent_reads_available_with_a_green_dot_and_installed_badge(
        cx: &mut TestAppContext,
    ) {
        use sirio_registry::LaunchSource;
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![AgentAvailability {
            id: "claude",
            display_name: "Claude Code",
            executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
        }];
        let sources = vec![(
            "claude".to_string(),
            LaunchSource::Builtin {
                program: "claude".into(),
                args: vec!["acp".into()],
            },
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        assert!(
            cx.debug_bounds("settings-agent-status-claude").is_some(),
            "an available CLI renders its status"
        );
        assert!(
            cx.debug_bounds("settings-agent-acp-claude").is_some(),
            "a builtin ACP server renders the Installed badge"
        );
        assert!(
            cx.debug_bounds("settings-agent-install-0").is_none(),
            "no Install control beside the badge"
        );
        assert!(
            cx.debug_bounds("settings-agent-update-0").is_none(),
            "no Update control beside the badge"
        );
    }

    #[gpui::test]
    async fn a_missing_acp_server_puts_the_reason_in_the_meta_line_not_a_badge(
        cx: &mut TestAppContext,
    ) {
        use sirio_registry::{LaunchSource, UnavailableReason};
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![AgentAvailability {
            id: "omp",
            display_name: "Oh-My-Pi",
            executable: None,
        }];
        let sources = vec![(
            "omp".to_string(),
            LaunchSource::Unavailable(UnavailableReason::NotInRegistry),
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        assert!(
            cx.debug_bounds("settings-agent-acp-reason-0").is_some(),
            "the missing server reads as a meta fragment under the name"
        );
        assert!(
            cx.debug_bounds("settings-agent-acp-omp").is_none(),
            "no badge claims a server that is not there"
        );
    }

    #[gpui::test]
    async fn the_checksum_note_is_a_meta_fragment_on_the_rows_line(cx: &mut TestAppContext) {
        use sirio_registry::{InstalledAgent, Integrity, LaunchSource};
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![AgentAvailability {
            id: "claude",
            display_name: "Claude Code",
            executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
        }];
        let sources = vec![(
            "claude".to_string(),
            LaunchSource::Installed(InstalledAgent {
                id: "claude-acp".into(),
                version: "1.0.0".into(),
                executable: "/opt/sirio/claude-acp".into(),
                args: vec![],
                integrity: Integrity::None,
            }),
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("the row renders");
        let version = cx
            .debug_bounds("settings-agent-version-0")
            .expect("the ACP version renders");
        let note = cx
            .debug_bounds("settings-agent-integrity-0")
            .expect("the checksum note renders");
        assert!(
            note.origin.x + note.size.width <= row.origin.x + row.size.width
                && note.origin.y >= row.origin.y
                && note.origin.y + note.size.height <= row.origin.y + row.size.height,
            "the note sits inside the row's bounds: note={note:?} row={row:?}"
        );
        assert!(
            note.origin.x >= version.origin.x + version.size.width,
            "the note follows the version on the meta line: version={version:?} note={note:?}"
        );
    }

    /// #334: the Codex row at the 240 px path cap — a 60-char path plus
    /// `ACP vX` plus the checksum note. Pins what the harness measures
    /// reliably: the path fragment stays capped and inside the row, and the
    /// note stays inside the row. There is deliberately no equal-top-bounds
    /// assert: under the harness font metrics the three fragments are 501px
    /// in a 454px body, so the note wraps at any cap above ~193px.
    #[gpui::test]
    async fn codex_path_keeps_capped_path_and_note_inside_its_row(cx: &mut TestAppContext) {
        use sirio_registry::{InstalledAgent, Integrity, LaunchSource};
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let path = "/opt/homebrew/bin/codex-acp-0123456789-abcdef-00000000000000";
        assert_eq!(path.len(), 60, "the fixture is a Codex-length path");
        let fixture = vec![AgentAvailability {
            id: "codex",
            display_name: "Codex",
            executable: Some(PathBuf::from(path)),
        }];
        let sources = vec![(
            "codex".to_string(),
            LaunchSource::Installed(InstalledAgent {
                id: "codex-acp".into(),
                version: "1.0.0".into(),
                executable: "/opt/sirio/codex-acp".into(),
                args: vec![],
                integrity: Integrity::None,
            }),
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("the Codex row renders");
        let pathb = cx
            .debug_bounds("settings-agent-path-0")
            .expect("the resolved-path fragment renders");
        let note = cx
            .debug_bounds("settings-agent-integrity-0")
            .expect("the checksum note renders");
        assert!(
            pathb.size.width <= px(PATH_FRAGMENT_MAX_WIDTH),
            "the path fragment never grows past its 240 cap: path={pathb:?}"
        );
        assert!(
            pathb.origin.x + pathb.size.width <= row.origin.x + row.size.width,
            "the path fragment stays inside the row: path={pathb:?} row={row:?}"
        );
        assert!(
            note.origin.x + note.size.width <= row.origin.x + row.size.width
                && note.origin.y >= row.origin.y
                && note.origin.y + note.size.height <= row.origin.y + row.size.height,
            "the note sits inside the row's bounds: note={note:?} row={row:?}"
        );
    }

    // A test-only triple with no better name: the two rows, their
    // sources and the registry versions behind them.
    #[allow(clippy::type_complexity)]
    fn outdated_fixture() -> (
        Vec<AgentAvailability>,
        Vec<(String, sirio_registry::LaunchSource)>,
        std::collections::BTreeMap<String, String>,
    ) {
        use sirio_registry::{InstalledAgent, Integrity, LaunchSource};
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: Some(PathBuf::from("/opt/homebrew/bin/opencode")),
            },
        ];
        let sources = vec![
            (
                "claude".to_string(),
                LaunchSource::Installed(InstalledAgent {
                    id: "claude-acp".into(),
                    version: "1.0.0".into(),
                    executable: "/opt/sirio/claude-acp".into(),
                    args: vec![],
                    integrity: Integrity::Sha256,
                }),
            ),
            (
                "opencode".to_string(),
                LaunchSource::Installed(InstalledAgent {
                    id: "opencode".into(),
                    version: "2.0.0".into(),
                    executable: "/opt/sirio/opencode-acp".into(),
                    args: vec![],
                    integrity: Integrity::Sha256,
                }),
            ),
        ];
        let versions = std::collections::BTreeMap::from([
            ("claude".to_string(), "1.1.0".to_string()),
            ("opencode".to_string(), "2.0.0".to_string()),
        ]);
        (fixture, sources, versions)
    }

    #[gpui::test]
    async fn an_outdated_installed_agent_shows_update_instead_of_installed(
        cx: &mut TestAppContext,
    ) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (fixture, sources, versions) = outdated_fixture();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |settings, cx| {
                settings.apply_launch_sources(sources, versions);
                cx.notify();
            });
        });
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        assert!(
            cx.debug_bounds("settings-agent-update-0").is_some(),
            "the outdated row offers Update"
        );
        assert!(
            cx.debug_bounds("settings-agent-acp-claude").is_none(),
            "the Update control replaces the Installed badge, not accompanies it"
        );
        assert!(
            cx.debug_bounds("settings-agent-latest-0").is_some(),
            "the row names the registry version as v1.1.0 available"
        );

        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorder = events.clone();
        cx.update(|window, app| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let subscription = app.subscribe(
                &settings,
                move |_entity, event: &SettingsEvent, _| match event {
                    SettingsEvent::UpdateAgent(id) => recorder.borrow_mut().push(id.clone()),
                    SettingsEvent::InstallAgent(_) | SettingsEvent::RefreshAgentSources => {}
                },
            );
            std::mem::forget(subscription);
        });
        let update = cx
            .debug_bounds("settings-agent-update-0")
            .expect("Update renders for claude");
        cx.simulate_click(update.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            events.borrow().as_slice(),
            ["claude".to_string()],
            "the click emits UpdateAgent for the outdated row"
        );
    }

    #[gpui::test]
    async fn update_all_emits_one_update_per_outdated_agent(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let (fixture, sources, versions) = outdated_fixture();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |settings, cx| {
                settings.apply_launch_sources(sources, versions);
                cx.notify();
            });
        });
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let update_all = cx
            .debug_bounds("agents-update-all")
            .expect("Update All renders while one row is outdated");
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorder = events.clone();
        cx.update(|window, app| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let subscription = app.subscribe(
                &settings,
                move |_entity, event: &SettingsEvent, _| match event {
                    SettingsEvent::UpdateAgent(id) => recorder.borrow_mut().push(id.clone()),
                    SettingsEvent::InstallAgent(_) | SettingsEvent::RefreshAgentSources => {}
                },
            );
            std::mem::forget(subscription);
        });
        cx.simulate_click(update_all.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(
            events.borrow().as_slice(),
            ["claude".to_string()],
            "Update All emits exactly one UpdateAgent, for the outdated id"
        );
    }

    #[gpui::test]
    async fn update_all_is_absent_when_nothing_is_outdated(cx: &mut TestAppContext) {
        use sirio_registry::LaunchSource;
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![AgentAvailability {
            id: "claude",
            display_name: "Claude Code",
            executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
        }];
        let sources = vec![(
            "claude".to_string(),
            LaunchSource::Builtin {
                program: "claude".into(),
                args: vec!["acp".into()],
            },
        )];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        assert!(
            cx.debug_bounds("agents-update-all").is_none(),
            "no Update All without an outdated row"
        );
    }

    #[gpui::test]
    async fn agent_rows_render_what_discovery_found(cx: &mut gpui::TestAppContext) {
        // The Agents screen renders the discovery list, not a fixed set of
        // "Available" claims: the fixture below mirrors this machine's
        // reality (opencode/omp absent) and must render absent rows too.
        cx.update(Theme::init);
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
            AgentAvailability {
                id: "omp",
                display_name: "Oh-My-Pi",
                executable: None,
            },
        ];
        let expected = fixture.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-row-0").is_some(),
            "the first provider row renders"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-2").is_some(),
            "the third provider row renders"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-3").is_none(),
            "rows follow the discovery list, not a fixed count"
        );
        assert!(
            cx.debug_bounds("settings-agent-status-claude").is_some(),
            "an installed CLI renders its status pill"
        );
        assert!(
            cx.debug_bounds("settings-agent-status-opencode").is_some(),
            "an absent CLI still renders its status pill"
        );
        assert!(cx.debug_bounds("settings-agent-status-omp").is_some());

        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_availability
                .clone()
        });
        assert_eq!(
            rendered, expected,
            "the surface renders exactly what discovery returned"
        );
    }

    #[gpui::test]
    async fn agent_description_stays_within_its_row(cx: &mut gpui::TestAppContext) {
        cx.update(Theme::init);
        let fixture = vec![AgentAvailability {
            id: "codex",
            display_name: "Codex agent with a deliberately long display name that exceeds the available row width",
            executable: None,
        }];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("the Codex row renders");
        let description = cx
            .debug_bounds("settings-agent-description-0")
            .expect("the Codex description renders");
        assert!(
            description.origin.x + description.size.width <= row.origin.x + row.size.width,
            "agent description must stay inside its row: description={description:?} row={row:?}"
        );
    }

    /// #334: the path fragment carries the resolved binary's full path at
    /// a capped width, clipped from its *start*, so a deep install prefix
    /// gives way before the binary's name does — and long before the row's
    /// own name, which is the row's identity and must keep its full width.
    /// The fragment lives in the meta line now, capped at 240 wide.
    #[gpui::test]
    async fn a_long_binary_path_never_squeezes_the_agent_name_out_of_its_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let fixture = vec![AgentAvailability {
            id: "pi",
            display_name: "Pi",
            executable: Some(std::path::PathBuf::from(
                "/home/user/.local/share/pi-node/node-v22.23.2-linux-x64/lib/node_modules/\
                 @mariozechner/pi-coding-agent/node_modules/.bin/some-very-deeply-nested/\
                 vendor/runtime/bin/pi",
            )),
        }];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("the Pi row renders");
        let name = cx
            .debug_bounds("settings-agent-name-0")
            .expect("the Pi name renders");
        let path = cx
            .debug_bounds("settings-agent-path-0")
            .expect("the resolved-path fragment renders");

        assert!(
            name.size.width >= px(7.8 * 2.0),
            "the agent's name keeps its full width — it is the row's identity: \
             name={name:?} path={path:?}"
        );
        assert!(
            path.size.width <= px(PATH_FRAGMENT_MAX_WIDTH),
            "the path fragment never grows past its 240 cap: path={path:?}"
        );
        assert!(
            path.origin.x + path.size.width <= row.origin.x + row.size.width,
            "the path fragment stays inside the row: path={path:?} row={row:?}"
        );
    }

    /// F-SET-18 (closed): an Installable row draws a real Install button
    /// whose click emits [`SettingsEvent::InstallAgent`] — the host owns
    /// the actual install (Task 8); this crate only renders and emits. A
    /// row with nothing to offer offers no button at all — never a
    /// fabricated one.
    #[gpui::test]
    async fn agent_install_click_emits_the_install_request(cx: &mut gpui::TestAppContext) {
        use sirio_registry::{Distribution, LaunchSource};
        cx.update(Theme::init);
        let fixture = vec![
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
            AgentAvailability {
                id: "omp",
                display_name: "Oh-My-Pi",
                executable: None,
            },
        ];
        let sources = vec![
            (
                "opencode".to_string(),
                LaunchSource::Installable {
                    agent: sirio_registry::RegistryAgent {
                        id: "opencode".into(),
                        name: "OpenCode".into(),
                        version: "1.18.21".into(),
                        description: None,
                        repository: None,
                        website: None,
                        license: None,
                        icon: None,
                        distributions: vec![Distribution::Binary(Default::default())],
                    },
                },
            ),
            (
                "omp".to_string(),
                LaunchSource::Unavailable(sirio_registry::UnavailableReason::NotInRegistry),
            ),
        ];
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let recorder = events.clone();
        let window = cx.add_window(move |_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(|window, app| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            let subscription = app.subscribe(
                &settings,
                move |_entity, event: &SettingsEvent, _| match event {
                    SettingsEvent::InstallAgent(id) => recorder.borrow_mut().push(id.clone()),
                    SettingsEvent::UpdateAgent(_) | SettingsEvent::RefreshAgentSources => {}
                },
            );
            // The subscription must outlive this update scope for the whole
            // test; forgetting it pins it to the entities' lifetimes.
            std::mem::forget(subscription);
        });

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-install-0").is_some(),
            "an Installable row offers Install"
        );
        assert!(
            cx.debug_bounds("settings-agent-install-1").is_none(),
            "a row with nothing to offer draws no button"
        );

        let install = cx
            .debug_bounds("settings-agent-install-0")
            .expect("Install renders for opencode");

        // The click below proves nothing unless the control is inside
        // the card that clips it: `group_box` sets
        // `overflow_hidden`, so a button pushed past the row's right
        // edge is invisible AND unhittable while still reporting real
        // `debug_bounds`. That is not hypothetical — the button already
        // overflowed the 720px column once. Assert the containment the
        // click depends on, so drift fails here instead of in the app.
        let row = cx
            .debug_bounds("settings-agent-row-0")
            .expect("the opencode row draws");
        assert!(
            install.origin.x + install.size.width <= row.origin.x + row.size.width,
            "Install must sit inside the row that clips it, not past its \
             right edge: install={install:?} row={row:?}"
        );

        cx.simulate_click(install.center(), Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            events.borrow().as_slice(),
            ["opencode".to_string()],
            "the click emits InstallAgent for the row's adapter id"
        );
    }

    /// An Installable opencode row, for the install-state tests below.
    fn install_state_window(
        cx: &mut gpui::TestAppContext,
    ) -> (gpui::WindowHandle<Settings>, gpui::Entity<Settings>) {
        use sirio_registry::{Distribution, LaunchSource};
        cx.update(Theme::init);
        let fixture = vec![AgentAvailability {
            id: "opencode",
            display_name: "OpenCode",
            executable: None,
        }];
        let sources = vec![(
            "opencode".to_string(),
            LaunchSource::Installable {
                agent: sirio_registry::RegistryAgent {
                    id: "opencode".into(),
                    name: "OpenCode".into(),
                    version: "1.18.21".into(),
                    description: None,
                    repository: None,
                    website: None,
                    license: None,
                    icon: None,
                    distributions: vec![Distribution::Binary(Default::default())],
                },
            },
        )];
        let window = cx.add_window(move |_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default())
                .with_availability(fixture)
                .with_launch_sources(sources)
        });
        let entity = window.entity(cx).expect("settings entity");
        (window, entity)
    }

    #[gpui::test]
    async fn an_in_flight_install_hides_the_action_and_shows_progress(
        cx: &mut gpui::TestAppContext,
    ) {
        let (window, settings) = install_state_window(cx);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        settings.update(&mut cx, |settings, cx| {
            settings.set_install_state("opencode", Some(InstallState::InFlight));
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-install-0").is_none(),
            "the action is not clickable while the install is in flight"
        );
        assert!(
            cx.debug_bounds("settings-agent-install-status-0").is_some(),
            "progress replaces the action"
        );
    }

    #[gpui::test]
    async fn a_failed_install_shows_the_reason_and_allows_retry(cx: &mut gpui::TestAppContext) {
        let (window, settings) = install_state_window(cx);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        settings.update(&mut cx, |settings, cx| {
            settings.set_install_state(
                "opencode",
                Some(InstallState::Failed(
                    "opencode: checksum did not match; nothing was installed".into(),
                )),
            );
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-install-0").is_some(),
            "a failed install offers the action again"
        );
        assert!(
            cx.debug_bounds("settings-agent-install-reason-0").is_some(),
            "the failure names its reason instead of disappearing"
        );
    }

    #[gpui::test]
    async fn a_settled_install_returns_to_the_normal_row(cx: &mut gpui::TestAppContext) {
        let (window, settings) = install_state_window(cx);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        settings.update(&mut cx, |settings, cx| {
            settings.set_install_state("opencode", Some(InstallState::InFlight));
            cx.notify();
        });
        cx.run_until_parked();
        settings.update(&mut cx, |settings, cx| {
            settings.set_install_state("opencode", None);
            cx.notify();
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agent-install-0").is_some(),
            "success returns the normal action"
        );
        assert!(
            cx.debug_bounds("settings-agent-install-status-0").is_none()
                && cx.debug_bounds("settings-agent-install-reason-0").is_none(),
            "no progress or failure line survives a settled install"
        );
    }

    /// F-SET-17: a failed discovery sweep renders the registry error banner
    /// over the rows from the last successful sweep — never five false
    /// "Not found on PATH" claims and never a blanked list — and the next
    /// successful sweep clears the banner and replaces the rows. The retry
    /// gesture itself is the Agents screen's existing "↻ Refresh" button,
    /// whose wiring `refresh_agents_re_runs_agent_discovery` already pins.
    #[gpui::test]
    async fn registry_error_renders_over_stale_rows_and_clears_on_recovery(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(Theme::init);
        let fixture = vec![AgentAvailability {
            id: "claude",
            display_name: "Claude Code",
            executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
        }];
        let stale = fixture.clone();
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let agents = cx
            .debug_bounds("settings-category-Agents")
            .expect("Agents category is offered");
        cx.simulate_click(agents.center(), Modifiers::none());
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agents-registry-error").is_none(),
            "no banner renders while the sweep has not failed"
        );

        // A sweep that cannot answer.
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |this, cx| {
                this.apply_agent_discovery(Err(DiscoveryError::Probe {
                    program: "claude".to_string(),
                    path: PathBuf::from("/locked/claude"),
                    source: std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "permission denied",
                    ),
                }));
                cx.notify();
            });
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agents-registry-error").is_some(),
            "the failed sweep renders the registry error banner"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-0").is_some(),
            "the rows from the last successful sweep stay on screen"
        );
        let (rendered, message) = cx.update(|window, cx| {
            let settings = window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx);
            (
                settings.provider_availability.clone(),
                settings.agent_registry_error.clone(),
            )
        });
        assert_eq!(rendered, stale, "a failed sweep never rewrites the rows");
        let message = message.expect("the failure is recorded renderably");
        assert!(
            message.starts_with("Could not load the agent registry:"),
            "the message keeps the Swift registry's shape: {message}"
        );

        // The next successful sweep replaces the rows and clears the banner.
        let recovered = vec![AgentAvailability {
            id: "codex",
            display_name: "Codex",
            executable: None,
        }];
        let applied = recovered.clone();
        cx.update(|window, cx| {
            let settings = window.root::<Settings>().flatten().expect("settings root");
            settings.update(cx, |this, cx| {
                this.apply_agent_discovery(Ok(applied));
                cx.notify();
            });
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agents-registry-error").is_none(),
            "a successful sweep clears the banner"
        );
        let rendered = cx.update(|window, cx| {
            window
                .root::<Settings>()
                .flatten()
                .expect("settings root")
                .read(cx)
                .provider_availability
                .clone()
        });
        assert_eq!(rendered, recovered, "recovery replaces the stale rows");
    }

    #[gpui::test]
    async fn a_search_with_no_match_shows_the_empty_state(cx: &mut TestAppContext) {
        cx.update(Theme::init);
        cx.update(bezel::ui::input::init);
        let fixture = vec![
            AgentAvailability {
                id: "claude",
                display_name: "Claude Code",
                executable: Some(PathBuf::from("/opt/homebrew/bin/claude")),
            },
            AgentAvailability {
                id: "opencode",
                display_name: "OpenCode",
                executable: None,
            },
            AgentAvailability {
                id: "omp",
                display_name: "Oh-My-Pi",
                executable: None,
            },
        ];
        let window = cx.add_window(|_window, cx| {
            Settings::with_snapshot(cx, SettingsSnapshot::default()).with_availability(fixture)
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        open_agents_category(&window, &mut cx);

        let search = cx
            .debug_bounds("agent-search-field")
            .expect("the search field is drawn");
        cx.simulate_click(search.center(), Modifiers::none());
        cx.run_until_parked();
        cx.simulate_input("gemini");
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings-agents-empty").is_some(),
            "a query with no match renders the empty state"
        );
        assert!(
            cx.debug_bounds("settings-agent-row-0").is_none()
                && cx.debug_bounds("settings-agent-row-1").is_none()
                && cx.debug_bounds("settings-agent-row-2").is_none(),
            "no row survives a query with no match"
        );
    }
}
