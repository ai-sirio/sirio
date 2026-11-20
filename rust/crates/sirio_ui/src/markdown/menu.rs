//! The right-click menu of a Markdown body: what it offers, by what was pressed.
//!
//! Sirio's popup draws it where a surface hosts the body (Preview merges these rows with the
//! file view's, change requests and the chat show them as they are); this module is the
//! rows, in order, and what each one does to the body.

use gpui::{App, ClipboardItem};

use super::view::{BodyActions, LinkHandler};

/// One row of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownMenuItem {
    /// Copies the selection; disabled while nothing is selected.
    Copy { enabled: bool },
    /// Selects the whole body.
    SelectAll,
    /// Opens the link pressed, through the surface's opener.
    OpenLink(String),
    /// Puts the link's address on the clipboard.
    CopyLinkAddress(String),
}

impl MarkdownMenuItem {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Copy { .. } => "Copy",
            Self::SelectAll => "Select all",
            Self::OpenLink(_) => "Open link",
            Self::CopyLinkAddress(_) => "Copy link address",
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        !matches!(self, Self::Copy { enabled: false })
    }
}

/// The rows for a press: Copy and Select all always, the link's two over a link.
pub(crate) fn items(selected: bool, link: Option<&str>) -> Vec<MarkdownMenuItem> {
    let mut items = vec![
        MarkdownMenuItem::Copy { enabled: selected },
        MarkdownMenuItem::SelectAll,
    ];
    if let Some(link) = link {
        items.push(MarkdownMenuItem::OpenLink(link.to_owned()));
        items.push(MarkdownMenuItem::CopyLinkAddress(link.to_owned()));
    }
    items
}

/// Does what `item` says to the body `actions` belongs to.
pub(crate) fn perform(
    item: &MarkdownMenuItem,
    actions: &BodyActions,
    links: &LinkHandler,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    match item {
        MarkdownMenuItem::Copy { enabled } => {
            if *enabled && let Some(text) = actions.selected(cx) {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
        MarkdownMenuItem::SelectAll => actions.select_all(window, cx),
        MarkdownMenuItem::OpenLink(link) => links(link, window, cx),
        MarkdownMenuItem::CopyLinkAddress(link) => {
            cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
        }
    }
}
