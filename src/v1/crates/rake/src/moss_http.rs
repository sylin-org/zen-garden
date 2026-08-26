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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    let connect = async {
        TcpStream::connect((ip, port))
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))
    };
    let mut stream = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| AttachError::ConnectionFailed("connect timed out".into()))??;

    let payload = body.map(serde_json::to_vec).transpose().map_err(|e| {
        AttachError::ProcessingError(format!("request body would not serialize: {e}"))
    })?;
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {ip}:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
    );
    if let Some(bytes) = &payload {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            bytes.len()
        ));
    }
    req.push_str("\r\n");

    let write = async {
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))?;
        if let Some(bytes) = &payload {
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

    parse_response(&raw)
}

/// Split status line / headers / body and parse. Pure — pinned by tests.
fn parse_response(raw: &[u8]) -> Result<serde_json::Value, AttachError> {
    let text_err = |_| AttachError::ProcessingError("response was not valid UTF-8".into());
    let full = std::str::from_utf8(raw).map_err(text_err)?;
    let (head, body) = full
        .split_once("\r\n\r\n")
        .ok_or_else(|| AttachError::ProcessingError("no header/body split".into()))?;

    let status_line = head.lines().next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AttachError::ProcessingError("unreadable status line".into()))?;

    let chunked = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .any(|(k, v)| {
            k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.trim().eq_ignore_ascii_case("chunked")
        });
    let decoded;
    let body: &str = if chunked {
        decoded = dechunk(body)?;
        &decoded
    } else {
        body.trim()
    };

    if status != 200 {
        return Err(AttachError::ResponseError(status, error_message(body)));
    }

    serde_json::from_str(body)
        .map_err(|e| AttachError::ProcessingError(format!("body was not JSON: {e}")))
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

/// Decode Transfer-Encoding: chunked framing into raw body bytes.
/// Hex sizes per RFC 9112 §7.1; trailers after the zero size are dropped.
fn dechunk(body: &str) -> Result<String, AttachError> {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let Some((size_line, tail)) = rest.split_once("\r\n") else {
            return Err(AttachError::ProcessingError(
                "chunked body ended mid-frame-size".into(),
            ));
        };
        let size = usize::from_str_radix(size_line.trim(), 16)
            .map_err(|_| AttachError::ProcessingError("unreadable chunk size".into()))?;
        if size == 0 {
            return Ok(out);
        }
        if tail.len() < size + 2 {
            return Err(AttachError::ProcessingError(
                "chunked body truncated inside frame".into(),
            ));
        }
        out.push_str(tail.get(..size).ok_or_else(|| {
            AttachError::ProcessingError("chunked body index broke UTF-8".into())
        })?);
        rest = tail
            .get(size..)
            .and_then(|t| t.strip_prefix("\r\n"))
            .ok_or_else(|| AttachError::ProcessingError("chunk missing terminator".into()))?;
    }
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
            "/api/v1/stone/offerings/redis",
            Some(&body),
            Duration::from_secs(3),
        )
        .await
        .unwrap();
        let seen = server.await.unwrap();

        assert_eq!(v["data"]["ok"], 1);
        assert!(seen.starts_with("POST /api/v1/stone/offerings/redis HTTP/1.1"));
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
