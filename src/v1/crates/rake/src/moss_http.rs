//! Speaking to a moss over HTTP — the smallest honest client.
//!
//! LAN-only, axum on the far side: one GET, Content-Length bodies,
//! no TLS yet (pond security arrives at M2). Zero dependencies by
//! design (P5); revisited when TLS lands. Error taxonomy harvested
//! from the PoC (StoneError): connection failures are retryable,
//! response and processing failures are not.

use std::net::IpAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// How talking to a moss can fail — and whether that means "try another".
#[derive(Debug)]
pub enum AttachError {
    /// Never reached us (refused, timeout) — the stone might be gone.
    ConnectionFailed(String),
    /// Reached us; it answered with something other than 200.
    ResponseError(u16),
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
            Self::ResponseError(status) => write!(f, "HTTP {status}"),
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
    let connect = async {
        TcpStream::connect((ip, port))
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))
    };
    let mut stream = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| AttachError::ConnectionFailed("connect timed out".into()))??;

    let req = format!("GET {path} HTTP/1.1\r\nHost: {ip}:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n");
    let write = async {
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| AttachError::ConnectionFailed(e.to_string()))?;
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
    let text_err = |_| {
        AttachError::ProcessingError("response was not valid UTF-8".into())
    };
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
    if status != 200 {
        return Err(AttachError::ResponseError(status));
    }

    serde_json::from_str(body.trim())
        .map_err(|e| AttachError::ProcessingError(format!("body was not JSON: {e}")))
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
        assert!(
            matches!(parse_response(raw), Err(AttachError::ResponseError(404))),
            "expected ResponseError(404)"
        );
    }

    #[test]
    fn garbage_fails_as_processing() {
        assert!(matches!(
            parse_response(b"not http at all"),
            Err(AttachError::ProcessingError(_))
        ));
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
}
