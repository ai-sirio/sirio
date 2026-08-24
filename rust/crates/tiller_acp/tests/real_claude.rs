use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};
use futures::future::Either;
use tiller_acp::{AcpClient, AcpEvent, AgentCommand, EventStream};

fn next_event(events: &EventStream, timeout: Duration) -> Result<AcpEvent> {
    let receive = events.recv();
    let timer = async_io::Timer::after(timeout);
    futures::executor::block_on(async move {
        futures::pin_mut!(receive, timer);
        match futures::future::select(receive, timer).await {
            Either::Left((event, _)) => event.map_err(|error| anyhow!(error.to_string())),
            Either::Right((_, _)) => bail!("real Claude ACP event timed out after {timeout:?}"),
        }
    })
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after Unix epoch")
        .as_nanos()
        .to_string()
}

#[test]
#[ignore = "requires the installed Claude credentials and an ACP adapter download"]
fn real_claude_streams_tool_permission_and_writes_nonce() -> Result<()> {
    let nonce = format!("tiller-acp-{}", unique_suffix());
    let cwd = std::env::temp_dir().join(format!("{nonce}-cwd"));
    fs::create_dir_all(&cwd)?;
    let nonce_file = cwd.join(format!("{nonce}.txt"));

    // This ignored drive needs a real agent; name it explicitly instead
    // of defaulting to a network fetch.
    let program = std::env::var_os("TILLER_ACP_PROGRAM")
        .expect("set TILLER_ACP_PROGRAM to run this gated test against a real agent");
    let command = AgentCommand::new(program);
    let launch = AcpClient::launch(command, &cwd);
    let (mut client, events) = match launch {
        Ok(connection) => connection,
        Err(error) => {
            let _ = fs::remove_dir_all(&cwd);
            return Err(error);
        }
    };

    let prompt = format!(
        "Use your file-writing tool to create an empty file named {nonce}.txt in the current working directory. Do not just describe it; perform the write. After the tool completes, reply with exactly DONE."
    );
    client.prompt(prompt)?;

    let mut assistant_chunks = Vec::new();
    let mut tool_calls = 0;
    let mut permission_requests = 0;
    let mut tool_completions = 0;
    let mut permission_response = None;

    loop {
        match next_event(&events, Duration::from_secs(180))? {
            AcpEvent::AgentMessageChunk(text) => assistant_chunks.push(text),
            AcpEvent::ToolCallStarted { .. } => tool_calls += 1,
            AcpEvent::ToolCallCompleted { .. } => tool_completions += 1,
            AcpEvent::PermissionRequest {
                request_id,
                options,
                ..
            } => {
                permission_requests += 1;
                let option = options
                    .iter()
                    .find(|option| option.kind.contains("Allow"))
                    .or_else(|| options.first())
                    .ok_or_else(|| anyhow!("Claude permission request had no options"))?;
                client.respond_permission(request_id, option.id.clone())?;
                permission_response = Some(option.id.clone());
            }
            AcpEvent::TurnEnded { stop_reason } => {
                if stop_reason != "EndTurn" {
                    bail!("Claude turn ended with {stop_reason}");
                }
                break;
            }
            AcpEvent::TransportError(error) => bail!("Claude ACP transport failed: {error}"),
            AcpEvent::Timeout {
                operation,
                duration,
            } => {
                bail!("Claude ACP timed out during {operation:?} after {duration:?}")
            }
            _ => {}
        }
    }

    assert!(
        assistant_chunks.len() >= 2,
        "expected streamed chunks: {assistant_chunks:?}"
    );
    assert!(tool_calls >= 1, "expected a tool call");
    assert!(tool_completions >= 1, "expected a completed tool call");
    assert!(permission_requests >= 1, "expected a permission request");
    assert!(
        permission_response.is_some(),
        "expected a permission response"
    );
    assert!(
        nonce_file.is_file(),
        "Claude did not write nonce file {:?}",
        nonce_file
    );

    client.shutdown()?;
    fs::remove_dir_all(&cwd)?;
    println!("real Claude ACP nonce: {nonce}");
    Ok(())
}
