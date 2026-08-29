//! #264 (part of #263): what does an agent CLI send to probe the terminal?
//!
//! Throwaway. Spawns the CLI under a real ConPTY, answers the queries that
//! would otherwise stall it, and reports every escape sequence it emitted --
//! with the Kitty graphics APCs called out.
//!
//! The DSR answer is not optional: #229 measured ConPTY blocking after four
//! bytes until the cursor-position report is answered, which made a probe
//! report "no queries" when the truth was "the handshake never finished".
//!
//! Usage: cargo run -p sirio_control --example agent_probe_capture -- <program> [args...]

fn main() {
    #[cfg(not(windows))]
    println!("windows-only probe");

    #[cfg(windows)]
    {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::{Read, Write};

        let mut args = std::env::args().skip(1);
        let program = args.next().expect("usage: agent_probe_capture <program> [args...]");
        let rest: Vec<String> = args.collect();
        let seconds: u64 = std::env::var("PROBE_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12);

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize { rows: 40, cols: 120, pixel_width: 0, pixel_height: 0 })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(&program);
        for arg in &rest {
            cmd.arg(arg);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // #264/R1.4: let a run claim a terminal identity, so the environment
        // gate pi decides on can be tested without touching Sirio itself.
        if let Ok(value) = std::env::var("PROBE_TERM_PROGRAM") {
            cmd.env("TERM_PROGRAM", value);
        }
        if let Ok(value) = std::env::var("PROBE_TERM") {
            cmd.env("TERM", value);
        }
        let _child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);

        let mut writer = pair.master.take_writer().expect("writer");
        let mut reader = pair.master.try_clone_reader().expect("reader");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16384];
            let mut all = Vec::new();
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];
                // Answer what would otherwise stall the handshake (#229).
                if chunk.windows(4).any(|w| w == b"\x1b[6n") {
                    let _ = writer.write_all(b"\x1b[1;1R");
                    let _ = writer.flush();
                }
                if chunk.windows(3).any(|w| w == b"\x1b[c") || chunk.windows(4).any(|w| w == b"\x1b[0c") {
                    // DA1: claim a VT220 with sixel, the usual shape.
                    let _ = writer.write_all(b"\x1b[?62;4c");
                    let _ = writer.flush();
                }
                all.extend_from_slice(chunk);
                let _ = tx.send(all.clone());
            }
        });

        std::thread::sleep(std::time::Duration::from_secs(seconds));
        let mut bytes = Vec::new();
        while let Ok(b) = rx.try_recv() {
            bytes = b;
        }

        println!("program: {program} {rest:?}");
        println!("bytes captured: {}", bytes.len());
        println!("raw: {:?}", String::from_utf8_lossy(&bytes));

        // Every APC the program emitted, Kitty ones flagged.
        let mut index = 0;
        let mut apcs = 0;
        while index + 1 < bytes.len() {
            if bytes[index] == 0x1b && bytes[index + 1] == b'_' {
                let start = index;
                let mut end = index + 2;
                while end + 1 < bytes.len() && !(bytes[end] == 0x1b && bytes[end + 1] == b'\\') {
                    end += 1;
                }
                let body = String::from_utf8_lossy(&bytes[start..(end + 2).min(bytes.len())]);
                let kitty = body.starts_with("\u{1b}_G");
                println!("APC{}: {:?}", if kitty { " [KITTY]" } else { "" }, body);
                apcs += 1;
                index = end + 2;
            } else {
                index += 1;
            }
        }
        println!("APC count: {apcs}");

        // And the CSI queries, which say what else it asked about.
        let text = String::from_utf8_lossy(&bytes);
        for needle in ["\u{1b}[c", "\u{1b}[>q", "\u{1b}[?u", "\u{1b}[6n", "\u{1b}[?1049"] {
            println!("{:?} present: {}", needle, text.contains(needle));
        }
    }
}
