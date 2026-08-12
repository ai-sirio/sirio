use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tiller_acp::{AcpClient, AcpEvent, AgentCommand};

fn main() {
    if let Err(error) = run() {
        eprintln!("acp_smoke failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_nanos();
    let cwd = std::env::temp_dir().join(format!("tiller-acp-smoke-{suffix}"));
    fs::create_dir_all(&cwd)?;

    let command = std::env::var_os("TILLER_ACP_PROGRAM")
        .map(PathBuf::from)
        .map(AgentCommand::new)
        .unwrap_or_else(|| {
            AgentCommand::new("npx").args(["-y", "@agentclientprotocol/claude-agent-acp@latest"])
        });
    println!("launching ACP agent: {command:?}");

    let (mut client, events) = AcpClient::launch(command, &cwd)?;
    println!("initialized: {:?}", client.initialize_info());
    println!("session: {}", client.session_id());
    client.prompt("Say exactly: ciao")?;

    let mut assistant_text = String::new();
    loop {
        let event = events
            .recv_blocking()
            .map_err(|error| anyhow::anyhow!("event stream closed: {error}"))?;
        println!("event: {event:?}");
        match event {
            AcpEvent::AgentMessageChunk(text) => assistant_text.push_str(&text),
            AcpEvent::PermissionRequest {
                request_id,
                options,
                ..
            } => {
                let option = options
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("permission request had no options"))?;
                client.respond_permission(request_id, &option.id)?;
            }
            AcpEvent::TurnEnded { .. } => break,
            AcpEvent::TransportError(error) => return Err(anyhow::anyhow!(error)),
            _ => {}
        }
    }

    let contains_ciao = assistant_text.to_lowercase().contains("ciao");
    client.shutdown()?;
    let _ = fs::remove_dir_all(&cwd);
    if !contains_ciao {
        anyhow::bail!("assistant text did not contain ciao: {assistant_text:?}");
    }
    println!("assistant text contained ciao");
    Ok(())
}
