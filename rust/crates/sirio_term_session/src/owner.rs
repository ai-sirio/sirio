use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use libghostty_vt::alloc::{Allocator, Bytes};
use libghostty_vt::kitty::graphics::{DecodePng, DecodedImage};
use libghostty_vt::screen::Screen;
use libghostty_vt::{Terminal, key};
use portable_pty::{Child, MasterPty, PtySize};

use crate::fanout::{self, AttachmentId, Chunk};
use crate::journal::Journal;
use crate::{Attach, Event, Exit, Format, Info, JOURNAL_BYTES, STREAM_QUEUE_BYTES};

/// #301 R1.3: the owner states the Kitty APC and image-store ceilings
/// explicitly, rather than inheriting libghostty-vt's defaults.
pub const KITTY_APC_MAX_BYTES: usize = 32 * 1024 * 1024;
pub const KITTY_IMAGE_STORAGE_LIMIT: u64 = 64 * 1024 * 1024;
/// #303 R2.4: cap PNG decoder allocations, including decompressed RGBA output.
pub const KITTY_DECOMPRESSED_MAX_BYTES: u64 = KITTY_IMAGE_STORAGE_LIMIT;

/// #301: PNG decoding is registered by each terminal owner so Kitty f=100
/// transmits are stored while the optional libghostty PNG feature stays off.
pub struct KittyPngDecoder {
    failure_flag: Arc<AtomicBool>,
}

impl KittyPngDecoder {
    pub fn new() -> Self {
        Self::with_failure_flag(Arc::new(AtomicBool::new(false)))
    }

    pub fn with_failure_flag(failure_flag: Arc<AtomicBool>) -> Self {
        Self { failure_flag }
    }
}

impl Default for KittyPngDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DecodePng for KittyPngDecoder {
    fn decode_png<'alloc>(
        &mut self,
        alloc: &'alloc Allocator<'_>,
        data: &[u8],
    ) -> Option<DecodedImage<'alloc>> {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(KITTY_DECOMPRESSED_MAX_BYTES);
        reader.limits(limits);
        let image = match reader.decode() {
            Ok(image) => image.into_rgba8(),
            Err(_) => {
                self.failure_flag.store(true, Ordering::Release);
                return None;
            }
        };
        let (width, height) = image.dimensions();
        let Some(mut out) = Bytes::new_with_alloc(alloc, image.as_raw().len()).ok() else {
            self.failure_flag.store(true, Ordering::Release);
            return None;
        };
        out.copy_from_slice(image.as_raw());
        Some(DecodedImage {
            width,
            height,
            data: out,
        })
    }
}

pub fn apply_kitty_ingest_limits(terminal: &mut Terminal<'_, '_>) {
    terminal
        .set_apc_max_bytes_kitty(Some(KITTY_APC_MAX_BYTES))
        .expect("set the Kitty APC buffer ceiling");
    terminal
        .set_kitty_image_storage_limit(KITTY_IMAGE_STORAGE_LIMIT)
        .expect("set the Kitty image storage limit");
}

pub(super) struct Inputs {
    pub cols: u16,
    pub rows: u16,
    pub prelude: Vec<u8>,
    pub command_rx: Receiver<Command>,
    pub bytes_rx: Receiver<Vec<u8>>,
    pub events: Sender<Event>,
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn Child + Send + Sync>,
    pub info: Arc<Mutex<Info>>,
    pub wait_state: Arc<(Mutex<Option<Exit>>, Condvar)>,
    pub ready: Sender<()>,
    #[cfg(unix)]
    pub master_fd: Option<libc::c_int>,
    #[cfg(windows)]
    pub kill_on_close: Option<crate::terminate::KillOnClose>,
}

pub(super) enum Command {
    Input(Vec<u8>, Sender<Result<(), crate::InputError>>),
    Key(String, Sender<Result<(), crate::KeyError>>),
    Feed(Vec<u8>),
    Resize(u16, u16),
    Attach(Option<u64>, Sender<Attach>),
    Detach(AttachmentId),
    Read(Format, Option<usize>, Sender<Vec<u8>>),
    Info(Sender<Info>),
    Close(Sender<()>),
}

fn input_reply_then_write<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    exited: bool,
    reply: Sender<Result<(), crate::InputError>>,
) {
    let result = if exited {
        Err(crate::InputError::Exited)
    } else {
        Ok(())
    };
    let _ = reply.send(result);
    if result.is_ok() {
        let _ = writer.write_all(bytes);
        let _ = writer.flush();
    }
}

pub(super) fn start(inputs: Inputs) {
    std::thread::spawn(move || run(inputs));
}

fn run(inputs: Inputs) {
    let Inputs {
        cols,
        rows,
        prelude,
        command_rx,
        bytes_rx,
        events,
        mut writer,
        master,
        mut child,
        info: shared_info,
        wait_state,
        ready,
        #[cfg(unix)]
        master_fd,
        #[cfg(windows)]
        mut kill_on_close,
    } = inputs;

    let mut terminal = crate::initialize_terminal(cols, rows);
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    terminal
        .on_pty_write(move |_terminal, bytes: &[u8]| {
            let _ = reply_tx.send(bytes.to_vec());
        })
        .expect("terminal session: register PTY replies");
    apply_kitty_ingest_limits(&mut terminal);
    let decode_failed = Arc::new(AtomicBool::new(false));
    libghostty_vt::kitty::graphics::set_png_decoder(Some(Box::new(
        KittyPngDecoder::with_failure_flag(Arc::clone(&decode_failed)),
    )))
    .expect("terminal session: register Kitty PNG decoder");

    let mut keys = key::Encoder::new().expect("terminal session: construct key encoder");
    let mut journal = Journal::new(JOURNAL_BYTES);
    let mut fanout = fanout::Fanout::new(STREAM_QUEUE_BYTES);
    let mut alternate_attachments = HashSet::new();
    let mut cached = shared_info
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let mut last_title = cached.title.clone();
    let mut last_foreground = cached.foreground;
    let mut was_alternate = terminal.active_screen().ok() == Some(Screen::Alternate);

    if !prelude.is_empty() {
        feed(&mut terminal, &mut journal, &mut fanout, prelude);
        flush_replies(&mut writer, &reply_rx);
    }
    publish_info(&shared_info, &cached);
    let _ = ready.send(());

    let mut carried = None;
    loop {
        // 1. Drain PTY bytes into the resident VT, journal, and every stream.
        while let Ok(bytes) = bytes_rx.try_recv() {
            let offset = journal.head();
            terminal.vt_write(&bytes);
            journal.push(&bytes);
            fanout.broadcast_output(offset, &bytes);
        }

        // 2. This VT alone answers queries and writes those replies to the PTY.
        flush_replies(&mut writer, &reply_rx);

        // 3. Attachments that saw an alternate screen receive a fresh primary
        // snapshot as soon as that screen is left.
        let is_alternate = terminal.active_screen().ok() == Some(Screen::Alternate);
        if was_alternate && !is_alternate {
            for id in alternate_attachments.drain() {
                fanout.send_to(id, Chunk::Resync);
                fanout.send_to(id, Chunk::Snapshot(crate::render_snapshot(&terminal)));
                fanout.send_to(
                    id,
                    Chunk::Output {
                        offset: journal.head(),
                        bytes: Vec::new(),
                    },
                );
            }
        }
        was_alternate = is_alternate;

        let mut should_close = false;
        let mut close_reply = None;

        // 4. Commands run between PTY drains, making attach/snapshot atomic
        // with respect to output already read from the master.
        while let Some(command) = carried.take().or_else(|| command_rx.try_recv().ok()) {
            match command {
                Command::Input(bytes, reply) => {
                    input_reply_then_write(&mut writer, &bytes, cached.exit.is_some(), reply);
                }
                Command::Key(name, reply) => {
                    if cached.exit.is_some() {
                        let _ = reply.send(Err(crate::KeyError::Session(
                            crate::InputError::Exited,
                        )));
                    } else {
                        match crate::keys::encode(&terminal, &mut keys, &name) {
                            Err(reason) => {
                                let _ = reply.send(Err(crate::KeyError::InvalidKey(reason)));
                            }
                            Ok(bytes) => {
                                let _ = reply.send(Ok(()));
                                let _ = writer.write_all(&bytes);
                                let _ = writer.flush();
                            }
                        }
                    }
                }
                Command::Feed(bytes) => {
                    feed(&mut terminal, &mut journal, &mut fanout, bytes);
                    flush_replies(&mut writer, &reply_rx);
                }
                Command::Resize(cols, rows) => {
                    let _ = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                    let _ = terminal.resize(cols, rows, 0, 0);
                    cached.cols = cols;
                    cached.rows = rows;
                }
                Command::Attach(resume_from, reply) => {
                    let (id, attachment) = fanout.add();
                    let resume = resume_from.and_then(|offset| {
                        journal.since(offset).map(|bytes| (offset, bytes))
                    });
                    let resumed = if let Some((offset, bytes)) = resume {
                        if !bytes.is_empty() {
                            fanout.send_to(id, Chunk::Output { offset, bytes });
                        }
                        true
                    } else {
                        fanout.send_to(id, Chunk::Snapshot(crate::render_snapshot(&terminal)));
                        fanout.send_to(
                            id,
                            Chunk::Output {
                                offset: journal.head(),
                                bytes: Vec::new(),
                            },
                        );
                        false
                    };
                    if terminal.active_screen().ok() == Some(Screen::Alternate) {
                        alternate_attachments.insert(id);
                    }
                    cached.attachments = fanout.len();
                    let _ = reply.send(Attach {
                        size: (cached.cols, cached.rows),
                        resumed,
                        attachment,
                    });
                }
                Command::Detach(id) => {
                    fanout.remove(id);
                    alternate_attachments.remove(&id);
                    cached.attachments = fanout.len();
                }
                Command::Read(format, max_bytes, reply) => {
                    let bytes = match format {
                        Format::Text => {
                            let text = crate::render_text(&terminal);
                            truncate_text(text, max_bytes)
                        }
                        Format::Raw => journal.last(max_bytes.unwrap_or(JOURNAL_BYTES)),
                    };
                    let _ = reply.send(bytes);
                }
                Command::Info(reply) => {
                    cached.attachments = fanout.len();
                    let _ = reply.send(cached.clone());
                }
                Command::Close(reply) => {
                    should_close = true;
                    close_reply = Some(reply);
                    break;
                }
            }
        }

        cached.attachments = fanout.len();

        // 5. Publish changes to observable process and VT state once per tick.
        if let Ok(title) = terminal.title()
            && title != last_title
        {
            last_title = title.to_string();
            cached.title = last_title.clone();
            let _ = events.send(Event::Title(last_title.clone()));
        }

        #[cfg(unix)]
        let foreground = master_fd.is_some_and(|fd| {
            let foreground_pgid = unsafe { libc::tcgetpgrp(fd) };
            foreground_pgid > 0 && foreground_pgid as u32 != cached.pid
        });
        #[cfg(windows)]
        let foreground = false;
        if foreground != last_foreground {
            last_foreground = foreground;
            cached.foreground = foreground;
            let _ = events.send(Event::Foreground(foreground));
        }

        if cached.exit.is_none() {
            let exit = match child.try_wait() {
                Ok(Some(status)) if status.success() => Some(Exit::Success),
                Ok(Some(status)) => Some(Exit::Code(status.exit_code() as i32)),
                Ok(None) => None,
                Err(_) => Some(Exit::Unknown),
            };
            if let Some(exit) = exit {
                cached.exit = Some(exit.clone());
                let _ = events.send(Event::Exited(exit.clone()));
                let (exit_lock, changed) = &*wait_state;
                *exit_lock
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(exit.clone());
                changed.notify_all();
            }
        }
        publish_info(&shared_info, &cached);

        if should_close {
            close_session(
                cached.pid,
                &mut child,
                master,
                writer,
                #[cfg(windows)]
                &mut kill_on_close,
            );
            if let Some(reply) = close_reply {
                let _ = reply.send(());
            }
            return;
        }

        // A disconnected handle means no caller can use the session anymore.
        match command_rx.recv_timeout(Duration::from_millis(4)) {
            Ok(command) => carried = Some(command),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                close_session(
                    cached.pid,
                    &mut child,
                    master,
                    writer,
                    #[cfg(windows)]
                    &mut kill_on_close,
                );
                return;
            }
        }
    }
}

fn feed(
    terminal: &mut Terminal<'static, 'static>,
    journal: &mut Journal,
    fanout: &mut fanout::Fanout,
    bytes: Vec<u8>,
) {
    let offset = journal.head();
    terminal.vt_write(&bytes);
    journal.push(&bytes);
    fanout.broadcast_output(offset, &bytes);
}

fn flush_replies(writer: &mut Box<dyn Write + Send>, replies: &Receiver<Vec<u8>>) {
    let mut wrote = false;
    while let Ok(reply) = replies.try_recv() {
        wrote = true;
        let _ = writer.write_all(&reply);
    }
    if wrote {
        let _ = writer.flush();
    }
}

fn truncate_text(text: String, max_bytes: Option<usize>) -> Vec<u8> {
    let Some(max_bytes) = max_bytes else {
        return text.into_bytes();
    };
    let mut start = text.len().saturating_sub(max_bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text.as_bytes()[start..].to_vec()
}

fn publish_info(shared: &Mutex<Info>, info: &Info) {
    *shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = info.clone();
}

fn close_session(
    pid: u32,
    child: &mut Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    #[cfg(windows)] kill_on_close: &mut Option<crate::terminate::KillOnClose>,
) {
    #[cfg(unix)]
    crate::terminate::terminate_descendant_process_groups(pid);
    #[cfg(windows)]
    drop(kill_on_close.take());
    let _ = child.kill();
    drop(master);
    drop(writer);
}

#[cfg(test)]
mod tests {
    use super::input_reply_then_write;
    use std::io::{self, Write};
    use std::sync::mpsc;
    use std::time::Duration;

    struct BlockingWriter {
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl Write for BlockingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let _ = self.started.send(());
            let _ = self.release.recv();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn input_reply_precedes_a_blocking_pty_write() {
        let (reply, response) = mpsc::channel();
        let (started, write_started) = mpsc::channel();
        let (release, unblock) = mpsc::channel();
        let mut writer = BlockingWriter {
            started,
            release: unblock,
        };
        let worker = std::thread::spawn(move || {
            input_reply_then_write(&mut writer, b"input", false, reply);
        });

        write_started
            .recv_timeout(Duration::from_secs(1))
            .expect("owner entered the blocking PTY write");
        let result = response.recv_timeout(Duration::from_millis(100));
        let _ = release.send(());
        worker.join().unwrap();
        assert_eq!(result, Ok(Ok(())));
    }
}
