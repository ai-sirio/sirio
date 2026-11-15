//! A socket whose expired timeouts read as `WouldBlock`.

use std::io::{self, Read, Write};

/// Wraps a stream so an expired read or write timeout is `ErrorKind::WouldBlock` on every
/// platform. Windows reports it as `TimedOut`, which tungstenite's handshake takes for a failure
/// (it resumes only on `WouldBlock`).
pub struct WouldBlockOnTimeout<S>(pub S);

fn would_block(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::TimedOut { io::ErrorKind::WouldBlock.into() } else { e }
}

/// Run `op` again while it fails with `Interrupted`: a signal (a child's SIGCHLD) cuts a socket
/// call short even with `SA_RESTART`, and tungstenite reads only once, so it would drop the
/// connection.
fn retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match op() {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => return other.map_err(would_block),
        }
    }
}

impl<S: Read> Read for WouldBlockOnTimeout<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        retry(|| self.0.read(buf))
    }
}

impl<S: Write> Write for WouldBlockOnTimeout<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        retry(|| self.0.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        retry(|| self.0.flush())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fails(io::ErrorKind);
    impl Read for Fails {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(self.0.into())
        }
    }

    #[test]
    fn timed_out_read_is_reported_as_would_block() {
        let mut buf = [0u8; 4];
        let e = WouldBlockOnTimeout(Fails(io::ErrorKind::TimedOut)).read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
        let e = WouldBlockOnTimeout(Fails(io::ErrorKind::ConnectionReset)).read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::ConnectionReset);
    }

    #[test]
    fn interrupted_read_is_retried() {
        struct InterruptedOnce(bool);
        impl Read for InterruptedOnce {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    buf[..2].copy_from_slice(b"ok");
                    return Ok(2);
                }
                self.0 = true;
                Err(io::ErrorKind::Interrupted.into())
            }
        }
        let mut buf = [0u8; 4];
        assert_eq!(WouldBlockOnTimeout(InterruptedOnce(false)).read(&mut buf).unwrap(), 2);
        assert_eq!(&buf[..2], b"ok");
    }

    #[test]
    fn data_and_eof_pass_through_unchanged() {
        let mut buf = [0u8; 4];
        let mut s = WouldBlockOnTimeout(&b"ab"[..]);
        assert_eq!(s.read(&mut buf).unwrap(), 2);
        assert_eq!(&buf[..2], b"ab");
        assert_eq!(s.read(&mut buf).unwrap(), 0);
    }
}
