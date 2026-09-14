//! The `Content-Length` envelope, and the only thing separating this crate
//! from `sirio_acp`'s line-delimited reader.
//!
//! ```text
//! Content-Length: 245\r\n
//! \r\n
//! {"jsonrpc":"2.0","id":1,"result":{…}}
//! ```
//!
//! Headers are read line by line to the blank line, then **exactly** that
//! many bytes. Reading by lines instead would work on every small fixture
//! and break on the first hover carrying markdown.

use futures::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::LspError;

const CONTENT_LENGTH: &str = "content-length:";

pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<(), LspError> {
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|error| LspError::Transport(format!("writing header: {error}")))?;
    writer
        .write_all(payload)
        .await
        .map_err(|error| LspError::Transport(format!("writing payload: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| LspError::Transport(format!("flushing: {error}")))?;
    Ok(())
}

pub async fn read_message<R: AsyncBufRead + AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, LspError> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|error| LspError::Transport(format!("reading header: {error}")))?;
        if read == 0 {
            return Err(LspError::Transport("stream ended before a header".into()));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        // Header names are case-insensitive; some servers send
        // `Content-Length`, the spec's examples also show lower case.
        if trimmed.to_ascii_lowercase().starts_with(CONTENT_LENGTH) {
            let value = trimmed[CONTENT_LENGTH.len()..].trim();
            length = Some(value.parse().map_err(|_| {
                LspError::Transport(format!("Content-Length is not a number: {value:?}"))
            })?);
        }
    }

    let length = length.ok_or_else(|| {
        LspError::Transport("a message arrived with no Content-Length header".into())
    })?;
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| LspError::Transport(format!("reading {length} byte payload: {error}")))?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;

    #[test]
    fn a_written_message_reads_back_identically() {
        futures::executor::block_on(async {
            let mut buffer = Vec::new();
            write_message(&mut buffer, br#"{"jsonrpc":"2.0"}"#).await.unwrap();

            assert_eq!(
                String::from_utf8(buffer.clone()).unwrap(),
                "Content-Length: 17\r\n\r\n{\"jsonrpc\":\"2.0\"}"
            );

            let mut reader = futures::io::BufReader::new(Cursor::new(buffer));
            let payload = read_message(&mut reader).await.unwrap();
            assert_eq!(payload, br#"{"jsonrpc":"2.0"}"#);
        });
    }

    #[test]
    fn a_payload_containing_newlines_survives_framing() {
        // This is the case a line-delimited reader gets wrong, and it is
        // not exotic: hover documentation is markdown with real newlines.
        futures::executor::block_on(async {
            let payload = b"{\"doc\":\"line one\nline two\n\"}";
            let mut buffer = Vec::new();
            write_message(&mut buffer, payload).await.unwrap();

            let mut reader = futures::io::BufReader::new(Cursor::new(buffer));
            assert_eq!(read_message(&mut reader).await.unwrap(), payload);
        });
    }

    #[test]
    fn two_messages_in_one_stream_read_in_order() {
        futures::executor::block_on(async {
            let mut buffer = Vec::new();
            write_message(&mut buffer, b"{\"n\":1}").await.unwrap();
            write_message(&mut buffer, b"{\"n\":2}").await.unwrap();

            let mut reader = futures::io::BufReader::new(Cursor::new(buffer));
            assert_eq!(read_message(&mut reader).await.unwrap(), b"{\"n\":1}");
            assert_eq!(read_message(&mut reader).await.unwrap(), b"{\"n\":2}");
        });
    }

    #[test]
    fn a_message_without_a_content_length_is_a_transport_error() {
        futures::executor::block_on(async {
            let stream = b"X-Other: 1\r\n\r\n{}".to_vec();
            let mut reader = futures::io::BufReader::new(Cursor::new(stream));
            let error = read_message(&mut reader).await.unwrap_err();
            assert!(
                matches!(error, LspError::Transport(detail) if detail.contains("no Content-Length")),
                "expected a transport error naming the missing header"
            );
        });
    }

    #[test]
    fn a_non_numeric_content_length_is_a_transport_error() {
        futures::executor::block_on(async {
            let stream = b"Content-Length: many\r\n\r\n{}".to_vec();
            let mut reader = futures::io::BufReader::new(Cursor::new(stream));
            assert!(matches!(
                read_message(&mut reader).await.unwrap_err(),
                LspError::Transport(_)
            ));
        });
    }

    #[test]
    fn a_truncated_payload_is_a_transport_error_rather_than_a_short_read() {
        // The header promises 50 bytes and the stream holds 2. Returning
        // the 2 would hand a half-message to serde and produce a confusing
        // parse error far from the real cause.
        futures::executor::block_on(async {
            let stream = b"Content-Length: 50\r\n\r\n{}".to_vec();
            let mut reader = futures::io::BufReader::new(Cursor::new(stream));
            assert!(matches!(
                read_message(&mut reader).await.unwrap_err(),
                LspError::Transport(_)
            ));
        });
    }

    #[test]
    fn a_lower_case_header_name_is_accepted() {
        futures::executor::block_on(async {
            let stream = b"content-length: 2\r\n\r\n{}".to_vec();
            let mut reader = futures::io::BufReader::new(Cursor::new(stream));
            assert_eq!(read_message(&mut reader).await.unwrap(), b"{}");
        });
    }
}
