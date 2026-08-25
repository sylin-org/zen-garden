//! Shared HTTP response helpers.

use anyhow::Result;

/// Maximum bytes of error body to include in diagnostics.
pub const ERROR_BODY_MAX: usize = 512;

/// Check HTTP response status. On error, logs and returns an `anyhow` error
/// with the truncated response body for diagnostics.
pub async fn check_response(resp: reqwest::Response, label: &str) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let body_summary = if body.len() > ERROR_BODY_MAX {
        format!("{}…", &body[..ERROR_BODY_MAX])
    } else {
        body
    };
    tracing::warn!(
        label = %label,
        status = %status,
        body = %body_summary,
        "upstream HTTP error"
    );
    anyhow::bail!("{label} HTTP {status}: {body_summary}")
}
