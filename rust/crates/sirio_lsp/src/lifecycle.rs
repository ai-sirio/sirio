//! The obligatory opening and closing of a session.
//!
//! ```text
//!   → initialize      (request)       rootUri, our capabilities, processId
//!   ← InitializeResult                its capabilities
//!   → initialized     (notification)
//!   … work …
//!   → shutdown        (request)       and wait for the response
//!   → exit            (notification)
//! ```
//!
//! What comes back from `initialize` decides what the UI offers. A menu
//! entry for a capability the server does not have is worse than no entry:
//! it looks like a feature and behaves like a bug.

use std::path::Path;

use lsp_types::{
    ClientCapabilities, GotoCapability, HoverClientCapabilities, InitializeParams,
    InitializeResult, MarkupKind, OneOf, PublishDiagnosticsClientCapabilities, ServerCapabilities,
    TextDocumentSyncClientCapabilities,
};

use crate::connection::Client;
use crate::LspError;

/// The four questions the UI asks of a server, flattened out of
/// `lsp_types`' nested options so no caller repeats the unwrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub hover: bool,
    pub definition: bool,
    pub references: bool,
    pub document_symbols: bool,
}

impl From<&ServerCapabilities> for Capabilities {
    fn from(raw: &ServerCapabilities) -> Self {
        fn offered(provider: Option<&OneOf<bool, impl Sized>>) -> bool {
            match provider {
                Some(OneOf::Left(enabled)) => *enabled,
                // The options form is always a real offer; only the boolean
                // form can say "no".
                Some(OneOf::Right(_)) => true,
                None => false,
            }
        }

        Self {
            hover: match &raw.hover_provider {
                Some(lsp_types::HoverProviderCapability::Simple(enabled)) => *enabled,
                Some(lsp_types::HoverProviderCapability::Options(_)) => true,
                None => false,
            },
            definition: offered(raw.definition_provider.as_ref()),
            references: offered(raw.references_provider.as_ref()),
            document_symbols: offered(raw.document_symbol_provider.as_ref()),
        }
    }
}

/// What we tell the server we can do. Two rules govern this value.
///
/// **Declare only what we can use.** `workspace.configuration` and
/// `window.workDoneProgress` are absent deliberately: declaring them
/// invites requests whose only honest answer from us is `null`.
///
/// **`linkSupport: false` is a decision, not a default.** With it off the
/// server must answer `Location` rather than `LocationLink`, which halves
/// the shapes `navigation::definition` has to understand.
fn client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            hover: Some(HoverClientCapabilities {
                dynamic_registration: Some(false),
                content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
            }),
            definition: Some(GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(false),
            }),
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                related_information: Some(false),
                ..Default::default()
            }),
            synchronization: Some(TextDocumentSyncClientCapabilities {
                dynamic_registration: Some(false),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                did_save: Some(true),
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Runs `initialize` then `initialized`, returning what the server offers.
pub async fn initialize(client: &Client, root: &Path) -> Result<Capabilities, LspError> {
    let root_uri = crate::uri::uri_for_path(root)?;

    #[allow(deprecated)] // `root_uri` is deprecated in the spec but still
    // what several servers actually read; sending both is the pragmatic
    // choice every client makes.
    let params = InitializeParams {
        process_id: Some(std::process::id()),
        root_uri: Some(root_uri.clone()),
        capabilities: client_capabilities(),
        workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
            uri: root_uri,
            name: root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }]),
        ..Default::default()
    };

    let result: InitializeResult = client.request("initialize", params).await?;
    client.notify("initialized", serde_json::json!({})).await?;
    Ok(Capabilities::from(&result.capabilities))
}

/// The closing half. `shutdown` is a request and its answer must be waited
/// for; `exit` is a notification and ends the conversation. The caller still
/// has to reap the process — and kill it if it ignores `exit`.
pub async fn shutdown(client: &Client) -> Result<(), LspError> {
    let _: serde_json::Value = client.request("shutdown", serde_json::json!(null)).await?;
    client.notify("exit", serde_json::json!(null)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::connection::tests::with_scripted_server;
    use lsp_types::{OneOf, ServerCapabilities};

    #[test]
    fn a_server_offering_everything_reports_all_four_capabilities() {
        let raw = ServerCapabilities {
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            ..Default::default()
        };
        let capabilities = Capabilities::from(&raw);
        assert_eq!(
            capabilities,
            Capabilities { hover: true, definition: true, references: true, document_symbols: true }
        );
    }

    #[test]
    fn a_server_offering_nothing_reports_no_capabilities() {
        let capabilities = Capabilities::from(&ServerCapabilities::default());
        assert_eq!(
            capabilities,
            Capabilities {
                hover: false,
                definition: false,
                references: false,
                document_symbols: false
            }
        );
    }

    #[test]
    fn a_provider_explicitly_set_to_false_is_not_offered() {
        // `Some(false)` and `None` mean the same thing to the UI, and a
        // client that only checked `is_some()` would offer a dead menu item.
        let raw = ServerCapabilities {
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(false)),
            definition_provider: Some(OneOf::Left(false)),
            ..Default::default()
        };
        let capabilities = Capabilities::from(&raw);
        assert!(!capabilities.hover);
        assert!(!capabilities.definition);
    }

    #[test]
    fn the_handshake_reports_what_the_server_advertised() {
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|request| {
                if request["method"] == "initialize" {
                    vec![serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request["id"],
                        "result": {
                            "capabilities": {
                                "hoverProvider": true,
                                "definitionProvider": true,
                                "referencesProvider": false
                            }
                        }
                    })]
                } else {
                    vec![]
                }
            })
            .await;

            let capabilities = initialize(&client, Path::new("/tmp")).await.unwrap();
            assert!(capabilities.hover);
            assert!(capabilities.definition);
            assert!(!capabilities.references, "a false provider is not an offer");
            assert!(!capabilities.document_symbols, "an absent provider is not an offer");
        });
    }

    #[test]
    fn initialize_declares_only_the_capabilities_we_can_serve() {
        let capabilities = client_capabilities();
        let json = serde_json::to_value(&capabilities).expect("capabilities serialise");

        let text_document = &json["textDocument"];
        assert!(text_document["hover"]["contentFormat"].is_array());
        assert_eq!(
            text_document["definition"]["linkSupport"],
            serde_json::json!(false),
            "linkSupport off keeps the answer a Location, halving what navigation parses"
        );
        assert_eq!(
            text_document["synchronization"]["didSave"],
            serde_json::json!(true)
        );
        assert!(text_document["publishDiagnostics"].is_object());

        // Declaring these invites requests we could only answer with null.
        // Their absence is the decision, so it is what the test pins.
        assert!(
            json["workspace"].get("configuration").is_none(),
            "workspace.configuration must stay undeclared"
        );
        assert!(
            json["window"].get("workDoneProgress").is_none(),
            "window.workDoneProgress must stay undeclared"
        );
    }
}
