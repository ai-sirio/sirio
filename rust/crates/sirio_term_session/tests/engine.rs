use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sirio_term_session::{fanout::Chunk, Event, Exit, Format, Session, SpawnSpec};

fn spec(id: &str, script: &str) -> SpawnSpec {
    SpawnSpec {
        id: id.into(),
        cwd: std::env::temp_dir(),
        program: Some(("/bin/sh".into(), vec!["-c".into(), script.into()])),
        env: std::env::vars().collect::<BTreeMap<_, _>>(),
        cols: 80,
        rows: 24,
        prelude: Vec::new(),
    }
}

fn text(session: &Session) -> String {
    String::from_utf8_lossy(&session.read(Format::Text, None)).into_owned()
}

fn until(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A client-side VT fed exactly what an attachment delivers, the way the
/// app's replica is.
struct Replica(libghostty_vt::Terminal<'static, 'static>);

impl Replica {
    fn new(cols: u16, rows: u16) -> Self {
        Self(
            libghostty_vt::Terminal::new(libghostty_vt::TerminalOptions {
                cols,
                rows,
                max_scrollback: 10_000,
            })
            .unwrap(),
        )
    }

    fn apply(&mut self, chunk: &Chunk) {
        match chunk {
            Chunk::Snapshot(bytes) => {
                self.0.reset();
                self.0.vt_write(bytes)
            }
            Chunk::Output { bytes, .. } => self.0.vt_write(bytes),
            Chunk::Resync | Chunk::Lagged => {}
        }
    }

    fn text(&self) -> String {
        sirio_term_session::render_text(&self.0)
    }
}

// These shell and process-group cases require POSIX `/bin/sh`, PTY job control,
// and unix process signals; there is no Windows variant for this integration test.
#[cfg(unix)]
#[test]
fn the_child_sees_its_pane_id_and_the_creators_environment() {
    let mut s = spec(
        "pane-7",
        "echo id=$SIRIO_PANE_ID old=$TILLER_PANE_ID term=$TERM x=$SIRIO_ENGINE_X; sleep 5",
    );
    s.env.insert("SIRIO_ENGINE_X".into(), "from-creator".into());
    let (tx, _rx) = mpsc::channel();
    let session = Session::spawn(s, tx).unwrap();
    until("the echo", Duration::from_secs(5), || {
        text(&session).contains("x=from-creator")
    });
    let t = text(&session);
    assert!(t.contains("id=pane-7 old=pane-7 term=xterm-256color"), "{t}");
    session.close();
}

#[test]
fn a_missing_cwd_is_refused_before_anything_spawns() {
    let mut s = spec("pane-1", "true");
    s.cwd = std::env::temp_dir().join("sirio-engine-no-such-dir");
    let (tx, _rx) = mpsc::channel();
    assert!(matches!(
        Session::spawn(s, tx),
        Err(sirio_term_session::SpawnError::InvalidCwd(_))
    ));
}

#[cfg(unix)]
#[test]
fn the_prelude_is_on_screen_before_the_childs_output() {
    let mut s = spec("pane-1", "echo AFTER; sleep 5");
    s.prelude = b"BEFORE\r\n".to_vec();
    let (tx, _rx) = mpsc::channel();
    let session = Session::spawn(s, tx).unwrap();
    until("AFTER", Duration::from_secs(5), || text(&session).contains("AFTER"));
    let t = text(&session);
    assert!(t.find("BEFORE").unwrap() < t.find("AFTER").unwrap(), "{t}");
    session.close();
}

#[cfg(unix)]
#[test]
fn a_terminal_query_with_no_client_gets_exactly_one_reply() {
    // `stty -echo raw` so the reply is not echoed; the script reads what came
    // back for six bytes and prints its length in hex.
    let session = Session::spawn(
        spec("pane-1", r#"stty -echo raw; printf '\033[6n'; r=$(dd bs=1 count=6 2>/dev/null); stty sane; printf 'reply=%s\n' "$(printf '%s' "$r" | od -An -c | tr -d ' \n')"; sleep 5"#),
        mpsc::channel().0,
    )
    .unwrap();
    until("the reply line", Duration::from_secs(8), || {
        text(&session).contains("reply=")
    });
    let t = text(&session);
    assert_eq!(t.matches("033[").count(), 1, "one cursor-position report, got: {t}");
    session.close();
}

#[cfg(unix)]
#[test]
fn a_snapshot_rebuilds_the_hosts_screen_in_a_fresh_replica() {
    let session = Session::spawn(
        spec("pane-1", "for i in $(seq 1 60); do printf '\\033[3%dmline %d\\033[0m\\n' $((i % 7)) $i; done; printf '\\033[?2004h'; sleep 30"),
        mpsc::channel().0,
    )
    .unwrap();
    until("line 60", Duration::from_secs(5), || text(&session).contains("line 60"));
    let attach = session.attach(None);
    assert!(!attach.resumed);
    let mut replica = Replica::new(attach.size.0, attach.size.1);
    let first = attach
        .attachment
        .receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(matches!(first, Chunk::Snapshot(_)));
    replica.apply(&first);
    assert_eq!(replica.text(), text(&session), "scrollback beyond one screen included");
    assert!(
        replica
            .0
            .mode(libghostty_vt::terminal::Mode::BRACKETED_PASTE)
            .unwrap(),
        "modes travel with the snapshot"
    );
    session.close();
}

#[cfg(unix)]
#[test]
fn a_resume_inside_the_journal_continues_byte_for_byte() {
    let session = Session::spawn(
        spec("pane-1", "i=0; while [ $i -lt 400 ]; do echo tick $i; i=$((i+1)); sleep 0.01; done; sleep 5"),
        mpsc::channel().0,
    )
    .unwrap();
    let first = session.attach(None);
    let mut seen = Vec::new();
    let mut last_offset = 0u64;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(Chunk::Output { offset, bytes }) = first
            .attachment
            .receiver
            .recv_timeout(Duration::from_millis(50))
        {
            assert_eq!(offset, last_offset.max(offset));
            last_offset = offset + bytes.len() as u64;
            seen.extend_from_slice(&bytes);
            first.attachment.consumed(bytes.len());
        }
        if seen.len() > 200 {
            break;
        }
    }
    session.detach(first.attachment.id);
    std::thread::sleep(Duration::from_millis(300));
    let second = session.attach(Some(last_offset));
    assert!(second.resumed);
    match second
        .attachment
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
    {
        Chunk::Output { offset, .. } => assert_eq!(offset, last_offset, "no gap, no overlap"),
        other => panic!("expected output, got {other:?}"),
    }
    session.close();
}

#[cfg(unix)]
#[test]
fn a_resume_from_outside_the_journal_falls_back_to_a_snapshot() {
    let session = Session::spawn(spec("pane-1", "echo hi; sleep 5"), mpsc::channel().0).unwrap();
    until("hi", Duration::from_secs(5), || text(&session).contains("hi"));
    let a = session.attach(Some(u64::MAX));
    assert!(!a.resumed);
    assert!(matches!(
        a.attachment
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap(),
        Chunk::Snapshot(_)
    ));
    session.close();
}

#[cfg(unix)]
#[test]
fn alternate_screen_exit_resyncs_a_stream_that_attached_during_it() {
    let session = Session::spawn(
        spec("pane-1", "echo PRIMARY-HISTORY; sleep 0.5; printf '\\033[?1049hALT-SCREEN'; sleep 1.5; printf '\\033[?1049l'; echo BACK; sleep 30"),
        mpsc::channel().0,
    )
    .unwrap();
    until("the alternate screen", Duration::from_secs(5), || {
        text(&session).contains("ALT-SCREEN")
    });
    let a = session.attach(None);
    let mut replica = Replica::new(a.size.0, a.size.1);
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut saw_resync = false;
    let mut restored = false;
    while Instant::now() < deadline && !restored {
        if let Ok(chunk) = a.attachment.receiver.recv_timeout(Duration::from_millis(50)) {
            match &chunk {
                Chunk::Resync => saw_resync = true,
                Chunk::Snapshot(_) if saw_resync => restored = true,
                _ => {}
            }
            replica.apply(&chunk);
        }
    }
    assert!(restored, "the primary-screen resync snapshot arrived");
    let t = replica.text();
    assert!(t.contains("PRIMARY-HISTORY") && t.contains("BACK"), "primary screen restored: {t}");
    assert_eq!(t, text(&session));
    session.close();
}

#[cfg(unix)]
#[test]
fn close_ends_the_shell_and_a_backgrounded_grandchild() {
    let dir = tempfile_dir();
    let pidfile = dir.join("grandchild.pid");
    let session = Session::spawn(
        spec(
            "pane-1",
            &format!("sleep 300 & echo $! > \"{}\"; wait", pidfile.display()),
        ),
        mpsc::channel().0,
    )
    .unwrap();
    until("the pid file", Duration::from_secs(5), || pidfile.exists());
    let grandchild: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    let shell = session.info().pid as i32;
    session.close();
    until("both gone", Duration::from_secs(3), || unsafe {
        libc::kill(grandchild, 0) != 0 && libc::kill(shell, 0) != 0
    });
}

#[cfg(unix)]
#[test]
fn exit_is_reported_once_as_an_event_and_by_wait() {
    let (tx, rx) = mpsc::channel();
    let session = Session::spawn(spec("pane-1", "echo finished; exit 3"), tx).unwrap();
    assert_eq!(session.wait(Duration::from_secs(5)), Some(Exit::Code(3)));
    let exits: Vec<_> = rx.try_iter().filter(|e| matches!(e, Event::Exited(_))).collect();
    assert_eq!(exits, vec![Event::Exited(Exit::Code(3))]);
    assert!(text(&session).contains("finished"), "an exited session keeps its screen");
}

#[cfg(unix)]
#[test]
fn the_title_and_the_foreground_state_arrive_as_events() {
    let (tx, rx) = mpsc::channel();
    let session = Session::spawn(SpawnSpec { program: None, ..spec("pane-1", "") }, tx).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    session.input(b"printf '\\033]0;hello title\\007'; sleep 2\r".to_vec());
    let mut title = false;
    let mut running = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !(title && running) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Title(t)) if t == "hello title" => title = true,
            Ok(Event::Foreground(true)) => running = true,
            _ => {}
        }
    }
    assert!(title && running, "title={title} running={running}");
    session.close();
}

#[cfg(unix)]
#[test]
fn a_named_key_is_encoded_against_the_terminals_modes() {
    // DECCKM on: the cursor keys are SS3 (ESC O A), not CSI (ESC [ A).
    let session = Session::spawn(
        spec("pane-1", r#"printf '\033[?1h'; stty -echo raw; r=$(dd bs=1 count=3 2>/dev/null); stty sane; printf 'got=%s\n' "$(printf '%s' "$r" | od -An -c | tr -d ' \n')"; sleep 5"#),
        mpsc::channel().0,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    session.key("up").unwrap();
    until("got=", Duration::from_secs(5), || text(&session).contains("got="));
    assert!(text(&session).contains("got=033OA"), "{}", text(&session));
    assert!(session.key("hyper-left").is_err());
    session.close();
}

#[cfg(unix)]
#[test]
fn feed_reaches_the_screen_and_every_attachment_but_not_the_child() {
    let session = Session::spawn(spec("pane-1", "cat > /dev/null"), mpsc::channel().0).unwrap();
    let a = session.attach(None);
    let _ = a.attachment.receiver.recv_timeout(Duration::from_secs(2)); // the snapshot
    session.feed(b"FED-TEXT\r\n".to_vec());
    match a.attachment.receiver.recv_timeout(Duration::from_secs(2)).unwrap() {
        Chunk::Output { bytes, .. } => assert_eq!(bytes, b"FED-TEXT\r\n"),
        other => panic!("{other:?}"),
    }
    assert!(text(&session).contains("FED-TEXT"));
    session.close();
}

#[cfg(unix)]
#[test]
fn a_flood_never_blocks_the_session_and_a_lagged_client_recovers() {
    let session = Session::spawn(
        spec("pane-1", "head -c 20000000 /dev/zero | tr '\\0' 'x'; echo; echo FLOOD-DONE; sleep 30"),
        mpsc::channel().0,
    )
    .unwrap();
    let stalled = session.attach(None); // never reads
    until("the flood to end", Duration::from_secs(60), || {
        text(&session).contains("FLOOD-DONE")
    });
    let chunks: Vec<Chunk> = stalled.attachment.receiver.try_iter().collect();
    assert_eq!(chunks.last(), Some(&Chunk::Lagged));
    let again = session.attach(None);
    let mut replica = Replica::new(again.size.0, again.size.1);
    replica.apply(&again.attachment.receiver.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(replica.text(), text(&session));
    session.close();
}

#[cfg(unix)]
fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sirio-engine-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
