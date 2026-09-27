//! Spawning a real language server and wiring its stdio to a [`Client`].
//!
//! Failing to launch names the command, because the command came from the
//! user's `languages.toml` and naming it is the difference between a fixable
//! problem and a mystery. This follows `sirio_apply`, which refuses an
//! install it cannot locate rather than guessing a path.

use std::path::Path;
use std::process::Stdio;

use async_process::Command;
use futures::future::{self, Either};
use futures::io::BufReader;

use crate::connection::{Client, Connection};
use crate::lifecycle::{self, Capabilities};
use crate::message::Incoming;
use crate::LspError;

pub struct Server {
    client: Client,
    incoming: async_channel::Receiver<Incoming>,
    capabilities: Capabilities,
    child: async_process::Child,
}

impl Server {
    /// Spawns the server with `root` as its working directory, wires the
    /// connection, and completes the handshake. The read loop is returned
    /// alongside so the caller can place it on gpui's executor.
    ///
    /// The `use<>` is load-bearing: without it the returned future
    /// captures the `args` and `root` borrows, and the caller could never
    /// move it anywhere (a thread, an executor) that outlives them — even
    /// though the loop itself owns everything it touches.
    pub async fn launch(
        command: &str,
        args: &[String],
        root: &Path,
    ) -> Result<(Self, impl std::future::Future<Output = ()> + use<>), LspError> {
        let mut child = Command::new(command)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                // "not on PATH" is the answer for every language the user
                // does not work in, so it gets its own variant and the
                // caller gets to stay quiet about it.
                if error.kind() == std::io::ErrorKind::NotFound {
                    LspError::NotInstalled {
                        command: command.to_owned(),
                    }
                } else {
                    LspError::Launch(format!("could not launch `{command}`: {error}"))
                }
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Launch(format!("`{command}` has no stdin")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Launch(format!("`{command}` has no stdout")))?;

        let (client, incoming, read_loop) = Connection::new(BufReader::new(stdout), stdin);
        // The handshake is a request like any other: it only completes
        // while the read loop is being polled, and the loop is not the
        // caller's until `launch` returns. Drive both concurrently and
        // hand the loop back afterwards — polling a future again after it
        // was polled here is how it picks up where the handshake left off.
        let mut read_loop: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(read_loop);
        let mut handshake = Box::pin(lifecycle::initialize(&client, root));
        let capabilities = {
            // Scoped so the handshake's borrows of `client` and `root`
            // end before `client` moves into the returned `Server`.
            let outcome = future::select(&mut read_loop, &mut handshake).await;
            match outcome {
                Either::Left(((), _)) => {
                    return Err(LspError::Transport(
                        "the server closed its stream during initialize".into(),
                    ));
                }
                Either::Right((outcome, _)) => outcome?,
            }
        };
        drop(handshake);

        Ok((
            Self { client, incoming, capabilities, child },
            async move { read_loop.await },
        ))
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn incoming(&self) -> &async_channel::Receiver<Incoming> {
        &self.incoming
    }

    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}

use std::time::Duration;

/// How long a server gets to honour `exit` before it is killed. Short on
/// purpose: this runs inside `cx.on_app_quit`, and a quit that hangs is a
/// worse bug than a server that misses its last chance to flush.
const EXIT_GRACE: Duration = Duration::from_millis(500);

impl Server {
    /// `shutdown` → `exit` → wait → kill. The kill is not a fallback for
    /// misbehaviour, it is the contract: without it a server that ignores
    /// `exit` keeps indexing after Sirio is gone.
    pub async fn stop(mut self) -> Result<(), LspError> {
        // A shutdown that fails is not a reason to skip the kill — a server
        // too broken to answer is exactly the one that needs killing.
        let outcome = lifecycle::shutdown(&self.client).await;
        kill_after_grace(&mut self.child, EXIT_GRACE).await?;
        outcome
    }
}

async fn kill_after_grace(
    child: &mut async_process::Child,
    grace: Duration,
) -> Result<(), LspError> {
    let deadline = std::time::Instant::now() + grace;
    while std::time::Instant::now() < deadline {
        match child.try_status() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => futures_timer::Delay::new(Duration::from_millis(10)).await,
            Err(error) => {
                return Err(LspError::Transport(format!("waiting for exit: {error}")));
            }
        }
    }
    child
        .kill()
        .map_err(|error| LspError::Transport(format!("killing the server: {error}")))?;
    let _ = child.status().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn stopping_a_server_that_ignores_exit_still_terminates_it() {
        // `sleep` never speaks the protocol and never exits on `exit`. The
        // kill is what makes `stop` a guarantee rather than a request, and
        // without it a server outlives the app that started it.
        futures::executor::block_on(async {
            let mut child = async_process::Command::new("/bin/sh")
                .args(["-c", "sleep 60"])
                .spawn()
                .unwrap();
            let id = child.id();
            assert!(kill_after_grace(&mut child, std::time::Duration::from_millis(50)).await.is_ok());
            assert!(child.try_status().unwrap().is_some(), "process {id} must be reaped");
        });
    }
}
