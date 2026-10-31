//! The agent's controls under the composer: permission mode, model, effort,
//! fast mode, thinking display, background tasks and the context meter, with
//! the popup each one opens. Every control draws only what the agent reported,
//! so a catalogue it never sent leaves no chevron and no chip behind.

use super::composer_row::RowLayout;
use super::composer_view::{EFFORT_RESET, EFFORT_ULTRACODE, effort_rows};
use super::*;
use ely_gpui_component::{
    forms::Input,
    menus::{Hang, Menu, MenuItem, MenuPanel, layer, panel_surface},
    primitives::{Icon, IconName},
    theme::IconSize,
};
use sirio_acp::EffortChoice;

/// A popup's width at the interface's text size: the 13px design size at
/// rest, wider as the text grows, so a larger setting never cuts a label
/// the default one fits.
pub(super) fn popup_width(theme: &Theme, at_rest: f32) -> Pixels {
    px(at_rest * (f32::from(theme.typography.ui_size) / 13.0).max(1.0))
}

/// How tall step `step` of `total` stands in a level's bar, in pixels: up to
/// five steps rise 2px each from 4px (4, 6, 8, 10, 12), and more than five are
/// spread over the same 4px to 12px.
fn effort_step_height(step: usize, total: usize) -> f32 {
    if total <= 5 {
        4.0 + 2.0 * step as f32
    } else {
        4.0 + 8.0 * step as f32 / (total - 1) as f32
    }
}

/// A level's bar in the effort list: one 3px bar a level, rising, with the
/// first `filled` in the strong tone. Its selector names where it stands.
fn effort_steps(
    selector: String,
    filled: usize,
    total: usize,
    filled_step: Hsla,
    empty_step: Hsla,
) -> AnyElement {
    div()
        .debug_selector(move || selector)
        .flex()
        .items_end()
        .gap(px(2.0))
        .h(px(12.0))
        .children((0..total).map(|step| {
            div()
                .w(px(3.0))
                .h(px(effort_step_height(step, total)))
                .rounded(px(1.0))
                .bg(if step < filled {
                    filled_step
                } else {
                    empty_step
                })
        }))
        .into_any_element()
}

/// The Lucide icon the status pill draws for a permission mode: one per
/// Claude mode, and a shield for any id Claude does not use, such as an ACP
/// agent's own, so a known catalogue never leaves the pill without one.
pub(crate) fn mode_icon(mode_id: &str) -> IconName {
    match mode_id {
        "default" => IconName::Hand,
        "acceptEdits" => IconName::FilePen,
        "plan" => IconName::ClipboardList,
        "auto" => IconName::Sparkles,
        BYPASS_PERMISSIONS_MODE => IconName::ShieldOff,
        _ => IconName::Shield,
    }
}

impl Chat {
    /// The name the model chip shows: the selected model's, whatever else the
    /// agent knows it by.
    pub(super) fn selected_model_name(&self) -> String {
        self.selected_model
            .as_deref()
            .and_then(|selected| {
                self.available_models
                    .iter()
                    .find(|option| option.id == selected)
                    .map(|option| option.name.clone())
            })
            .or_else(|| self.selected_model.clone())
            .unwrap_or_else(|| "Claude Code".into())
    }

    /// The effort level's own name, as the agent reported it. Not uppercased
    /// any more: it used to be a bare caption that needed to read as chrome,
    /// and now it stands on its own, as the model's name does.
    pub(super) fn effort_level_name(&self) -> Option<String> {
        let effort = self.effort.as_ref()?;
        let current = effort.current_value.as_ref()?;
        Some(
            effort
                .choices
                .iter()
                .find(|choice| choice.value == *current)
                .map(|choice| choice.name.clone())
                .unwrap_or_else(|| current.clone()),
        )
    }

    /// How many steps the effort bars fill, and of how many, for a level that
    /// stands on the model's own scale; `None` for one that does not (the reset,
    /// Ultracode).
    pub(super) fn effort_bar_position(&self) -> Option<(usize, usize)> {
        let effort = self.effort.as_ref()?;
        let levels = effort_rows(&effort.choices).levels;
        let current = effort.current_value.as_deref()?;
        let filled = levels.iter().position(|level| level.value == current)? + 1;
        Some((filled, levels.len()))
    }

    /// Fast mode, while there is a control to draw for it: the session reported
    /// the feature and the model controls are shown. The chip in the Full row,
    /// the row in ⋯ in the Compact one and the width the Full row needs all ask
    /// this, so they agree.
    pub(super) fn fast_state(&self) -> Option<&FastMode> {
        self.fast_mode
            .as_ref()
            .filter(|_| self.model_control_visible())
    }

    /// The thinking display, while there is one to draw: the transport has it
    /// and the CLI has not refused the verb. The chip in the Full row and the
    /// row in ⋯ in the Compact one both ask this, so they appear together.
    pub(super) fn thinking_state(&self) -> Option<ThinkingDisplay> {
        self.thinking_display
            .clone()
            .filter(|state| !state.unsupported)
            .filter(|_| self.model_control_visible())
    }

    /// The thinking picker while it is open, hung from the control the caller
    /// puts it in: the chip in the Full row (`Hang::AboveEnd`, growing leftward
    /// from the right-aligned group) or ⋯ in the Compact one (`Hang::Above`,
    /// growing rightward from ⋯, which is in the left group).
    pub(super) fn render_thinking_picker(
        &self,
        theme: &Theme,
        hang: Hang,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !self.thinking_picker_open {
            return None;
        }
        let chosen = self.thinking_state().and_then(|state| state.chosen);
        let rows = self.picker_rows(cx);
        // The agent's own answer is a row, not the absence of one:
        // choosing it is how a user undoes a choice they made.
        let menu = rows.iter().fold(Menu::new(), |menu, row| {
            let PickerRow::Thinking(value) = row else {
                return menu;
            };
            let id = value.unwrap_or("default");
            let label = value.map_or("Agent's own", thinking_display_name);
            menu.item(
                MenuItem::radio(label, chosen.as_deref() == *value)
                    .selectors(format!("thinking-option-{id}"), None)
                    .on_click(self.picker_click(row.clone(), cx)),
            )
        });
        Some(layer(
            "thinking-picker-menu",
            hang,
            div()
                .id("thinking-picker")
                .debug_selector(|| "thinking-picker".into())
                .key_context("ChatThinkingPicker")
                .track_focus(&self.thinking_picker_focus)
                .on_action(cx.listener(Self::cancel))
                .on_action(cx.listener(Self::picker_previous))
                .on_action(cx.listener(Self::picker_next))
                .on_action(cx.listener(Self::picker_choose))
                .w(popup_width(theme, 200.0))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.thinking_picker_open = false;
                    cx.notify();
                }))
                .child(
                    MenuPanel::new("thinking-picker-rows", menu)
                        .current(self.marked_picker_row(rows.len())),
                ),
            cx,
        ))
    }

    /// The control row's chips, each with the popup it hangs. One element so
    /// the composer takes it as a single trailing slot; it is one line, and
    /// the model chip, which may shrink, is what gives on a narrow one.
    pub(super) fn render_agent_controls(
        &self,
        theme: &Theme,
        layout: RowLayout,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let compact = layout == RowLayout::Compact;
        let typography = theme.typography;
        let bezel_theme = bezel_theme::Theme::of(cx).clone();
        let entity = cx.entity();

        // Swift's `modePill` (ComposerControlBar.swift) pairs a status dot
        // with the permission mode's name: the dot carries the connection
        // state, the label the mode, and a raw state word ("idle"/"working")
        // only stands in while no mode is known. The port used to let
        // "working" and "connecting" take the label over from the mode, so
        // the permission the user picked was unreadable exactly while a
        // turn ran (`status_pill_content`, `composer_view.rs`).
        let connecting = self.connecting;
        // F-CHAT-15 / #136: a known mode names itself from the first frame,
        // never waiting on `has_completed_turn`.
        let (dot, label) = composer_view::status_pill_content(
            connecting,
            self.streaming,
            self.client.is_some(),
            self.mode_catalog.as_ref(),
        );
        let dot = dot.color();
        let mode_selectable = self.mode_selectable();
        let status_pill = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .h(px(24.0))
            .px(px(7.0))
            .rounded(theme.radii.control)
            .text_size(typography.ui_size);
        let status_pill = if connecting {
            status_pill
                .id("chat-connecting")
                .debug_selector(|| "chat-connecting".into())
        } else {
            status_pill
                .id("chat-status")
                .debug_selector(|| "chat-status".into())
        };
        // The agent's own words for the mode, whole, for a label a narrow
        // pane has to cut.
        let mode_tooltip = SharedString::from(label.to_string());
        // Once the agent has advertised its modes the pill shows the current
        // one as an icon, with the connection state a small dot on its
        // corner; before that it keeps the plain dot and the state word.
        // Bypass permissions is the one mode drawn as a warning.
        let current_mode = self
            .mode_catalog
            .as_ref()
            .map(|catalog| catalog.current_id.as_str());
        let danger = current_mode == Some(BYPASS_PERMISSIONS_MODE);
        let label_color = if danger {
            theme.ely.danger
        } else {
            theme.ely.fg
        };
        let mode_lead = match current_mode {
            Some(mode_id) => {
                let icon = mode_icon(mode_id);
                let icon_selector = format!("mode-icon-{}", icon.name());
                div()
                    .relative()
                    .flex_none()
                    .w(px(14.0))
                    .h(px(14.0))
                    .debug_selector(move || icon_selector)
                    .child(Icon::new(icon).size(IconSize::Sm).color(label_color))
                    .child(
                        div()
                            .absolute()
                            .right(px(-3.5))
                            .bottom(px(-3.5))
                            .w(px(9.0))
                            .h(px(9.0))
                            .rounded(px(4.5))
                            .p(px(1.5))
                            .bg(theme.ely.surface)
                            .debug_selector(|| "mode-connection-dot".into())
                            .child(div().size_full().rounded(px(3.0)).bg(dot)),
                    )
                    .into_any_element()
            }
            None => div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(dot)
                .into_any_element(),
        };
        let status_pill = status_pill
            .tooltip(crate::controls::tooltip(mode_tooltip))
            .when(danger, |this| this.bg(theme.ely.danger_subtle))
            .when(mode_selectable, |this| {
                this.hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_mode_picker(window, cx);
                    }))
            })
            .child(mode_lead)
            // Compact keeps the icon (or the dot) and the chevron: the name is
            // the tooltip's.
            .when(!compact, |this| {
                this.child(
                    div()
                        .debug_selector(|| "mode-label".into())
                        .text_color(label_color)
                        .child(label),
                )
            })
            .when(mode_selectable, |this| this.child(picker_chevron(theme)));
        // The pill's selector is taken, so the danger fill is named on a
        // wrapper that is exactly as big as the pill.
        let status_pill = if danger {
            div()
                .flex()
                .flex_none()
                .debug_selector(|| "mode-chip-danger".into())
                .child(status_pill)
                .into_any_element()
        } else {
            status_pill.into_any_element()
        };

        let selected_model_name = self.selected_model_name();
        let effort_label = self.effort_level_name();
        // The picker chip — the value in title text and a chevron, with no key
        // label: the name speaks for itself — opens only once a turn has
        // completed and the agent reported models. Without models the pill
        // degrades to a plain agent badge (F-CHAT-36): no chevron, no picker.
        let model_control = if self.model_control_visible() {
            let model_selection_id = self
                .selected_model
                .as_deref()
                .map(|id| format!("model-selection-{id}"))
                .unwrap_or_else(|| "model-selection-none".into());
            let model_tooltip = SharedString::from(format!("Model: {selected_model_name}"));
            div()
                .id("model-chip")
                .debug_selector(|| "model-chip".into())
                .tooltip(crate::controls::tooltip(model_tooltip))
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Sized to its content, not to the row. It used to carry
                // `flex_1`, which stretched the pill the whole width of the
                // control row and stranded its own chevron ~200px from the
                // model name, next to the overflow button -- so the chevron
                // read as belonging to nothing and the model value read as a
                // caption rather than a picker. It still shrinks on a tight
                // row -- that is what keeps the name's ellipsis working --
                // but a 56px floor stops it collapsing to nothing: the name
                // ellipsizes down to the floor, never erasing the picker. The
                // row is one line and compacts instead of wrapping (the
                // compact layout is the band's Task 3).
                .min_w(px(56.0))
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
                .child(
                    div()
                        .id(model_selection_id.clone())
                        .debug_selector(move || model_selection_id)
                        // No `flex_1`. Inert on its own now that the pill
                        // hugs its content -- there is no slack left inside
                        // to absorb, and removing it alone does not move the
                        // chevron, which the test below confirms. It goes
                        // because the two together are what stranded the
                        // chevron: restore `flex_1` on the pill and this
                        // would push it to the far edge again.
                        // `min_w_0` + `text_ellipsis` do the real work,
                        // truncating a long name when the row is tight.
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.ely.fg)
                        .child(selected_model_name.clone()),
                )
                .child(
                    div()
                        .id("model-chip-chevron")
                        .debug_selector(|| "model-chip-chevron".into())
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .child(picker_chevron(theme)),
                )
        } else {
            // #206: this badge names the *agent*, so it reads the agent.
            // It used to render `selected_model_name`, a model variable
            // whose fallback is the literal "Claude Code" -- so it named
            // the wrong agent for every other one until models arrived,
            // and named an agent at all for a chat whose banner two
            // inches above says the agent is unknowable. `agent_name` is
            // `None` in exactly that case, which is the case the banner
            // is about.
            let agent_badge_name = self.agent_badge_name();
            div()
                .id("agent-badge")
                .debug_selector(|| "agent-badge".into())
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                // Same rule as the chip above: hug the content.
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .text_ellipsis()
                        .text_color(theme.ely.fg)
                        .child(agent_badge_name),
                )
        };

        // The effort level is a peer of the model, not a caption inside it:
        // it is changed about as often, so it belongs at the same depth. It
        // reads as its bars (how high the level stands on the model's own
        // scale) and its name. Drawn only when the agent reports a value AND
        // the picker can actually open, so this is never a click target that
        // leads nowhere. `flex_none` keeps it intact while the model chip
        // beside it absorbs the squeeze on a narrow pane. The row is one line
        // and compacts instead of wrapping (the band's Task 3).
        let effort_bars = self.effort_bar_position();
        let effort_control = effort_label
            .filter(|_| self.model_control_visible())
            .map(|label| {
                let bars_stand_in = compact && effort_bars.is_some();
                let effort_entity = entity.clone();
                div()
                    .id("effort-chip")
                    .debug_selector(|| "effort-chip".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(move |_, window, cx| {
                        effort_entity.update(cx, |chat, cx| chat.toggle_effort_picker(window, cx));
                    })
                    // Not a level (the reset, Ultracode): no bars.
                    .children(effort_bars.map(|(filled, total)| {
                        effort_steps(
                            "effort-chip-bars".into(),
                            filled,
                            total,
                            theme.ely.fg_muted,
                            theme.ely.fg_muted.opacity(0.3),
                        )
                    }))
                    // Compact drops the level's name where the bars say it;
                    // a level with no bars has only its name to show.
                    .when(bars_stand_in, |chip| {
                        chip.tooltip(crate::controls::tooltip(SharedString::from(
                            label.clone(),
                        )))
                    })
                    .when(!bars_stand_in, |chip| {
                        chip.child(
                            div()
                                .id("model-effort-label")
                                .debug_selector(|| "model-effort-label".into())
                                .text_color(theme.ely.fg)
                                .child(
                                    div()
                                        .debug_selector(|| "effort-chip-level".into())
                                        .child(label.clone()),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .child(picker_chevron(theme)),
                    )
            });

        // Fast mode: a toggle, not a picker, so it is a chip that shows its
        // own state rather than one that opens a list. Drawn only when the
        // session reported the feature at all — an older CLI says nothing
        // and gets no chip, rather than a dead one. When the CLI has
        // refused it, the chip stays and carries the CLI's own sentence,
        // the way a missing language server names the program.
        let fast_control = self
            .fast_state()
            .cloned()
            .filter(|_| !compact)
            .map(|fast| {
                let blocked = fast.blocked_by.clone();
                let fast_entity = entity.clone();
                let selector = if blocked.is_some() {
                    "fast-mode-chip-blocked"
                } else {
                    "fast-mode-chip"
                };
                let label_colour: gpui::Hsla = match (&blocked, fast.enabled) {
                    (Some(_), _) => theme.ely.fg_subtle,
                    // On, the chip is filled with the theme's own accent
                    // pair rather than a colour of Sirio's: the palette is
                    // bezel's, all of it.
                    (None, true) => bezel_theme.on_solid,
                    (None, false) => theme.ely.fg,
                };
                div()
                    .id("fast-mode-chip")
                    .debug_selector(move || selector.into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .when(fast.enabled && blocked.is_none(), |chip| {
                        chip.bg(bezel_theme.solid)
                    })
                    .when(blocked.is_none(), |chip| {
                        chip.hover(|style| style.bg(bezel_theme.element_hover))
                            .on_click(move |_, _, cx| {
                                fast_entity.update(cx, |chat, cx| chat.toggle_fast_mode(cx));
                            })
                    })
                    .when_some(blocked, |chip, reason| {
                        let reason = SharedString::from(reason);
                        chip.tooltip(crate::controls::tooltip(reason))
                    })
                    .child(div().text_color(label_colour).child("Fast"))
            });

        // Background tasks: a count, not a list. The CLI sends the whole
        // running set on every change and drains it as each task ends, so
        // this is only ever "how many are still going" — what *finished*
        // arrives separately, as a notice in the transcript, because the
        // list is already empty by the time that is known.
        let background_tasks = (self.background_tasks > 0).then(|| {
            let running = self.background_tasks;
            div()
                .id("background-tasks-chip")
                .debug_selector(|| "background-tasks-chip".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                .text_color(theme.ely.fg_subtle)
                .tooltip(crate::controls::tooltip(if running == 1 {
                    "1 background task running".to_string()
                } else {
                    format!("{running} background tasks running")
                }))
                .child(format!("⌁ {running}"))
        });

        // The thinking display. Drawn only while the transport has it and
        // the CLI has not refused the verb; unset shows no value at all,
        // because nothing reports the session's own and a word here would
        // be one nobody read (F-CHAT-18's rule, as the effort chip already
        // applies it).
        // Compact leaves Fast and Thinking out of the row: they are rows in ⋯.
        let thinking_control = self.thinking_state().filter(|_| !compact).map(|state| {
            let thinking_entity = entity.clone();
            let chosen = state.chosen.clone();
            div()
                .id("thinking-chip")
                .debug_selector(|| "thinking-chip".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(7.0))
                .rounded(theme.radii.control)
                .text_size(typography.ui_size)
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(move |_, window, cx| {
                    thinking_entity.update(cx, |chat, cx| chat.toggle_thinking_picker(window, cx));
                })
                .child(div().text_color(theme.ely.fg_subtle).child("Thinking"))
                .when_some(chosen, |chip, chosen| {
                    chip.child(
                        div()
                            .debug_selector(|| "thinking-chip-value".into())
                            .text_color(theme.ely.fg)
                            .child(thinking_display_name(&chosen).to_string()),
                    )
                })
        });

        let thinking_picker = (!compact)
            .then(|| self.render_thinking_picker(theme, Hang::AboveEnd, cx))
            .flatten();

        // The effort list, anchored to the chip above. The levels are the
        // session's own — `supportedEffortLevels` differs per model — so the
        // list grows and shrinks with the model instead of naming rungs the
        // model does not have. Each level shows its step on that scale;
        // Ultracode is no further step, and the reset is the model's own pick,
        // set apart below them.
        let effort_picker = self
            .effort_picker_open
            .then(|| self.effort.clone())
            .flatten()
            .map(|effort| {
                let split = effort_rows(&effort.choices);
                let current = effort.current_value.as_deref().unwrap_or(EFFORT_RESET);
                let (filled_step, empty_step) =
                    (theme.ely.fg_muted, theme.ely.fg_muted.opacity(0.3));
                let row = |choice: &EffortChoice| {
                    let value = choice.value.clone();
                    let note_selector =
                        (value == EFFORT_ULTRACODE).then(|| "effort-note-ultracode".into());
                    MenuItem::check(choice.name.clone(), current == value)
                        .selectors(format!("effort-option-{value}"), note_selector)
                        .on_click(self.picker_click(PickerRow::Effort(value), cx))
                };

                // The menu's entries in the order drawn, `None` being a separator.
                // This is the one place the order is written down.
                let total = split.levels.len();
                let mut entries: Vec<Option<MenuItem>> = Vec::new();
                for (index, level) in split.levels.iter().enumerate() {
                    let filled = index + 1;
                    let selector = format!("effort-steps-{}-{filled}-{total}", level.value);
                    entries.push(Some(row(level).trailing(move |_| {
                        effort_steps(selector.clone(), filled, total, filled_step, empty_step)
                    })));
                }
                if let Some(ultracode) = &split.ultracode {
                    entries.push(Some(row(ultracode).note("+ workflow")));
                }
                if let Some(reset) = &split.reset {
                    if !entries.is_empty() {
                        entries.push(None);
                    }
                    entries.push(Some(row(reset).muted(true).note("model's own")));
                }
                // The keys mark a row, and `MenuPanel` marks an entry: a
                // separator is an entry no key lands on.
                let mut entry_of_row = Vec::new();
                let mut menu = Menu::new();
                for (entry, item) in entries.into_iter().enumerate() {
                    match item {
                        Some(item) => {
                            entry_of_row.push(entry);
                            menu = menu.item(item);
                        }
                        None => menu = menu.separator(),
                    }
                }
                let marked = self
                    .marked_picker_row(entry_of_row.len())
                    .map(|at| entry_of_row[at]);

                layer(
                    "effort-picker-menu",
                    Hang::AboveEnd,
                    div()
                        .id("effort-picker")
                        .debug_selector(|| "effort-picker".into())
                        .key_context("ChatEffortPicker")
                        .track_focus(&self.effort_picker_focus)
                        .on_action(cx.listener(Self::cancel))
                        .on_action(cx.listener(Self::picker_previous))
                        .on_action(cx.listener(Self::picker_next))
                        .on_action(cx.listener(Self::picker_choose))
                        .w(popup_width(theme, 220.0))
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.effort_picker_open = false;
                            cx.notify();
                        }))
                        .child(MenuPanel::new("effort-picker-rows", menu).current(marked)),
                    cx,
                )
            });

        let model_picker = self.model_picker_open.then(|| {
            // F-CHAT-16: "Recommended" is not a protocol flag — `ModelOption`
            // has none, and the ACP layer never carries one — it is purely
            // the driver's own first-listed choice, matching Swift's
            // `recommendedId = models.first?.modelId` exactly. The search
            // filter runs over the full, unfiltered list order (`filter`,
            // not `sort`) so a query never reorders results.
            let recommended_id = self
                .available_models
                .first()
                .map(|option| option.id.clone());
            let selected_id = self.selected_model.clone();
            let rows = self.picker_rows(cx);
            let menu = rows.iter().fold(Menu::new(), |menu, row| {
                let PickerRow::Model(option) = row else {
                    return menu;
                };
                let is_recommended = recommended_id.as_deref() == Some(option.id.as_str());
                let item = MenuItem::radio(
                    option.name.clone(),
                    selected_id.as_deref() == Some(option.id.as_str()),
                )
                .tooltip(option.name.clone())
                .selectors(
                    format!("model-option-{}", option.id),
                    is_recommended.then(|| "model-option-recommended".into()),
                )
                .on_click(self.picker_click(row.clone(), cx));
                menu.item(if is_recommended {
                    item.note("Recommended")
                } else {
                    item
                })
            });
            let no_models = self.available_models.is_empty();
            let empty = if no_models {
                div()
                    .px_2()
                    .py_1p5()
                    .text_size(typography.footnote)
                    .text_color(theme.ely.fg_subtle)
                    .child("The connected agent did not report any models.")
                    .into_any_element()
            } else {
                div()
                    .id("model-picker-no-match")
                    .debug_selector(|| "model-picker-no-match".into())
                    .px_2()
                    .py_1p5()
                    .text_size(typography.footnote)
                    .text_color(theme.ely.fg_subtle)
                    .child("No models match")
                    .into_any_element()
            };
            let panel = MenuPanel::new("model-picker-rows", menu)
                .current(self.marked_picker_row(rows.len()))
                .scroll(&self.model_picker_scroll, px(MODEL_PICKER_LIST_MAX_H))
                .selector("model-picker-list")
                .empty(empty)
                .when(!no_models, |panel| {
                    panel.head(
                        div()
                            .id("model-search-input")
                            .debug_selector(|| "model-search-input".into())
                            .w_full()
                            .mb(px(6.0))
                            .child(Input::new(&self.model_search_field)),
                    )
                });
            layer(
                "model-picker-menu",
                Hang::AboveEnd,
                div()
                    .id("model-picker")
                    .debug_selector(|| "model-picker".into())
                    .key_context("ChatModelPicker")
                    .on_action(cx.listener(Self::cancel))
                    .on_action(cx.listener(Self::picker_previous))
                    .on_action(cx.listener(Self::picker_next))
                    .on_action(cx.listener(Self::picker_choose))
                    .w(popup_width(theme, 245.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.model_picker_open = false;
                        cx.notify();
                    }))
                    .child(panel),
                cx,
            )
        });

        // F-CHAT-15: the session-mode picker, anchored above the status
        // pill the same way `model_picker` anchors above the model chip.
        // No search field — mode lists are small and entirely agent-defined
        // (ask/plan/auto today), so a flat list of rows is enough.
        let mode_picker = self.mode_picker_open.then(|| {
            let current_id = self
                .mode_catalog
                .as_ref()
                .map(|catalog| catalog.current_id.clone())
                .unwrap_or_default();
            let rows = self.picker_rows(cx);
            let menu = rows.iter().fold(Menu::new(), |menu, row| {
                let PickerRow::Mode(mode) = row else {
                    return menu;
                };
                menu.item(
                    MenuItem::radio(mode.name.clone(), mode.id == current_id)
                        .tooltip(mode.name.clone())
                        .selectors(format!("mode-option-{}", mode.id), None)
                        .on_click(self.picker_click(row.clone(), cx)),
                )
            });
            layer(
                "mode-picker-menu",
                // The pill is the first chip of the right-aligned group: from
                // its right edge the picker would cross the chat's left edge in
                // a narrow chat, so it grows rightward from its left.
                Hang::Above,
                div()
                    .id("mode-picker")
                    .debug_selector(|| "mode-picker".into())
                    .key_context("ChatModelPicker")
                    .track_focus(&self.mode_picker_focus)
                    .on_action(cx.listener(Self::cancel))
                    .on_action(cx.listener(Self::picker_previous))
                    .on_action(cx.listener(Self::picker_next))
                    .on_action(cx.listener(Self::picker_choose))
                    .w(popup_width(theme, 200.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.mode_picker_open = false;
                        cx.notify();
                    }))
                    .child(
                        MenuPanel::new("mode-picker-rows", menu)
                            .current(self.marked_picker_row(rows.len()))
                            .empty(
                                div()
                                    .px_2()
                                    .py_1p5()
                                    .text_size(typography.footnote)
                                    .text_color(theme.ely.fg_subtle)
                                    .child("No modes offered"),
                            ),
                    ),
                cx,
            )
        });

        let context_usage = self.context_usage.clone();
        let context_amount = context_usage
            .as_ref()
            .filter(|usage| usage.size > 0)
            .map(|usage| (usage.used as f32 / usage.size as f32).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let context_ring_entity = entity.clone();
        // Above the 80% warning threshold the arc switches from the gauge
        // blue to the shared danger token, the same rule as the reference
        // `contextRingColor`. The warning wrapper element exists only in
        // that state, so drawn tests can see the colour decision without
        // reading pixels.
        let context_warning = context_amount > 0.8;
        let warning_element = div()
            .id("context-ring-warning")
            .debug_selector(|| "context-ring-warning".into());
        let context_ring = div()
            .id("context-ring")
            .debug_selector(|| "context-ring".into())
            .relative()
            .w(px(16.0))
            .h(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.ely.border)
            .when(compact, |this| {
                this.tooltip(crate::controls::tooltip("Context"))
            })
            .hover(|style| style.bg(theme.sirio.overlay))
            .on_click(move |_, window, cx| {
                context_ring_entity.update(cx, |chat, cx| chat.toggle_context_popover(window, cx));
            })
            .when(context_warning, |this| this.child(warning_element))
            .child(
                div()
                    .id("context-ring-progress")
                    .debug_selector(|| "context-ring-progress".into())
                    .absolute()
                    .inset_0()
                    .child({
                        // The paint closure is `'static`, so it cannot borrow
                        // `theme`; copy out the two colours it draws with.
                        let ring_danger = theme.ely.danger;
                        let ring_accent = theme.sirio.quantity;
                        canvas(
                            move |_, _, _| {},
                            move |bounds, _, window, _| {
                                if context_amount <= 0.0 {
                                    return;
                                }
                                let origin_x: f32 = bounds.origin.x.into();
                                let origin_y: f32 = bounds.origin.y.into();
                                let width: f32 = bounds.size.width.into();
                                let center =
                                    point(px(origin_x + width / 2.0), px(origin_y + width / 2.0));
                                let radius = width / 2.0 - 2.0;
                                let start = point(center.x, px(origin_y + 1.0));
                                let angle = -std::f32::consts::FRAC_PI_2
                                    + context_amount * std::f32::consts::TAU;
                                let end = point(
                                    px(origin_x + width / 2.0 + radius * angle.cos()),
                                    px(origin_y + width / 2.0 + radius * angle.sin()),
                                );
                                let mut path = PathBuilder::stroke(px(2.0));
                                path.move_to(start);
                                if context_amount >= 1.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        point(center.x, px(origin_y + width - 1.0)),
                                    );
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        false,
                                        true,
                                        start,
                                    );
                                } else if context_amount > 0.0 {
                                    path.arc_to(
                                        point(px(radius), px(radius)),
                                        px(0.0),
                                        context_amount > 0.5,
                                        true,
                                        end,
                                    );
                                }
                                if let Ok(path) = path.build() {
                                    window.paint_path(
                                        path,
                                        if context_warning {
                                            ring_danger
                                        } else {
                                            ring_accent
                                        },
                                    );
                                }
                            },
                        )
                        .absolute()
                        .size_full()
                    }),
            );

        let context_popover = if self.context_popover_open {
            let usage = context_usage.clone();
            Some(layer(
                "context-popover-menu",
                Hang::AboveEnd,
                div()
                    .id("context-popover")
                    .debug_selector(|| "context-popover".into())
                    .key_context("ChatContextPopover")
                    .track_focus(&self.context_popover_focus)
                    .on_action(cx.listener(Self::cancel))
                    .w(popup_width(theme, 285.0))
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.context_popover_open = false;
                        cx.notify();
                    }))
                    .child(
                        panel_surface("context-popover-surface", cx).p_2().child(
                            div()
                                .when_some(usage, |this, usage| {
                                    // `None` is a state of its own, not a zero: a
                                    // session whose `usage_update` named no window size
                                    // has nothing to report, and folding that into "0%"
                                    // would claim an empty context nobody measured.
                                    // Agents that never send a usage update (Pi's ACP
                                    // adapter among them) are exactly the ones a
                                    // fabricated zero would libel.
                                    let percent = if usage.size == 0 {
                                        None
                                    } else {
                                        Some(
                                            ((usage.used as f64 / usage.size as f64) * 100.0)
                                                .round()
                                                as u64,
                                        )
                                    };
                                    let cost = usage.cost.map(|cost| {
                                        format!("Cost: {:.2} {}", cost.amount, cost.currency)
                                    });
                                    let this = match percent {
                            Some(percent) => this
                                .child(
                                    div()
                                        .id(format!(
                                            "context-usage-{}-of-{}",
                                            usage.used, usage.size
                                        ))
                                        .debug_selector(move || {
                                            format!(
                                                "context-usage-{}-of-{}",
                                                usage.used, usage.size
                                            )
                                        })
                                        .text_size(typography.footnote)
                                        .text_color(theme.ely.fg)
                                        .child(format!("{percent}% of context used")),
                                )
                                .child(
                                    div()
                                        .mt(px(4.0))
                                        .text_size(typography.caption2)
                                        .text_color(theme.ely.fg_subtle)
                                        .child(format!("{} / {} tokens", usage.used, usage.size)),
                                ),
                            // A partial update — the breakdown merged in
                            // before any used/size pair, or one that named
                            // no window — must not read as "0 of 0 tokens":
                            // the breakdown rows below are real, the
                            // fraction simply is not known.
                            None => this.child(
                                div()
                                    .id("context-usage-unreported")
                                    .debug_selector(|| "context-usage-unreported".into())
                                    .text_size(typography.footnote)
                                    .text_color(theme.ely.fg_subtle)
                                    .child("The agent has not reported its context window size."),
                            ),
                        };
                                    this.when_some(cost, |this, cost| {
                                        this.child(
                                            div()
                                                .mt(px(4.0))
                                                .text_size(typography.caption2)
                                                .text_color(theme.ely.fg_subtle)
                                                .child(cost),
                                        )
                                    })
                                    // F-CHAT-18: input/output/cache breakdown, present
                                    // only for agents that report end-of-turn usage
                                    // (`unstable_end_turn_token_usage`) -- absent for
                                    // every other agent, so the rows are opt-in rather
                                    // than showing zeros.
                                    .when(
                                        usage.input_tokens.is_some()
                                            || usage.output_tokens.is_some()
                                            || usage.cached_read_tokens.is_some(),
                                        |this| {
                                            this.child(
                                                div()
                                                    .id("context-usage-breakdown")
                                                    .debug_selector(|| {
                                                        "context-usage-breakdown".into()
                                                    })
                                                    .mt(px(6.0))
                                                    .pt(px(6.0))
                                                    .border_t_1()
                                                    .border_color(theme.ely.border)
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(2.0))
                                                    .when_some(
                                                        usage.input_tokens,
                                                        |this, tokens| {
                                                            this.child(
                                                                div()
                                                                    .text_size(typography.caption2)
                                                                    .text_color(theme.ely.fg_subtle)
                                                                    .child(format!(
                                                                        "Input: {tokens} tokens"
                                                                    )),
                                                            )
                                                        },
                                                    )
                                                    .when_some(
                                                        usage.output_tokens,
                                                        |this, tokens| {
                                                            this.child(
                                                                div()
                                                                    .text_size(typography.caption2)
                                                                    .text_color(theme.ely.fg_subtle)
                                                                    .child(format!(
                                                                        "Output: {tokens} tokens"
                                                                    )),
                                                            )
                                                        },
                                                    )
                                                    .when_some(
                                                        usage.cached_read_tokens,
                                                        |this, tokens| {
                                                            this.child(
                                                div()
                                                    .text_size(typography.caption2)
                                                    .text_color(theme.ely.fg_subtle)
                                                    .child(format!("Cache read: {tokens} tokens")),
                                            )
                                                        },
                                                    ),
                                            )
                                        },
                                    )
                                })
                                .when(self.context_usage.is_none(), |this| {
                                    this.child(
                                        div()
                                            .id("context-usage-never-reported")
                                            .debug_selector(|| {
                                                "context-usage-never-reported".into()
                                            })
                                            .text_size(typography.footnote)
                                            .text_color(theme.ely.fg_subtle)
                                            .child("The agent has not reported context usage yet."),
                                    )
                                }),
                        ),
                    ),
                cx,
            ))
        } else {
            None
        };

        // The chips clip: on a pane too narrow for all of them the rightmost
        // are cut at this element's edge, and send, which the band keeps after
        // it, is never covered.
        let chips_frame = self.composer_row.clone();
        div()
            .debug_selector(|| "composer-chips".into())
            .relative()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .overflow_hidden()
            // Measures the container; paints nothing.
            .child(
                canvas(
                    move |bounds, _, _| chips_frame.chips.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(status_pill)
                    .children(mode_picker),
            )
            // The one shrinkable chip: on a tight line it ellipsizes its name
            // down to its 56px floor, never to nothing. The floor is the
            // wrapper's too: a wrapper sized to its content would hold the
            // name whole and leave the row nothing to shrink.
            .child(
                div()
                    .relative()
                    .min_w(px(56.0))
                    .child(model_control)
                    .children(model_picker),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .children(effort_control)
                    .children(effort_picker),
            )
            .child(div().relative().flex_none().children(fast_control))
            .child(div().relative().flex_none().children(background_tasks))
            .child(
                div()
                    .relative()
                    .flex_none()
                    .children(thinking_control)
                    .children(thinking_picker),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(theme.radii.control)
                    .text_size(typography.ui_size)
                    .child(context_ring)
                    // Compact keeps the ring, which is the control, and leaves
                    // its caption out.
                    .when(!compact, |this| {
                        this.child(
                            div()
                                .id("context-label")
                                .debug_selector(|| "context-label".into())
                                .text_color(theme.ely.fg_subtle)
                                .child("Context"),
                        )
                    })
                    .children(context_popover),
            )
            .into_any_element()
    }
}

/// One row of the open catalogue popup, as a press and the keys both see it.
#[derive(Clone, Debug)]
pub(super) enum PickerRow {
    Model(ModelOption),
    Mode(AgentMode),
    Thinking(Option<&'static str>),
    /// An effort level, by its wire value.
    Effort(String),
    Overflow(OverflowRow),
}

/// The rows of the composer's "more" menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverflowRow {
    /// Fast mode, a toggle. Only in the Compact row, which has no chip for it.
    Fast,
    /// The thinking picker. Only in the Compact row.
    Thinking,
    /// The session panel's toggle. Only in the Compact row.
    SessionPanel,
    Follow,
    NewConversation,
    History,
}

impl Chat {
    /// The rows of whichever catalogue popup is open, in the order drawn.
    pub(super) fn picker_rows(&self, cx: &App) -> Vec<PickerRow> {
        if self.model_picker_open {
            let query = self.model_search_field.read(cx).text().to_owned();
            self.available_models
                .iter()
                .filter(|option| model_query_matches(option, &query))
                .cloned()
                .map(PickerRow::Model)
                .collect()
        } else if self.mode_picker_open {
            self.mode_catalog
                .as_ref()
                .map(|catalog| {
                    catalog
                        .options
                        .iter()
                        .cloned()
                        .map(PickerRow::Mode)
                        .collect()
                })
                .unwrap_or_default()
        } else if self.thinking_picker_open {
            thinking_rows().map(PickerRow::Thinking).collect()
        } else if self.effort_picker_open {
            self.effort
                .as_ref()
                .map(|effort| {
                    effort_rows(&effort.choices)
                        .in_order()
                        .map(|choice| PickerRow::Effort(choice.value.clone()))
                        .collect()
                })
                .unwrap_or_default()
        } else if self.overflow_open {
            self.overflow_rows()
                .into_iter()
                .map(PickerRow::Overflow)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// What the current layout moves out of the row and into ⋯, whether or not
    /// ⋯ is open: in the Full row, nothing. In order, these are the rows ahead
    /// of the menu's own.
    pub(super) fn overflowed_rows(&self) -> Vec<OverflowRow> {
        if self.composer_row.layout.get() != RowLayout::Compact {
            return Vec::new();
        }
        [
            self.fast_state().map(|_| OverflowRow::Fast),
            self.thinking_state().map(|_| OverflowRow::Thinking),
            Some(OverflowRow::SessionPanel),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Every row of ⋯, in the order drawn: what the layout moved out of the
    /// row, then the menu's own.
    pub(super) fn overflow_rows(&self) -> Vec<OverflowRow> {
        let mut rows = self.overflowed_rows();
        rows.extend([
            OverflowRow::Follow,
            OverflowRow::NewConversation,
            OverflowRow::History,
        ]);
        rows
    }

    /// Whether a row can be chosen. The only one that cannot is Fast mode while
    /// the CLI has refused it; the menu draws that row disabled, and the keys
    /// step over it.
    pub(super) fn picker_row_enabled(&self, row: &PickerRow) -> bool {
        match row {
            PickerRow::Overflow(OverflowRow::Fast) => self
                .fast_state()
                .is_some_and(|fast| fast.blocked_by.is_none()),
            _ => true,
        }
    }

    /// The row the keys have marked: the cursor, held to the rows there are.
    pub(super) fn marked_picker_row(&self, rows: usize) -> Option<usize> {
        (rows > 0).then(|| self.picker_cursor.unwrap_or(0).min(rows - 1))
    }

    /// A popup opens on the row in force: the model, mode or setting now chosen.
    pub(super) fn start_picker_cursor(&mut self, cx: &App) {
        let rows = self.picker_rows(cx);
        let chosen = self
            .thinking_display
            .as_ref()
            .and_then(|state| state.chosen.as_deref());
        let effort = self
            .effort
            .as_ref()
            .and_then(|effort| effort.current_value.as_deref())
            .unwrap_or(EFFORT_RESET);
        let current = rows.iter().position(|row| match row {
            PickerRow::Model(option) => self.selected_model.as_deref() == Some(option.id.as_str()),
            PickerRow::Mode(mode) => self
                .mode_catalog
                .as_ref()
                .is_some_and(|catalog| catalog.current_id == mode.id),
            PickerRow::Thinking(value) => chosen == *value,
            PickerRow::Effort(value) => value == effort,
            PickerRow::Overflow(_) => false,
        });
        // A row that cannot be chosen is never where the cursor starts.
        let first_enabled = rows.iter().position(|row| self.picker_row_enabled(row));
        self.picker_cursor = current.or(first_enabled);
    }

    /// The press a row runs. It is the path the keys take, so a click and
    /// Enter can never choose differently.
    pub(super) fn picker_click(
        &self,
        row: PickerRow,
        cx: &Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let entity = cx.entity();
        move |window, cx| {
            entity.update(cx, |chat, cx| {
                chat.choose_picker_row(row.clone(), window, cx)
            });
        }
    }

    /// Chooses a row, which closes its popup, and hands the focus back to the
    /// composer the popup took it from.
    pub(super) fn choose_picker_row(
        &mut self,
        row: PickerRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match row {
            PickerRow::Model(option) => self.select_model(option, cx),
            PickerRow::Mode(mode) => self.select_mode(mode, cx),
            PickerRow::Thinking(value) => {
                self.select_thinking_display(value.map(str::to_string), cx)
            }
            PickerRow::Effort(value) => {
                self.select_effort(value, cx);
                self.effort_picker_open = false;
            }
            // Fast is a toggle like Follow: the menu stays open with the focus
            // where it is, and its check moves when the CLI has agreed.
            PickerRow::Overflow(OverflowRow::Fast) => {
                self.toggle_fast_mode(cx);
                return;
            }
            // The picker takes the focus itself, and keeps it.
            PickerRow::Overflow(OverflowRow::Thinking) => {
                self.overflow_open = false;
                self.toggle_thinking_picker(window, cx);
                return;
            }
            PickerRow::Overflow(OverflowRow::SessionPanel) => {
                self.overflow_open = false;
                self.toggle_session_panel(cx);
            }
            // Follow is a toggle: the menu stays open, with the focus where it is.
            PickerRow::Overflow(OverflowRow::Follow) => {
                self.following_edited_files = !self.following_edited_files;
                cx.notify();
                return;
            }
            PickerRow::Overflow(OverflowRow::NewConversation) => self.new_conversation(cx),
            // History opens a popover that takes the focus itself.
            PickerRow::Overflow(OverflowRow::History) => {
                self.picker_cursor = None;
                self.toggle_chat_history(window, cx);
                return;
            }
        }
        self.picker_cursor = None;
        self.focus_composer(window, cx);
    }

    pub(super) fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_field
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    }

    fn step_picker(&mut self, by: isize, cx: &mut Context<Self>) {
        let all = self.picker_rows(cx);
        let rows = all.len() as isize;
        if rows == 0 {
            return;
        }
        let mut at = self.marked_picker_row(rows as usize).unwrap_or(0) as isize;
        // The next row that can be chosen, wrapping; none at all leaves the
        // cursor where it is.
        for _ in 0..rows {
            at = (at + by).rem_euclid(rows);
            if self.picker_row_enabled(&all[at as usize]) {
                self.picker_cursor = Some(at as usize);
                break;
            }
        }
        cx.notify();
    }

    pub(super) fn picker_previous(
        &mut self,
        _: &PickerPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_picker(-1, cx);
    }

    pub(super) fn picker_next(&mut self, _: &PickerNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_picker(1, cx);
    }

    pub(super) fn picker_choose(
        &mut self,
        _: &PickerChoose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.picker_rows(cx);
        if let Some(at) = self.marked_picker_row(rows.len()) {
            self.choose_picker_row(rows[at].clone(), window, cx);
        }
    }
}
