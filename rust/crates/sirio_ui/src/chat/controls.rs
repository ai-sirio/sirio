//! The agent's controls under the composer: permission mode, model, effort,
//! fast mode, thinking display, background tasks and the context meter, with
//! the popup each one opens. Every control draws only what the agent reported,
//! so a catalogue it never sent leaves no chevron and no chip behind.

use super::composer_view::{
    EFFORT_RESET, effort_fraction_for_stop, effort_stop_for_fraction, effort_stop_share,
    effort_stops,
};
use super::*;
use ely_gpui_component::menus::{Hang, Menu, MenuItem, MenuPanel, layer, panel_surface};
use sirio_acp::EffortChoice;

/// A popup's width at the interface's text size: the 13px design size at
/// rest, wider as the text grows, so a larger setting never cuts a label
/// the default one fits.
pub(super) fn popup_width(theme: &Theme, at_rest: f32) -> Pixels {
    px(at_rest * (f32::from(theme.typography.ui_size) / 13.0).max(1.0))
}

impl Chat {
    /// The control row's chips, each with the popup it hangs. One element so
    /// the composer takes it as a single tool slot; its own wrapping keeps
    /// the chips inside a narrow pane.
    pub(super) fn render_agent_controls(
        &self,
        theme: &Theme,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let typography = theme.typography;
        let bezel_theme = bezel::theme::Theme::of(cx).clone();
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
        let status_pill = status_pill
            .tooltip(move |window, cx| Tooltip::text(mode_tooltip.clone(), window, cx))
            .when(mode_selectable, |this| {
                this.hover(|style| style.bg(bezel_theme.element_hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_mode_picker(window, cx);
                    }))
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded(px(3.0)).bg(dot))
            .child(div().text_color(theme.ely.fg).child(label))
            .when(mode_selectable, |this| this.child(picker_chevron(theme)));

        let selected_model_name = self
            .selected_model
            .as_deref()
            .and_then(|selected| {
                self.available_models
                    .iter()
                    .find(|option| option.id == selected)
                    .map(|option| option.name.clone())
            })
            .or_else(|| self.selected_model.clone())
            .unwrap_or_else(|| "Claude Code".into());
        let effort_label = self.effort.as_ref().and_then(|effort| {
            effort.current_value.as_ref().map(|current| {
                let name = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == *current)
                    .map(|choice| choice.name.clone())
                    .unwrap_or_else(|| current.clone());
                // Not uppercased any more: it used to be a bare caption that
                // needed to read as chrome, and now it sits beside its own
                // "Effort" label exactly the way the model value sits beside
                // "Model".
                name
            })
        });
        // The picker chip — "Model" in muted chrome text, the value in
        // title text, the effort level when one is selected — opens only
        // once a turn has completed and the agent reported models. Without
        // models the pill degrades to a plain agent badge (F-CHAT-36): no
        // label, no chevron, no picker.
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
                .tooltip(move |window, cx| Tooltip::text(model_tooltip.clone(), window, cx))
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
                // ellipsizes while the row wraps around it, never erasing
                // the picker.
                .min_w(px(56.0))
                .hover(|style| style.bg(bezel_theme.element_hover))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_model_picker(window, cx);
                }))
                .child(div().text_color(theme.ely.fg_subtle).child("Model"))
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
        // it is changed about as often, so it belongs at the same depth and
        // carries its own label. Drawn only when the agent reports a value
        // AND the picker can actually open, so this is never a click target
        // that leads nowhere. `flex_none` keeps it intact while the model
        // chip beside it absorbs the squeeze on a narrow pane — past that
        // the chip wraps whole to the next line, never over the send disc.
        let effort_control = effort_label
            .filter(|_| self.model_control_visible())
            .map(|label| {
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
                    .child(div().text_color(theme.ely.fg_subtle).child("Effort"))
                    .child(
                        div()
                            .id("model-effort-label")
                            .debug_selector(|| "model-effort-label".into())
                            .text_color(theme.ely.fg)
                            .child(label),
                    )
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
            .fast_mode
            .clone()
            .filter(|_| self.model_control_visible())
            .map(|fast| {
                let blocked = fast.blocked_by.clone();
                let fast_entity = entity.clone();
                let selector = if blocked.is_some() {
                    "fast-mode-chip-blocked"
                } else {
                    "fast-mode-chip"
                };
                let label_colour: gpui::Hsla = match (&blocked, fast.enabled) {
                    (Some(_), _) => theme.ely.fg_subtle.into(),
                    // On, the chip is filled with the theme's own accent
                    // pair rather than a colour of Sirio's: the palette is
                    // bezel's, all of it.
                    (None, true) => bezel_theme.on_solid,
                    (None, false) => theme.ely.fg.into(),
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
                        chip.tooltip(move |window, cx| Tooltip::text(reason.clone(), window, cx))
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
                .tooltip(move |window, cx| {
                    Tooltip::text(
                        SharedString::from(if running == 1 {
                            "1 background task running".to_string()
                        } else {
                            format!("{running} background tasks running")
                        }),
                        window,
                        cx,
                    )
                })
                .child(format!("⌁ {running}"))
        });

        // The thinking display. Drawn only while the transport has it and
        // the CLI has not refused the verb; unset shows no value at all,
        // because nothing reports the session's own and a word here would
        // be one nobody read (F-CHAT-18's rule, as the effort track already
        // applies it).
        let thinking = self
            .thinking_display
            .clone()
            .filter(|state| !state.unsupported)
            .filter(|_| self.model_control_visible());
        let thinking_control = thinking.clone().map(|state| {
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

        let thinking_picker = self.thinking_picker_open.then(|| {
            let chosen = thinking.and_then(|state| state.chosen);
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
            layer(
                "thinking-picker-menu",
                Hang::Above,
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
            )
        });

        // The effort selector, anchored to the chip above. A slider rather
        // than a row per level: effort is a scale, and the levels are the
        // session's own — `supportedEffortLevels` differs per model, so the
        // track grows and shrinks with the model instead of naming rungs the
        // model does not have.
        let effort_picker = self
            .effort_picker_open
            .then(|| self.effort.clone())
            .flatten()
            .map(|effort| {
                let stops: Vec<EffortChoice> =
                    effort_stops(&effort.choices).into_iter().cloned().collect();
                let count = stops.len();
                let current = effort
                    .current_value
                    .clone()
                    .unwrap_or_else(|| EFFORT_RESET.to_string());
                // `None` while the session is on its model's own default:
                // that is not a point on the track, so nothing is filled and
                // the value reads in the muted tone the rest of the row uses
                // for "not chosen".
                let selected = stops.iter().position(|stop| stop.value == current);
                let fraction = selected.map_or(0.0, |index| effort_fraction_for_stop(count, index));
                let current_name = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == current)
                    .map(|choice| choice.name.clone())
                    .unwrap_or_else(|| current.clone());
                let reset = effort
                    .choices
                    .iter()
                    .find(|choice| choice.value == EFFORT_RESET)
                    .cloned();
                let heading = effort.name.clone().unwrap_or_else(|| "Effort".to_string());

                let zones: Vec<AnyElement> = stops
                    .iter()
                    .enumerate()
                    .map(|(index, stop)| {
                        let value = stop.value.clone();
                        let selector_value = value.clone();
                        let zone_entity = entity.clone();
                        div()
                            .id(format!("effort-stop-{value}"))
                            .debug_selector(move || format!("effort-stop-{selector_value}"))
                            // Not an equal share each: a stop owns the track
                            // that is nearer to it than to its neighbours, and
                            // the two ends have a neighbour on one side only.
                            // Equal shares would put every boundary half a
                            // step away from where the drag snaps.
                            .w(relative(effort_stop_share(count, index)))
                            .h_full()
                            .cursor_pointer()
                            // The zones sit above the track, so they are what
                            // the pointer actually lands on: gpui only arms a
                            // drag on the element whose hitbox is topmost at
                            // mouse-down. Each one therefore starts the
                            // track's drag, under the track's own id — the
                            // move is read against the whole track's bounds
                            // by the listener below, not against one zone's.
                            .on_drag(SliderDrag("effort-slider".into()), |_, _, _, cx| {
                                cx.new(|_| gpui::Empty)
                            })
                            .on_click(move |_, _, cx| {
                                zone_entity.update(cx, |chat, cx| {
                                    chat.select_effort(value.clone(), cx);
                                });
                            })
                            .into_any_element()
                    })
                    .collect();

                let drag_stops = stops.clone();
                let track = div()
                    .relative()
                    .w_full()
                    .h(px(16.0))
                    .child(bezel_theme.slider(fraction))
                    .child(
                        div()
                            .id("effort-slider")
                            .debug_selector(|| "effort-slider".into())
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_stretch()
                            .on_drag_move(cx.listener(
                                move |chat, event: &DragMoveEvent<SliderDrag>, _, cx| {
                                    let Some(fraction) =
                                        slider_fraction(event, "effort-slider", cx)
                                    else {
                                        return;
                                    };
                                    let Some(stop) =
                                        drag_stops.get(effort_stop_for_fraction(count, fraction))
                                    else {
                                        return;
                                    };
                                    // Every stop reached is a control request
                                    // to the agent, and a drag crosses the
                                    // track in dozens of moves. Only a stop
                                    // the session is not already on is worth
                                    // asking for.
                                    let standing = chat
                                        .effort
                                        .as_ref()
                                        .and_then(|effort| effort.current_value.as_deref());
                                    if standing != Some(stop.value.as_str()) {
                                        chat.select_effort(stop.value.clone(), cx);
                                    }
                                },
                            ))
                            .children(zones),
                    );

                let ends = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(typography.caption2)
                    .text_color(theme.ely.fg_subtle)
                    .child(
                        stops
                            .first()
                            .map(|stop| stop.name.clone())
                            .unwrap_or_default(),
                    )
                    .child(
                        stops
                            .last()
                            .map(|stop| stop.name.clone())
                            .unwrap_or_default(),
                    );

                let reset_row = reset.map(|reset| {
                    let reset_entity = entity.clone();
                    let reset_value = reset.value.clone();
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(div().h(px(1.0)).w_full().bg(theme.ely.border))
                        .child(
                            div()
                                .id("effort-reset")
                                .debug_selector(|| "effort-reset".into())
                                .h(px(22.0))
                                .px(px(6.0))
                                .flex()
                                .items_center()
                                .rounded(theme.radii.control)
                                .text_size(typography.caption2)
                                .text_color(theme.ely.fg)
                                .when(selected.is_none(), |this| this.bg(theme.ely.active))
                                .hover(|style| style.bg(theme.sirio.overlay))
                                .on_click(move |_, _, cx| {
                                    reset_entity.update(cx, |chat, cx| {
                                        chat.select_effort(reset_value.clone(), cx);
                                        // A reset is a command, not an
                                        // adjustment: it answers and closes,
                                        // where the track stays open because
                                        // moving a knob is something you do
                                        // more than once.
                                        chat.effort_picker_open = false;
                                        cx.notify();
                                    });
                                })
                                .child("Use the model's default"),
                        )
                });

                // A scale needs two ends. One level is a choice between it
                // and the model's default, which is a row — a knob with
                // nowhere to slide would be a control that cannot be worked.
                let scale = (count > 1).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        // The knob is 14px centred on the track ends, so it
                        // overhangs 7px each side: without this inset it runs
                        // into the card's 4px padding and clips.
                        .px(px(8.0))
                        .child(track)
                        .child(ends)
                });
                let lone_stop = stops.first().filter(|_| count == 1).map(|stop| {
                    let value = stop.value.clone();
                    let selector_value = value.clone();
                    let lone_entity = entity.clone();
                    div()
                        .id(format!("effort-stop-{value}"))
                        .debug_selector(move || format!("effort-stop-{selector_value}"))
                        .h(px(22.0))
                        .px(px(6.0))
                        .flex()
                        .items_center()
                        .rounded(theme.radii.control)
                        .text_size(typography.caption2)
                        .text_color(theme.ely.fg)
                        .when(selected.is_some(), |this| this.bg(theme.ely.active))
                        .hover(|style| style.bg(theme.sirio.overlay))
                        .on_click(move |_, _, cx| {
                            lone_entity
                                .update(cx, |chat, cx| chat.select_effort(value.clone(), cx));
                        })
                        .child(stop.name.clone())
                });

                layer(
                    "effort-picker-menu",
                    Hang::Above,
                    div()
                        .id("effort-picker")
                        .debug_selector(|| "effort-picker".into())
                        .key_context("ChatEffortPicker")
                        .track_focus(&self.effort_picker_focus)
                        .on_action(cx.listener(Self::cancel))
                        .w(popup_width(theme, 220.0))
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.effort_picker_open = false;
                            cx.notify();
                        }))
                        .child(
                            panel_surface("effort-picker-surface", cx).p_2().child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(8.0))
                                            .text_size(typography.caption2)
                                            .child(
                                                div().text_color(theme.ely.fg_subtle).child(heading),
                                            )
                                            .child(
                                                div()
                                                    .id("effort-current")
                                                    .debug_selector(move || {
                                                        if selected.is_some() {
                                                            "effort-current".into()
                                                        } else {
                                                            "effort-current-unset".into()
                                                        }
                                                    })
                                                    .text_color(if selected.is_some() {
                                                        theme.ely.fg
                                                    } else {
                                                        theme.ely.fg_subtle
                                                    })
                                                    .child(current_name),
                                            ),
                                    )
                                    .children(scale)
                                    .children(lone_stop)
                                    .children(reset_row),
                            ),
                        ),
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
                            .child(self.model_search_field.clone()),
                    )
                });
            layer(
                "model-picker-menu",
                Hang::Above,
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

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(status_pill)
                    .children(mode_picker),
            )
            // The one shrinkable chip: on a tight line it ellipsizes its name
            // down to its 56px floor, never to nothing.
            .child(div().relative().child(model_control).children(model_picker))
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
                    .child(
                        div()
                            .id("context-label")
                            .debug_selector(|| "context-label".into())
                            .text_color(theme.ely.fg_subtle)
                            .child("Context"),
                    )
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
    Overflow(OverflowRow),
}

/// The rows of the composer's "more" menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverflowRow {
    Follow,
    NewConversation,
    History,
}

impl Chat {
    /// The rows of whichever catalogue popup is open, in the order drawn.
    pub(super) fn picker_rows(&self, cx: &App) -> Vec<PickerRow> {
        if self.model_picker_open {
            let query = self.model_search_field.read(cx).content().to_string();
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
        } else if self.overflow_open {
            [
                OverflowRow::Follow,
                OverflowRow::NewConversation,
                OverflowRow::History,
            ]
            .into_iter()
            .map(PickerRow::Overflow)
            .collect()
        } else {
            Vec::new()
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
        let current = rows.iter().position(|row| match row {
            PickerRow::Model(option) => self.selected_model.as_deref() == Some(option.id.as_str()),
            PickerRow::Mode(mode) => self
                .mode_catalog
                .as_ref()
                .is_some_and(|catalog| catalog.current_id == mode.id),
            PickerRow::Thinking(value) => chosen == *value,
            PickerRow::Overflow(_) => false,
        });
        self.picker_cursor = current.or((!rows.is_empty()).then_some(0));
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
        let rows = self.picker_rows(cx).len() as isize;
        if rows == 0 {
            return;
        }
        let at = self.marked_picker_row(rows as usize).unwrap_or(0) as isize;
        self.picker_cursor = Some((at + by).rem_euclid(rows) as usize);
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
