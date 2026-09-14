//! Request correlation and the loop that feeds it.
//!
//! The loop is returned, never spawned. In the app it goes on gpui's
//! background executor; in tests it goes into `block_on`. That is the whole
//! reason this crate needs no gpui dependency, and it is what keeps the
//! protocol suite out of reach of the deterministic test scheduler that
//! panics on cross-thread activity.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::io::{AsyncBufRead, AsyncRead, AsyncWrite};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::framing::{read_message, write_message};
use crate::message::{Incoming, RequestId, ResponseError};
use crate::LspError;

type Pending = Arc<Mutex<HashMap<String, async_channel::Sender<Result<Value, ResponseError>>>>>;

/// How long a single request may go unanswered. Generous, because
/// rust-analyzer answers nothing useful until it has finished indexing —
/// but finite, because a request that never resolves is a spinner that
/// never stops.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct Connection;

impl Connection {
    /// Wires a reader and a writer into a client, a stream of everything the
    /// server says unprompted, and the loop that drives both. The caller
    /// spawns the loop.
    pub fn new<R, W>(
        reader: R,
        writer: W,
    ) -> (
        Client,
        async_channel::Receiver<Incoming>,
        impl std::future::Future<Output = ()>,
    )
    where
        R: AsyncBufRead + AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (incoming_tx, incoming_rx) = async_channel::unbounded();
        let client = Client {
            writer: Arc::new(futures::lock::Mutex::new(Box::new(writer))),
            pending: Arc::clone(&pending),
            next_id: Arc::new(AtomicI64::new(1)),
        };

        let loop_pending = Arc::clone(&pending);
        let read_loop = async move {
            let mut reader = reader;
            loop {
                let payload = match read_message(&mut reader).await {
                    Ok(payload) => payload,
                    Err(error) => {
                        fail_all_pending(&loop_pending, &error);
                        return;
                    }
                };
                match Incoming::parse(&payload) {
                    Ok(Incoming::Response { id, result }) => {
                        let waiter = loop_pending.lock().unwrap().remove(&id.to_string());
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(result).await;
                        }
                    }
                    Ok(other) => {
                        if incoming_tx.send(other).await.is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        // A malformed frame means the stream is
                        // desynchronised: the next byte is not a header and
                        // is indistinguishable from data. Recovering would
                        // mean trusting a stream that has already lied.
                        fail_all_pending(&loop_pending, &error);
                        return;
                    }
                }
            }
        };

        (client, incoming_rx, read_loop)
    }
}

fn fail_all_pending(pending: &Pending, error: &LspError) {
    let waiters: Vec<_> = pending.lock().unwrap().drain().map(|(_, tx)| tx).collect();
    for waiter in waiters {
        let _ = waiter.try_send(Err(ResponseError {
            code: 0,
            message: error.to_string(),
        }));
    }
}

#[derive(Clone)]
pub struct Client {
    writer: Arc<futures::lock::Mutex<Box<dyn AsyncWrite + Unpin + Send>>>,
    pending: Pending,
    next_id: Arc<AtomicI64>,
}

impl Client {
    pub async fn request<P: Serialize, T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: P,
    ) -> Result<T, LspError> {
        self.request_with_timeout(method, params, REQUEST_TIMEOUT).await
    }

    /// The timeout is a parameter so tests can use milliseconds where the
    /// app uses [`REQUEST_TIMEOUT`] — the same seam `sirio_acp` opened with
    /// `launch_with_timeout`.
    pub async fn request_with_timeout<P: Serialize, T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: P,
        timeout: Duration,
    ) -> Result<T, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let key = Value::from(id).to_string();
        let (tx, rx) = async_channel::bounded(1);
        self.pending.lock().unwrap().insert(key.clone(), tx);

        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.send(&envelope).await?;

        let answered = futures::future::select(
            Box::pin(rx.recv()),
            Box::pin(futures_timer::Delay::new(timeout)),
        )
        .await;
        let received = match answered {
            futures::future::Either::Left((received, _)) => received,
            futures::future::Either::Right(((), _)) => {
                // Unregister before giving up, or the map grows by one entry
                // for every request a slow server never answers.
                self.pending.lock().unwrap().remove(&key);
                return Err(LspError::Timeout { method });
            }
        };

        match received {
            Ok(Ok(value)) => serde_json::from_value(value).map_err(|error| {
                LspError::Transport(format!("`{method}` answered with unexpected shape: {error}"))
            }),
            // A zero code is this crate's own marker for "the connection
            // died underneath you", set by `fail_all_pending`.
            Ok(Err(ResponseError { code: 0, message })) => Err(LspError::Transport(message)),
            Ok(Err(ResponseError { code, message })) => Err(LspError::Server { code, message }),
            Err(_) => Err(LspError::Transport(format!(
                "`{method}` was abandoned: the connection closed"
            ))),
        }
    }

    pub async fn notify<P: Serialize>(&self, method: &str, params: P) -> Result<(), LspError> {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    /// Answers a request the **server** made of us. Not optional: a server
    /// request left unanswered stalls the server silently.
    pub async fn respond(&self, id: RequestId, result: Value) -> Result<(), LspError> {
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }))
        .await
    }

    /// Test seam: proves a giving-up request unregistered itself.
    #[cfg(test)]
    pub(crate) fn pending_is_empty(&self) -> bool {
        self.pending.lock().unwrap().is_empty()
    }

    async fn send(&self, envelope: &Value) -> Result<(), LspError> {
        let bytes = serde_json::to_vec(envelope)
            .map_err(|error| LspError::Transport(format!("serialising: {error}")))?;
        let mut writer = self.writer.lock().await;
        write_message(&mut *writer, &bytes).await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use futures::io::{AsyncWriteExt, BufReader};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::stream::StreamExt;
    use futures::TryStreamExt;

    /// One direction of an in-memory pipe: bytes written to the returned
    /// writer come out of the returned reader. This is what keeps the whole
    /// protocol suite process-free — and therefore fast and deterministic.
    fn async_pipe() -> (PipeWriter, impl futures::io::AsyncRead + Unpin + Send + 'static) {
        let (sender, receiver) = futures::channel::mpsc::unbounded::<Vec<u8>>();
        let reader = receiver.map(Ok::<Vec<u8>, std::io::Error>).into_async_read();
        (PipeWriter(sender), reader)
    }

    struct PipeWriter(futures::channel::mpsc::UnboundedSender<Vec<u8>>);

    impl futures::io::AsyncWrite for PipeWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            // Unbounded, so this never returns Pending — which is exactly
            // why no waker bookkeeping is needed here.
            self.0.unbounded_send(buf.to_vec()).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe closed")
            })?;
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            // Closing the channel is what ends the peer's read loop, which
            // is what `a_closed_stream_fails_every_request_still_waiting`
            // relies on to observe the failure path.
            self.0.close_channel();
            Poll::Ready(Ok(()))
        }
    }

    /// Drives a `Connection` against a scripted peer. `script` receives each
    /// payload the client writes and returns the payloads to feed back, so a
    /// test reads like a transcript instead of a pile of channel plumbing.
    ///
    /// A script returning **no** replies closes the connection: that is how
    /// a test asks for the "the server died" path. Without this, a silent
    /// peer would leave the client parked forever and the test would hang
    /// rather than fail.
    pub(crate) async fn with_scripted_server<F>(
        script: F,
    ) -> (Client, async_channel::Receiver<Incoming>)
    where
        F: Fn(&serde_json::Value) -> Vec<serde_json::Value> + Send + 'static,
    {
        let (to_client, from_server) = async_pipe();
        let (to_server, from_client) = async_pipe();
        let (client, incoming, read_loop) = Connection::new(BufReader::new(from_server), to_server);

        // The peer: read what the client sends, answer per the script.
        let peer = async move {
            let mut reader = BufReader::new(from_client);
            let mut writer = to_client;
            while let Ok(payload) = crate::framing::read_message(&mut reader).await {
                let request: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                let replies = script(&request);
                if replies.is_empty() {
                    break;
                }
                for reply in replies {
                    let bytes = serde_json::to_vec(&reply).unwrap();
                    if crate::framing::write_message(&mut writer, &bytes).await.is_err() {
                        return;
                    }
                }
            }
            let _ = writer.close().await;
        };

        std::thread::spawn(move || {
            futures::executor::block_on(futures::future::join(read_loop, peer))
        });
        (client, incoming)
    }

    #[test]
    fn a_request_resolves_with_the_matching_response() {
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|request| {
                vec![serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {"echo": request["method"]}
                })]
            })
            .await;

            let value: serde_json::Value =
                client.request("textDocument/hover", serde_json::json!({})).await.unwrap();
            assert_eq!(value, serde_json::json!({"echo": "textDocument/hover"}));
        });
    }

    #[test]
    fn two_requests_answered_out_of_order_each_get_their_own_response() {
        // The correlation map earns its keep here: a server is free to
        // answer the second request first, and a client that assumed FIFO
        // would hand each caller the other one's answer.
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|request| {
                let id = request["id"].clone();
                vec![serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"id": request["id"]}
                })]
            })
            .await;

            let first = client.request::<_, serde_json::Value>("a", serde_json::json!({}));
            let second = client.request::<_, serde_json::Value>("b", serde_json::json!({}));
            let (first, second) = futures::future::join(first, second).await;
            assert_eq!(first.unwrap()["id"], serde_json::json!(1));
            assert_eq!(second.unwrap()["id"], serde_json::json!(2));
        });
    }

    #[test]
    fn a_server_error_becomes_a_typed_server_error() {
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|request| {
                vec![serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "error": {"code": -32601, "message": "unknown method"}
                })]
            })
            .await;

            let error = client
                .request::<_, serde_json::Value>("nope", serde_json::json!({}))
                .await
                .unwrap_err();
            assert!(matches!(error, LspError::Server { code: -32601, .. }));
        });
    }

    #[test]
    fn a_notification_reaches_the_incoming_channel() {
        futures::executor::block_on(async {
            let (client, incoming) = with_scripted_server(|request| {
                vec![
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {"uri": "file:///x.rs", "diagnostics": []}
                    }),
                    serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": null}),
                ]
            })
            .await;

            let _: serde_json::Value =
                client.request("anything", serde_json::json!({})).await.unwrap();
            let notification = incoming.recv().await.unwrap();
            assert!(matches!(
                notification,
                Incoming::Notification { ref method, .. }
                    if method == "textDocument/publishDiagnostics"
            ));
        });
    }

    #[test]
    fn a_server_request_reaches_the_incoming_channel_so_it_can_be_answered() {
        futures::executor::block_on(async {
            let (client, incoming) = with_scripted_server(|request| {
                vec![
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": 99,
                        "method": "workspace/configuration",
                        "params": {}
                    }),
                    serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": null}),
                ]
            })
            .await;

            let _: serde_json::Value =
                client.request("anything", serde_json::json!({})).await.unwrap();
            let received = incoming.recv().await.unwrap();
            assert!(matches!(
                received,
                Incoming::ServerRequest { ref method, .. } if method == "workspace/configuration"
            ));
        });
    }

    #[test]
    fn a_request_the_server_never_answers_times_out() {
        // The peer stays alive and talkative — it just answers an id nobody
        // is waiting for. Nothing correlates, the connection never closes,
        // and without a timeout the caller would wait for the heat death of
        // the universe with the UI spinner still turning.
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|_| {
                vec![serde_json::json!({"jsonrpc": "2.0", "id": 9999, "result": null})]
            })
            .await;

            let error = client
                .request_with_timeout::<_, serde_json::Value>(
                    "textDocument/hover",
                    serde_json::json!({}),
                    std::time::Duration::from_millis(50),
                )
                .await
                .unwrap_err();
            assert!(
                matches!(error, LspError::Timeout { method: "textDocument/hover" }),
                "the timeout must name the method that went unanswered, got {error}"
            );
        });
    }

    #[test]
    fn a_timed_out_request_leaves_no_entry_behind_in_the_pending_map() {
        // A timeout that forgets to unregister leaks one entry per giving-up
        // request, and a long session of a slow server would grow the map
        // without bound.
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|_| {
                vec![serde_json::json!({"jsonrpc": "2.0", "id": 9999, "result": null})]
            })
            .await;

            let _ = client
                .request_with_timeout::<_, serde_json::Value>(
                    "textDocument/hover",
                    serde_json::json!({}),
                    std::time::Duration::from_millis(50),
                )
                .await;
            assert!(client.pending_is_empty(), "the timed-out request must be unregistered");
        });
    }

    #[test]
    fn a_closed_stream_fails_every_request_still_waiting() {
        // A server that dies mid-request must not leave the caller parked
        // forever; the read loop's exit has to drain the pending map.
        futures::executor::block_on(async {
            let (client, _incoming) = with_scripted_server(|_| vec![]).await;
            let error = client
                .request::<_, serde_json::Value>("orphan", serde_json::json!({}))
                .await
                .unwrap_err();
            assert!(matches!(error, LspError::Transport(_)));
        });
    }
}
