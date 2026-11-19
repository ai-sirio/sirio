//! The one place a Sirio `Application` is built.
//!
//! gpui-pre ships no HTTP client: an `Application` starts with one that refuses every request,
//! so the Markdown Preview's remote images, a chat's linked images and the like would silently
//! stop loading. The engine Sirio ran on before installed `ReqwestClient::new()` in its
//! `application()`; this module installs exactly that, with no user agent, no proxy override and
//! no extra header, so the traffic is what it was.
//!
//! Every program that opens a window (the app and every example and probe) starts through
//! [`application`], so a screen that shows remote content looks the same in each of them.

use std::sync::Arc;

use reqwest_client::ReqwestClient;

/// The platform's `Application`, with the HTTP client installed.
pub fn application() -> gpui::Application {
    gpui_platform::application().with_http_client(Arc::new(ReqwestClient::new()))
}
