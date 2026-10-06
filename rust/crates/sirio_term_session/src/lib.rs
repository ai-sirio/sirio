//! The terminal session engine runs PTYs, a resident VT, and session byte
//! streams without GPUI; it is used by `sirio_host` and by the app's in-process
//! fallback.

pub mod fanout;
pub mod journal;
