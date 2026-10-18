use std::{ops::Range, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, Context, ElementInputHandler, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Render,
    StatefulInteractiveElement, Styled, StyledText, Window, canvas, div, fill, font, point, px, size,
    uniform_list,
};

use super::{
    decor::ReadOnlyBanner,
    keys::{CONTEXT, listen},
    layout::{MinimapPlan, Row, Rows, hidden, minimap_plan},
    state::{CodeEditor, EditorEvent},
    syntax::{self, Bracket},
};
use crate::theme::{ActiveTheme, TextSize};

/// What one render worked out, for the rows the list builds after it.
#[derive(Default)]
pub(crate) struct Frame {
    pub rows: Rows,
    pub brackets: Rc<Vec<Bracket>>,
    pub advance: Pixels,
    pub line: Pixels,
    pub matched: Option<(usize, usize)>,
    /// The text whose other appearances are washed, found on the lines as they are drawn.
    pub needle: Option<String>,
    /// The indent of the innermost block around the caret, whose guide is the strong one.
    pub active: Option<usize>,
    /// The buffer stamp these offsets were found in.
    pub stamp: u64,
    /// The minimap's box, from its last paint.
    pub minimap: Bounds<Pixels>,
}

/// Lines of code a row's height holds, as leading.
const LEADING: f32 = 1.6;
/// Blocks the sticky header shows at most.
const STICKY: usize = 3;
/// Columns the minimap spans, in code characters.
const MINIMAP: f32 = 12.0;

impl CodeEditor {
    /// The text the primary selection or its word gives, for highlighting where else it appears.
    fn needle(&self) -> Option<String> {
        let primary = self.primary();
        let range = if primary.is_empty() {
            let word = self.buffer.word_at(primary.head);
            let text = &self.buffer.text()[word.clone()];
            text.chars()
                .all(|ch| ch.is_alphanumeric() || ch == '_')
                .then_some(word)?
        } else {
            primary.range()
        };
        // Only a piece of one line is washed; a selection over several lines is never copied.
        let one_line = self.buffer.line_of(range.start) == self.buffer.line_of(range.end);
        (!range.is_empty() && one_line).then(|| self.buffer.text()[range].to_string())
    }

    /// The first line the list shows, after scrolling.
    fn top_line(&self) -> Option<usize> {
        let scrolled = -self.scroll.0.borrow().base_handle.offset().y;
        let first = (scrolled / self.frame.line).floor().max(0.0) as usize;
        self.frame
            .rows
            .line_from(first.min(self.frame.rows.len().saturating_sub(1)))
    }

    fn sticky(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let top = self.top_line()?;
        let mut headers = self.enclosing(top);
        headers.truncate(STICKY);
        if headers.is_empty() {
            return None;
        }
        let rows = headers.into_iter().map(|line| {
            let text = self.buffer.line(line).to_string();
            let styles = self.piece_colors(line, 0, &text, cx);
            let jump = cx.entity();
            div()
                .id(("sticky", line))
                .h(self.frame.line)
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|style| style.bg(colors.hover))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    jump.update(cx, |editor, cx| {
                        if line >= editor.buffer.lines() {
                            return;
                        }
                        let at = editor.buffer.line_range(line).start + editor.buffer.indent(line);
                        let cursor = at..at;
                        editor.select([cursor], cx);
                        editor.reveal(false, cx);
                    });
                })
                .child(
                    div()
                        .flex_none()
                        .w(self.frame.advance * (self.gutter_columns() + 1) as f32),
                )
                .child(
                    div()
                        .line_height(self.frame.line)
                        .whitespace_nowrap()
                        .text_color(colors.syntax.variable)
                        .child(StyledText::new(text).with_highlights(styles)),
                )
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bg(colors.surface)
                .border_b_1()
                .border_color(colors.border)
                .children(rows)
                .into_any_element(),
        )
    }

    /// The rows the minimap draws for a height, each at its top in pixels with the colors of its line's characters (start, length, color): every row while a row is a pixel tall, else one a pixel.
    fn minimap_rows(
        &self,
        height: Pixels,
        cx: &App,
    ) -> (MinimapPlan, Vec<(f32, Vec<(usize, usize, gpui::Hsla)>)>) {
        let rows = self.frame.rows.len();
        let plan = minimap_plan(rows, f32::from(self.frame.line), f32::from(height));
        let drawn = plan
            .samples(rows)
            .into_iter()
            .filter_map(|(y, ix)| match self.frame.rows.get(ix)? {
                Row::Line(at) => {
                    let text = self.buffer.line(at);
                    let tokens: Vec<_> = self
                        .piece_colors(at, 0, text, cx)
                        .into_iter()
                        .filter_map(|(range, style)| {
                            let start = text[..range.start].chars().count();
                            let len = text[range].chars().count();
                            style.color.map(|color| (start, len, color))
                        })
                        .collect();
                    (!tokens.is_empty()).then_some((y, tokens))
                }
                _ => None,
            })
            .collect();
        (plan, drawn)
    }

    fn minimap_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let (advance, line) = (self.frame.advance, self.frame.line);
        let scroll = self.scroll.clone();
        let entity = cx.entity();
        let (drag, press) = (cx.entity(), cx.entity());
        let id = cx.entity_id().as_u64();
        let shade = colors.hover;
        div()
            .id("minimap")
            .relative()
            .flex_none()
            .w(advance * MINIMAP)
            .h_full()
            .border_l_1()
            .border_color(colors.border)
            .cursor_pointer()
            .child(
                canvas(
                    // The colors are read here, where the minimap's height is known, so the work follows the pixels it has and not the file's length.
                    move |bounds, _, cx| {
                        entity.update(cx, |editor, cx| {
                            editor.frame.minimap = bounds;
                            let _scan =
                                sirio_perf::span("CodeEditor.minimap_rows", cx.entity_id().as_u64());
                            editor.minimap_rows(bounds.size.height, cx)
                        })
                    },
                    move |bounds, (plan, tokens), window, _| {
                        let _paint = sirio_perf::span("CodeEditor.minimap_paint", id);
                        let tall = plan.tall;
                        // A row a pixel tall or more keeps its proportion, a thinner one is drawn as most of its pixel.
                        let thick = tall.max(1.0) * 0.7;
                        let wide = advance * 0.12;
                        let state = scroll.0.borrow();
                        let viewport = state.base_handle.bounds().size.height;
                        let scrolled = -state.base_handle.offset().y;
                        let top = bounds.origin.y + scrolled / line * px(tall);
                        let extent = size(bounds.size.width, viewport / line * px(tall));
                        window.paint_quad(fill(
                            Bounds::new(point(bounds.origin.x, top), extent),
                            shade,
                        ));
                        // The tokens sit side by side on rows a pixel apart: one layer gives them a single draw order, so each is not placed among everything painted before it (a quad costs about a microsecond that way, and a screenful of code has thousands).
                        window.paint_layer(bounds, |window| {
                            for (y, spans) in &tokens {
                                for (start, len, color) in spans {
                                    let origin = point(
                                        bounds.origin.x + wide * (*start + 2) as f32,
                                        bounds.origin.y + px(*y),
                                    );
                                    let extent = size(wide * *len as f32, px(thick));
                                    window.paint_quad(fill(
                                        Bounds::new(origin, extent),
                                        color.opacity(0.7),
                                    ));
                                }
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                press.update(cx, |editor, cx| editor.jump(event.position.y, cx));
            })
            .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                if event.dragging() {
                    drag.update(cx, |editor, cx| editor.jump(event.position.y, cx));
                }
            })
            .into_any_element()
    }

    /// The offset under a window point over the code column: none over the gutter, past the text, or on a row that is not text.
    pub(super) fn offset_over_code(
        &self,
        position: Point<Pixels>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<usize> {
        // The code column begins one advance before the text, past the gutter.
        if position.x < self.metrics.left - self.metrics.advance {
            return None;
        }
        self.offset_at(position, window, cx)
    }

    /// Scrolls so the row under a minimap point sits in the middle of the view.
    fn jump(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let bounds = self.frame.minimap;
        let rows = self.frame.rows.len().max(1) as f32;
        let tall = (self.frame.line * 0.15).min(bounds.size.height / rows);
        let row = ((y - bounds.origin.y) / tall).max(0.0);
        let state = self.scroll.0.borrow();
        let viewport = state.base_handle.bounds().size.height;
        let top = (self.frame.line * row - viewport / 2.0).max(Pixels::ZERO);
        state.base_handle.set_offset(point(Pixels::ZERO, -top));
        drop(state);
        cx.notify();
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = cx.entity_id().as_u64();
        let _render = sirio_perf::span("CodeEditor.render", id);
        let theme = cx.theme();
        let colors = theme.colors.clone();
        let mono_family = theme.mono_family.clone();
        let size = theme.text_size(TextSize::Sm);
        let pixels = size.to_pixels(window.rem_size());
        let mono = window
            .text_system()
            .resolve_font(&font(theme.mono_family.clone()));
        let advance = match window.text_system().advance(mono, pixels, 'm') {
            Ok(advance) => advance.width,
            Err(error) => {
                log::error!(
                    "code editor: no advance for the code font: {error:#}; columns guessed"
                );
                pixels * 0.6
            }
        };
        let line = (pixels * LEADING).round();
        let scan = sirio_perf::span("CodeEditor.active_block", id);
        // The strong guide is the innermost block around the caret's: asked for only while guides are drawn.
        let active = self
            .options
            .guides
            .then(|| {
                let caret_line = self.buffer.line_of(self.primary().head);
                self.enclosing(caret_line)
                    .last()
                    .map(|header| self.buffer.indent(*header))
            })
            .flatten();
        drop(scan);
        let scan = sirio_perf::span("CodeEditor.brackets", id);
        let found_brackets = self.brackets();
        let matched = syntax::matched(&found_brackets, self.primary().head);
        drop(scan);
        let needle = self.needle().filter(|_| self.focus.is_focused(window));
        let minimap = self.frame.minimap;
        let scan = sirio_perf::span("CodeEditor.rows", id);
        let laid = Rows::of(
            &self.buffer,
            || hidden(self.buffer.lines(), &self.folds(), &self.folded),
            &self.marks,
            &self.folded,
        );
        drop(scan);
        self.frame = Frame {
            rows: laid,
            brackets: found_brackets,
            advance,
            line,
            matched,
            needle,
            active,
            stamp: self.buffer.stamp(),
            minimap,
        };
        self.metrics.advance = advance;
        self.metrics.line = line;
        self.metrics.size = pixels;
        let scan = sirio_perf::span("CodeEditor.sticky", id);
        let sticky = if self.options.sticky {
            self.sticky(cx)
        } else {
            None
        };
        drop(scan);
        let count = self.frame.rows.len();
        let list = uniform_list(
            "editor-rows",
            count,
            cx.processor(|editor, range: Range<usize>, window, cx| {
                let _rows = sirio_perf::span("CodeEditor.visible_rows", cx.entity_id().as_u64());
                range.map(|ix| editor.row(ix, window, cx)).collect()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        let (top, focus, input) = (cx.entity(), self.focus.clone(), cx.entity());
        let unlock = cx.entity();
        let banner = self.draws_read_only_banner().then(|| {
            ReadOnlyBanner::new(
                ("read-only", cx.entity_id().as_u64()),
                "This file is read-only.",
            )
            .action("Make editable", move |_, cx| {
                unlock.update(cx, |editor, cx| {
                    editor.set_read_only(false, cx);
                    cx.emit(EditorEvent::Unlock);
                })
            })
        });
        let body = div()
            .id(("code-editor-body", cx.entity_id().as_u64()))
            .relative()
            .flex_1()
            .h_full()
            .child(list)
            .children(sticky)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        top.update(cx, |editor, _| editor.metrics.top = bounds.origin.y);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_move(cx.listener(|editor, event: &MouseMoveEvent, window, cx| {
                if event.dragging() {
                    return;
                }
                let offset = editor.offset_over_code(event.position, window, cx);
                cx.emit(EditorEvent::Pointer {
                    offset,
                    position: event.position,
                });
            }))
            // A pointer that leaves the code moves nothing here, so the owner
            // would keep what it drew for the place the pointer last rested.
            .on_hover(cx.listener(|_, hovered: &bool, window, cx| {
                if !hovered {
                    cx.emit(EditorEvent::Pointer {
                        offset: None,
                        position: window.mouse_position(),
                    });
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|editor, event: &MouseDownEvent, window, cx| {
                    let offset = editor.offset_over_code(event.position, window, cx);
                    cx.emit(EditorEvent::SecondaryClick {
                        offset,
                        position: event.position,
                    });
                }),
            );
        let scan = sirio_perf::span("CodeEditor.minimap", id);
        let minimap_view = self.options.minimap.then(|| self.minimap_view(cx));
        drop(scan);
        let root = div()
            .id(("code-editor", cx.entity_id().as_u64()))
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(colors.surface)
            .font_family(mono_family)
            .text_size(size)
            .text_color(colors.fg)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|editor, event: &MouseUpEvent, window, cx| {
                    editor.release(event, window, cx)
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|editor, _, _, _| {
                    editor.dragging = false;
                    editor.pressed = None;
                }),
            )
            .children(banner)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(body)
                    .children(minimap_view),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, input.clone()),
                            cx,
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        listen(root, cx)
    }
}
