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

mod model;
mod parse;
mod scan;

pub use model::{
    Alignment, Block, Document, Inline, ListItem, ListKind, TableCell,
};
pub use parse::{parse, parse_with_options};

pub use pulldown_cmark::Options;
