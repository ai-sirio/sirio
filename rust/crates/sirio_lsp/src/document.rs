//! Telling a server which documents we are looking at, and what is in them.
//!
//! Full-text synchronisation, decided in the parent spec: Sirio's editor is
//! for reading, so shipping a whole buffer per change is affordable — and it
//! avoids incremental sync's range arithmetic, which is where a client and a
//! server most often drift apart without either noticing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::connection::Client;
use crate::uri::uri_for_path;
use crate::LspError;

/// The monotonic version each open document carries. The protocol requires
/// it to rise on every change; a server that sees it fall may ignore the
/// change, and then hover answers about text that is no longer there.
#[derive(Debug, Default)]
pub struct DocumentVersions(HashMap<PathBuf, i32>);

impl DocumentVersions {
    /// Registers a newly opened document and returns its first version.
    /// Reopening resets to 1: to the server it is a new document.
    pub fn opened(&mut self, path: &Path) -> i32 {
        self.0.insert(path.to_path_buf(), 1);
        1
    }

    pub fn changed(&mut self, path: &Path) -> i32 {
        let version = self.0.entry(path.to_path_buf()).or_insert(1);
        *version += 1;
        *version
    }

    pub fn closed(&mut self, path: &Path) {
        self.0.remove(path);
    }

    pub fn is_open(&self, path: &Path) -> bool {
        self.0.contains_key(path)
    }
}

pub async fn did_open(
    client: &Client,
    path: &Path,
    language_id: &str,
    version: i32,
    text: &str,
) -> Result<(), LspError> {
    client
        .notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri_for_path(path)?,
                    "languageId": language_id,
                    "version": version,
                    "text": text,
                }
            }),
        )
        .await
}

pub async fn did_change(
    client: &Client,
    path: &Path,
    version: i32,
    text: &str,
) -> Result<(), LspError> {
    client
        .notify(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri_for_path(path)?, "version": version },
                // One change, no range: the whole document. That is exactly
                // what `TextDocumentSyncKind::FULL` means on the wire.
                "contentChanges": [ { "text": text } ],
            }),
        )
        .await
}

pub async fn did_save(client: &Client, path: &Path, text: &str) -> Result<(), LspError> {
    client
        .notify(
            "textDocument/didSave",
            serde_json::json!({
                "textDocument": { "uri": uri_for_path(path)? },
                "text": text,
            }),
        )
        .await
}

pub async fn did_close(client: &Client, path: &Path) -> Result<(), LspError> {
    client
        .notify(
            "textDocument/didClose",
            serde_json::json!({ "textDocument": { "uri": uri_for_path(path)? } }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use crate::connection::tests::with_scripted_server;

    /// Runs `body` against a peer that records every message the client
    /// sends, and returns what it recorded. Notifications have no reply, so
    /// the script answers with a response nobody is waiting for — enough to
    /// keep the peer's loop alive, harmless because an unmatched id is
    /// dropped.
    async fn messages_sent<F, Fut>(body: F) -> Vec<serde_json::Value>
    where
        F: FnOnce(crate::Client) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let captured = seen.clone();
        let (client, _incoming) = with_scripted_server(move |request| {
            captured.lock().unwrap().push(request.clone());
            vec![serde_json::json!({"jsonrpc": "2.0", "id": 9999, "result": null})]
        })
        .await;

        body(client).await;

        for _ in 0..50 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            futures_timer::Delay::new(std::time::Duration::from_millis(10)).await;
        }
        let out = seen.lock().unwrap().clone();
        out
    }

    #[test]
    fn opening_a_document_sends_its_whole_text() {
        futures::executor::block_on(async {
            let sent = messages_sent(|client| async move {
                did_open(&client, Path::new("/tmp/a.rs"), "rust", 1, "fn main() {}")
                    .await
                    .expect("notification is sent");
            })
            .await;

            let message = sent.first().expect("one notification");
            assert_eq!(message["method"], serde_json::json!("textDocument/didOpen"));
            let document = &message["params"]["textDocument"];
            assert_eq!(document["uri"], serde_json::json!("file:///tmp/a.rs"));
            assert_eq!(document["languageId"], serde_json::json!("rust"));
            assert_eq!(document["version"], serde_json::json!(1));
            assert_eq!(document["text"], serde_json::json!("fn main() {}"));
            assert!(
                message.get("id").is_none(),
                "didOpen is a notification: an id would make the server answer it"
            );
        });
    }

    #[test]
    fn a_change_carries_the_whole_document_with_no_range() {
        futures::executor::block_on(async {
            let sent = messages_sent(|client| async move {
                did_change(&client, Path::new("/tmp/a.rs"), 2, "fn main() {} // edited")
                    .await
                    .expect("notification is sent");
            })
            .await;

            let message = sent.first().expect("one notification");
            assert_eq!(message["method"], serde_json::json!("textDocument/didChange"));
            assert_eq!(message["params"]["textDocument"]["version"], serde_json::json!(2));
            let changes = message["params"]["contentChanges"]
                .as_array()
                .expect("an array of changes");
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0]["text"], serde_json::json!("fn main() {} // edited"));
            assert!(
                changes[0].get("range").is_none(),
                "a change with no range is the whole document — that is what full sync means"
            );
        });
    }

    #[test]
    fn closing_a_document_names_only_its_uri() {
        futures::executor::block_on(async {
            let sent = messages_sent(|client| async move {
                did_close(&client, Path::new("/tmp/a.rs"))
                    .await
                    .expect("notification is sent");
            })
            .await;
            let message = sent.first().expect("one notification");
            assert_eq!(message["method"], serde_json::json!("textDocument/didClose"));
            assert_eq!(
                message["params"]["textDocument"]["uri"],
                serde_json::json!("file:///tmp/a.rs")
            );
        });
    }

    #[test]
    fn versions_start_at_one_and_only_ever_rise() {
        let mut versions = DocumentVersions::default();
        let path = Path::new("/tmp/a.rs");
        assert_eq!(versions.opened(path), 1);
        assert_eq!(versions.changed(path), 2);
        assert_eq!(versions.changed(path), 3);
        assert!(versions.is_open(path));
    }

    #[test]
    fn reopening_a_closed_document_starts_over() {
        let mut versions = DocumentVersions::default();
        let path = Path::new("/tmp/a.rs");
        versions.opened(path);
        versions.changed(path);
        versions.closed(path);
        assert!(!versions.is_open(path));
        assert_eq!(
            versions.opened(path),
            1,
            "a reopened document is a new document to the server"
        );
    }
}
