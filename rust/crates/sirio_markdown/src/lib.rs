//! Markdown source in, a structured document out.
//!
//! The chat pane renders an assistant's reply as markdown; that reply
//! arrives *streaming*, chunk by chunk, so this crate's job is turning a
//! possibly-truncated prefix of a document into a tree a renderer can walk
//! without re-parsing. The parser is [`pulldown_cmark`] — CommonMark-correct
//! and the crate everyone uses for this — and this crate translates its flat
//! event stream into the nested [`Document`] tree below.
//!
//! # Streaming contract
//!
//! The text handed to [`parse`] may end anywhere: mid-word, mid-fence,
//! mid-list. Parsing a truncated document never panics, never loses blocks
//! that were already complete, and reports an unterminated code fence as a
//! [`Block::CodeBlock`] with `open = true` — the renderer can keep appending
//! to it instead of drawing a closed block. [`pulldown_cmark`] closes an
//! unterminated fence at end of input, so open-ness is recovered from the
//! raw source: a lightweight, CommonMark-aware fence scan decides whether
//! the source ends inside an unclosed fence and marks the last code block
//! accordingly (see [`scan::ends_inside_fence`]).
//!
//! Re-parsing a longer prefix costs a full re-parse of everything seen so
//! far: [`pulldown_cmark`] has no incremental API, and CommonMark's
//! contextual rules mean a new line can retroactively change earlier
//! structure (a paragraph becomes a setext heading, a list goes tight or
//! loose). For typical replies — a few kilobytes — the cost is negligible
//! even at dozens of parses per reply, but a caller streaming a very long
//! document can throttle (parse at most once per frame, or only when the
//! pending bytes exceed a budget) and/or batch chunks instead of parsing
//! every byte.

mod document;
mod editing;
mod file_events;
mod model;
mod parse;
mod scan;

/// A scratch directory per test, unique by construction.
///
/// Three fixtures in this crate used to name their directory after
/// `SystemTime::now().as_nanos()` plus the process id, and one of them ended a
/// test with `remove_dir_all`. libtest starts every test in a binary at once
/// and that clock has microsecond resolution on macOS, so two fixtures created
/// in the same tick got the same path and one test deleted the other's file
/// out from under it — `dirty_external_changes_become_conflict_and_deletion_
/// is_explicit` failing on the release runner with `left: Deleted, right:
/// Conflict`, once, in a way that never reproduced.
///
/// A counter cannot collide within a process and the process id separates
/// processes, so the name is unique without depending on how fast the machine
/// is. Same shape as `sirio_project::git::tests::scratch_dir`. A stale
/// directory from a reused process id is harmless: every caller writes its
/// fixture file before reading it.
#[cfg(test)]
pub(crate) fn scratch_dir(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}

pub use document::{DocumentChange, DocumentError, MarkdownDocument};
pub use editing::{EditedText, SelectionRange, prefix_selected_lines, wrap_selection};
pub use file_events::{FileEventKind, FileSystemEvent, FileSystemEventMonitor};
pub use model::{Alignment, Block, Document, Inline, ListItem, ListKind, TableCell};
pub use parse::{parse, parse_with_options};

pub use pulldown_cmark::Options;
