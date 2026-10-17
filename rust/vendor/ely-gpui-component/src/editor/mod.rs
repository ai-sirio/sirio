mod anchors;
mod buffer;
mod cursor;
mod decor;
mod edit;
mod find;
mod gutter;
mod ime;
mod keys;
mod layout;
mod line;
mod moves;
mod row;
mod state;
mod status;
mod syntax;
#[cfg(all(test, feature = "test-support"))]
mod tests;
mod view;

pub use buffer::Buffer;
pub use cursor::Selection;
pub use decor::{
    CodeLens, Diagnostic, DiffHunk, GhostText, GitMark, InlayHint, ReadOnlyBanner, VimMode,
    VimModeIndicator,
};
pub use find::{FindOptions, FindWidget, find_all};
pub(crate) use keys::bind_keys;
pub(crate) use layout::stack;
pub use state::{CodeEditor, CursorShape, EditorEvent, LineNumbers};
pub use status::{
    BranchIndicator, CursorPosition, Encoding, IndentSettings, LanguageMode, LineEnding, LspState,
    LspStatus, NotificationBell,
};
pub(crate) use syntax::colors as code_colors;
