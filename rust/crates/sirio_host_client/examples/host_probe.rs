//! Drives a real sirio-host for Scripts/Tests/test-host-e2e.sh. The data
//! root comes from SIRIO_HOST_HOME; the packaged binary from SIRIO_HOST_BIN.
//!
//!   host_probe ensure [--hold]           ensure_host; print pid, major, method; --hold keeps the connection until killed
//!   host_probe observe <major>           print verdict=Live|Unverifiable|Absent
//!   host_probe info                      print the host.info fields
//!   host_probe hold <on|off>             host.debug.hold_session
//!   host_probe shutdown [--force]        host.shutdown; print ok or error=<code>
//!   host_probe subscribe <n>             print n host.state events
//!   host_probe conformance <dir>         run every case in <dir>; print `case=<name> PASS|FAIL <why>`
//!   host_probe raw-ndjson                write one NDJSON line to the endpoint; print what came back
//!   host_probe alive <pid>               exit 0 while the process runs, 1 otherwise (the check the
//!                                        client's verdict uses; needs no data root)
//!
//! `ensure` failing prints `error=<Debug of EnsureError>` (the variant name
//! first) and exits 1.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use sirio_host_client::connection::{CallError, Connection};
use sirio_host_client::{EnsureOptions, ensure_host};
use sirio_host_protocol::frame::{FrameDecoder, FrameKind, encode};
use sirio_host_protocol::liveness::Verdict;
use sirio_host_protocol::messages::{HelloReply, method};
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::version::{effective_major, majors_spoken};
use sirio_ipc::LocalStreamExt;

fn main() {
    let mut argv = std::env::args().skip(1);
    if argv.next().as_deref() == Some("alive") {
        let pid: u32 = argv.next().and_then(|p| p.parse().ok()).expect("alive <pid>");
        std::process::exit(if sirio_ipc::process::exists(pid) { 0 } else { 1 });
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let paths = HostPaths::from_environment(&env).expect("SIRIO_HOST_HOME");
    let major = effective_major(env.get("SIRIO_HOST_PROTOCOL_MAJOR").map(String::as_str));
    let majors = majors_spoken(major);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = EnsureOptions {
        paths: paths.clone(),
        client_version: env!("CARGO_PKG_VERSION").into(),
        current_exe: std::env::current_exe().unwrap(),
        environment: env.clone(),
        major,
    };
    match args.first().map(String::as_str) {
        Some("ensure") => match ensure_host(&options) {
            Ok(mut handle) => {
                let info = handle
                    .primary
                    .connection
                    .call(method::INFO, serde_json::json!({}), None)
                    .unwrap();
                println!("pid={}", info["pid"]);
                println!("major={}", handle.primary.major);
                println!("method={:?}", handle.method);
                println!(
                    "previous={}",
                    handle
                        .previous
                        .as_ref()
                        .map(|p| p.major.to_string())
                        .unwrap_or("none".into())
                );
                if args.get(1).map(String::as_str) == Some("--hold") {
                    loop {
                        std::thread::sleep(Duration::from_secs(3600));
                    }
                }
            }
            Err(e) => {
                println!("error={e:?}");
                std::process::exit(1);
            }
        },
        Some("observe") => {
            let m: u32 = args[1].parse().unwrap();
            let v = sirio_host_client::observe::observe(&paths, m, &majors).verdict;
            println!(
                "verdict={}",
                match v {
                    Verdict::Live => "Live",
                    Verdict::Unverifiable => "Unverifiable",
                    Verdict::Absent => "Absent",
                }
            );
        }
        Some("info") => {
            let mut c = welcomed(&paths, major, &majors);
            for (k, v) in
                c.0.call(method::INFO, serde_json::json!({}), None)
                    .unwrap()
                    .as_object()
                    .unwrap()
            {
                println!("{k}={v}");
            }
        }
        Some("hold") => {
            let mut c = welcomed(&paths, major, &majors);
            let held = args[1] == "on";
            c.0.call(
                method::DEBUG_HOLD_SESSION,
                serde_json::json!({ "held": held }),
                Some(&c.1),
            )
            .unwrap();
            println!("ok");
        }
        Some("shutdown") => {
            let mut c = welcomed(&paths, major, &majors);
            let force = args.get(1).map(String::as_str) == Some("--force");
            match c.0.call(
                method::SHUTDOWN,
                serde_json::json!({ "force": force }),
                Some(&c.1),
            ) {
                Ok(_) => println!("ok"),
                Err(CallError::Remote(b)) => println!(
                    "error={}",
                    serde_json::to_value(b.code).unwrap().as_str().unwrap()
                ),
                Err(e) => println!("error={e}"),
            }
        }
        Some("subscribe") => {
            let n: usize = args[1].parse().unwrap();
            let mut c = welcomed(&paths, major, &majors);
            c.0.call(
                method::SUBSCRIBE,
                serde_json::json!({ "topic": "host.state" }),
                None,
            )
            .unwrap();
            for _ in 0..n {
                match c.0.next_event(Duration::from_secs(5)).unwrap() {
                    Some(e) => println!("event seq={} payload={}", e.seq, e.payload),
                    None => {
                        println!("event timeout");
                        break;
                    }
                }
            }
        }
        Some("conformance") => std::process::exit(conformance(&paths, major, Path::new(&args[1]))),
        Some("raw-ndjson") => raw_ndjson(&paths, major),
        _ => {
            eprintln!("see the header of examples/host_probe.rs");
            std::process::exit(2);
        }
    }
}

fn welcomed(paths: &HostPaths, major: u32, majors: &[u32]) -> (Connection, String) {
    let mut c = Connection::open(&paths.endpoint(major), Duration::from_secs(5)).expect("connect");
    match c.hello("host_probe", majors).expect("hello") {
        HelloReply::Welcome(w) => (c, w.generation),
        other => panic!("not welcomed: {other:?}"),
    }
}

/// Runs every `*.json` case in `dir`, in name order, each on its own new
/// connection to the host already running. Returns the process exit code.
fn conformance(paths: &HostPaths, major: u32, dir: &Path) -> i32 {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
        Err(e) => {
            println!("case=none FAIL cannot read {}: {e}", dir.display());
            return 1;
        }
    };
    files.sort();
    if files.is_empty() {
        println!("case=none FAIL no cases in {}", dir.display());
        return 1;
    }
    let mut failed = false;
    for file in files {
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let case: Value = match std::fs::read(&file)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
        {
            Ok(case) => case,
            Err(e) => {
                println!("case={stem} FAIL cannot read the case: {e}");
                failed = true;
                continue;
            }
        };
        let name = case["name"].as_str().unwrap_or(&stem).to_string();
        match run_case(&paths.endpoint(major), &case) {
            Ok(()) => println!("case={name} PASS"),
            Err(why) => {
                println!("case={name} FAIL {why}");
                failed = true;
            }
        }
    }
    i32::from(failed)
}

/// One case on one raw connection (not `Connection`: case 08 must send a
/// request before any hello). A host that closes the connection after its
/// last answer — cases 02 and 08 — is a pass: only an answer is read.
fn run_case(endpoint: &Path, case: &Value) -> Result<(), String> {
    let mut stream =
        sirio_ipc::connect(endpoint).map_err(|e| format!("step=0 got=connect: {e}"))?;
    LocalStreamExt::set_read_timeout(&stream, Some(Duration::from_millis(50)))
        .map_err(|e| format!("step=0 got=set_read_timeout: {e}"))?;
    let mut decoder = FrameDecoder::new();
    let mut generation: Option<String> = None;
    let steps = case["steps"].as_array().ok_or("the case has no steps")?;
    for (i, step) in steps.iter().enumerate() {
        let mut send = step["send"].clone();
        if let Some(generation) = &generation {
            substitute_generation(&mut send, generation);
        }
        let payload = serde_json::to_vec(&send).map_err(|e| e.to_string())?;
        let frame = encode(FrameKind::Request, &payload).map_err(|e| e.to_string())?;
        stream
            .write_all(&frame)
            .and_then(|_| stream.flush())
            .map_err(|e| format!("step={i} got=write: {e}"))?;
        let response = read_response(&mut stream, &mut decoder, Duration::from_secs(5))
            .map_err(|why| format!("step={i} got={why}"))?;
        if response["result"]["type"] == "welcome"
            && let Some(g) = response["result"]["generation"].as_str()
        {
            generation = Some(g.to_string());
        }
        if !subset(&step["expect"], &response) {
            return Err(format!(
                "step={i} got={response} expected={}",
                step["expect"]
            ));
        }
    }
    Ok(())
}

fn substitute_generation(value: &mut Value, generation: &str) {
    match value {
        Value::String(s) if s == "$generation" => *s = generation.to_string(),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|v| substitute_generation(v, generation)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|v| substitute_generation(v, generation)),
        _ => {}
    }
}

/// The next Response frame as raw JSON; events on the way are skipped.
fn read_response(
    stream: &mut sirio_ipc::LocalStream,
    decoder: &mut FrameDecoder,
    timeout: Duration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 16 * 1024];
    loop {
        match decoder.next_frame() {
            Ok(Some(frame)) if frame.kind == FrameKind::Response => {
                return serde_json::from_slice(&frame.payload)
                    .map_err(|e| format!("undecodable response: {e}"));
            }
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => return Err(format!("frame error: {e}")),
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for a response".into());
        }
        match stream.read(&mut buf) {
            Ok(0) => return Err("the host closed the connection before answering".into()),
            Ok(n) => decoder.push(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(format!("read: {e}")),
        }
    }
}

/// Every key of `expected` is in `actual` with a value that is itself a
/// subset (objects, recursively) or equal (everything else), so `{}` matches
/// any object and `{"major":1}` matches `{"major":1,"minor":0}`.
fn subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| subset(v, av))),
        _ => expected == actual,
    }
}

/// Writes a line of the control socket's old protocol to the host's endpoint:
/// the host must refuse a foreign protocol by closing, not by guessing.
fn raw_ndjson(paths: &HostPaths, major: u32) {
    let mut stream = sirio_ipc::connect(&paths.endpoint(major)).expect("connect");
    LocalStreamExt::set_read_timeout(&stream, Some(Duration::from_millis(50))).unwrap();
    stream
        .write_all(b"{\"id\":1,\"method\":\"ping\"}\n")
        .unwrap();
    stream.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let (mut bytes, mut closed) = (0usize, false);
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => {
                closed = true;
                break;
            }
            Ok(n) => bytes += n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            // A reset is the host closing hard.
            Err(_) => {
                closed = true;
                break;
            }
        }
    }
    if bytes > 0 {
        println!("bytes={bytes}");
    }
    println!("closed={closed}");
}
