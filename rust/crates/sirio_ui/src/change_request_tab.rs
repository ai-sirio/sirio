//! One change request, read-only, in the Secondary half of the centre split
//! (spec §7.2, mockup A): a fixed header, then Conversation, Commits, Checks
//! and Files. Its identity is a `ChangeRef`, which is what the host keys the
//! tab by and what the session store keeps.

use gpui::{Context, EventEmitter, IntoElement, Render, Window, div, prelude::*};
use sirio_forge::ChangeRef;

/// The inner tab a change request shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InnerTab {
    #[default]
    Conversation,
    Commits,
    Checks,
    Files,
}

impl InnerTab {
    pub const ALL: [Self; 4] = [Self::Conversation, Self::Commits, Self::Checks, Self::Files];

    /// The session store's spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Commits => "commits",
            Self::Checks => "checks",
            Self::Files => "files",
        }
    }

    /// Anything unknown — a session written before, or by a later build —
    /// opens on the conversation.
    pub fn parse(value: &str) -> Self {
        match value {
            "commits" => Self::Commits,
            "checks" => Self::Checks,
            "files" => Self::Files,
            _ => Self::Conversation,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Conversation => "Conversation",
            Self::Commits => "Commits",
            Self::Checks => "Checks",
            Self::Files => "Files",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeRequestTabEvent {
    /// The title the forge now reports; the host's tab strip follows it.
    TitleChanged(String),
    /// A commit was clicked: the host opens it locally when the object is in
    /// the repository, otherwise on the forge.
    OpenCommit { sha: String, web_url: String },
    /// The user closed a tab that can no longer reach its change request.
    Close,
}

pub struct ChangeRequestTab {
    reference: ChangeRef,
    title: String,
    inner: InnerTab,
}

impl ChangeRequestTab {
    pub fn new(reference: ChangeRef, title: String, cx: &mut Context<Self>) -> Self {
        Self::restored(reference, title, InnerTab::Conversation, cx)
    }

    /// A tab brought back from the session: it shows its saved title at once
    /// and loads when it is shown (spec §8).
    pub fn restored(
        reference: ChangeRef,
        title: String,
        inner: InnerTab,
        cx: &mut Context<Self>,
    ) -> Self {
        let _ = cx;
        Self {
            reference,
            title,
            inner,
        }
    }

    pub fn reference(&self) -> &ChangeRef {
        &self.reference
    }

    pub fn inner_tab(&self) -> InnerTab {
        self.inner
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// The strip title: `#578 Fix the login redirect`, or the label alone.
    pub fn tab_title(reference: &ChangeRef, title: &str) -> String {
        if title.is_empty() {
            reference.label()
        } else {
            format!("{} {title}", reference.label())
        }
    }

    /// The bare title inside a saved strip title.
    pub fn title_from_tab(reference: &ChangeRef, tab_title: &str) -> String {
        tab_title
            .strip_prefix(&reference.label())
            .map(str::trim)
            .unwrap_or(tab_title)
            .to_string()
    }
}

impl EventEmitter<ChangeRequestTabEvent> for ChangeRequestTab {}

impl Render for ChangeRequestTab {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().id("change-request-tab").size_full()
    }
}
