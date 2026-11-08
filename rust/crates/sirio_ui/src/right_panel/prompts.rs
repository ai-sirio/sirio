//! The right panel's Prompts view (spec `2026-11-01-prompt-list-design.md`
//! §2 and §5, mockup list B): the saved prompts as cards, newest first, each
//! with its title, the first two lines of its text, and Edit and Copy.
//!
//! The list is global, not per worktree, so it renders with or without one.
//! Without a worktree the centre draws no tabs, so New prompt, Edit and the
//! card click are disabled; Copy and Delete still work.
//!
//! The view reads the folder when it is entered and whenever Sirio itself
//! writes to it ([`PromptsRevision`]). Opening and closing tabs is the host's:
//! the view only raises [`RightPanelActionEvent::OpenPrompt`] and
//! [`RightPanelActionEvent::PromptDeleted`].
//!
//! Every button the E2E clicks records its window bounds at prepaint, for
//! [`RightPanel::prompts_facts`]; gpui's debug selectors exist only in tests.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    menus::{ContextMenu, Menu, MenuItem},
    overlays::Dialog,
    primitives::IconName,
    theme::ControlSize,
};
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, Entity, FontWeight, Pixels, ScrollHandle,
    Subscription, Window, canvas, div, prelude::*, px,
};
use sirio_theme::Theme;

use super::{PanelView, RightPanel, RightPanelActionEvent};
use crate::ely_ui;
use crate::prompts::{self, PromptsRevision};
use crate::text_selection::selectable_text;

/// How long Copy shows its check.
const COPIED_FOR: Duration = Duration::from_secs(2);
const NEEDS_WORKTREE: &str = "Select a worktree to edit prompts";
const UNTITLED: &str = "Untitled prompt";

type BoundsCell = Rc<Cell<Option<Bounds<Pixels>>>>;

/// What the debug verb `surface.prompts.read` reports. A bounds field is
/// `None` while that element is not drawn.
#[derive(Clone, Debug, Default)]
pub struct PromptsFacts {
    /// The Prompts view is the panel's current view, and the panel is drawn.
    pub shown: bool,
    /// New prompt, Edit and the card click work: a worktree is selected.
    pub can_edit: bool,
    pub rail_button: Option<Bounds<Pixels>>,
    pub new_button: Option<Bounds<Pixels>>,
    pub cards: Vec<PromptCardFacts>,
    /// A point inside the context menu's `Delete…` row. The row is Ely's
    /// own and cannot be measured from here, so this is the box of a
    /// marker drawn at the row's end.
    pub menu_delete: Option<Bounds<Pixels>>,
    pub confirm_delete: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug)]
pub struct PromptCardFacts {
    pub path: PathBuf,
    /// The index title; empty when untitled.
    pub title: String,
    pub preview: String,
    pub card: Option<Bounds<Pixels>>,
    pub edit: Option<Bounds<Pixels>>,
    pub copy: Option<Bounds<Pixels>>,
    /// Copy is showing its check.
    pub copied: bool,
}

/// One card, as read from the folder.
#[derive(Clone)]
struct Card {
    path: PathBuf,
    title: String,
    preview: String,
}

impl Card {
    fn shown_title(&self) -> &str {
        if self.title.is_empty() { UNTITLED } else { &self.title }
    }
}

#[derive(Default)]
struct CardBounds {
    card: BoundsCell,
    edit: BoundsCell,
    copy: BoundsCell,
}

/// The Prompts view's state, held by the panel.
pub(super) struct PromptsView {
    cards: Vec<Card>,
    /// Watches Sirio's own writes. Taken the first time the view renders.
    revision: Option<Subscription>,
    /// The prompt whose Copy shows its check, and which copy did it: a later
    /// copy restarts the timer, so an earlier timer must not clear it.
    copied: Option<(PathBuf, u64)>,
    copies: u64,
    /// The prompt the delete dialog asks about.
    deleting: Option<Card>,
    scroll: ScrollHandle,
    rail: BoundsCell,
    new_button: BoundsCell,
    menu_delete: BoundsCell,
    confirm_delete: BoundsCell,
    card_bounds: Vec<CardBounds>,
}

impl Default for PromptsView {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            revision: None,
            copied: None,
            copies: 0,
            deleting: None,
            scroll: ScrollHandle::new(),
            rail: BoundsCell::default(),
            new_button: BoundsCell::default(),
            menu_delete: BoundsCell::default(),
            confirm_delete: BoundsCell::default(),
            card_bounds: Vec::new(),
        }
    }
}

impl PromptsView {
    /// Forgets every recorded bounds; the frame being built records again
    /// what it draws.
    pub(super) fn clear_bounds(&self) {
        for cell in [&self.rail, &self.new_button, &self.menu_delete, &self.confirm_delete] {
            cell.set(None);
        }
        for card in &self.card_bounds {
            for cell in [&card.card, &card.edit, &card.copy] {
                cell.set(None);
            }
        }
    }

    /// The rail button's cell, for the header to measure into.
    pub(super) fn rail_cell(&self) -> &BoundsCell {
        &self.rail
    }
}

impl RightPanel {
    /// What the debug verb reports about the Prompts view.
    pub fn prompts_facts(&self) -> PromptsFacts {
        let view = &self.prompts;
        let drawn = self.panel_width > 0.0;
        let shown = drawn && self.last_panel_view == Some(PanelView::Prompts);
        let get = |cell: &BoundsCell, visible: bool| if visible { cell.get() } else { None };
        PromptsFacts {
            shown,
            can_edit: self.worktree_selected,
            rail_button: get(&view.rail, drawn),
            new_button: get(&view.new_button, shown),
            cards: view
                .cards
                .iter()
                .enumerate()
                .map(|(ix, card)| {
                    let bounds = view.card_bounds.get(ix);
                    let at = |pick: fn(&CardBounds) -> &BoundsCell| {
                        bounds.and_then(|bounds| get(pick(bounds), shown))
                    };
                    PromptCardFacts {
                        path: card.path.clone(),
                        title: card.title.clone(),
                        preview: card.preview.clone(),
                        card: at(|b| &b.card),
                        edit: at(|b| &b.edit),
                        copy: at(|b| &b.copy),
                        copied: view.copied.as_ref().is_some_and(|(path, _)| *path == card.path),
                    }
                })
                .collect(),
            menu_delete: get(&view.menu_delete, shown),
            confirm_delete: get(&view.confirm_delete, shown),
        }
    }

    /// Reads the folder again. Draws nothing by itself.
    fn load_prompts(&mut self, cx: &App) {
        let cards: Vec<Card> = prompts::store(cx)
            .map(|store| {
                store
                    .list()
                    .into_iter()
                    .map(|entry| Card {
                        preview: store.read_preview(&entry.path),
                        path: entry.path,
                        title: entry.title,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.prompts.card_bounds = cards.iter().map(|_| CardBounds::default()).collect();
        self.prompts.cards = cards;
    }

    fn open_prompt(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.worktree_selected {
            cx.emit(RightPanelActionEvent::OpenPrompt(path));
        }
    }

    fn new_prompt(&mut self, cx: &mut Context<Self>) {
        if !self.worktree_selected {
            return;
        }
        match prompts::create(cx) {
            Ok(path) => cx.emit(RightPanelActionEvent::OpenPrompt(path)),
            Err(error) => cx.emit(RightPanelActionEvent::PromptFailed(format!(
                "Could not create a prompt: {error}"
            ))),
        }
    }

    fn copy_prompt(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let text = match prompts::copy_text(&path, cx) {
            Ok(text) => text,
            Err(error) => {
                cx.emit(RightPanelActionEvent::PromptFailed(format!(
                    "Could not copy the prompt: {error}"
                )));
                return;
            }
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.prompts.copies += 1;
        let copy = self.prompts.copies;
        self.prompts.copied = Some((path, copy));
        cx.notify();
        cx.spawn(async move |panel, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = panel.update(cx, |panel, cx| {
                if panel.prompts.copied.as_ref().is_some_and(|(_, at)| *at == copy) {
                    panel.prompts.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn ask_delete(&mut self, card: Card, cx: &mut Context<Self>) {
        self.prompts.deleting = Some(card);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if self.prompts.deleting.take().is_some() {
            cx.notify();
        }
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(card) = self.prompts.deleting.take() else {
            return;
        };
        match prompts::delete(&card.path, cx) {
            Ok(()) => {
                if self.prompts.copied.as_ref().is_some_and(|(path, _)| *path == card.path) {
                    self.prompts.copied = None;
                }
                cx.emit(RightPanelActionEvent::PromptDeleted(card.path));
            }
            Err(error) => cx.emit(RightPanelActionEvent::PromptFailed(format!(
                "Could not delete the prompt: {error}"
            ))),
        }
        cx.notify();
    }

    /// The view's body. `entered` is true on the first frame after the user
    /// switched to it, when outside edits are picked up.
    pub(super) fn render_prompts(
        &mut self,
        entered: bool,
        entity: Entity<Self>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Ely draws from its own theme global; a surface outside the chat
        // must sync it, or its buttons stay dark in Light mode.
        crate::ely::sync_theme_if_changed(cx);
        if self.prompts.revision.is_none() {
            self.prompts.revision = Some(cx.observe_global::<PromptsRevision>(|panel, cx| {
                if PanelView::get(cx) == PanelView::Prompts {
                    panel.load_prompts(cx);
                    cx.notify();
                }
            }));
            self.load_prompts(cx);
        } else if entered {
            self.load_prompts(cx);
        }
        let can_edit = self.worktree_selected;
        let count = self.prompts.cards.len();
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .pl(px(12.0))
            .pr(px(8.0))
            .pt(px(10.0))
            .pb(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(theme.typography.scaled(13.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.ely.fg)
                            .child("Prompts"),
                    )
                    .child(div().text_color(theme.ely.fg_muted).child(count.to_string())),
            )
            .child(measured(
                &self.prompts.new_button,
                new_prompt_button("prompts-new", can_edit, entity.clone()),
            ));
        let body = if self.prompts.cards.is_empty() {
            div()
                .id("prompts-empty")
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .p(px(12.0))
                .child(
                    div()
                        .text_size(theme.typography.scaled(13.0))
                        .text_color(theme.ely.fg_muted)
                        .child("No prompts yet"),
                )
                .child(new_prompt_button("prompts-new-empty", can_edit, entity.clone()))
                .into_any_element()
        } else {
            let cards = self
                .prompts
                .cards
                .iter()
                .enumerate()
                .map(|(ix, card)| self.render_card(ix, card, can_edit, &entity, theme))
                .collect::<Vec<_>>();
            div()
                .flex_1()
                .min_h(px(0.0))
                .relative()
                .child(
                    div()
                        .id("prompts-list")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.prompts.scroll)
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .px(px(6.0))
                        .pt(px(2.0))
                        .pb(px(10.0))
                        .children(cards),
                )
                .child(crate::scroll::bar("prompts-scrollbar", &self.prompts.scroll))
                .into_any_element()
        };
        div()
            .id("right-panel-prompts")
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(header)
            .child(body)
            .children(self.render_delete_dialog(&entity))
            .into_any_element()
    }

    fn render_card(
        &self,
        ix: usize,
        card: &Card,
        can_edit: bool,
        entity: &Entity<Self>,
        theme: Theme,
    ) -> AnyElement {
        let Some(bounds) = self.prompts.card_bounds.get(ix) else {
            return div().into_any_element();
        };
        let copied = self.prompts.copied.as_ref().is_some_and(|(path, _)| *path == card.path);
        let (border, border_hover) = (theme.ely.border, theme.ely.border_strong);
        let title = div()
            .truncate()
            .text_size(theme.typography.scaled(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if card.title.is_empty() { theme.ely.fg_muted } else { theme.ely.fg })
            .child(card.shown_title().to_owned());
        let text = div()
            .id(("prompt-card-text", ix))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(title)
            .when(!card.preview.is_empty(), |text| {
                text.child(
                    div()
                        .text_size(theme.typography.scaled(13.0))
                        .text_color(theme.ely.fg_muted)
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(card.preview.clone()),
                )
            })
            .when(!can_edit, |text| text.tooltip(crate::controls::tooltip(NEEDS_WORKTREE)));
        let edit = {
            let (entity, path) = (entity.clone(), card.path.clone());
            ely_ui::small_icon_button(
                "prompt-edit",
                IconName::Pencil,
                if can_edit { "Edit" } else { NEEDS_WORKTREE },
                can_edit,
                move |_, cx| {
                    cx.stop_propagation();
                    entity.update(cx, |panel, cx| panel.open_prompt(path.clone(), cx));
                },
            )
        };
        let copy = {
            let (entity, path) = (entity.clone(), card.path.clone());
            ely_ui::small_icon_button(
                "prompt-copy",
                if copied { IconName::Check } else { IconName::Copy },
                if copied { "Copied" } else { "Copy" },
                true,
                move |_, cx| {
                    cx.stop_propagation();
                    entity.update(cx, |panel, cx| panel.copy_prompt(path.clone(), cx));
                },
            )
        };
        let body = div()
            .id(("prompt-card", ix))
            .w_full()
            .flex()
            .items_start()
            .gap(px(6.0))
            .pl(px(10.0))
            .pr(px(6.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .bg(theme.ely.surface)
            .border_1()
            .border_color(border)
            .hover(move |style| style.border_color(border_hover))
            .when(can_edit, |body| {
                let (entity, path) = (entity.clone(), card.path.clone());
                body.cursor_pointer().on_click(move |_, _, cx| {
                    entity.update(cx, |panel, cx| panel.open_prompt(path.clone(), cx))
                })
            })
            .child(text)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_start()
                    .child(measured(&bounds.edit, edit))
                    .child(measured(&bounds.copy, copy)),
            );
        let menu = {
            let (entity, card, marker) =
                (entity.clone(), card.clone(), self.prompts.menu_delete.clone());
            let measure = marker.clone();
            Menu::new().item(
                MenuItem::new("Delete…")
                    .icon(IconName::Trash2)
                    .selectors("prompt-menu-delete", None)
                    .trailing(move |_| marker_at(&measure))
                    .on_click(move |_, cx| {
                        marker.set(None);
                        entity.update(cx, |panel, cx| panel.ask_delete(card.clone(), cx));
                    }),
            )
        };
        ContextMenu::new(("prompt-card-menu", ix), menu)
            .child(measured(&bounds.card, body))
            .into_any_element()
    }

    fn render_delete_dialog(&self, entity: &Entity<Self>) -> Option<AnyElement> {
        let card = self.prompts.deleting.as_ref()?;
        let title = format!("Delete “{}”?", card.shown_title());
        let (close, confirm) = (entity.clone(), entity.clone());
        let cell = self.prompts.confirm_delete.clone();
        let dialog = Dialog::new("prompt-delete-dialog", title, move |_, cx| {
            close.update(cx, |panel, cx| panel.cancel_delete(cx))
        })
        .child(selectable_text("This removes the prompt from disk."))
        .action(|close| {
            dialog_button("prompt-delete-cancel", "Cancel", ButtonVariant::Ghost, move |window, cx| {
                close(window, cx)
            })
        })
        .action(move |close| {
            measured(
                &cell,
                dialog_button("prompt-delete-confirm", "Delete", ButtonVariant::Danger, move |window, cx| {
                    // Confirm first: closing runs `cancel_delete`, which would
                    // take the prompt before the confirm could read it.
                    confirm.update(cx, |panel, cx| panel.confirm_delete(cx));
                    close(window, cx);
                }),
            )
        });
        Some(div().child(dialog).into_any_element())
    }
}

/// New prompt, in the header and in the empty state. Disabled without a
/// worktree, saying why.
fn new_prompt_button(id: &'static str, enabled: bool, entity: Entity<RightPanel>) -> AnyElement {
    div()
        .id(id)
        .flex_none()
        .when(!enabled, |wrapper| wrapper.tooltip(crate::controls::tooltip(NEEDS_WORKTREE)))
        .child(
            Button::new(id, "New prompt")
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .icon(IconName::Plus)
                .disabled(!enabled)
                .on_click(move |_, _, cx| entity.update(cx, |panel, cx| panel.new_prompt(cx))),
        )
        .into_any_element()
}

/// One of the delete dialog's buttons.
fn dialog_button(
    id: &'static str,
    label: &'static str,
    variant: ButtonVariant,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .flex_none()
        .child(Button::new(id, label).variant(variant).on_click(move |_, window, cx| on_click(window, cx)))
        .into_any_element()
}

/// `child`, with its window bounds recorded into `cell` at prepaint, the way
/// `project_identity_dialog.rs` measures its field.
fn measured(cell: &BoundsCell, child: impl IntoElement) -> AnyElement {
    div().relative().child(child).child(bounds_probe(cell)).into_any_element()
}

/// Covers its `relative` parent and records the parent's window bounds into
/// `cell` at prepaint.
pub(super) fn bounds_probe(cell: &BoundsCell) -> AnyElement {
    let cell = cell.clone();
    canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
}

/// An invisible point that records where it is drawn: the `Delete…` row's
/// trailing element, the one hook Ely gives a host inside a menu row.
fn marker_at(cell: &BoundsCell) -> AnyElement {
    let cell = cell.clone();
    canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {})
        .w(px(1.0))
        .h(px(1.0))
        .into_any_element()
}
