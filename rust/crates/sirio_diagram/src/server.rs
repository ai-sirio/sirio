//! The PlantUML server fallback (design §3): `GET {server}/svg/{encoded}`,
//! the plantuml-server protocol that plantuml.com and self-hosted instances
//! both speak. Used only when the local command is not available and a
//! server is configured, because it sends the source off the machine.

use std::time::Duration;

use crate::{DiagramError, Svg, encode, plantuml_document, svg};

pub(crate) const SERVER_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn render_remote(
    source: &str,
    server: &str,
    timeout: Duration,
) -> Result<Svg, DiagramError> {
    let url = format!(
        "{}/svg/{}",
        server.trim().trim_end_matches('/'),
        encode::encode(&plantuml_document(source))
    );
    let response = ureq::get(&url)
        .config()
        .timeout_global(Some(timeout))
        // A 4xx carries the diagram's error in its headers: keep the response.
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| match error {
            ureq::Error::Timeout(_) => DiagramError::Timeout {
                seconds: timeout.as_secs(),
            },
            other => DiagramError::Io(format!("the PlantUML server is unreachable: {other}")),
        })?;

    let status = response.status().as_u16();
    if status != 200 {
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        return Err(match header("X-PlantUML-Diagram-Error") {
            Some(message) => DiagramError::Syntax {
                message,
                line: header("X-PlantUML-Diagram-Error-Line")
                    .and_then(|line| line.trim().parse::<u32>().ok()),
            },
            None => DiagramError::Server {
                status,
                message: response
                    .status()
                    .canonical_reason()
                    .unwrap_or("error")
                    .to_string(),
            },
        });
    }

    let not_svg = || DiagramError::Server {
        status,
        message: "the response is not an SVG".into(),
    };
    let mut body = response.into_body();
    let text = body
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => DiagramError::Server {
                status,
                message: "the response is larger than 2 MiB".into(),
            },
            other => DiagramError::Io(other.to_string()),
        })?;
    let head = text.trim_start();
    if !(head.starts_with("<svg") || head.starts_with("<?xml")) {
        return Err(not_svg());
    }
    svg::as_rendered(&text).ok_or_else(not_svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    /// Serves one canned response on loopback and hands back the request line.
    fn serve_once(response: Vec<u8>) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(1) => head.push(byte[0]),
                    _ => break,
                }
            }
            let _ = stream.write_all(&response);
            String::from_utf8_lossy(&head)
                .lines()
                .next()
                .unwrap_or_default()
                .to_string()
        });
        (base, handle)
    }

    fn http(status: &str, headers: &[&str], body: &str) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\n");
        for header in headers {
            out.push_str(header);
            out.push_str("\r\n");
        }
        out.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ));
        out.into_bytes()
    }

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"30\" height=\"10\"></svg>";

    #[test]
    fn a_200_svg_is_the_diagram_and_the_request_carries_the_encoded_source() {
        let (base, server) = serve_once(http("200 OK", &["Content-Type: image/svg+xml"], SVG));
        let svg = render_remote("A -> B", &format!("{base}/"), SERVER_TIMEOUT).expect("renders");
        assert_eq!((svg.logical_width, svg.logical_height), (30, 10));
        let request = server.join().expect("server");
        let path = request
            .strip_prefix("GET /svg/")
            .and_then(|rest| rest.split(' ').next())
            .expect("a GET under /svg/, the trailing slash of the base trimmed");
        assert_eq!(
            crate::encode::decode(path).as_deref(),
            Some("@startuml\nA -> B\n@enduml\n")
        );
    }

    #[test]
    fn a_400_with_error_headers_is_a_syntax_error_at_its_line() {
        let (base, _server) = serve_once(http(
            "400 Bad Request",
            &[
                "X-PlantUML-Diagram-Error: Syntax Error?",
                "X-PlantUML-Diagram-Error-Line: 2",
            ],
            "",
        ));
        assert_eq!(
            render_remote("A ->", &base, SERVER_TIMEOUT),
            Err(DiagramError::Syntax {
                message: "Syntax Error?".into(),
                line: Some(2)
            })
        );
    }

    #[test]
    fn a_failure_without_headers_carries_the_status() {
        let (base, _server) = serve_once(http("500 Internal Server Error", &[], ""));
        assert_eq!(
            render_remote("A -> B", &base, SERVER_TIMEOUT),
            Err(DiagramError::Server {
                status: 500,
                message: "Internal Server Error".into()
            })
        );
    }

    #[test]
    fn a_body_over_2_mib_is_refused() {
        let body = format!(
            "<svg width=\"1\" height=\"1\">{}</svg>",
            "x".repeat(2 * 1024 * 1024)
        );
        let (base, _server) = serve_once(http("200 OK", &[], &body));
        match render_remote("A -> B", &base, SERVER_TIMEOUT) {
            Err(DiagramError::Server {
                status: 200,
                message,
            }) => assert!(message.contains("2 MiB")),
            other => panic!("expected a refused body, got {other:?}"),
        }
    }

    #[test]
    fn a_body_that_is_not_svg_is_refused() {
        let (base, _server) = serve_once(http("200 OK", &[], "<html>nope</html>"));
        assert_eq!(
            render_remote("A -> B", &base, SERVER_TIMEOUT),
            Err(DiagramError::Server {
                status: 200,
                message: "the response is not an SVG".into()
            })
        );
    }
}
