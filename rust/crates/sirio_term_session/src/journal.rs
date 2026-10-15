//! The last `capacity` bytes a session's PTY produced, addressed by a
//! monotonic offset since the session started (SP2 spec §3.4). A client that
//! knows the offset it stopped at resumes from here instead of taking a
//! snapshot, as long as the ring still holds that offset.

use std::collections::VecDeque;

pub struct Journal {
    bytes: VecDeque<u8>,
    capacity: usize,
    head: u64,
}

impl Journal {
    pub fn new(capacity: usize) -> Self {
        Self { bytes: VecDeque::with_capacity(capacity.min(1 << 16)), capacity, head: 0 }
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.head += bytes.len() as u64;
        let keep = &bytes[bytes.len().saturating_sub(self.capacity)..];
        let overflow = (self.bytes.len() + keep.len()).saturating_sub(self.capacity);
        self.bytes.drain(..overflow);
        self.bytes.extend(keep);
    }

    pub fn head(&self) -> u64 {
        self.head
    }

    pub fn tail(&self) -> u64 {
        self.head - self.bytes.len() as u64
    }

    pub fn since(&self, offset: u64) -> Option<Vec<u8>> {
        if offset < self.tail() || offset > self.head {
            return None;
        }
        let skip = (offset - self.tail()) as usize;
        Some(self.bytes.iter().skip(skip).copied().collect())
    }

    pub fn last(&self, max: usize) -> Vec<u8> {
        let skip = self.bytes.len().saturating_sub(max);
        self.bytes.iter().skip(skip).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_journal_holds_nothing_at_offset_zero() {
        let j = Journal::new(8);
        assert_eq!((j.tail(), j.head()), (0, 0));
        assert_eq!(j.since(0), Some(vec![]));
        assert_eq!(j.since(1), None);
    }

    #[test]
    fn bytes_are_addressed_by_their_offset_since_the_start() {
        let mut j = Journal::new(8);
        j.push(b"abc");
        j.push(b"de");
        assert_eq!((j.tail(), j.head()), (0, 5));
        assert_eq!(j.since(0), Some(b"abcde".to_vec()));
        assert_eq!(j.since(3), Some(b"de".to_vec()));
        assert_eq!(j.since(5), Some(vec![]));
    }

    #[test]
    fn the_ring_forgets_the_oldest_bytes_and_says_so() {
        let mut j = Journal::new(4);
        j.push(b"abcdef");
        assert_eq!((j.tail(), j.head()), (2, 6));
        assert_eq!(j.since(2), Some(b"cdef".to_vec()));
        assert_eq!(j.since(1), None, "an offset that wrapped out cannot be resumed");
        j.push(b"gh");
        assert_eq!((j.tail(), j.head()), (4, 8));
        assert_eq!(j.since(5), Some(b"fgh".to_vec()));
    }

    #[test]
    fn an_offset_ahead_of_the_head_is_not_resumable() {
        let mut j = Journal::new(4);
        j.push(b"ab");
        assert_eq!(j.since(3), None);
        assert_eq!(j.since(u64::MAX), None);
    }

    #[test]
    fn a_push_larger_than_the_ring_keeps_its_last_bytes() {
        let mut j = Journal::new(3);
        j.push(b"x");
        j.push(b"0123456789");
        assert_eq!((j.tail(), j.head()), (8, 11));
        assert_eq!(j.since(8), Some(b"789".to_vec()));
    }

    #[test]
    fn last_returns_at_most_the_newest_bytes() {
        let mut j = Journal::new(8);
        j.push(b"abcdef");
        assert_eq!(j.last(3), b"def".to_vec());
        assert_eq!(j.last(100), b"abcdef".to_vec());
        assert_eq!(j.last(0), Vec::<u8>::new());
    }
}
