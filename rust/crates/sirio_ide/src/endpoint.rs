//! The endpoint Claude connects to: a fresh auth token and a loopback listener.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::ops::RangeInclusive;

use socket2::{Domain, Socket, Type};

/// The ports a free one is drawn from when the saved one is taken.
pub const PORT_RANGE: RangeInclusive<u16> = 10000..=65535;

const RANDOM_TRIES: usize = 64;

/// Bind 127.0.0.1:`port`, failing when the port is taken.
///
/// On unix the socket sets `SO_REUSEADDR`, so a port held only by the TIME_WAIT of a connection
/// Sirio closed first stays bindable across a quick relaunch (Claude retries for only 15-25 s).
/// That option would also let a `127.0.0.1` bind succeed next to another process listening on
/// `*:port` on macOS, so first a connect to the port must be refused: any other outcome counts as
/// taken. On Windows neither the option nor the probe is used (there the option lets a socket take
/// a port in active use). The gap between the probe and the bind is a race that is accepted.
fn listen(port: u16) -> io::Result<TcpListener> {
    #[cfg(unix)]
    {
        let probe = std::net::TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            std::time::Duration::from_millis(250),
        );
        match probe {
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {}
            _ => return Err(io::Error::new(io::ErrorKind::AddrInUse, "port is taken")),
        }
    }
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}

/// 128 bits from the OS CSPRNG as 32 lowercase hex characters. Never log it.
pub fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Listen on 127.0.0.1 only: the saved port when it is in `PORT_RANGE` and free, else a random
/// free one in `PORT_RANGE` (up to 64 tries).
pub fn bind(saved: Option<u16>) -> io::Result<TcpListener> {
    if let Some(port) = saved.filter(|p| PORT_RANGE.contains(p))
        && let Ok(l) = listen(port)
    {
        return Ok(l);
    }
    let span = u32::from(*PORT_RANGE.end() - *PORT_RANGE.start()) + 1;
    let mut last = io::Error::new(io::ErrorKind::AddrInUse, "no free port in range");
    for _ in 0..RANDOM_TRIES {
        let mut b = [0u8; 4];
        getrandom::fill(&mut b).map_err(io::Error::other)?;
        let port = *PORT_RANGE.start() + (u32::from_le_bytes(b) % span) as u16;
        match listen(port) {
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
    fn saved_port_path_also_listens_on_loopback_only() {
        let probe = bind(None).unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let l = bind(Some(port)).unwrap();
        assert_eq!(l.local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn saved_port_is_refused_while_another_listens_on_any_address() {
        use socket2::{Domain, Socket, Type};
        let held = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
        held.bind(&std::net::SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)).into()).unwrap();
        held.listen(8).unwrap();
        let taken = held.local_addr().unwrap().as_socket().unwrap().port();
        let l = bind(Some(taken)).unwrap();
        assert_ne!(l.local_addr().unwrap().port(), taken);
    }

    #[cfg(unix)]
    #[test]
    fn saved_port_survives_a_lingering_close() {
        let l = bind(None).unwrap();
        let port = l.local_addr().unwrap().port();
        let client = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let (accepted, _) = l.accept().unwrap();
        drop(accepted);
        drop(client);
        drop(l);
        let again = bind(Some(port)).unwrap();
        assert_eq!(again.local_addr().unwrap().port(), port);
    }

    #[test]
    fn out_of_range_saved_port_is_ignored() {
        let free_low = (9000u16..10000)
            .find(|p| TcpListener::bind((Ipv4Addr::LOCALHOST, *p)).is_ok())
            .expect("a free port below the range");
        let l = bind(Some(free_low)).unwrap();
        assert!(PORT_RANGE.contains(&l.local_addr().unwrap().port()));
        let l = bind(Some(0)).unwrap();
        assert!(PORT_RANGE.contains(&l.local_addr().unwrap().port()));
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
