//! The endpoint Claude connects to: a fresh auth token and a loopback listener.

use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::ops::RangeInclusive;

/// The ports a free one is drawn from when the saved one is taken.
pub const PORT_RANGE: RangeInclusive<u16> = 10000..=65535;

const RANDOM_TRIES: usize = 64;

/// 128 bits from the OS CSPRNG as 32 lowercase hex characters. Never log it.
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Listen on 127.0.0.1 only: the saved port when it is free, else a random free one in
/// `PORT_RANGE` (up to 64 tries).
pub fn bind(saved: Option<u16>) -> io::Result<TcpListener> {
    if let Some(port) = saved {
        if let Ok(l) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            return Ok(l);
        }
    }
    let span = u32::from(*PORT_RANGE.end() - *PORT_RANGE.start()) + 1;
    let mut last = io::Error::new(io::ErrorKind::AddrInUse, "no free port in range");
    for _ in 0..RANDOM_TRIES {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).map_err(io::Error::other)?;
        let port = *PORT_RANGE.start() + (u32::from_le_bytes(b) % span) as u16;
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(l) => return Ok(l),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, TcpListener};

    #[test]
    fn token_is_32_lowercase_hex() {
        let t = new_token().unwrap();
        assert_eq!(t.len(), 32);
        assert!(t.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn two_tokens_differ() {
        assert_ne!(new_token().unwrap(), new_token().unwrap());
    }

    #[test]
    fn bind_listens_on_ipv4_loopback_only() {
        let l = bind(None).unwrap();
        assert_eq!(l.local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(PORT_RANGE.contains(&l.local_addr().unwrap().port()));
    }

    #[test]
    fn bind_reuses_a_free_saved_port() {
        let probe = bind(None).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let l = bind(Some(port)).unwrap();
        assert_eq!(l.local_addr().unwrap().port(), port);
    }

    #[test]
    fn bind_moves_when_saved_port_is_taken() {
        let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let taken = held.local_addr().unwrap().port();
        let l = bind(Some(taken)).unwrap();
        let got = l.local_addr().unwrap().port();
        assert_ne!(got, taken);
        assert!(PORT_RANGE.contains(&got));
    }
}
