//! The composer band's measurements. The band spans the chat column; its
//! content keeps to the transcript's reading column, so the field's first
//! character sits on the transcript text's left edge.

use super::TRANSCRIPT_WIDTH;

/// The band's side padding: what is left between its edge and its content in a
/// pane too narrow for the reading column.
pub(crate) const BAND_PAD_X: f32 = 12.0;

/// How far the transcript pads its text from its own column's edge (the
/// `px(24.0)` on `chat-transcript`).
pub(crate) const TRANSCRIPT_PAD_X: f32 = 24.0;

/// The room between the band's top rule and the field's first row.
pub(crate) const BAND_TOP_PAD: f32 = 10.0;

/// The widest the band's content gets: the transcript's text column, so a wide
/// pane's field lines up with the text above it.
pub(crate) const BAND_COLUMN: f32 = TRANSCRIPT_WIDTH - 2.0 * TRANSCRIPT_PAD_X;
