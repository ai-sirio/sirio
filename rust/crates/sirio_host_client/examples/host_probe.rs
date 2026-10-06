//! Drives a real sirio-host for Scripts/Tests/test-host-e2e.sh. The data
//! root comes from SIRIO_HOST_HOME; the packaged binary from SIRIO_HOST_BIN.
//!
//!   host_probe ensure [--hold]           ensure_host; print pid, major, method; --hold keeps the connection until killed
//!   host_probe observe <major>           print verdict=Live|Unverifiable|Absent
//!   host_probe info                      print the host.info fields
//!   host_probe hold <on|off>             host.debug.hold_session
//!   host_probe shutdown [--force]        host.shutdown; print ok or error=<code>
//!   host_probe subscribe <n>             print n host.state events
//!   host_probe silent <seconds>          connect without sending; print when the host closes
//!   host_probe conformance <dir>         run every case in <dir>; print `case=<name> PASS|FAIL <why>`
//!   host_probe raw-ndjson                write one NDJSON line to the endpoint; print what came back
//!   host_probe alive <pid>               exit 0 while the process runs, 1 otherwise (the check the
//!                                        client's verdict uses; needs no data root)
//!   host_probe term-create <cwd> [--id ID] [--origin app|control] [--prelude TEXT]
//!       [-- <program> [args…]]
//!   host_probe term-list
//!   host_probe term-read <id> [text|raw] [--max N]
//!   host_probe term-write <id> <text> | term-key <id> <name> | term-feed <id> <text>
//!   host_probe term-resize <id> <cols> <rows> | term-close <id> | term-wait <id> <ms>
//!   host_probe term-attach <id> [--resume OFFSET] [--for MS] [--no-read]
//!       [--replica-text FILE] [--last-offset FILE]
//!   host_probe term-echo-latency <id> <samples>
//!
//! `ensure` failing prints `error=<Debug of EnsureError>` (the variant name
//! first) and exits 1.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use sirio_host_client::connection::{CallError, Connection};
use sirio_host_client::{EnsureOptions, Mux, StreamItem, TermClient, ensure_host};
use sirio_host_protocol::frame::{FrameDecoder, FrameKind, encode};
use sirio_host_protocol::liveness::Verdict;
use sirio_host_protocol::messages::{HelloReply, method};
use sirio_host_protocol::paths::HostPaths;
use sirio_host_protocol::term::{
    AttachMode, CreateParams, ExitStatus, Origin, ReadFormat, TermCommand, TermSize, WaitResult,
    b64_decode, b64_encode,
};
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
        Some("silent") => {
            let seconds: u64 = args[1].parse().expect("silent <seconds>");
            let mut stream = sirio_ipc::connect(&paths.endpoint(major)).expect("connect");
            LocalStreamExt::set_read_timeout(&stream, Some(Duration::from_millis(100)))
                .expect("read timeout");
            let started = Instant::now();
            let deadline = started + Duration::from_secs(seconds);
            let mut buf = [0u8; 1];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => {
                        println!("closed_after_ms={}", started.elapsed().as_millis());
                        break;
                    }
                    Ok(_) => {}
                    Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => {
                        println!("closed_after_ms={}", started.elapsed().as_millis());
                        break;
                    }
                }
                if Instant::now() >= deadline {
                    println!("still_open");
                    break;
                }
            }
        }
        Some("conformance") => std::process::exit(conformance(&paths, major, Path::new(&args[1]))),
        Some("raw-ndjson") => raw_ndjson(&paths, major),
        Some("term-create") => term_create(&paths, major, &majors, &args),
        Some("term-list") => {
            let client = term_client(&paths, major, &majors);
            for session in client.list().expect("term.list").sessions {
                println!(
                    "session={} pid={} exit={} origin={} clients={} fg={}",
                    session.session_id,
                    session.pid,
                    exit_label(session.exit.as_ref()),
                    origin_label(&session.origin),
                    session.clients,
                    u8::from(session.foreground.running)
                );
            }
        }
        Some("term-read") => term_read(&paths, major, &majors, &args),
        Some("term-write") => {
            let client = term_client(&paths, major, &majors);
            let bytes = interpret_escapes(args.get(2).expect("term-write <id> <text>").as_str());
            client.write(args.get(1).expect("term-write <id> <text>"), &bytes).expect("term.write");
            println!("ok");
        }
        Some("term-key") => {
            let client = term_client(&paths, major, &majors);
            client
                .key(
                    args.get(1).expect("term-key <id> <name>"),
                    args.get(2).expect("term-key <id> <name>"),
                )
                .expect("term.key");
            println!("ok");
        }
        Some("term-feed") => {
            let client = term_client(&paths, major, &majors);
            client
                .feed(
                    args.get(1).expect("term-feed <id> <text>"),
                    args.get(2).expect("term-feed <id> <text>").as_bytes(),
                )
                .expect("term.feed");
            println!("ok");
        }
        Some("term-resize") => {
            let client = term_client(&paths, major, &majors);
            client
                .resize(
                    args.get(1).expect("term-resize <id> <cols> <rows>"),
                    args.get(2).and_then(|v| v.parse().ok()).expect("cols"),
                    args.get(3).and_then(|v| v.parse().ok()).expect("rows"),
                )
                .expect("term.resize");
            println!("ok");
        }
        Some("term-close") => {
            let client = term_client(&paths, major, &majors);
            client.close(args.get(1).expect("term-close <id>")).expect("term.close");
            println!("ok");
        }
        Some("term-wait") => {
            let client = term_client(&paths, major, &majors);
            let millis = args.get(2).and_then(|v| v.parse().ok()).expect("term-wait <id> <ms>");
            let result = client
                .wait(args.get(1).expect("term-wait <id> <ms>"), Duration::from_millis(millis))
                .expect("term.wait");
            println!("exit={}", wait_exit_label(&result));
        }
        Some("term-attach") => term_attach(&paths, major, &majors, &args),
        Some("term-echo-latency") => term_echo_latency(&paths, major, &majors, &args),
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

fn term_client(paths: &HostPaths, major: u32, majors: &[u32]) -> TermClient {
    let (mux, welcome) = Mux::open(
        &paths.endpoint(major),
        env!("CARGO_PKG_VERSION"),
        majors,
    )
    .expect("terminal host connection");
    assert!(welcome.capabilities.iter().any(|name| name == "terminal"));
    TermClient::new(Arc::new(mux)).expect("host did not advertise the terminal capability")
}

fn term_create(paths: &HostPaths, major: u32, majors: &[u32], args: &[String]) {
    let cwd = args.get(1).expect("term-create <cwd>");
    let separator = args.iter().enumerate().skip(2).find_map(|(index, value)| {
        (value == "--").then_some(index)
    });
    let option_end = separator.unwrap_or(args.len());
    let mut index = 2;
    let mut session_id = None;
    let mut origin = Origin::Control;
    let mut prelude = None;
    while index < option_end {
        match args[index].as_str() {
            "--id" => {
                index += 1;
                session_id = Some(args.get(index).expect("--id ID").clone());
            }
            "--origin" => {
                index += 1;
                origin = match args.get(index).map(String::as_str) {
                    Some("app") => Origin::App,
                    Some("control") => Origin::Control,
                    _ => panic!("--origin app|control"),
                };
            }
            "--prelude" => {
                index += 1;
                prelude = Some(b64_encode(
                    args.get(index).expect("--prelude TEXT").as_bytes(),
                ));
            }
            option => panic!("unknown term-create option {option}"),
        }
        index += 1;
    }
    let command = separator
        .and_then(|at| args.get(at + 1..))
        .filter(|program| !program.is_empty())
        .map(|program| TermCommand::Program {
            program: program[0].clone(),
            args: program[1..].to_vec(),
        })
        .unwrap_or(TermCommand::Shell);
    let client = term_client(paths, major, majors);
    let created = client
        .create(&CreateParams {
            session_id,
            cwd: cwd.clone(),
            command,
            env: BTreeMap::new(),
            size: TermSize { cols: 80, rows: 24 },
            origin,
            parent: None,
            prelude,
        })
        .expect("term.create");
    println!("session={}", created.session_id);
    println!("pid={}", created.pid);
}

fn term_read(paths: &HostPaths, major: u32, majors: &[u32], args: &[String]) {
    let session_id = args.get(1).expect("term-read <id>");
    let mut format = None;
    let mut max_bytes = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "text" => format = Some(ReadFormat::Text),
            "raw" => format = Some(ReadFormat::Raw),
            "--max" => {
                index += 1;
                max_bytes = Some(args.get(index).and_then(|v| v.parse().ok()).expect("--max N"));
            }
            option => panic!("unknown term-read option {option}"),
        }
        index += 1;
    }
    let result = term_client(paths, major, majors)
        .read(session_id, format, max_bytes)
        .expect("term.read");
    let bytes = match result.format {
        ReadFormat::Raw => b64_decode(&result.data).expect("term.read returned invalid base64"),
        ReadFormat::Text | ReadFormat::Other => result.data.into_bytes(),
    };
    std::io::stdout().write_all(&bytes).expect("write terminal data");
}

fn term_attach(paths: &HostPaths, major: u32, majors: &[u32], args: &[String]) {
    let session_id = args.get(1).expect("term-attach <id>");
    let mut resume_from = None;
    let mut for_ms = None;
    let mut no_read = false;
    let mut replica_path = None;
    let mut offset_path = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--resume" => {
                index += 1;
                resume_from = Some(args.get(index).and_then(|v| v.parse().ok()).expect("--resume OFFSET"));
            }
            "--for" => {
                index += 1;
                for_ms = Some(args.get(index).and_then(|v| v.parse().ok()).expect("--for MS"));
            }
            "--no-read" => no_read = true,
            "--replica-text" => {
                index += 1;
                replica_path = Some(PathBuf::from(args.get(index).expect("--replica-text FILE")));
            }
            "--last-offset" => {
                index += 1;
                offset_path = Some(PathBuf::from(args.get(index).expect("--last-offset FILE")));
            }
            option => panic!("unknown term-attach option {option}"),
        }
        index += 1;
    }
    let client = term_client(paths, major, majors);
    let (attached, stream) = client.attach(session_id, resume_from).expect("term.attach");
    println!(
        "mode={}",
        match attached.mode {
            AttachMode::Snapshot | AttachMode::Other => "snapshot",
            AttachMode::Resume => "resume",
        }
    );
    let wait = Duration::from_millis(for_ms.unwrap_or(if no_read { 5000 } else { 1000 }));
    if no_read {
        std::thread::sleep(wait);
        let deadline = Instant::now() + Duration::from_secs(5);
        let (mut lagged, mut resyncs, mut bytes) = (0usize, 0usize, 0usize);
        while Instant::now() < deadline {
            match stream.recv_timeout(Duration::from_millis(100)) {
                Ok(StreamItem::Snapshot(snapshot)) => bytes += snapshot.len(),
                Ok(StreamItem::Output { bytes: output, .. }) => bytes += output.len(),
                Ok(StreamItem::Resync) => resyncs += 1,
                Ok(StreamItem::Lagged) => {
                    lagged += 1;
                    break;
                }
                Ok(StreamItem::Closed) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        println!("lagged={lagged}");
        println!("resyncs={resyncs}");
        println!("bytes={bytes}");
        return;
    }

    let mut replica = Replica::new(attached.size.cols, attached.size.rows);
    let deadline = Instant::now() + wait;
    let mut lagged = 0usize;
    let mut resyncs = 0usize;
    let mut bytes = 0usize;
    let mut next_offset = resume_from.unwrap_or(0);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match stream.recv_timeout(remaining) {
            Ok(StreamItem::Snapshot(snapshot)) => {
                replica.0.reset();
                replica.0.vt_write(&snapshot);
                bytes += snapshot.len();
            }
            Ok(StreamItem::Output { offset, bytes: output }) => {
                replica.0.vt_write(&output);
                next_offset = offset.saturating_add(output.len() as u64);
                bytes += output.len();
            }
            Ok(StreamItem::Resync) => resyncs += 1,
            Ok(StreamItem::Lagged) => lagged += 1,
            Ok(StreamItem::Closed) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
        }
    }
    if let Some(path) = replica_path {
        std::fs::write(path, replica.text()).expect("write replica text");
    }
    if let Some(path) = offset_path {
        std::fs::write(path, format!("{next_offset}\n")).expect("write next offset");
    }
    println!("lagged={lagged}");
    println!("resyncs={resyncs}");
    println!("bytes={bytes}");
}

struct Replica(libghostty_vt::Terminal<'static, 'static>);

impl Replica {
    fn new(cols: u16, rows: u16) -> Self {
        Self(
            libghostty_vt::Terminal::new(libghostty_vt::TerminalOptions {
                cols,
                rows,
                max_scrollback: 10_000,
            })
            .expect("construct replica terminal"),
        )
    }

    fn text(&self) -> String {
        sirio_term_session::render_text(&self.0)
    }
}

fn term_echo_latency(paths: &HostPaths, major: u32, majors: &[u32], args: &[String]) {
    let session_id = args.get(1).expect("term-echo-latency <id> <samples>");
    let samples: usize = args.get(2).and_then(|v| v.parse().ok()).expect("samples");
    assert!(samples > 0, "samples must be positive");
    let client = term_client(paths, major, majors);
    let (attached, stream) = client.attach(session_id, None).expect("term.attach");
    let mut times = Vec::with_capacity(samples);
    for sample in 0..samples {
        let marker = format!("SIRIO-LATENCY-{}-{sample}-END", std::process::id());
        let started = Instant::now();
        let mut input = marker.as_bytes().to_vec();
        input.push(b'\n');
        client.input(attached.stream, &input).expect("stream input");
        let deadline = started + Duration::from_secs(5);
        let mut seen = false;
        let mut observed = Vec::new();
        while !seen {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for echo marker");
            }
            match stream.recv_timeout(remaining) {
                Ok(StreamItem::Snapshot(bytes)) | Ok(StreamItem::Output { bytes, .. }) => {
                    observed.extend_from_slice(&bytes);
                    seen = observed
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes());
                }
                Ok(StreamItem::Resync) => {}
                Ok(StreamItem::Lagged | StreamItem::Closed)
                | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("terminal stream closed before echo marker")
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("timed out waiting for echo marker")
                }
            }
        }
        times.push(started.elapsed().as_millis() as u64);
    }
    times.sort_unstable();
    println!("p50_ms={}", percentile(&times, 50));
    println!("p95_ms={}", percentile(&times, 95));
    println!("max_ms={}", times[times.len() - 1]);
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn interpret_escapes(text: &str) -> Vec<u8> {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next() {
            Some('r') => output.push('\r'),
            Some('n') => output.push('\n'),
            Some('e') => output.push('\x1b'),
            Some('\\') => output.push('\\'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output.into_bytes()
}

fn exit_label(exit: Option<&ExitStatus>) -> String {
    match exit {
        None => "none".into(),
        Some(ExitStatus::Success) => "success".into(),
        Some(ExitStatus::Code { code }) => format!("code:{code}"),
        Some(ExitStatus::Unknown | ExitStatus::Other) => "unknown".into(),
    }
}

fn wait_exit_label(result: &WaitResult) -> String {
    exit_label(result.exit.as_ref())
}

fn origin_label(origin: &Origin) -> &'static str {
    match origin {
        Origin::App => "app",
        Origin::Control => "control",
        Origin::Other => "other",
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
