//! #229 probe: does ConPTY request win32-input-mode (DECSET 9001) from us?
//!
//! Throwaway. Spawns a shell under a real ConPTY, reads whatever the console
//! host writes back, and reports which private modes it asked for. The answer
//! decides whether a win32 input path in Tiller would be mode-gated (only
//! active once the app asks) or a blanket re-encoding of every keystroke.
//!
//! Usage: cargo run -p tiller_control --example conpty_mode_probe -- powershell.exe

fn main() {
    #[cfg(not(windows))]
    println!("windows-only probe");

    #[cfg(windows)]
    {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::Read;

        let program = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "powershell.exe".to_string());

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let cmd = CommandBuilder::new(&program);
        let _child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);

        let mut writer = pair.master.take_writer().expect("writer");
        let mut reader = pair.master.try_clone_reader().expect("reader");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut all = Vec::new();
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&buf[..n]);
                // #246 already recorded this: ConPTY emits DSR (ESC[6n) and
                // BLOCKS until it is answered. Without a reply the probe
                // measures a stalled handshake, not the real mode set.
                if buf[..n].windows(4).any(|w| w == b"[6n") {
                    use std::io::Write;
                    let _ = writer.write_all(b"[1;1R");
                    let _ = writer.flush();
                }
                let _ = tx.send(all.clone());
            }
        });

        // Give the console host time to emit its startup handshake.
        std::thread::sleep(std::time::Duration::from_secs(3));
        let mut bytes = Vec::new();
        while let Ok(b) = rx.try_recv() {
            bytes = b;
        }

        println!("program: {program}");
        println!("bytes read: {}", bytes.len());
        println!("raw: {:?}", String::from_utf8_lossy(&bytes));

        // Report every DECSET/DECRST the host asked for.
        let text = String::from_utf8_lossy(&bytes);
        let mut modes: Vec<String> = Vec::new();
        let raw: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i + 3 < raw.len() {
            if raw[i] == '\u{1b}' && raw[i + 1] == '[' && raw[i + 2] == '?' {
                let mut j = i + 3;
                let mut num = String::new();
                while j < raw.len() && (raw[j].is_ascii_digit() || raw[j] == ';') {
                    num.push(raw[j]);
                    j += 1;
                }
                if j < raw.len() && (raw[j] == 'h' || raw[j] == 'l') {
                    modes.push(format!("?{}{}", num, raw[j]));
                }
                i = j;
            } else {
                i += 1;
            }
        }
        modes.dedup();
        println!("private modes requested: {modes:?}");
        println!(
            "win32-input-mode (9001) requested: {}",
            modes.iter().any(|m| m.starts_with("?9001"))
        );
    }
}
