//! PROBE — throwaway. Sends one arbitrary `browser.*` control request and
//! prints the response with the wall-clock time it took.
//!
//! `tillerctl` exposes no `browser` command, so #142 cannot be exercised from
//! it. This uses the same `client::round_trip` transport `tillerctl` does,
//! which matters: the Windows named pipe is not readable by a plain file open.
//!
//! Usage:
//!   browser_probe <pipe-or-socket-path> <method> [key=value ...]

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use tiller_control::client;
use tiller_control::protocol::ControlRequest;

fn main() {
    let mut args = std::env::args().skip(1);
    let socket = args.next().expect("socket path");
    let method = args.next().expect("method");
    let mut params = BTreeMap::new();
    for pair in args {
        if let Some((key, value)) = pair.split_once('=') {
            params.insert(key.to_string(), value.to_string());
        }
    }

    let request = ControlRequest {
        id: "probe-1".to_string(),
        method: method.clone(),
        params,
    };

    let started = Instant::now();
    let response = client::round_trip(Path::new(&socket), &request, Duration::from_secs(30));
    let elapsed = started.elapsed();

    println!("elapsed_ms={}", elapsed.as_millis());
    match response {
        Ok(response) => {
            println!("ok={}", response.ok);
            if let Some(error) = &response.error {
                println!("error={error}");
            }
            for (key, value) in response.result.iter().flatten() {
                let value = if value.len() > 400 {
                    format!("{}…", &value[..400])
                } else {
                    value.clone()
                };
                println!("{key}={value}");
            }
        }
        Err(error) => println!("client error: {error}"),
    }
}
