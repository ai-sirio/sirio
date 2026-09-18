//! Claude Code driven over its own stdio protocol, in place of an ACP
//! wrapper.
//!
//! The shape mirrors the ACP client next door — one worker thread, a
//! command channel in, an unbounded event channel out, four bounded waits —
//! and, crucially, emits the same [`AcpEvent`](crate::AcpEvent) values. The
//! chat surface is written against that vocabulary, so which transport a
//! tab uses is invisible above this line.

mod events;
