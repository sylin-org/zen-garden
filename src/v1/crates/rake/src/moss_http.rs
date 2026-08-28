//! Speaking to a moss over HTTP — the smallest honest client.
//!
//! LAN-only, axum on the far side: Content-Length bodies, no TLS yet
//! (pond security arrives at M2). Zero dependencies by design (P5);
//! revisited when TLS lands. Error taxonomy harvested from the PoC
//! (StoneError): connection failures are retryable, response and
//! processing failures are not. Non-200 answers carry the moss's error
//! envelope message when present — rake speaks moss's refusals, not bare codes.

use std::net::IpAddr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How talking to a moss can fail — and whether that means "try another".
#[derive(Debug)]
pub enum AttachError {
    /// Never reached us (refused, timeout) — the stone might be gone.
    ConnectionFailed(String),
    /// Reached us; it answered with something other than 200. The moss's
    /// error message rides along when the body carried one.
    ResponseError(u16, String),
    /// Reached us; the body was not what standard formats promised (L21).
    ProcessingError(String),
}

impl AttachError {
    pub fn is_connection_failed(&self) -> bool {
        matches!(self, Self::ConnectionFailed(_))
    }
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(e) => write!(f, "connection failed: {e}"),
            Self::ResponseError(status, msg) if msg.is_empty() => write!(f, "HTTP {status}"),
            Self::ResponseError(status, msg) => write!(f, "HTTP {status}: {msg}"),
            Self::ProcessingError(e) => write!(f, "unexpected response: {e}"),
        }
    }
}

/// GET `path` from a moss and parse the JSON body.
pub async fn get_json(
    ip: IpAddr,
    port: u16,
    path: &str,
    timeout: Duration,
) -> Result<serde_json::Value, AttachError> {
    request_json("GET", ip, port, path, None, timeout).await
}

/// One HTTP request with an optional JSON body to a moss; parse the JSON
/// answer. POST/DELETE ride this alongside GET — the verbs' full surface.
pub async fn request_json(
    method: &str,
    ip: IpAddr,
    port: u16,
    path: &str,
    body: Option<&serde_json::Value>,
    timeout: Duration,
) -> Result<serde_json::Value, AttachError> {
    let payload = body.map(serde_json::to_vec).transpose().map_err(|e| {
        AttachError::ProcessingError(format!("request body would not serialize: {e}"))
    })?;
    let (status, raw) = request_bytes(
        method,
        ip,
        port,
        path,
        Some("application/json"),
        payload.as_deref(),
        timeout,
    )
    .await?;
    json_from_parts(status, &raw)
}

/// Status + body bytes → JSON answer or honest `AttachError`. The one
/// interpretation shared by the JSON and raw transports.
fn json_from_parts(status: u16, body: &[u8]) -> Result<serde_json::Value, AttachError> {
    if status != 200 {
        return Err(AttachError::ResponseError(
            status,
            error_message(&String::from_utf8_lossy(body)),
        ));
    }
    serde_json::from_slice(body)
        .map_err(|e| AttachError::ProcessingError(format!("body was not JSON: {e}")))
}

/// One HTTP request carrying raw bytes (or none), answering the status
/// and the raw body — the file faces' transport (bytes in, bytes out;
/// JSON is never assumed). Every status rides home; the CALLER decides
/// what a non-200 means for its verb.
pub async fn request_bytes(
    method: &str,
    ip: IpAddr,
    port: u16,
    path: &str,
    content_type: Option<&str>,
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<(u16, Vec<u8>), AttachError> {
    let connect = async {
        TcpStream::connect((ip, port))
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))
    };
    let mut stream = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| AttachError::ConnectionFailed("connect timed out".into()))??;

    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {ip}:{port}\r\nConnection: close\r\n"
    );
    if let Some(ct) = content_type {
        req.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    if let Some(bytes) = body {
        req.push_str(&format!("Content-Length: {}\r\n", bytes.len()));
    }
    req.push_str("\r\n");

    let write = async {
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))?;
        if let Some(bytes) = body {
            stream
                .write_all(bytes)
                .await
                .map_err(|e| AttachError::ConnectionFailed(e.to_string()))?;
        }
        stream
            .flush()
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))?;
        Ok::<(), AttachError>(())
    };
    tokio::time::timeout(timeout, write)
        .await
        .map_err(|_| AttachError::ConnectionFailed("write timed out".into()))??;

    let read = async {
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(timeout, read)
        .await
        .map_err(|_| AttachError::ConnectionFailed("read timed out".into()))?
        .map_err(AttachError::ConnectionFailed)?;

    parse_response_bytes(&raw)
}

/// Split status line / headers / body at the byte level and hand back
/// `(status, body)` — binary-safe (the file faces carry payloads that
/// are not text). Pure — pinned by tests.
fn parse_response_bytes(raw: &[u8]) -> Result<(u16, Vec<u8>), AttachError> {
    let Some(split) = find(raw, b"\r\n\r\n") else {
        return Err(AttachError::ProcessingError("no header/body split".into()));
    };
    let (head, body) = (&raw[..split], &raw[split + 4..]);

    let head = std::str::from_utf8(head)
        .map_err(|_| AttachError::ProcessingError("unreadable headers".into()))?;
    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AttachError::ProcessingError("unreadable status line".into()))?;

    let chunked = head.lines().skip(1).filter_map(|l| l.split_once(':')).any(
        |(k, v)| {
            k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.trim().eq_ignore_ascii_case("chunked")
        },
    );
    let body = if chunked {
        dechunk_bytes(body)?
    } else {
        body.to_vec()
    };
    Ok((status, body))
}

/// The raw→JSON pipeline the tests pin: transport bytes in, one answer
/// out. Production callers ride `request_json`/`request_bytes`; this is
/// the pure composition of [`parse_response_bytes`] + [`json_from_parts`].
#[cfg(test)]
fn parse_response(raw: &[u8]) -> Result<serde_json::Value, AttachError> {
    let (status, body) = parse_response_bytes(raw)?;
    json_from_parts(status, &body)
}

/// Pull the moss's refusal out of its standard error envelope; bodies
/// without one contribute their text verbatim (proxies like a 502 page).
fn error_message(body: &str) -> String {
    let trimmed = body.trim();
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => v["error"]["message"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        _ => {
            // Not the envelope — keep a short honest excerpt of what did come.
            const CAP: usize = 120;
            trimmed.chars().take(CAP).collect()
        }
    }
}

/// First index of `needle` inside `haystack`, byte-honest.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Decode Transfer-Encoding: chunked framing into raw body bytes.
/// Hex sizes per RFC 9112 §7.1; trailers after the zero size are dropped.
/// Byte-level: file payloads are not text and survive whole.
fn dechunk_bytes(mut body: &[u8]) -> Result<Vec<u8>, AttachError> {
    let mut out = Vec::new();
    loop {
        let Some(pos) = find(body, b"\r\n") else {
            return Err(AttachError::ProcessingError(
                "chunked body ended mid-frame-size".into(),
            ));
        };
        let size_line = std::str::from_utf8(&body[..pos])
            .map_err(|_| AttachError::ProcessingError("unreadable chunk size".into()))?;
        let size = usize::from_str_radix(size_line.trim(), 16)
            .map_err(|_| AttachError::ProcessingError("unreadable chunk size".into()))?;
        if size == 0 {
            return Ok(out);
        }
        let start = pos + 2;
        if body.len() < start + size + 2 {
            return Err(AttachError::ProcessingError(
                "chunked body truncated inside frame".into(),
            ));
        }
        out.extend_from_slice(&body[start..start + size]);
        if &body[start + size..start + size + 2] != b"\r\n" {
            return Err(AttachError::ProcessingError(
                "chunk missing terminator".into(),
            ));
        }
        body = &body[start + size + 2..];
    }
}

// ---- long-lived streams (the watch faces' transport) ------------------------

/// Why a stream did not start. `Refused` carries the RAW body so the
/// caller can read structured refusals (the not-here redirect lives in
/// there, not just a message).
#[derive(Debug)]
pub enum StreamError {
    Connection(String),
    Refused { status: u16, body: Vec<u8> },
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(e) => write!(f, "connection failed: {e}"),
            Self::Refused { status, body } => {
                let text = String::from_utf8_lossy(body);
                match serde_json::from_str::<serde_json::Value>(text.trim()) {
                    Ok(v) if v["error"]["message"].is_string() => {
                        write!(f, "{}", v["error"]["message"].as_str().unwrap_or_default())
                    }
                    _ => write!(f, "HTTP {status}"),
                }
            }
        }
    }
}

/// A live SSE connection: hands back one `data:` payload at a time,
/// until the moss ends the stream or the caller hangs up.
pub struct SseStream {
    reader: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
}

impl SseStream {
    /// The next SSE `data:` payload, or `None` when the connection ends.
    /// Multi-`data:` events arrive joined with newlines.
    pub async fn next_data(&mut self) -> Option<Vec<u8>> {
        let mut data: Option<Vec<u8>> = None;
        loop {
            let mut raw = Vec::new();
            let n = self
                .reader
                .read_until(b'\n', &mut raw)
                .await
                .ok()?;
            if n == 0 {
                return None; // EOF: the moss hung up
            }
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = line.strip_prefix("data:") {
                let payload = rest.trim_start().as_bytes().to_vec();
                match &mut data {
                    Some(d) => {
                        d.push(b'\n');
                        d.extend_from_slice(&payload);
                    }
                    None => data = Some(payload),
                }
            } else if line.is_empty() {
                // The event separator: a buffered payload is complete.
                if data.is_some() {
                    return data;
                }
            }
            // event:/id:/retry:/comment lines carry nothing we need.
        }
    }
}

/// Open a long-lived SSE stream. Connect, request, read the head with a
/// budget — then read the body forever (follow semantics; the budget
/// does NOT apply to the stream itself).
pub async fn open_stream(
    ip: IpAddr,
    port: u16,
    path: &str,
    connect_timeout: Duration,
) -> Result<SseStream, StreamError> {
    let connect = async {
        TcpStream::connect((ip, port))
            .await
            .map_err(|e| StreamError::Connection(e.to_string()))
    };
    let stream = tokio::time::timeout(connect_timeout, connect)
        .await
        .map_err(|_| StreamError::Connection("connect timed out".into()))?
        .map_err(|e| StreamError::Connection(e.to_string()))?;

    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {ip}:{port}\r\nAccept: text/event-stream\r\nConnection: close\r\n\r\n"
    );
    let (read_half, mut write_half) = stream.into_split();
    let write = async {
        write_half
            .write_all(req.as_bytes())
            .await
            .map_err(|e| StreamError::Connection(e.to_string()))?;
        write_half
            .flush()
            .await
            .map_err(|e| StreamError::Connection(e.to_string()))
    };
    tokio::time::timeout(connect_timeout, write)
        .await
        .map_err(|_| StreamError::Connection("write timed out".into()))?
        .map_err(|e| StreamError::Connection(e.to_string()))?;

    // The head: status line + headers, line by line, under the budget.
    use tokio::io::{AsyncBufReadExt, BufReader};
    let head_read = async {
        let mut reader = BufReader::new(read_half);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).await.map_err(|e| StreamError::Connection(e.to_string()))?;
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| StreamError::Connection("unreadable status line".into()))?;
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.map_err(|e| StreamError::Connection(e.to_string()))?;
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
        Ok::<_, StreamError>((status, reader))
    };
    let (status, reader) = tokio::time::timeout(connect_timeout, head_read)
        .await
        .map_err(|_| StreamError::Connection("head read timed out".into()))??;

    if status != 200 {
        let mut reader = reader;
        let mut body = Vec::new();
        // Refusals are small envelopes; drain what came (best effort).
        let _ = reader.read_until(b'\0', &mut body).await;
        return Err(StreamError::Refused { status, body });
    }
    Ok(SseStream { reader })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn parses_ok_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"data\":1}";
        let v = parse_response(raw).unwrap();
        assert_eq!(v["data"], 1);
    }

    #[test]
    fn non_200_is_a_response_error_not_retryable_body() {
        let raw = b"HTTP/1.1 404 Not Found\r\n\r\nnope";
        let err = parse_response(raw).unwrap_err();
        assert!(
            matches!(&err, AttachError::ResponseError(404, m) if m == "nope"),
            "expected ResponseError(404) with the raw excerpt, got {err:?}"
        );
    }

    #[test]
    fn refusal_envelope_message_surfaces() {
        let raw = b"HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\n\r\n{\"error\":{\"message\":\"'redis' is already planted\"}}";
        let err = parse_response(raw).unwrap_err();
        assert!(
            matches!(
                &err,
                AttachError::ResponseError(409, m) if m.contains("already planted")
            ),
            "envelope message should surface in the error, got {err:?}"
        );
    }

    #[test]
    fn garbage_fails_as_processing() {
        assert!(matches!(
            parse_response(b"not http at all"),
            Err(AttachError::ProcessingError(_))
        ));
    }

    #[test]
    fn dechunks_framed_bodies() {
        // `{"d":` is 5 bytes; `1}\n` is 3.
        let raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"d\":\r\n3\r\n1}\n\r\n0\r\n\r\n";
        let v = parse_response(raw).unwrap();
        assert_eq!(v["d"], 1);
    }

    #[test]
    fn dechunk_keeps_multibyte_characters_whole() {
        // `"öhä"` is 7 bytes, not 4 characters' worth of code points.
        let raw =
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n\"öhä\"\r\n0\r\n\r\n".as_bytes();
        let v = parse_response(raw).unwrap();
        assert_eq!(v, serde_json::json!("öhä"));
    }

    /// End-to-end against a real socket: canned moss answers, we parse.
    #[tokio::test]
    async fn get_json_over_real_socket() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await;
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n{\"ok\":true}")
                .await
                .unwrap();
        });

        let v = get_json(Ipv4Addr::LOCALHOST.into(), port, "/health", Duration::from_secs(3))
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(v["ok"], true);
    }

    // ---- the raw transport (the file faces' pipes) ----------------------

    /// Binary payloads survive whole: status and bytes come home apart.
    #[test]
    fn binary_bodies_survive_with_content_length() {
        let payload: Vec<u8> = (0..=255u8).chain([0, 255, 0x0d, 0x0a]).collect();
        let mut raw = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            payload.len()
        )
        .into_bytes();
        raw.extend_from_slice(&payload);

        let (status, body) = parse_response_bytes(&raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, payload, "every byte, including NULs and CRLFs");
    }

    /// Chunked framing decodes at the byte level — binary chunks too.
    #[test]
    fn dechunk_bytes_keeps_binary_whole() {
        // Chunk 1 is 4 bytes including an embedded CRLF; chunk 2 is 0xFF.
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    4\r\na\r\nb\r\n1\r\n\xff\r\n0\r\n\r\n";
        let (status, body) = parse_response_bytes(raw).unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, vec![b'a', b'\r', b'\n', b'b', 0xff]);
    }

    /// Non-200 rides home with its status — the caller decides (a file
    /// GET wants to raise 404 as "not on the bank", not as a transport
    /// fault).
    #[test]
    fn non_200_rides_home_with_status_and_body() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\r\n\
                    {\"error\":{\"message\":\"nothing answers at 'x' on this bank\"}}";
        let (status, body) = parse_response_bytes(raw).unwrap();
        assert_eq!(status, 404);
        let msg = error_message(&String::from_utf8_lossy(&body));
        assert!(msg.contains("nothing answers"), "{msg}");
    }

    /// POST flows carry method, body headers and the payload itself.
    #[tokio::test]
    async fn post_sends_method_body_and_lengths() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap();
            let got = String::from_utf8_lossy(&buf[..n]).into_owned();
            sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"data\":{\"ok\":1}}")
                .await
                .unwrap();
            got
        });

        let body = serde_json::json!({ "ports": { "default": 6379 } });
        let v = request_json(
            "POST",
            Ipv4Addr::LOCALHOST.into(),
            port,
            "/api/v1/offerings/redis",
            Some(&body),
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        let seen = server.await.unwrap();

        assert_eq!(v["data"]["ok"], 1);
        assert!(seen.starts_with("POST /api/v1/offerings/redis HTTP/1.1"));
        assert!(seen.contains("Content-Type: application/json"));
        let clen: usize = seen
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .and_then(|s| s.trim().parse().ok())
            .unwrap();
        let sent_body = seen.split_once("\r\n\r\n").unwrap().1;
        assert_eq!(clen, sent_body.len());
        assert_eq!(sent_body, body.to_string());
    }
}
