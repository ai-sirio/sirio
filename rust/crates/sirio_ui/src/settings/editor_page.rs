//! Settings → Editor: the code editor's preferences, drawn with Ely's
//! controls. Every control writes through `Settings::set_editor_prefs`, so a
//! change reaches the snapshot the host saves and applies.

use super::*;
use ely_gpui_component::buttons::{ToggleGroup, ToggleItem};
use ely_gpui_component::forms::{Checkbox, Input, InputEvent, TextInput};
use ely_gpui_component::theme::ControlSize;
use sirio_persistence::{EditorCursorShape, EditorLineNumbers, settings_ranges};
use std::ops::RangeInclusive;

/// A number field's own fixed width, the same kind of geometry as a control's
/// footprint rather than spacing.
const NUMBER_FIELD_WIDTH: f32 = 88.0;

/// The Editor page's two number fields. Cloning shares the entities.
#[derive(Clone)]
pub(super) struct EditorInputs {
    pub(super) ruler_column: Entity<TextInput>,
    pub(super) tab_size: Entity<TextInput>,
}

/// The number fields a commit can write.
#[derive(Clone, Copy)]
enum EditorNumber {
    RulerColumn,
    TabSize,
}

/// The flag one switch writes in `EditorPrefs`.
type EditorFlag = fn(&mut EditorPrefs) -> &mut bool;

impl Settings {
    /// The Editor page's number fields, made on first use and kept after.
    pub(super) fn editor_fields(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EditorInputs {
        if let Some(inputs) = &self.editor_inputs {
            return inputs.clone();
        }
        let ruler_column = number_field(self.editor.ruler_column, !self.editor.ruler, window, cx);
        let tab_size = number_field(self.editor.tab_size, false, window, cx);
        // Enter and leaving the field both commit; typing alone changes nothing
        // the host sees until then.
        cx.subscribe(&ruler_column, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit | InputEvent::Blur) {
                this.commit_editor_number(EditorNumber::RulerColumn, cx);
            }
        })
        .detach();
        cx.subscribe(&tab_size, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit | InputEvent::Blur) {
                this.commit_editor_number(EditorNumber::TabSize, cx);
            }
        })
        .detach();
        let inputs = EditorInputs {
            ruler_column,
            tab_size,
        };
        self.editor_inputs = Some(inputs.clone());
        inputs
    }

    /// Writes a number field's text to its preference. An empty field keeps
    /// the stored value; anything else is clamped into the field's range, and
    /// the field shows the value that was kept.
    fn commit_editor_number(&mut self, number: EditorNumber, cx: &mut Context<Self>) {
        let Some(inputs) = self.editor_inputs.as_ref() else {
            return;
        };
        let (input, range, current) = match number {
            EditorNumber::RulerColumn => (
                &inputs.ruler_column,
                settings_ranges::EDITOR_RULER_COLUMN,
                self.editor.ruler_column,
            ),
            EditorNumber::TabSize => (
                &inputs.tab_size,
                settings_ranges::EDITOR_TAB_SIZE,
                self.editor.tab_size,
            ),
        };
        let value = committed_number(input.read(cx).text(), range, current);
        input.update(cx, |input, cx| input.set_text(value.to_string(), cx));
        if value == current {
            return;
        }
        let mut prefs = self.editor.clone();
        match number {
            EditorNumber::RulerColumn => prefs.ruler_column = value,
            EditorNumber::TabSize => prefs.tab_size = value,
        }
        self.set_editor_prefs(prefs, cx);
    }

    pub(super) fn render_editor(
        &self,
        theme: Theme,
        entity: Entity<Self>,
        fields: EditorInputs,
    ) -> gpui::Div {
        let prefs = &self.editor;
        let switches: [(&'static str, &'static str, bool, EditorFlag); 8] = [
            ("settings-editor-minimap", "Minimap", prefs.minimap, |p| {
                &mut p.minimap
            }),
            (
                "settings-editor-sticky-scroll",
                "Sticky scroll",
                prefs.sticky_scroll,
                |p| &mut p.sticky_scroll,
            ),
            (
                "settings-editor-indent-guides",
                "Indent guides",
                prefs.indent_guides,
                |p| &mut p.indent_guides,
            ),
            (
                "settings-editor-rainbow-brackets",
                "Rainbow brackets",
                prefs.rainbow_brackets,
                |p| &mut p.rainbow_brackets,
            ),
            (
                "settings-editor-code-lens",
                "Code lens",
                prefs.code_lens,
                |p| &mut p.code_lens,
            ),
            (
                "settings-editor-inlay-hints",
                "Inlay hints",
                prefs.inlay_hints,
                |p| &mut p.inlay_hints,
            ),
            (
                "settings-editor-whitespace",
                "Visible whitespace",
                prefs.whitespace,
                |p| &mut p.whitespace,
            ),
            ("settings-editor-ruler", "Ruler", prefs.ruler, |p| &mut p.ruler),
        ];
        let mut display = controls::card(theme);
        for (index, (id, label, on, flag)) in switches.into_iter().enumerate() {
            if index > 0 {
                display = display.child(controls::separator(theme));
            }
            display = display.child(editor_switch(id, label, on, flag, entity.clone()));
        }

        let line_entity = entity.clone();
        let line_numbers = ToggleGroup::new("settings-editor-line-numbers-group")
            .size(ControlSize::Sm)
            .item(ToggleItem::new(EditorLineNumbers::Absolute.raw()).label("Absolute"))
            .item(ToggleItem::new(EditorLineNumbers::Relative.raw()).label("Relative"))
            .item(ToggleItem::new(EditorLineNumbers::Hidden.raw()).label("Hidden"))
            .selected([prefs.line_numbers.raw()])
            .on_change(move |selected, _, cx| {
                // A press on the chosen face empties the selection; keep the mode.
                let Some(value) = selected
                    .first()
                    .and_then(|value| EditorLineNumbers::parse(value))
                else {
                    return;
                };
                line_entity.update(cx, |settings, cx| {
                    let mut prefs = settings.editor.clone();
                    prefs.line_numbers = value;
                    settings.set_editor_prefs(prefs, cx);
                });
            });
        let cursor_entity = entity.clone();
        let cursor_shape = ToggleGroup::new("settings-editor-cursor-shape-group")
            .size(ControlSize::Sm)
            .item(ToggleItem::new(EditorCursorShape::Line.raw()).label("Line"))
            .item(ToggleItem::new(EditorCursorShape::Block.raw()).label("Block"))
            .item(ToggleItem::new(EditorCursorShape::Underline.raw()).label("Underline"))
            .selected([prefs.cursor_shape.raw()])
            .on_change(move |selected, _, cx| {
                let Some(value) = selected
                    .first()
                    .and_then(|value| EditorCursorShape::parse(value))
                else {
                    return;
                };
                cursor_entity.update(cx, |settings, cx| {
                    let mut prefs = settings.editor.clone();
                    prefs.cursor_shape = value;
                    settings.set_editor_prefs(prefs, cx);
                });
            });
        display = display
            .child(controls::separator(theme))
            .child(
                div()
                    .id("settings-editor-line-numbers-row")
                    .debug_selector(|| "settings-editor-line-numbers".into())
                    .child(controls::row("Line numbers", None, line_numbers, theme)),
            )
            .child(controls::separator(theme))
            .child(
                div()
                    .id("settings-editor-cursor-shape-row")
                    .debug_selector(|| "settings-editor-cursor-shape".into())
                    .child(controls::row("Cursor shape", None, cursor_shape, theme)),
            );

        let text = controls::card(theme)
            .child(
                div()
                    .id("settings-editor-ruler-column-row")
                    .child(controls::row(
                        "Ruler column",
                        None,
                        div()
                            .debug_selector(|| "settings-editor-ruler-column".into())
                            .w(px(NUMBER_FIELD_WIDTH))
                            .child(Input::new(&fields.ruler_column)),
                        theme,
                    )),
            )
            .child(controls::separator(theme))
            .child(
                div()
                    .id("settings-editor-tab-size-row")
                    .child(controls::row(
                        "Tab size",
                        None,
                        div()
                            .debug_selector(|| "settings-editor-tab-size".into())
                            .w(px(NUMBER_FIELD_WIDTH))
                            .child(Input::new(&fields.tab_size)),
                        theme,
                    )),
            );

        div()
            .w(px(CONTENT_WIDTH))
            .pt(px(DETAIL_TOP_PADDING))
            .pb(px(DETAIL_BOTTOM_PADDING))
            .child(settings_section("Display", display, theme))
            .child(settings_section("Text", text, theme))
    }
}

/// One on/off preference as a checkbox row. The checkbox reports the value it
/// turns to, and the handler writes that into a copy of the current
/// preferences, so a click never writes back a value drawn earlier.
fn editor_switch(
    id: &'static str,
    label: &'static str,
    on: bool,
    flag: EditorFlag,
    entity: Entity<Settings>,
) -> impl IntoElement {
    div()
        .id(format!("{id}-row"))
        .debug_selector(move || id.into())
        // The 44px touch target and padding of `controls::row`.
        .min_h(px(44.0))
        .px(px(BezelTheme::SPACE_MD))
        .flex()
        .items_center()
        // The checkbox fills the row, so a press anywhere on the row toggles it.
        .child(div().flex_1().min_w_0().child(
            Checkbox::new(id, on)
                .label(label)
                .on_change(move |value, _, cx| {
                    entity.update(cx, |settings, cx| {
                        let mut prefs = settings.editor.clone();
                        *flag(&mut prefs) = value;
                        settings.set_editor_prefs(prefs, cx);
                    });
                }),
        ))
}

/// A digits-only field holding `value`. Four digits is more than either range
/// needs, and it keeps the parse at commit clear of overflow.
fn number_field(
    value: usize,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<Settings>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        let mut input = TextInput::new(window, cx)
            .filter(|c| c.is_ascii_digit())
            .max_len(4);
        input.set_text(value.to_string(), cx);
        input.set_disabled(disabled, cx);
        input
    })
}

/// The value a number field commits: the text read as a number and clamped
/// into `range`, or `current` when the field is empty.
fn committed_number(text: &str, range: RangeInclusive<i64>, current: usize) -> usize {
    text.parse::<i64>().map_or(current, |value| {
        value.clamp(*range.start(), *range.end()) as usize
    })
}
