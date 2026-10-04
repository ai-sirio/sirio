//! A CI job's log: the tail kept, the redirect fetched with no credential
//! (spec §4, §7.4, §15.2).

use crate::{ForgeError, LOG_DOWNLOAD_LIMIT, LOG_TAIL_BYTES, Log};

/// Fetches a log from the signed URL a forge redirected to — with no
/// credential, because the URL is its own authorisation and names another
/// host (spec §4). Over https only, except in a debug build, whose e2e fake
/// serves it on loopback.
pub(crate) fn fetch_signed(host: &str, location: &str) -> Result<Vec<u8>, ForgeError> {
    if !cfg!(debug_assertions) && !location.starts_with("https://") {
        return Err(ForgeError::UnexpectedResponse {
            host: host.to_string(),
            detail: "the log was offered over plain http".to_string(),
        });
    }
    let _perf = sirio_perf::span("forge.log_download", 0);
    let agent = crate::transport::http_agent(crate::transport::LOG_TIMEOUT);
    let response = agent
        .get(location)
        .header("User-Agent", "Sirio")
        .call()
        .map_err(|error| crate::transport::classify(host, error))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        // An expired signature is a 403 from the storage host, not the forge's.
        return Err(ForgeError::UnexpectedResponse {
            host: host.to_string(),
            detail: format!("the log's download answered HTTP {status}"),
        });
    }
    let mut body = response.into_body();
    body.with_config()
        .limit(LOG_DOWNLOAD_LIMIT)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => ForgeError::UnexpectedResponse {
                host: host.to_string(),
                detail: "the log is larger than 64 MiB; open it in the browser".to_string(),
            },
            other => crate::transport::classify(host, other),
        })
}

/// The log as the tab gets it: its tail, and what the forge said of the job.
pub(crate) fn finish(bytes: Vec<u8>, complete: bool, published: bool) -> Log {
    let (bytes, dropped) = keep_tail(bytes, LOG_TAIL_BYTES);
    Log { bytes, dropped, complete, published }
}

/// Keeps at most `cap` bytes from the end of `bytes`, starting at a line:
/// a cut that lands inside a line moves forward to the next one, so neither
/// half a line nor half an escape sequence is shown (spec §7.4). Only a
/// single line longer than `cap` is cut inside, at a character boundary.
/// Returns the kept bytes and how many were dropped from the start.
pub(crate) fn keep_tail(bytes: Vec<u8>, cap: usize) -> (Vec<u8>, u64) {
    if bytes.len() <= cap {
        return (bytes, 0);
    }
    let start = bytes.len() - cap;
    let cut = if bytes[start - 1] == b'\n' {
        start
    } else {
        match bytes[start..].iter().position(|&byte| byte == b'\n') {
            Some(offset) if start + offset + 1 < bytes.len() => start + offset + 1,
            _ => {
                // The kept part is one unfinished line: keep its end, from
                // the first whole character or escape sequence.
                let mut cut = start;
                let mut cursor = bytes[..start].iter().rposition(|&byte| byte == b'\n')
                    .map_or(0, |newline| newline + 1);
                while cursor < cut {
                    if bytes[cursor] != 0x1b {
                        cursor += 1;
                        continue;
                    }
                    cursor += 1;
                    match bytes.get(cursor) {
                        Some(b'[') => {
                            cursor += 1;
                            while cursor < bytes.len() && !(0x40..=0x7e).contains(&bytes[cursor]) {
                                cursor += 1;
                            }
                            cursor = (cursor + 1).min(bytes.len());
                        }
                        Some(b']' | b'P' | b'X' | b'^' | b'_') => {
                            let osc = bytes[cursor] == b']';
                            cursor += 1;
                            while cursor < bytes.len() {
                                if osc && bytes[cursor] == 0x07 {
                                    cursor += 1;
                                    break;
                                }
                                if bytes[cursor] == 0x1b && bytes.get(cursor + 1) == Some(&b'\\') {
                                    cursor += 2;
                                    break;
                                }
                                cursor += 1;
                            }
                        }
                        _ => {
                            while cursor < bytes.len() && (0x20..=0x2f).contains(&bytes[cursor]) {
                                cursor += 1;
                            }
                            cursor = (cursor + 1).min(bytes.len());
                        }
                    }
                    cut = cut.max(cursor);
                }
                while cut < bytes.len() && (bytes[cut] & 0b1100_0000) == 0b1000_0000 {
                    cut += 1;
                }
                cut
            }
        }
    };
    (bytes[cut..].to_vec(), cut as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_within_the_cap_is_kept_whole() {
        assert_eq!(keep_tail(b"a\nb\n".to_vec(), 4), (b"a\nb\n".to_vec(), 0));
        assert_eq!(keep_tail(b"a\nb\n".to_vec(), 100), (b"a\nb\n".to_vec(), 0));
        assert_eq!(keep_tail(Vec::new(), 4), (Vec::new(), 0));
    }

    #[test]
    fn a_cut_inside_a_line_moves_to_the_next_line() {
        // The last 9 of these 19 bytes start inside "second".
        let (kept, dropped) = keep_tail(b"first\nsecond\nthird\n".to_vec(), 9);
        assert_eq!(kept, b"third\n".to_vec());
        assert_eq!(dropped, 13);
    }

    #[test]
    fn a_cut_right_after_a_newline_keeps_that_line() {
        let (kept, dropped) = keep_tail(b"first\nsecond\n".to_vec(), 7);
        assert_eq!(kept, b"second\n".to_vec());
        assert_eq!(dropped, 6);
    }

    #[test]
    fn a_cut_inside_an_escape_or_a_multibyte_character_never_shows_half_of_it() {
        let log = "ok\n\x1b[31mrötten\x1b[0m\nlast\n";
        for cap in 1..log.len() {
            let (kept, _) = keep_tail(log.as_bytes().to_vec(), cap);
            let text = String::from_utf8(kept).expect("never half a character");
            // Once the cap holds the whole last line, the tail starts at a
            // line: never inside the colour sequence or the `ö`.
            if cap >= "last\n".len() {
                assert!(log.contains(&format!("\n{text}")) || text.is_empty(), "cap {cap}: {text:?}");
            }
        }
    }

    #[test]
    fn a_cut_inside_an_escape_on_an_overlong_line_starts_after_it() {
        assert_eq!(keep_tail(b"\x1b[31mred".to_vec(), 6), (b"red".to_vec(), 5));
        let osc = b"\x1b]8;;https://example.test\x07link";
        assert_eq!(keep_tail(osc.to_vec(), 9), (b"link".to_vec(), (osc.len() - 4) as u64));
        assert_eq!(keep_tail(b"\x1bPignored\x1b\\text".to_vec(), 8), (b"text".to_vec(), 11));
        assert_eq!(keep_tail(b"\x1b(Bxyz".to_vec(), 4), (b"xyz".to_vec(), 3));
        assert_eq!(keep_tail(b"\x1b[123".to_vec(), 2), (Vec::new(), 5));
    }

    #[test]
    fn one_line_longer_than_the_cap_keeps_its_end_from_a_whole_character() {
        let (kept, dropped) = keep_tail(b"abcdefghij".to_vec(), 4);
        assert_eq!(kept, b"ghij".to_vec());
        assert_eq!(dropped, 6);
        // "aé" is 3 bytes; the last 1 is the second half of `é`: keep nothing
        // rather than half a character.
        assert_eq!(keep_tail("aé".as_bytes().to_vec(), 1), (Vec::new(), 3));
    }
}
