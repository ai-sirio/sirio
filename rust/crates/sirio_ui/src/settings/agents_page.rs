//! Settings › Agents, drawn on bezel's settings scaffolding.
//!
//! Moved out of `settings.rs`: the screen's renderer, its private helpers
//! and its tests live here. State, events, `provider_row` and
//! `installed_integrity_note` stay in `settings.rs`.

use super::*;
use bezel::ui::widgets::{ButtonStyle, Buttons as _, Scaffolding as _};
use gpui::Focusable;

impl Settings {
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
        let query = self
            .agent_search_field
            .read(cx)
            .content()
            .trim()
            .to_lowercase();
        let first_load = self.provider_availability.is_empty()
            && self.agent_registry_error.is_none()
            && query.is_empty();
        let mut agent_rows = controls::card(theme);
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
                agent_rows = agent_rows.child(controls::separator(theme));
            }
            first_visible_row = false;
            let label = div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            IconElement::new(row.icon, IconSize::Small)
                                .text_color(provider_glyph_color(theme, row.id)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.0))
                        .child(
                            div()
                                .debug_selector(move || format!("settings-agent-name-{index}"))
                                .text_size(theme.typography.headline)
                                .text_color(theme.text)
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(text!(id = ("settings-agent-name", index), row.name)),
                        )
                        .child(
                            div()
                                .debug_selector(move || {
                                    format!("settings-agent-description-{index}")
                                })
                                .text_size(theme.typography.footnote)
                                .text_color(theme.text_muted)
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(text!(
                                    id = ("settings-agent-description", index),
                                    row.description
                                )),
                        ),
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
            let install_state = self.install_states.get(availability.id);
            let action = if matches!(install_state, Some(InstallState::InFlight)) {
                None
            } else {
                match &source {
                    sirio_registry::LaunchSource::Installable { .. } => Some((
                        SettingsEvent::InstallAgent(availability.id.to_string()),
                        "Install",
                    )),
                    sirio_registry::LaunchSource::Installed(installed) => self
                        .registry_versions
                        .get(availability.id)
                        .filter(|latest| *latest != &installed.version)
                        .map(|_| {
                            (
                                SettingsEvent::UpdateAgent(availability.id.to_string()),
                                "Update",
                            )
                        }),
                    _ => None,
                }
            };
            let install_control = if matches!(install_state, Some(InstallState::InFlight)) {
                Some(
                    div()
                        .id(("settings-agent-install-spinner", index))
                        .debug_selector(move || format!("settings-agent-install-spinner-{index}"))
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(loading::compact("settings-install-spinner", window, cx)),
                )
            } else {
                action.map(|(event, label)| {
                    let install_entity = entity.clone();
                    div()
                        .id(("settings-agent-install", index))
                        .debug_selector(move || format!("settings-agent-install-{index}"))
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(theme.radii.row_card)
                        .text_size(theme.typography.caption2)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .bg(theme.element_active)
                        .hover(|style| style.bg(theme.element_hover))
                        .on_click(move |_, _, cx| {
                            install_entity.update(cx, |_, cx| {
                                cx.emit(event.clone());
                            });
                        })
                        .child(text!(id = ("settings-agent-install-label", index), label))
                })
            };
            let mut row_container = div()
                .id(("settings-agent-row", index))
                .debug_selector(move || format!("settings-agent-row-{index}"))
                .flex()
                .flex_col()
                .child(controls::row_view(
                    label,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(Self::render_provider_status(availability, theme))
                        .children(row.version.as_ref().map(|version| {
                            div()
                                .px(px(6.0))
                                .text_size(theme.typography.caption2)
                                .text_color(theme.text_faint)
                                .child(text!(
                                    id = ("settings-agent-version", index),
                                    // #197: name the subject. This is the
                                    // ACP server package's version, from
                                    // `sirio_registry` -- the same thing
                                    // the badge and Install button beside
                                    // it are about. Bare, it sat next to
                                    // the CLI's path in the same colour at
                                    // the same size and read as that
                                    // binary's version, which it never was:
                                    // this row said "v0.70.0" beside a
                                    // claude.exe reporting 2.1.247.
                                    format!("ACP v{version}")
                                ))
                        }))
                        .child(Self::render_acp_badge(
                            availability.id.to_string(),
                            &source,
                            theme,
                        ))
                        .children(install_control),
                    theme,
                ));
            if let Some(note) = installed_integrity_note(&source) {
                row_container = row_container.child(
                    div()
                        .id(("settings-agent-integrity", index))
                        .debug_selector(move || format!("settings-agent-integrity-{index}"))
                        .px(px(BezelTheme::SPACE_MD))
                        .text_size(theme.typography.footnote)
                        .text_color(theme.text_muted)
                        .child(text!(note)),
                );
            }
            if let Some(state) = install_state {
                let (kind, label): (&'static str, String) = match state {
                    InstallState::InFlight => (
                        "status",
                        "Installing… this can take up to ten minutes.".to_string(),
                    ),
                    InstallState::Failed(message) => ("reason", message.clone()),
                };
                let failed = matches!(state, InstallState::Failed(_));
                row_container = row_container.child(
                    div()
                        .id((kind, index))
                        .debug_selector(move || format!("settings-agent-install-{kind}-{index}"))
                        .px(px(BezelTheme::SPACE_MD))
                        .text_size(theme.typography.footnote)
                        .font_weight(if failed {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(theme.text_muted)
                        .child(text!(label)),
                );
            }
            agent_rows = agent_rows.child(row_container);
        }

        let bezel_theme = theme.to_bezel_theme();
        let agent_count = self.provider_availability.len();
        let focus_search_field = self.agent_search_field.clone();
        let search_field = self.agent_search_field.clone();
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
        // the last successful sweep, with the "↻ Refresh" button above as
        // the retry — the Swift original's warning label in
        // `AgentsSettingsView`.
        if let Some(error) = self.agent_registry_error.clone() {
            surface = surface.child(
                div()
                    .id("settings-agents-registry-error")
                    .debug_selector(|| "settings-agents-registry-error".to_string())
                    .w_full()
                    .mb(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(theme.typography.footnote)
                    .text_color(theme.warning)
                    .child(text!("⚠"))
                    .child(text!(error)),
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
        } else {
            surface.child(agent_rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

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
}
