use std::{path::PathBuf, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, ExternalPaths, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Pixels, RenderOnce, SharedString, Styled, Window, canvas,
    div, prelude::*, px,
};
use smallvec::SmallVec;

use crate::{
    buttons::{ButtonVariant, IconButton},
    forms::{
        Choice, Enter, OnValue, Pick, Run, Suggestions, TextInput, active_trigger, choose, dropped,
        replace_trigger,
    },
    primitives::{Icon, IconName},
    theme::{ActiveTheme, ControlSize, IconSize, Radius, TextSize},
    typography::Kbd,
};

type OnPaths = Rc<dyn Fn(Vec<PathBuf>, &mut Window, &mut App)>;
type OnBounds = Rc<dyn Fn(Bounds<Pixels>)>;

/// Opens the system's file dialog to attach files; `on_pick` gets what was chosen.
#[derive(IntoElement)]
pub struct AttachmentButton {
    id: ElementId,
    on_pick: OnPaths,
}

impl AttachmentButton {
    pub fn new(
        id: impl Into<ElementId>,
        on_pick: impl Fn(Vec<PathBuf>, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            on_pick: Rc::new(on_pick),
        }
    }
}

impl RenderOnce for AttachmentButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let pick = self.on_pick;
        IconButton::new(self.id, IconName::Paperclip)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .tooltip("Attach files")
            .on_click(move |_, window, cx| {
                let pick = pick.clone();
                let options = PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: true,
                    prompt: Some("Attach".into()),
                };
                choose(
                    "attachment button".into(),
                    options,
                    window,
                    cx,
                    move |paths, window, cx| pick(paths, window, cx),
                );
            })
    }
}

/// Sends a message once there is one, or stops the answer while it streams.
#[derive(IntoElement)]
pub struct SendButton {
    id: ElementId,
    ready: bool,
    on_send: Run,
    on_stop: Option<Run>,
    variant: ButtonVariant,
}

impl SendButton {
    pub fn new(
        id: impl Into<ElementId>,
        ready: bool,
        on_send: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            ready,
            on_send: Rc::new(on_send),
            on_stop: None,
            variant: ButtonVariant::Primary,
        }
    }

    /// Draws send and stop in a quieter variant than `Primary`, for a shell
    /// whose fill already carries the theme's colour.
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// While an answer streams: the button stops it.
    pub fn busy(mut self, on_stop: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(on_stop));
        self
    }
}

impl RenderOnce for SendButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (button, state) = match self.on_stop {
            Some(stop) => (
                // Solid like send: a Secondary square on the composer's
                // sunken card is a faint outline on a near-equal fill.
                IconButton::new(self.id, IconName::Square)
                    .variant(self.variant)
                    .size(ControlSize::Sm)
                    .tooltip("Stop")
                    .on_click(move |_, window, cx| {
                        log::info!("send button: stop");
                        stop(window, cx)
                    }),
                Some("stop-glyph"),
            ),
            None => {
                let send = self.on_send;
                (
                    IconButton::new(self.id, IconName::ArrowUp)
                        .variant(self.variant)
                        .size(ControlSize::Sm)
                        .tooltip("Send")
                        .disabled(!self.ready)
                        .on_click(move |_, window, cx| {
                            log::info!("send button: send");
                            send(window, cx)
                        }),
                    self.ready.then_some("send-ready"),
                )
            }
        };
        // Test hooks: `send` is the control, the inner selector names what it
        // currently does. GPUI's `debug_selector` is a no-op in release builds.
        div().flex_none().debug_selector(|| "send".into()).child(
            div()
                .when_some(state, |inner, name| {
                    inner.debug_selector(move || name.to_string())
                })
                .child(button),
        )
    }
}

/// The keys a composer takes, said quietly: Enter sends, Shift-Enter breaks the line.
#[derive(IntoElement)]
pub struct InputHint {
    enter: SharedString,
}

impl InputHint {
    pub fn new() -> Self {
        Self {
            enter: "to send ·".into(),
        }
    }

    /// What Enter does right now, such as `to queue ·` while an answer streams.
    pub fn enter(mut self, what: impl Into<SharedString>) -> Self {
        self.enter = what.into();
        self
    }
}

impl Default for InputHint {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for InputHint {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .text_size(theme.text_size(TextSize::Xs))
            .text_color(theme.colors.fg_subtle)
            .child(Kbd::new("enter"))
            .child(self.enter)
            .child(Kbd::new("shift-enter"))
            .child("for a new line")
    }
}

/// While files hover its parent, a veil over it that says a drop attaches them; it takes the drop. The parent is `relative`.
#[derive(IntoElement)]
pub struct DragDropOverlay {
    on_drop: OnPaths,
}

impl DragDropOverlay {
    /// `on_drop` gets the files dropped; a drop with a folder is refused.
    pub fn new(on_drop: impl Fn(Vec<PathBuf>, &mut Window, &mut App) + 'static) -> Self {
        Self {
            on_drop: Rc::new(on_drop),
        }
    }
}

impl RenderOnce for DragDropOverlay {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors;
        let take = self.on_drop;
        div()
            .absolute()
            .inset_0()
            .invisible()
            .drag_over::<ExternalPaths>(|style, _, _, _| style.visible())
            .on_drop(
                move |paths: &ExternalPaths, window, cx| match dropped(paths.paths(), true) {
                    Ok(paths) => {
                        log::info!("drag drop overlay: {} files dropped", paths.len());
                        take(paths, window, cx)
                    }
                    Err(reason) => log::info!("drag drop overlay: refused a drop: {reason}"),
                },
            )
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius(Radius::Xl))
            .border_1()
            .border_dashed()
            .border_color(colors.focus)
            .bg(colors.surface.opacity(0.94))
            .text_size(theme.text_size(TextSize::Sm))
            .text_color(colors.fg_muted)
            .child(
                div()
                    .debug_selector(|| "drag-drop-overlay".into())
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Upload)
                            .size(IconSize::Sm)
                            .color(colors.fg_muted),
                    )
                    .child("Drop to attach"),
            )
    }
}

/// The rows a composer offers at `/` or `@`, and what a pick does.
fn offers(
    id: ElementId,
    field: &Entity<TextInput>,
    commands: &[Choice],
    context: &[Choice],
    on_command: Option<OnValue>,
    on_mention: Option<OnValue>,
    cx: &App,
) -> Suggestions {
    let input = field.read(cx);
    let (text, caret) = (input.text().to_string(), input.cursor());
    let active = active_trigger(&text, caret, &['/', '@'], |ch| !ch.is_whitespace())
        .filter(|(at, mark)| *mark == '@' || *at == 0);
    let query = active.map_or(String::new(), |(at, mark)| {
        text[at + mark.len_utf8()..caret].to_lowercase()
    });
    let (listed, run) = match active {
        Some((_, '/')) => (commands, on_command),
        Some(_) => (context, on_mention),
        None => (&[][..], None),
    };
    let found: Vec<Choice> = listed
        .iter()
        .filter(|choice| choice.label.to_lowercase().contains(&query))
        .cloned()
        .collect();
    let rows = if run.is_some() {
        found.clone()
    } else {
        Vec::new()
    };
    let (state, field) = (field.clone(), field.clone());
    let pick: Pick = Rc::new(move |ix: usize, window: &mut Window, cx: &mut App| {
        let (at, caret) = (
            active.expect("rows show at a trigger").0,
            field.read(cx).cursor(),
        );
        replace_trigger(&field, at, caret, "", cx);
        log::info!("composer: picked {}", found[ix].value);
        if let Some(run) = &run {
            run(&found[ix].value, window, cx);
        }
    });
    Suggestions {
        id,
        state,
        trigger: active.map(|(at, _)| at),
        rows,
        pick,
    }
}

/// Where a message is written: its attachments and context above, the field, and a row of the owner's tools with send. Enter sends, Shift-Enter breaks the line; `/` at the start offers commands and `@` offers context; files dropped on it attach.
#[derive(IntoElement)]
pub struct PromptInput {
    id: ElementId,
    field: Option<Entity<TextInput>>,
    /// The host's own editor, drawn instead of an Ely field (see [`Self::custom`]).
    editor: Option<AnyElement>,
    ready: Option<bool>,
    focused: Option<bool>,
    on_send: Run,
    on_stop: Option<Run>,
    above: SmallVec<[AnyElement; 2]>,
    tools: SmallVec<[AnyElement; 4]>,
    trailing: SmallVec<[AnyElement; 2]>,
    commands: (Vec<Choice>, Option<OnValue>),
    context: (Vec<Choice>, Option<OnValue>),
    on_drop: Option<OnPaths>,
    /// The band look of [`Self::band`]: the content column's widest.
    band: Option<Pixels>,
    /// Told where the band's send control was laid out (see [`Self::send_probe`]).
    send_probe: Option<OnBounds>,
}

impl PromptInput {
    /// `field` is the owner's multi-line text; `on_send` sends it.
    pub fn new(
        id: impl Into<ElementId>,
        field: &Entity<TextInput>,
        on_send: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            field: Some(field.clone()),
            editor: None,
            ready: None,
            focused: None,
            on_send: Rc::new(on_send),
            on_stop: None,
            above: SmallVec::new(),
            tools: SmallVec::new(),
            trailing: SmallVec::new(),
            commands: (Vec::new(), None),
            context: (Vec::new(), None),
            on_drop: None,
            band: None,
            send_probe: None,
        }
    }

    /// Ely's visual shell around the host's own editor. The host decides when
    /// sending is possible (`ready`); the shell draws the same border focused
    /// or not, takes no Enter action and offers no completion
    /// rows of its own, so the host's keyboard, IME and popups stay in charge.
    /// The tool row wraps, so a narrow pane never clips the send control,
    /// unless [`Self::band`] is set: the band's row is one line.
    pub fn custom(
        id: impl Into<ElementId>,
        editor: impl IntoElement,
        ready: bool,
        on_send: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            field: None,
            editor: Some(editor.into_any_element()),
            ready: Some(ready),
            focused: Some(false),
            on_send: Rc::new(on_send),
            on_stop: None,
            above: SmallVec::new(),
            tools: SmallVec::new(),
            trailing: SmallVec::new(),
            commands: (Vec::new(), None),
            context: (Vec::new(), None),
            on_drop: None,
            band: None,
            send_probe: None,
        }
    }

    /// Draws a host's editor (see [`Self::custom`]) as a band across the
    /// bottom of its pane instead of a card: as wide as its parent, with a 1px
    /// top rule in `border`, no radius, the theme's `surface` as its fill and
    /// 12px of side padding, 10px above and 8px below. Its content sits in a
    /// centred column at most `column` wide. The tool row never wraps: the
    /// tools stay at the start in a group that does not shrink, and the
    /// trailing tools and send stay at the end in a group that may shrink.
    /// Without a host editor this does nothing.
    pub fn band(mut self, column: Pixels) -> Self {
        self.band = Some(column);
        self
    }

    /// Calls `probe` with the send control's bounds at each paint of the
    /// [`Self::band`] row, so a host can tell where it lies (a release build
    /// has no debug selectors to read). Paints nothing; without the band this
    /// does nothing.
    pub fn send_probe(mut self, probe: impl Fn(Bounds<Pixels>) + 'static) -> Self {
        self.send_probe = Some(Rc::new(probe));
        self
    }

    /// A tool that travels with the send control at the end of the row.
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing.push(element.into_any_element());
        self
    }

    /// While an answer streams: send becomes stop.
    pub fn busy(mut self, on_stop: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_stop = Some(Rc::new(on_stop));
        self
    }

    /// Shown over the field, such as attachments, context and a quote.
    pub fn above(mut self, element: impl IntoElement) -> Self {
        self.above.push(element.into_any_element());
        self
    }

    /// A tool in the row under the field, such as attach or the model.
    pub fn tool(mut self, element: impl IntoElement) -> Self {
        self.tools.push(element.into_any_element());
        self
    }

    /// Commands offered at `/` at the start; `on_command` gets the one picked.
    pub fn commands(
        mut self,
        commands: impl IntoIterator<Item = Choice>,
        on_command: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.commands = (commands.into_iter().collect(), Some(Rc::new(on_command)));
        self
    }

    /// Context offered at `@`; `on_mention` gets the one picked.
    pub fn context(
        mut self,
        context: impl IntoIterator<Item = Choice>,
        on_mention: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.context = (context.into_iter().collect(), Some(Rc::new(on_mention)));
        self
    }

    /// Takes files dropped on the composer.
    pub fn on_drop(
        mut self,
        handler: impl Fn(Vec<PathBuf>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drop = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PromptInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let colors = theme.colors;
        let (radius, text_size) = (theme.radius(Radius::Xl), theme.text_size(TextSize::Base));
        let ready = self.ready.unwrap_or_else(|| {
            self.field
                .as_ref()
                .is_some_and(|field| !field.read(cx).text().trim().is_empty())
        });
        let focused = self.focused.unwrap_or_else(|| {
            self.field
                .as_ref()
                .is_some_and(|field| field.read(cx).focus().contains_focused(window, cx))
        });
        let (send, enter) = (self.on_send.clone(), self.on_send.clone());
        let custom = self.editor.is_some();
        let button = SendButton::new((self.id.clone(), "send"), ready, move |window, cx| {
            send(window, cx)
        });
        // A host's editor sits on the theme's surface: a solid accent square
        // there is the one off-palette mark on the card, so send and stop take
        // the theme's own hover fill instead.
        let button = if custom {
            button.variant(ButtonVariant::Subtle)
        } else {
            button
        };
        let button = match self.on_stop.clone() {
            Some(stop) => button.busy(move |window, cx| stop(window, cx)),
            None => button,
        };
        let busy = self.on_stop.is_some();
        let editor = match (self.editor, self.field.as_ref()) {
            (Some(editor), _) => editor,
            (None, Some(field)) => {
                let suggestions = offers(
                    (self.id.clone(), "offers").into(),
                    field,
                    &self.commands.0,
                    &self.context.0,
                    self.commands.1.clone(),
                    self.context.1.clone(),
                    cx,
                );
                suggestions
                    .wrap(
                        div()
                            .capture_action(move |_: &Enter, window, cx| {
                                cx.stop_propagation();
                                if ready && !busy {
                                    log::info!("composer: sent with Enter");
                                    enter(window, cx);
                                }
                            })
                            .text_size(text_size)
                            .child(field.clone()),
                        window,
                        cx,
                    )
                    .into_any_element()
            }
            (None, None) => unreachable!("a prompt input has a field or an editor"),
        };
        let band = self.band.filter(|_| custom);
        let end = div().flex().items_center().gap_1().children(self.trailing);
        let end = if band.is_some() {
            // The band's end group may shrink (its chips ellipsize or give way
            // inside it) while send keeps its size and its place at the end.
            let send = div().flex_none().relative().child(button);
            let send = match self.send_probe {
                Some(probe) => send.child(
                    canvas(move |bounds, _, _| probe(bounds), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                ),
                None => send,
            };
            end.min_w_0().justify_end().child(send)
        } else {
            end.flex_none().child(button)
        };
        let shell = div()
            .id(self.id.clone())
            .debug_selector(|| "ely-prompt-input".into())
            .relative()
            .flex()
            .flex_col()
            // A host's editor paints no fill of its own (Sirio's is Ely's raw
            // `TextInput`); the shell's `surface` is the one surface both
            // read as, the same card fill as the rest of the theme.
            .bg(colors.surface)
            // Text below inherits this: attachment names and the like carry no
            // colour of their own, and gpui's default is black.
            .text_color(colors.fg);
        let body = if let Some(column) = band {
            // The band: the rule on top and nothing else around it, the
            // content in a column no wider than `column`, and the row on one
            // line at any width.
            shell
                .w_full()
                .items_center()
                .px(px(12.0))
                .pt(px(10.0))
                .pb(px(8.0))
                .border_t_1()
                .border_color(colors.border)
                .child(
                    div()
                        .w_full()
                        .max_w(column)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(self.above)
                        .child(editor)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_none()
                                        .items_center()
                                        .gap_1()
                                        .children(self.tools),
                                )
                                .child(div().flex_1().min_w_0())
                                .child(end),
                        ),
                )
        } else {
            shell
                .gap_2()
                .p_3()
                .rounded(radius)
                .border_1()
                .border_color(match (focused, custom) {
                    // A host's editor keeps the theme's quiet rule in both
                    // states: the caret already says where focus is, and a
                    // brighter outline read as a white frame around the card.
                    (_, true) => colors.border,
                    (true, false) => colors.focus,
                    (false, false) => colors.border_strong,
                })
                .children(self.above)
                .child(editor)
                .child(if custom {
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .children(self.tools)
                        .child(end.ml_auto())
                } else {
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .children(self.tools)
                        .child(div().flex_1())
                        .child(end)
                })
        };
        body.when_some(self.on_drop, |body, take| {
            body.child(DragDropOverlay { on_drop: take })
        })
    }
}
