//! Shell commands run from a chat draft that starts with `!`.
//!
//! Each piece here is pure or self-contained, so the chat's block can be fed
//! the raw bytes of a command and drawn from the result.

pub mod output;
