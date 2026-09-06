//! Opt-in local performance trace. No application content is recorded.

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static RECORDER: OnceLock<Option<Recorder>> = OnceLock::new();
static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static THREAD: u64 = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
}

struct Recorder {
    origin: Instant,
    sender: SyncSender<Event>,
    dropped: Arc<AtomicU64>,
}

impl Recorder {
    fn record(&self, event: Event) {
        if self.sender.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn recorder() -> Option<&'static Recorder> {
    RECORDER
        .get_or_init(|| {
            let path = std::env::var_os("SIRIO_PERF_TRACE")?;
            let file = match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(file) => file,
                Err(error) => {
                    eprintln!("[sirio-perf] TRACE INVALID: cannot create trace: {error}");
                    return None;
                }
            };
            let origin = Instant::now();
            let epoch_ns = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let (sender, receiver) = mpsc::sync_channel::<Event>(32768);
            let dropped = Arc::new(AtomicU64::new(0));
            let writer_dropped = dropped.clone();
            std::thread::Builder::new()
                .name("sirio-perf-writer".into())
                .spawn(move || {
                    let result = (|| -> io::Result<()> {
                        let mut output = BufWriter::new(file);
                        writeln!(
                            output,
                            "# sirio-perf-v1\tpid={}\tepoch_ns={epoch_ns}",
                            std::process::id()
                        )?;
                        writeln!(
                            output,
                            "# start_ns\tthread\tkind\tname\tentity\tduration_ns"
                        )?;
                        output.flush()?;
                        let mut last_flush = Instant::now();
                        loop {
                            match receiver.recv_timeout(Duration::from_secs(1)) {
                                Ok(event) => encode_event(&mut output, event)?,
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(mpsc::RecvTimeoutError::Timeout) => (),
                            }
                            if last_flush.elapsed() >= Duration::from_secs(1) {
                                let lost = writer_dropped.swap(0, Ordering::Relaxed);
                                encode_event(
                                    &mut output,
                                    Event {
                                        started_ns: origin.elapsed().as_nanos() as u64,
                                        thread: 0,
                                        kind: "health",
                                        name: "trace.dropped",
                                        entity: lost,
                                        duration_ns: 0,
                                    },
                                )?;
                                output.flush()?;
                                last_flush = Instant::now();
                            }
                        }
                        output.flush()
                    })();
                    if let Err(error) = result {
                        eprintln!("[sirio-perf] TRACE INVALID: writer failed: {error}");
                    }
                })
                .expect("start performance trace writer");
            Some(Recorder {
                origin,
                sender,
                dropped,
            })
        })
        .as_ref()
}

/// Initialize before application startup, without starting anything when disabled.
pub fn init() {
    let _ = recorder();
}

pub fn enabled() -> bool {
    recorder().is_some()
}

/// A bounded, nonblocking trace event. `name` must be a content-free static label.
pub fn event(name: &'static str, entity: u64) {
    if let Some(recorder) = recorder() {
        recorder.record(Event {
            started_ns: recorder.origin.elapsed().as_nanos() as u64,
            thread: THREAD.with(|id| *id),
            kind: "event",
            name,
            entity,
            duration_ns: 0,
        });
    }
}

/// RAII timing span. Kept on its original thread so nesting is meaningful.
pub struct Span {
    state: Option<(&'static Recorder, Event, Instant)>,
    _not_send: PhantomData<Rc<()>>,
}

pub fn span(name: &'static str, entity: u64) -> Span {
    let state = recorder().map(|recorder| {
        let started = Instant::now();
        let event = Event {
            started_ns: started.duration_since(recorder.origin).as_nanos() as u64,
            thread: THREAD.with(|id| *id),
            kind: "span",
            name,
            entity,
            duration_ns: 0,
        };
        (recorder, event, started)
    });
    Span {
        state,
        _not_send: PhantomData,
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((recorder, mut event, started)) = self.state.take() {
            event.duration_ns = started.elapsed().as_nanos() as u64;
            recorder.record(event);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Event {
    started_ns: u64,
    thread: u64,
    kind: &'static str,
    name: &'static str,
    entity: u64,
    duration_ns: u64,
}

fn encode_event(output: &mut impl Write, event: Event) -> io::Result<()> {
    writeln!(
        output,
        "{}\t{}\t{}\t{}\t{}\t{}",
        event.started_ns, event.thread, event.kind, event.name, event.entity, event.duration_ns
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_is_counted_and_never_blocks_the_measured_thread() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let recorder = Recorder {
            origin: Instant::now(),
            sender,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        let event = Event {
            started_ns: 0,
            thread: 1,
            kind: "event",
            name: "present",
            entity: 0,
            duration_ns: 0,
        };
        recorder.record(event);
        recorder.record(event);
        assert_eq!(recorder.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(receiver.recv().unwrap().name, "present");
        drop(receiver);
        recorder.record(event);
        assert_eq!(recorder.dropped.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn trace_preserves_start_time_duration_entity_and_thread() {
        let event = Event {
            started_ns: 123,
            thread: 2,
            kind: "span",
            name: "Chat.render",
            entity: 42,
            duration_ns: 789,
        };
        let mut bytes = Vec::new();
        encode_event(&mut bytes, event).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "123\t2\tspan\tChat.render\t42\t789\n"
        );
    }

    #[test]
    fn trace_write_failure_is_not_silently_accepted() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("disk full"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let event = Event {
            started_ns: 0,
            thread: 1,
            kind: "event",
            name: "present",
            entity: 0,
            duration_ns: 0,
        };
        assert!(encode_event(&mut Broken, event).is_err());
    }
}
