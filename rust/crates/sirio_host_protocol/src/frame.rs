//! The SRH1 frame: a fixed 12-byte header and a payload, on any byte stream
//! (spec §6.1). One connection carries every kind, so a single-stream
//! transport (SSH stdio, SP7) needs nothing more.

pub const MAGIC: [u8; 4] = *b"SRH1";
pub const HEADER_LEN: usize = 12;
/// Payload limit for control frames — the limit `sirio_control` enforces.
pub const MAX_CONTROL_PAYLOAD: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Request,
    Response,
    Event,
    /// Raw bytes. Reserved by major 1 so that SP2 can use it without a major;
    /// refused until a capability announces it.
    Data,
}

impl FrameKind {
    fn to_byte(self) -> u8 {
        match self {
            Self::Request => 1,
            Self::Response => 2,
            Self::Event => 3,
            Self::Data => 4,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Request),
            2 => Some(Self::Response),
            3 => Some(Self::Event),
            4 => Some(Self::Data),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    BadMagic([u8; 4]),
    UnknownKind(u8),
    NonZeroFlags(u8),
    NonZeroReserved(u16),
    TooLarge { len: u64, limit: usize },
    DataNotSupported,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMagic(m) => write!(f, "not an SRH1 frame (magic {m:?})"),
            Self::UnknownKind(k) => write!(f, "unknown frame kind {k}"),
            Self::NonZeroFlags(v) => write!(f, "frame flags must be zero, got {v}"),
            Self::NonZeroReserved(v) => write!(f, "frame reserved bits must be zero, got {v}"),
            Self::TooLarge { len, limit } => write!(f, "frame of {len} bytes exceeds {limit}"),
            Self::DataNotSupported => write!(f, "data frames are not enabled"),
        }
    }
}

impl std::error::Error for FrameError {}

pub fn encode(kind: FrameKind, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if kind == FrameKind::Data {
        return Err(FrameError::DataNotSupported);
    }
    if payload.len() > MAX_CONTROL_PAYLOAD {
        return Err(FrameError::TooLarge {
            len: payload.len() as u64,
            limit: MAX_CONTROL_PAYLOAD,
        });
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC);
    out.push(kind.to_byte());
    out.push(0);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Accumulates bytes from a stream and yields whole frames. Never buffers
/// more than one header plus the limit: an over-limit length is refused as
/// soon as the header is complete, a foreign protocol as soon as its first
/// bytes disagree with the magic.
#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes are held that do not yet form a frame — at end of stream this
    /// means the peer stopped mid-frame.
    pub fn has_partial(&self) -> bool {
        !self.buf.is_empty()
    }

    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        let seen = self.buf.len().min(MAGIC.len());
        if self.buf[..seen] != MAGIC[..seen] {
            let mut magic = [0u8; 4];
            magic[..seen].copy_from_slice(&self.buf[..seen]);
            return Err(FrameError::BadMagic(magic));
        }
        if self.buf.len() < HEADER_LEN {
            return Ok(None);
        }
        let kind_byte = self.buf[4];
        let kind = FrameKind::from_byte(kind_byte).ok_or(FrameError::UnknownKind(kind_byte))?;
        if kind == FrameKind::Data {
            return Err(FrameError::DataNotSupported);
        }
        if self.buf[5] != 0 {
            return Err(FrameError::NonZeroFlags(self.buf[5]));
        }
        let reserved = u16::from_be_bytes([self.buf[6], self.buf[7]]);
        if reserved != 0 {
            return Err(FrameError::NonZeroReserved(reserved));
        }
        let len =
            u32::from_be_bytes([self.buf[8], self.buf[9], self.buf[10], self.buf[11]]) as usize;
        if len > MAX_CONTROL_PAYLOAD {
            return Err(FrameError::TooLarge {
                len: len as u64,
                limit: MAX_CONTROL_PAYLOAD,
            });
        }
        if self.buf.len() < HEADER_LEN + len {
            return Ok(None);
        }
        let payload = self.buf[HEADER_LEN..HEADER_LEN + len].to_vec();
        self.buf.drain(..HEADER_LEN + len);
        Ok(Some(Frame { kind, payload }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: u8, flags: u8, reserved: u16, len: u32) -> Vec<u8> {
        let mut h = MAGIC.to_vec();
        h.push(kind);
        h.push(flags);
        h.extend_from_slice(&reserved.to_be_bytes());
        h.extend_from_slice(&len.to_be_bytes());
        h
    }

    #[test]
    fn each_control_kind_round_trips() {
        for kind in [FrameKind::Request, FrameKind::Response, FrameKind::Event] {
            let mut d = FrameDecoder::new();
            d.push(&encode(kind, b"{\"a\":1}").unwrap());
            assert_eq!(
                d.next_frame(),
                Ok(Some(Frame {
                    kind,
                    payload: b"{\"a\":1}".to_vec()
                }))
            );
            assert!(!d.has_partial());
        }
    }

    #[test]
    fn a_foreign_protocol_is_refused_from_its_first_bytes() {
        let mut d = FrameDecoder::new();
        d.push(b"{\"m");
        assert_eq!(d.next_frame(), Err(FrameError::BadMagic(*b"{\"m\0")));
    }

    #[test]
    fn a_truncated_header_waits_for_more() {
        let mut d = FrameDecoder::new();
        d.push(&header(1, 0, 0, 2)[..7]);
        assert_eq!(d.next_frame(), Ok(None));
        assert!(d.has_partial());
    }

    #[test]
    fn a_truncated_payload_waits_for_more() {
        let mut d = FrameDecoder::new();
        let mut bytes = header(1, 0, 0, 4);
        bytes.extend_from_slice(b"ab");
        d.push(&bytes);
        assert_eq!(d.next_frame(), Ok(None));
        d.push(b"cd");
        assert_eq!(d.next_frame().unwrap().unwrap().payload, b"abcd");
    }

    #[test]
    fn an_over_limit_length_is_refused_from_the_header_alone() {
        let mut d = FrameDecoder::new();
        d.push(&header(1, 0, 0, MAX_CONTROL_PAYLOAD as u32 + 1));
        assert_eq!(
            d.next_frame(),
            Err(FrameError::TooLarge {
                len: MAX_CONTROL_PAYLOAD as u64 + 1,
                limit: MAX_CONTROL_PAYLOAD
            })
        );
    }

    #[test]
    fn a_payload_of_exactly_the_limit_is_accepted() {
        let payload = vec![b'x'; MAX_CONTROL_PAYLOAD];
        let mut d = FrameDecoder::new();
        d.push(&encode(FrameKind::Event, &payload).unwrap());
        assert_eq!(
            d.next_frame().unwrap().unwrap().payload.len(),
            MAX_CONTROL_PAYLOAD
        );
    }

    #[test]
    fn an_unknown_kind_is_refused() {
        let mut d = FrameDecoder::new();
        d.push(&header(9, 0, 0, 0));
        assert_eq!(d.next_frame(), Err(FrameError::UnknownKind(9)));
    }

    #[test]
    fn data_frames_are_refused_in_major_one() {
        let mut d = FrameDecoder::new();
        d.push(&header(4, 0, 0, 0));
        assert_eq!(d.next_frame(), Err(FrameError::DataNotSupported));
        assert_eq!(
            encode(FrameKind::Data, b""),
            Err(FrameError::DataNotSupported)
        );
    }

    #[test]
    fn non_zero_flags_and_reserved_are_refused() {
        let mut d = FrameDecoder::new();
        d.push(&header(1, 1, 0, 0));
        assert_eq!(d.next_frame(), Err(FrameError::NonZeroFlags(1)));
        let mut d = FrameDecoder::new();
        d.push(&header(1, 0, 7, 0));
        assert_eq!(d.next_frame(), Err(FrameError::NonZeroReserved(7)));
    }

    #[test]
    fn two_frames_in_one_read_come_out_in_order() {
        let mut bytes = encode(FrameKind::Request, b"1").unwrap();
        bytes.extend(encode(FrameKind::Event, b"2").unwrap());
        let mut d = FrameDecoder::new();
        d.push(&bytes);
        assert_eq!(d.next_frame().unwrap().unwrap().payload, b"1");
        assert_eq!(d.next_frame().unwrap().unwrap().payload, b"2");
        assert_eq!(d.next_frame(), Ok(None));
    }

    #[test]
    fn one_frame_fed_byte_by_byte_comes_out_once() {
        let bytes = encode(FrameKind::Response, b"hello").unwrap();
        let mut d = FrameDecoder::new();
        let mut frames = Vec::new();
        for b in bytes {
            d.push(&[b]);
            if let Some(f) = d.next_frame().unwrap() {
                frames.push(f);
            }
        }
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].payload, b"hello");
    }

    #[test]
    fn encode_refuses_an_oversized_payload() {
        let payload = vec![0u8; MAX_CONTROL_PAYLOAD + 1];
        assert!(matches!(
            encode(FrameKind::Request, &payload),
            Err(FrameError::TooLarge { .. })
        ));
    }
}
