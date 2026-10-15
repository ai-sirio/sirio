//! The payload of a `Data` frame (SP2 spec §3.3): a 13-byte header naming the
//! stream, the phase and the journal offset, then raw terminal bytes.

use crate::frame::MAX_PAYLOAD;

pub const DATA_HEADER_LEN: usize = 13;
pub const MAX_DATA_BYTES: usize = MAX_PAYLOAD - DATA_HEADER_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Snapshot = 0,
    Output = 1,
    Input = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataHeader {
    pub stream: u32,
    pub phase: Phase,
    pub offset: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DataError {
    /// Fewer bytes than a header: the count received.
    Short(usize),
    UnknownPhase(u8),
    /// More terminal bytes than one frame may carry: the count offered.
    TooLarge(usize),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Short(n) => write!(f, "data payload of {n} bytes is shorter than its header"),
            Self::UnknownPhase(p) => write!(f, "unknown data phase {p}"),
            Self::TooLarge(n) => write!(f, "{n} data bytes exceed one frame"),
        }
    }
}

impl std::error::Error for DataError {}

pub fn encode_data(header: &DataHeader, bytes: &[u8]) -> Result<Vec<u8>, DataError> {
    if bytes.len() > MAX_DATA_BYTES {
        return Err(DataError::TooLarge(bytes.len()));
    }
    let mut payload = Vec::with_capacity(DATA_HEADER_LEN + bytes.len());
    payload.extend_from_slice(&header.stream.to_be_bytes());
    payload.push(header.phase as u8);
    payload.extend_from_slice(&header.offset.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(payload)
}

pub fn decode_data(payload: &[u8]) -> Result<(DataHeader, &[u8]), DataError> {
    if payload.len() < DATA_HEADER_LEN {
        return Err(DataError::Short(payload.len()));
    }
    let stream = u32::from_be_bytes(payload[0..4].try_into().expect("4 bytes"));
    let phase = match payload[4] {
        0 => Phase::Snapshot,
        1 => Phase::Output,
        2 => Phase::Input,
        other => return Err(DataError::UnknownPhase(other)),
    };
    let offset = u64::from_be_bytes(payload[5..13].try_into().expect("8 bytes"));
    Ok((DataHeader { stream, phase, offset }, &payload[DATA_HEADER_LEN..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_output_chunk_is_these_exact_bytes() {
        let header = DataHeader { stream: 7, phase: Phase::Output, offset: 0x0102_0304_0506_0708 };
        let payload = encode_data(&header, b"hi").unwrap();
        assert_eq!(
            payload,
            [0, 0, 0, 7, 1, 1, 2, 3, 4, 5, 6, 7, 8, b'h', b'i'].to_vec()
        );
        assert_eq!(decode_data(&payload).unwrap(), (header, &b"hi"[..]));
    }

    #[test]
    fn an_empty_chunk_is_a_bare_header() {
        let header = DataHeader { stream: 1, phase: Phase::Snapshot, offset: 0 };
        let payload = encode_data(&header, b"").unwrap();
        assert_eq!(payload.len(), DATA_HEADER_LEN);
        assert_eq!(decode_data(&payload).unwrap(), (header, &b""[..]));
    }

    #[test]
    fn a_payload_shorter_than_the_header_is_refused() {
        assert_eq!(decode_data(&[0; 12]), Err(DataError::Short(12)));
        assert_eq!(decode_data(&[]), Err(DataError::Short(0)));
    }

    #[test]
    fn an_unknown_phase_is_refused() {
        let mut payload = encode_data(&DataHeader { stream: 1, phase: Phase::Input, offset: 0 }, b"x").unwrap();
        payload[4] = 3;
        assert_eq!(decode_data(&payload), Err(DataError::UnknownPhase(3)));
        payload[4] = 255;
        assert_eq!(decode_data(&payload), Err(DataError::UnknownPhase(255)));
    }

    #[test]
    fn more_bytes_than_a_frame_holds_are_refused_at_encode() {
        let header = DataHeader { stream: 1, phase: Phase::Output, offset: 0 };
        let bytes = vec![0u8; MAX_DATA_BYTES + 1];
        assert_eq!(encode_data(&header, &bytes), Err(DataError::TooLarge(MAX_DATA_BYTES + 1)));
        assert!(encode_data(&header, &bytes[..MAX_DATA_BYTES]).is_ok());
    }

    #[test]
    fn the_largest_encodable_payload_fits_a_frame() {
        let header = DataHeader { stream: u32::MAX, phase: Phase::Output, offset: u64::MAX };
        let payload = encode_data(&header, &vec![7u8; MAX_DATA_BYTES]).unwrap();
        assert!(crate::frame::encode(crate::frame::FrameKind::Data, &payload).is_ok());
    }
}
