//! Moss volume API client — the bridge between the orchestrator's
//! local dependency cache and a remote ComfyUI instance (ORCH-0029).
//!
//! Moss exposes each offering's mounted volume as an HTTP
//! filesystem. This module wraps the three verbs the provisioner
//! needs:
//!
//! - `HEAD {moss}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}`
//!   — existence check, 5-second timeout, boolean result.
//! - `PUT  {moss}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}`
//!   — streaming upload from a local file, `Content-Length` set,
//!   **no global timeout** (large files need sustained throughput —
//!   TCP keepalive detects dead connections instead).
//! - `PUT`-a-small-buffer variant for skill JSON files (Tier 3
//!   persistence).
//!
//! All endpoints are scoped to a single offering FQN and volume
//! name. For ComfyUI that's `fqn = "comfyui"` and `volume =
//! "comfyui-models"`.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;

use garden_common::constants::MOSS_HTTP;

/// The Moss volume the ComfyUI offering mounts for models and skill
/// definitions.
pub const COMFYUI_MODELS_VOLUME: &str = "comfyui-models";

/// Short timeout for HEAD probes — the provisioner fires these
/// against every required model on every instance during discovery,
/// and a slow stone shouldn't block the whole pass.
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);

/// Build the full volume URL for a given path. Path segments are
/// NOT url-encoded — the caller is responsible for producing a
/// safe `{model_type}/{filename}` string.
fn volume_url(moss_endpoint: &str, fqn: &str, volume: &str, path: &str) -> String {
    let base = moss_endpoint.trim_end_matches('/');
    format!("{base}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}")
}

/// Derive the Moss HTTP endpoint from a service endpoint like
/// `http://stone-crystal:8188`. Replaces the port with Moss's
/// well-known port (7185). Used by the ComfyUI adapter to turn a
/// discovered ComfyUI URL into the matching Moss URL for volume
/// operations.
pub fn derive_moss_endpoint(service_endpoint: &str) -> String {
    let trimmed = service_endpoint.trim_end_matches('/');
    if let Some(colon_pos) = trimmed.rfind(':') {
        // Only rewrite the port if what follows the last `:` is a
        // plain number. Otherwise we're looking at an IPv6 host or
        // a scheme slash and the heuristic fails — fall through.
        let after = &trimmed[colon_pos + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}:{MOSS_HTTP}", &trimmed[..colon_pos]);
        }
    }
    format!("{trimmed}:{MOSS_HTTP}")
}

/// Check whether a file exists on the remote volume via HEAD.
/// Returns `false` on any network error or non-success response —
/// the caller treats that as "not present" and will re-upload.
pub async fn file_exists(
    http: &Client,
    moss_endpoint: &str,
    fqn: &str,
    volume: &str,
    path: &str,
) -> bool {
    let url = volume_url(moss_endpoint, fqn, volume, path);
    match http.head(&url).timeout(HEAD_TIMEOUT).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Stream a local file to the remote volume via PUT.
///
/// Opens the source file, wraps it in a `ReaderStream`, sets
/// `Content-Length` from `metadata()`, and POSTs it. **No global
/// timeout** — multi-gigabyte checkpoints need throughput, not a
/// wall-clock deadline. The underlying TCP keepalive catches dead
/// connections.
pub async fn push_file_streaming(
    http: &Client,
    moss_endpoint: &str,
    fqn: &str,
    volume: &str,
    remote_path: &str,
    local_path: &Path,
) -> Result<()> {
    let file_size = tokio::fs::metadata(local_path)
        .await
        .with_context(|| format!("stat local file {}", local_path.display()))?
        .len();

    let url = volume_url(moss_endpoint, fqn, volume, remote_path);
    tracing::info!(
        url = %url,
        bytes = file_size,
        "moss_volume: streaming PUT to instance"
    );

    let file = tokio::fs::File::open(local_path)
        .await
        .with_context(|| format!("open local file {}", local_path.display()))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = reqwest::Body::wrap_stream(stream);

    let resp = http
        .put(&url)
        .header(reqwest::header::CONTENT_LENGTH, file_size)
        .body(body)
        .send()
        .await
        .with_context(|| format!("PUT {url}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("moss volume PUT failed HTTP {status}: {text}");
    }

    tracing::info!(
        remote_path,
        bytes = file_size,
        "moss_volume: PUT complete"
    );
    Ok(())
}

/// Push a small in-memory buffer to the remote volume. Used by the
/// Tier-3 skill persistence code to upload `skill.json` and workflow
/// template files alongside the cached models.
pub async fn push_bytes(
    http: &Client,
    moss_endpoint: &str,
    fqn: &str,
    volume: &str,
    remote_path: &str,
    contents: &[u8],
) -> Result<()> {
    let url = volume_url(moss_endpoint, fqn, volume, remote_path);
    let resp = http
        .put(&url)
        .header(reqwest::header::CONTENT_LENGTH, contents.len())
        .body(contents.to_vec())
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("PUT {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("moss volume PUT (bytes) failed HTTP {status}: {text}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_url_composes() {
        assert_eq!(
            volume_url("http://stone:7185", "comfyui", "comfyui-models", "checkpoints/a.pth"),
            "http://stone:7185/api/v1/stone/offerings/comfyui/volumes/comfyui-models/checkpoints/a.pth"
        );
        // Trailing slash on the endpoint is stripped.
        assert_eq!(
            volume_url("http://stone:7185/", "comfyui", "comfyui-models", "foo"),
            "http://stone:7185/api/v1/stone/offerings/comfyui/volumes/comfyui-models/foo"
        );
    }

    #[test]
    fn derive_moss_replaces_service_port() {
        assert_eq!(
            derive_moss_endpoint("http://stone-crystal:8188"),
            format!("http://stone-crystal:{MOSS_HTTP}")
        );
        assert_eq!(
            derive_moss_endpoint("http://stone-crystal:8188/"),
            format!("http://stone-crystal:{MOSS_HTTP}")
        );
        // No port → append Moss port.
        assert_eq!(
            derive_moss_endpoint("http://stone-crystal"),
            format!("http://stone-crystal:{MOSS_HTTP}")
        );
    }
}
