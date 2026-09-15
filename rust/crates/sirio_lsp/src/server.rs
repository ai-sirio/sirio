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
                LspError::Launch(format!("could not launch `{command}`: {error}"))
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

    /// A language server in nine lines of shell: read the Content-Length
    /// header, skip the blank line, read exactly that many bytes, answer.
    /// The same trick `sirio_acp` uses for fake agents, with LSP's framing.
    ///
    /// `dd` is what makes it exact — `read` would stop at a newline inside
    /// the payload, which is the very thing the header exists to prevent.
    ///
    /// The carriage return is stripped after the read rather than matched in
    /// the `case` pattern. An earlier version matched `'' | $'\r'`, which is
    /// **bash's ANSI-C quoting and not POSIX**: on dash — Ubuntu's `/bin/sh`,
    /// and therefore the CI runner's — that pattern is the literal text `$\r`,
    /// the blank-line arm never fires, the body is never read, and the
    /// handshake dies on its 30-second timeout. It passed on every developer
    /// machine whose `/bin/sh` is bash, which is the worst way for a test to
    /// be wrong.
    #[cfg(unix)]
    fn fixture_server() -> (&'static str, Vec<String>) {
        (
            "/bin/sh",
            vec![
                "-c".to_string(),
                r#"
while IFS= read -r line; do
  header=$(printf '%s' "$line" | tr -d '\r')
  case "$header" in
    Content-Length:*) len=$(printf '%s' "$header" | tr -dc '0-9') ;;
    '')
      body=$(dd bs=1 count="$len" 2>/dev/null)
      id=$(printf '%s' "$body" | sed -E 's/.*"id":([0-9]+).*/\1/')
      payload='{"jsonrpc":"2.0","id":'"$id"',"result":{"capabilities":{"hoverProvider":true}}}'
      printf 'Content-Length: %s\r\n\r\n%s' "${#payload}" "$payload"
      ;;
  esac
done
"#
                .to_string(),
            ],
        )
    }

    #[cfg(unix)]
    #[test]
    fn launching_a_server_completes_the_handshake() {
        futures::executor::block_on(async {
            let (command, args) = fixture_server();
            // `launch` drives its read loop internally while the
            // handshake runs, then hands the loop back for the caller to
            // place — on a thread here, on gpui's executor in the app.
            let (server, read_loop) = Server::launch(command, &args, std::path::Path::new("/tmp"))
                .await
                .expect("the fixture server must complete initialize");
            std::thread::spawn(move || futures::executor::block_on(read_loop));
            assert!(server.capabilities().hover);
        });
    }

    #[test]
    fn a_missing_binary_is_a_launch_error_naming_the_command() {
        futures::executor::block_on(async {
            // `.err()` rather than `.unwrap_err()`: the `Ok` half holds
            // the `Server` and its read-loop future, neither of which is
            // `Debug`, and `unwrap_err` demands it of both.
            let error = Server::launch(
                "sirio-no-such-language-server",
                &[],
                std::path::Path::new("/tmp"),
            )
            .await
            .err()
            .expect("launching a missing binary must fail");
            match error {
                LspError::Launch(detail) => assert!(
                    detail.contains("sirio-no-such-language-server"),
                    "the error must name the command so the user can fix their languages.toml: {detail}"
                ),
                other => panic!("expected a launch error, got {other}"),
            }
        });
    }

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
