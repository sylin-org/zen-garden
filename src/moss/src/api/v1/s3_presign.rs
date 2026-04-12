//! Moss-native presigned URL generation and validation (STORAGE-0016 §2f)
//!
//! Uses HMAC-SHA256 with a pond-derived or stone-derived secret to generate
//! time-limited, operation-scoped access tokens for S3 objects.
//!
//! Secret derivation (two-tier):
//! - **Pond active**: `SHA256(ca_fingerprint + ":moss-presign-v1")` — garden-scoped,
//!   portable across stones. Presigned URLs survive storage migration.
//! - **No pond**: `SHA256(stone_id + ":moss-presign-v1")` — stone-scoped fallback.
//!
//! Token format: `HMAC-SHA256(secret, "{method}\n{bucket}/{key}\n{expires_timestamp}")`
//! URL format: `http://host:port/{bucket}/{key}?X-Moss-Token={token}&X-Moss-Expires={ts}`

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::debug;

use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Request body for presigned URL generation
#[derive(Debug, Deserialize)]
pub struct PresignRequest {
    pub bucket: String,
    pub key: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default = "default_expires")]
    pub expires_in_secs: u64,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_expires() -> u64 {
    3600
}

/// Response with the presigned URL
#[derive(Debug, Serialize)]
pub struct PresignResponse {
    pub url: String,
    pub expires_at: String,
}

/// Generate a presigned URL for an S3 object
pub async fn generate_presigned_url(
    State(state): State<AppState>,
    Json(req): Json<PresignRequest>,
) -> Response {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(req.expires_in_secs as i64);
    let expires_ts = expires_at.timestamp();

    let secret = derive_presign_secret(&state).await;
    let token = compute_token(&secret, &req.method, &req.bucket, &req.key, expires_ts);

    // Determine the S3 endpoint for the URL
    let s3_port = {
        let catalog = state
            .orchestration
            .storage
            .s3_listeners
            .port_catalog()
            .await;
        catalog.values().next().copied()
    };

    let host = &state.current.stone.name;
    let url = if let Some(port) = s3_port {
        format!(
            "http://{}.local:{}/{}/{}?X-Moss-Token={}&X-Moss-Expires={}",
            host,
            port,
            urlencoding::encode(&req.bucket),
            urlencoding::encode(&req.key),
            urlencoding::encode(&token),
            expires_ts,
        )
    } else {
        format!(
            "http://{}.local:{}/api/v1/storage/s3/{}/{}?X-Moss-Token={}&X-Moss-Expires={}",
            host,
            state.current.api_port,
            urlencoding::encode(&req.bucket),
            urlencoding::encode(&req.key),
            urlencoding::encode(&token),
            expires_ts,
        )
    };

    debug!(
        bucket = %req.bucket,
        key = %req.key,
        method = %req.method,
        expires_at = %expires_at.to_rfc3339(),
        "Presigned URL generated"
    );

    let resp = PresignResponse {
        url,
        expires_at: expires_at.to_rfc3339(),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

/// Validate a presigned token on an incoming S3 request.
///
/// Returns `Ok(())` if the token is valid and not expired, or `Err(reason)`.
pub fn validate_presign_token(
    secret: &[u8],
    method: &str,
    bucket: &str,
    key: &str,
    token: &str,
    expires_ts: i64,
) -> Result<(), &'static str> {
    let now = chrono::Utc::now().timestamp();
    if now > expires_ts {
        return Err("Token expired");
    }

    let expected = compute_token(secret, method, bucket, key, expires_ts);

    if !constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        return Err("Invalid token");
    }

    Ok(())
}

/// Resolve pond-or-stone key material for cryptographic derivation.
///
/// Two-tier:
/// - **Pond active + CA fingerprint available**: CA fingerprint (garden-scoped,
///   portable across stones in the pond).
/// - **Fallback**: stone_id (stone-scoped).
///
/// Used by both presigned URL generation and S3 credential derivation.
pub(crate) async fn resolve_key_material(state: &AppState) -> String {
    if state.security.pond_active()
        && let Ok(handle) = state.discovery.koi().certmesh()
        && let Ok(core) = handle.core()
    {
        let status = core.certmesh_status().await;
        if let Some(ref fp) = status.ca_fingerprint
            && !fp.is_empty()
        {
            debug!("Key material: pond CA fingerprint (garden-scoped)");
            return fp.clone();
        }
    }

    debug!("Key material: stone_id (stone-scoped)");
    state.current.stone.id.clone()
}

/// Derive the HMAC secret for presigned URLs from AppState.
///
/// Two-tier:
/// - **Pond active + CA fingerprint available**: garden-scoped secret from CA fingerprint.
///   Presigned URLs are portable across all stones in the pond.
/// - **Fallback**: stone-scoped secret from stone_id.
pub async fn derive_presign_secret(state: &AppState) -> Vec<u8> {
    let material = resolve_key_material(state).await;
    derive_secret_from_material(&material)
}

/// Derive HMAC secret from arbitrary key material.
fn derive_secret_from_material(material: &str) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(material.as_bytes());
    hasher.update(b":moss-presign-v1");
    hasher.finalize().to_vec()
}

/// Compute the HMAC token for a presigned URL
fn compute_token(secret: &[u8], method: &str, bucket: &str, key: &str, expires_ts: i64) -> String {
    let message = format!(
        "{}\n{}/{}\n{}",
        method.to_uppercase(),
        bucket,
        key,
        expires_ts
    );
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC key length");
    mac.update(message.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_deterministic() {
        let secret = derive_secret_from_material("test-stone-id");
        let t1 = compute_token(&secret, "GET", "bucket", "key.txt", 1710000000);
        let t2 = compute_token(&secret, "GET", "bucket", "key.txt", 1710000000);
        assert_eq!(t1, t2);
    }

    #[test]
    fn token_varies_by_method() {
        let secret = derive_secret_from_material("test-stone-id");
        let get = compute_token(&secret, "GET", "bucket", "key.txt", 1710000000);
        let put = compute_token(&secret, "PUT", "bucket", "key.txt", 1710000000);
        assert_ne!(get, put);
    }

    #[test]
    fn token_varies_by_key() {
        let secret = derive_secret_from_material("test-stone-id");
        let t1 = compute_token(&secret, "GET", "bucket", "a.txt", 1710000000);
        let t2 = compute_token(&secret, "GET", "bucket", "b.txt", 1710000000);
        assert_ne!(t1, t2);
    }

    #[test]
    fn token_varies_by_expiry() {
        let secret = derive_secret_from_material("test-stone-id");
        let t1 = compute_token(&secret, "GET", "bucket", "key.txt", 1710000000);
        let t2 = compute_token(&secret, "GET", "bucket", "key.txt", 1710000001);
        assert_ne!(t1, t2);
    }

    #[test]
    fn validate_valid_token() {
        let secret = derive_secret_from_material("stone-123");
        let expires = chrono::Utc::now().timestamp() + 3600;
        let token = compute_token(&secret, "GET", "b", "k", expires);
        assert!(validate_presign_token(&secret, "GET", "b", "k", &token, expires).is_ok());
    }

    #[test]
    fn validate_expired_token() {
        let secret = derive_secret_from_material("stone-123");
        let expires = chrono::Utc::now().timestamp() - 1;
        let token = compute_token(&secret, "GET", "b", "k", expires);
        assert_eq!(
            validate_presign_token(&secret, "GET", "b", "k", &token, expires),
            Err("Token expired")
        );
    }

    #[test]
    fn validate_wrong_token() {
        let secret = derive_secret_from_material("stone-123");
        let expires = chrono::Utc::now().timestamp() + 3600;
        assert_eq!(
            validate_presign_token(&secret, "GET", "b", "k", "bad-token", expires),
            Err("Invalid token")
        );
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn pond_secret_differs_from_stone_secret() {
        let stone = derive_secret_from_material("stone-id-abc");
        let pond = derive_secret_from_material("ca:sha256:deadbeef...");
        assert_ne!(stone, pond);
    }

    #[test]
    fn same_ca_fingerprint_same_secret_across_stones() {
        // Two different stones with the same CA fingerprint should derive the same secret
        let s1 = derive_secret_from_material("ca:sha256:same-fingerprint");
        let s2 = derive_secret_from_material("ca:sha256:same-fingerprint");
        assert_eq!(s1, s2);
    }
}
