//! What a server can send us, and which of the three it is.
//!
//! The classification hinges on one pair of fields. A message carrying a
//! `method` **and** an `id` is a request the server is making of us, and it
//! blocks until we answer; a `method` without an `id` is a notification we
//! may ignore. Treating the first as the second is a hang with no error
//! message, which is why this is its own module with its own tests.

use serde_json::Value;

use crate::LspError;

/// The protocol permits string ids as well as numbers, and a server echoes
/// back whatever it received. Keeping the raw value compares correctly for
/// both without teaching the rest of the crate about either.
pub type RequestId = Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug)]
pub enum Incoming {
    Response {
        id: RequestId,
        result: Result<Value, ResponseError>,
    },
    ServerRequest {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

impl Incoming {
    pub fn parse(payload: &[u8]) -> Result<Self, LspError> {
        let value: Value = serde_json::from_slice(payload)
            .map_err(|error| LspError::Transport(format!("message is not JSON: {error}")))?;

        let method = value.get("method").and_then(Value::as_str).map(str::to_owned);
        let id = value.get("id").cloned();
        let params = value.get("params").cloned().unwrap_or(Value::Null);

        match (method, id) {
            (Some(method), Some(id)) => Ok(Self::ServerRequest { id, method, params }),
            (Some(method), None) => Ok(Self::Notification { method, params }),
            (None, Some(id)) => {
                let result = match value.get("error") {
                    Some(error) => Err(ResponseError {
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("(no message)")
                            .to_owned(),
                    }),
                    None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                };
                Ok(Self::Response { id, result })
            }
            (None, None) => Err(LspError::Transport(
                "message carries neither a method nor an id".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_with_an_id_is_a_response() {
        let incoming = Incoming::parse(br#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).unwrap();
        match incoming {
            Incoming::Response { id, result } => {
                assert_eq!(id, serde_json::json!(1));
                assert_eq!(result.unwrap(), serde_json::json!({"ok": true}));
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn an_error_with_an_id_is_a_response_carrying_the_error() {
        let incoming =
            Incoming::parse(br#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"nope"}}"#)
                .unwrap();
        match incoming {
            Incoming::Response { result, .. } => {
                let error = result.unwrap_err();
                assert_eq!(error.code, -32601);
                assert_eq!(error.message, "nope");
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn a_method_without_an_id_is_a_notification() {
        let incoming =
            Incoming::parse(br#"{"jsonrpc":"2.0","method":"$/progress","params":{"n":1}}"#).unwrap();
        assert!(matches!(incoming, Incoming::Notification { ref method, .. } if method == "$/progress"));
    }

    #[test]
    fn a_method_with_an_id_is_a_server_request_not_a_notification() {
        // The whole reason this module exists. rust-analyzer sends this
        // during initialization and blocks until it is answered.
        let incoming = Incoming::parse(
            br#"{"jsonrpc":"2.0","id":7,"method":"workspace/configuration","params":{}}"#,
        )
        .unwrap();
        match incoming {
            Incoming::ServerRequest { id, method, .. } => {
                assert_eq!(id, serde_json::json!(7));
                assert_eq!(method, "workspace/configuration");
            }
            other => panic!("a method carrying an id must be a server request, got {other:?}"),
        }
    }

    #[test]
    fn a_string_id_is_preserved_verbatim() {
        let incoming = Incoming::parse(br#"{"jsonrpc":"2.0","id":"abc","result":null}"#).unwrap();
        assert!(matches!(incoming, Incoming::Response { id, .. } if id == serde_json::json!("abc")));
    }

    #[test]
    fn a_message_with_neither_method_nor_id_is_a_transport_error() {
        assert!(matches!(
            Incoming::parse(br#"{"jsonrpc":"2.0"}"#).unwrap_err(),
            LspError::Transport(_)
        ));
    }

    #[test]
    fn a_payload_that_is_not_json_is_a_transport_error() {
        assert!(matches!(
            Incoming::parse(b"not json at all").unwrap_err(),
            LspError::Transport(_)
        ));
    }
}
