//! Hands a session's bytes to every attachment without ever waiting for one
//! (SP2 spec §3.4). Each attachment counts the bytes queued for it and not
//! yet consumed; a chunk that would take that count over the limit lags the
//! attachment instead: it receives `Lagged`, is removed, and re-attaches for a
//! snapshot. A chunk is always delivered to an attachment that has nothing
//! queued, however large, so a snapshot bigger than the limit still arrives.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Snapshot(Vec<u8>),
    Output { offset: u64, bytes: Vec<u8> },
    /// The alternate screen was left; a `Snapshot` follows.
    Resync,
    /// The attachment fell too far behind and was removed.
    Lagged,
}

impl Chunk {
    fn weight(&self) -> usize {
        match self {
            Chunk::Snapshot(b) | Chunk::Output { bytes: b, .. } => b.len(),
            Chunk::Resync | Chunk::Lagged => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AttachmentId(u64);

/// The consumer's end: the chunks, and the counter it decrements as it
/// writes them out.
pub struct Attachment {
    pub id: AttachmentId,
    pub receiver: Receiver<Chunk>,
    queued: Arc<AtomicUsize>,
}

impl Attachment {
    pub fn consumed(&self, bytes: usize) {
        let _ = self.queued.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |q| {
            Some(q.saturating_sub(bytes))
        });
    }
}

struct Slot {
    id: AttachmentId,
    sender: Sender<Chunk>,
    queued: Arc<AtomicUsize>,
}

pub struct Fanout {
    slots: Vec<Slot>,
    limit: usize,
    next: u64,
}

impl Fanout {
    pub fn new(limit: usize) -> Self {
        Self { slots: Vec::new(), limit, next: 1 }
    }

    pub fn add(&mut self) -> (AttachmentId, Attachment) {
        let id = AttachmentId(self.next);
        self.next += 1;
        let (sender, receiver) = channel();
        let queued = Arc::new(AtomicUsize::new(0));
        self.slots.push(Slot { id, sender, queued: Arc::clone(&queued) });
        (id, Attachment { id, receiver, queued })
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn remove(&mut self, id: AttachmentId) {
        self.slots.retain(|slot| slot.id != id);
    }

    pub fn send_to(&mut self, id: AttachmentId, chunk: Chunk) {
        let limit = self.limit;
        self.slots.retain(|slot| slot.id != id || deliver(slot, chunk.clone(), limit));
    }

    pub fn broadcast_output(&mut self, offset: u64, bytes: &[u8]) {
        let limit = self.limit;
        self.slots.retain(|slot| {
            deliver(slot, Chunk::Output { offset, bytes: bytes.to_vec() }, limit)
        });
    }
}

/// `false` removes the slot: the receiver is gone, or the attachment lagged.
fn deliver(slot: &Slot, chunk: Chunk, limit: usize) -> bool {
    let weight = chunk.weight();
    let reserved = slot.queued.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |queued| {
        let total = queued.saturating_add(weight);
        if queued > 0 && total > limit {
            None
        } else {
            Some(total)
        }
    });
    if reserved.is_err() {
        let _ = slot.sender.send(Chunk::Lagged);
        return false;
    }
    slot.sender.send(chunk).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(rx: &std::sync::mpsc::Receiver<Chunk>) -> Vec<Chunk> {
        rx.try_iter().collect()
    }

    #[test]
    fn output_reaches_every_attachment_in_order() {
        let mut f = Fanout::new(100);
        let (_a, ra) = f.add();
        let (_b, rb) = f.add();
        f.broadcast_output(0, b"ab");
        f.broadcast_output(2, b"c");
        for rx in [&ra, &rb] {
            assert_eq!(
                drain(&rx.receiver),
                vec![
                    Chunk::Output { offset: 0, bytes: b"ab".to_vec() },
                    Chunk::Output { offset: 2, bytes: b"c".to_vec() },
                ]
            );
        }
    }

    #[test]
    fn a_consumer_that_keeps_up_is_never_lagged() {
        let mut f = Fanout::new(4);
        let (_a, ra) = f.add();
        for i in 0..100u64 {
            f.broadcast_output(i * 3, b"xyz");
            for chunk in ra.receiver.try_iter() {
                if let Chunk::Output { bytes, .. } = chunk {
                    ra.consumed(bytes.len());
                } else {
                    panic!("unexpected {chunk:?}");
                }
            }
        }
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn a_consumer_that_stops_reading_is_lagged_once_and_removed() {
        let mut f = Fanout::new(4);
        let (_slow, rs) = f.add();
        let (_fast, rf) = f.add();
        f.broadcast_output(0, b"abc");
        rf.consumed(3);
        f.broadcast_output(3, b"de");
        rf.consumed(2);
        f.broadcast_output(5, b"fg");
        assert_eq!(
            drain(&rs.receiver),
            vec![Chunk::Output { offset: 0, bytes: b"abc".to_vec() }, Chunk::Lagged]
        );
        assert_eq!(f.len(), 1, "the slow attachment is gone, the fast one stays");
        assert_eq!(drain(&rf.receiver).len(), 3);
    }

    #[test]
    fn a_dropped_receiver_is_removed_without_a_lagged() {
        let mut f = Fanout::new(100);
        let (_a, ra) = f.add();
        drop(ra);
        f.broadcast_output(0, b"x");
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn a_snapshot_counts_against_the_limit_like_output() {
        let mut f = Fanout::new(4);
        let (a, ra) = f.add();
        f.send_to(a, Chunk::Snapshot(b"12345".to_vec()));
        assert_eq!(drain(&ra.receiver), vec![Chunk::Snapshot(b"12345".to_vec())],
            "one chunk larger than the limit is still delivered to an idle consumer");
        f.broadcast_output(0, b"x");
        assert_eq!(drain(&ra.receiver), vec![Chunk::Lagged]);
    }

    #[test]
    fn removing_an_attachment_stops_its_chunks() {
        let mut f = Fanout::new(100);
        let (a, ra) = f.add();
        f.remove(a);
        f.broadcast_output(0, b"x");
        assert!(drain(&ra.receiver).is_empty());
        assert_eq!(f.len(), 0);
    }
}
