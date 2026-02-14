//! Pond security API handlers
//!
//! Translates zen-garden Pond vocabulary to koi-certmesh operations.
//! All cryptographic work is delegated to certmesh — these handlers
//! are a thin domain-vocabulary facade.
//!
//! Security model:
//! - CA (keystone) creation requires a passphrase
//! - Enrollment uses TOTP codes shared out-of-band
//! - After initialization, the CA starts locked on reboot; unlock required
//! - All cert lifecycle managed by certmesh (issue, renew, revoke)

use crate::api::responses::ApiResponse;
use crate::{error_response, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use garden_common::api_utils::ApiErrorResponse;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tower::ServiceExt;

// ============================================================================
// Request types
// ============================================================================

#[derive(Deserialize)]
pub struct PondInitRequest {
    /// Passphrase to encrypt the CA private key at rest
    pub passphrase: String,
    /// Trust profile: "just-me" (default), "my-team", "my-organization"
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Deserialize)]
pub struct PondJoinRequest {
    /// TOTP code from the authenticator app
    pub code: String,
    /// Override hostname for this enrollment (default: system hostname)
    #[serde(default)]
    pub hostname: Option<String>,
    /// Additional SANs for the certificate (e.g. extra IPs)
    #[serde(default)]
    pub sans: Vec<String>,
}

#[derive(Deserialize)]
pub struct PondUnlockRequest {
    /// Passphrase to decrypt the CA private key
    pub passphrase: String,
}

#[derive(Deserialize)]
pub struct PondInviteRequest {
    /// Passphrase needed to rotate the enrollment auth
    pub passphrase: String,
    /// Enrollment window duration in minutes (default: 30)
    #[serde(default)]
    pub ttl_minutes: Option<u64>,
}

#[derive(Deserialize)]
pub struct PondPromoteRequest {
    /// Passphrase for CA key decryption during promotion
    pub passphrase: String,
}

// ============================================================================
// Response types
// ============================================================================

#[derive(Serialize)]
pub struct PondInitResponse {
    pub cornerstone: String,
    pub keystone_path: String,
    pub certificate_expires: String,
    pub status: String,
    pub totp_uri: Option<String>,
    pub ca_fingerprint: String,
}

#[derive(Serialize)]
pub struct PondInviteResponse {
    pub totp_uri: String,
    pub expires_at: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub inviter_stone: String,
    pub enrollment_state: String,
}

#[derive(Serialize)]
pub struct PondJoinResponse {
    pub stone_name: String,
    pub cornerstone: Option<String>,
    pub certificate_expires: String,
    pub status: String,
    pub ca_fingerprint: String,
}

#[derive(Serialize)]
pub struct PondStatusResponse {
    pub active: bool,
    pub locked: bool,
    pub cornerstone: Option<String>,
    pub stones: Vec<PondStoneInfo>,
    pub profile: String,
    pub ca_fingerprint: Option<String>,
    pub enrollment_state: String,
}

#[derive(Serialize)]
pub struct PondStoneInfo {
    pub name: String,
    pub role: String,
    pub status: String,
    pub certificate_expires: String,
    pub joined_at: Option<String>,
}

// ============================================================================
// Common result type
// ============================================================================

type PondResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiErrorResponse>)>;

/// Map koi CertmeshError to HTTP error response
fn certmesh_err(e: koi_certmesh::CertmeshError) -> (StatusCode, Json<ApiErrorResponse>) {
    use koi_certmesh::CertmeshError;
    match e {
        CertmeshError::CaNotInitialized => error_response(
            StatusCode::CONFLICT,
            "POND_NOT_INITIALIZED",
            "Pond not initialized. Run 'garden-rake place keystone' first.",
            None,
        ),
        CertmeshError::CaLocked => error_response(
            StatusCode::LOCKED,
            "POND_LOCKED",
            "Pond is locked. Run 'garden-rake pond unlock' after restart.",
            None,
        ),
        CertmeshError::InvalidAuth => error_response(
            StatusCode::UNAUTHORIZED,
            "INVALID_AUTH",
            "Invalid passphrase or TOTP code.",
            None,
        ),
        CertmeshError::RateLimited { remaining_secs } => error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED",
            format!("Too many failed attempts. Try again in {remaining_secs}s."),
            None,
        ),
        CertmeshError::EnrollmentClosed => error_response(
            StatusCode::FORBIDDEN,
            "ENROLLMENT_CLOSED",
            "Enrollment is closed. Ask the keystone operator to run 'garden-rake pond invite'.",
            None,
        ),
        CertmeshError::AlreadyEnrolled(hostname) => error_response(
            StatusCode::CONFLICT,
            "ALREADY_ENROLLED",
            format!("Stone '{hostname}' is already enrolled in the pond."),
            None,
        ),
        CertmeshError::NotFound(what) => error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("Not found: {what}"),
            None,
        ),
        CertmeshError::Revoked(hostname) => error_response(
            StatusCode::FORBIDDEN,
            "REVOKED",
            format!("Stone '{hostname}' has been revoked from the pond."),
            None,
        ),
        CertmeshError::ApprovalDenied => error_response(
            StatusCode::FORBIDDEN,
            "APPROVAL_DENIED",
            "Enrollment request was denied by the operator.",
            None,
        ),
        _ => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CERTMESH_ERROR",
            format!("Certmesh operation failed: {e}"),
            None,
        ),
    }
}

/// Get the CertmeshCore from AppState, returning appropriate HTTP errors
fn get_certmesh_core(
    state: &AppState,
) -> Result<std::sync::Arc<koi_certmesh::CertmeshCore>, (StatusCode, Json<ApiErrorResponse>)> {
    let handle = state.koi_handle.certmesh().map_err(|e| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "CERTMESH_UNAVAILABLE",
            format!("Certmesh subsystem not available: {e}"),
            None,
        )
    })?;
    handle.core().map_err(|e| {
        error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "CERTMESH_CORE_UNAVAILABLE",
            format!("Certmesh core not ready: {e}"),
            None,
        )
    })
}

/// Refresh the pond_active flag from certmesh state
async fn refresh_pond_active(state: &AppState) {
    if let Ok(handle) = state.koi_handle.certmesh() {
        if let Ok(core) = handle.core() {
            let status = core.certmesh_status().await;
            let active = status.ca_initialized && !status.ca_locked;
            state.pond_active.store(active, Ordering::Relaxed);
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /api/v1/pond/init — Place keystone (create CA)
///
/// Creates the certificate authority for this garden. The calling stone
/// becomes the cornerstone (primary CA holder). Returns a TOTP URI for
/// the authenticator app — this is used to authorize future stone enrollments.
pub async fn pond_init_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondInitRequest>,
) -> PondResult<PondInitResponse> {
    let core = get_certmesh_core(&state)?;

    // Translate trust profile from pond vocabulary
    let profile = match payload.profile.as_deref() {
        Some("just-me") | Some("1") | None => koi_certmesh::profiles::TrustProfile::JustMe,
        Some("my-team") | Some("2") => koi_certmesh::profiles::TrustProfile::MyTeam,
        Some("my-organization") | Some("3") => koi_certmesh::profiles::TrustProfile::MyOrganization,
        Some(other) => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_PROFILE",
                format!(
                    "Unknown trust profile: '{other}'. Valid: just-me, my-team, my-organization"
                ),
                None,
            ))
        }
    };

    // Generate cryptographic entropy for CA creation
    let entropy = {
        use rand::RngCore;
        let mut buf = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut buf);
        hex::encode(buf)
    };

    // Call the certmesh create handler via its HTTP routes (in-process, no network).
    // CA creation logic lives exclusively in certmesh's HTTP handler to avoid
    // divergence between two code paths, so we invoke it via tower::Service.
    let create_req = koi_certmesh::protocol::CreateCaRequest {
        passphrase: payload.passphrase.clone(),
        entropy_hex: entropy,
        profile,
        operator: None,
        enrollment_open: None,
        requires_approval: None,
    };

    let body = serde_json::to_vec(&create_req).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZE_ERROR",
            format!("Failed to build certmesh request: {e}"),
            None,
        )
    })?;

    let http_req = axum::http::Request::builder()
        .method("POST")
        .uri("/create")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .expect("valid request");

    let response = core
        .http_routes()
        .oneshot(http_req)
        .await
        .expect("Router is infallible");

    let status_code = response.status();
    let resp_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RESPONSE_READ_ERROR",
                format!("Failed to read certmesh response: {e}"),
                None,
            )
        })?;

    if !status_code.is_success() {
        let error_text = String::from_utf8_lossy(&resp_bytes);
        tracing::error!(status = %status_code, body = %error_text, "Certmesh CA creation failed");
        return Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CA_CREATION_FAILED",
            format!("Failed to create CA: {error_text}"),
            None,
        ));
    }

    let create_resp: koi_certmesh::protocol::CreateCaResponse = serde_json::from_slice(&resp_bytes)
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "PARSE_ERROR",
                format!("Failed to parse certmesh response: {e}"),
                None,
            )
        })?;

    // Extract TOTP URI from auth setup
    let totp_uri = match &create_resp.auth_setup {
        koi_crypto::auth::AuthSetup::Totp { totp_uri } => Some(totp_uri.clone()),
        _ => None,
    };

    // Update pond state
    refresh_pond_active(&state).await;

    // Re-register mDNS with pond TXT properties
    if let Some(ref mdns) = state.mdns_handle {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            let _ = mdns.reregister(&ip, mac.as_deref()).await;
        }
    }

    tracing::info!(
        cornerstone = %state.stone_name,
        profile = ?profile,
        fingerprint = %create_resp.ca_fingerprint,
        "Pond initialized — keystone placed"
    );

    Ok(Json(ApiResponse::new(PondInitResponse {
        cornerstone: state.stone_name.clone(),
        keystone_path: koi_certmesh::ca::ca_dir().display().to_string(),
        certificate_expires: "30 days".to_string(),
        status: "active".to_string(),
        totp_uri,
        ca_fingerprint: create_resp.ca_fingerprint,
    })))
}

/// GET /api/v1/pond/status — Get pond status and membership
pub async fn pond_status_v1(State(state): State<AppState>) -> PondResult<PondStatusResponse> {
    let core = get_certmesh_core(&state)?;
    let status = core.certmesh_status().await;

    let active = status.ca_initialized && !status.ca_locked;
    let cornerstone = status
        .members
        .iter()
        .find(|m| m.role == "primary")
        .map(|m| m.hostname.clone());

    let stones: Vec<PondStoneInfo> = status
        .members
        .iter()
        .map(|m| PondStoneInfo {
            name: m.hostname.clone(),
            role: m.role.clone(),
            status: m.status.clone(),
            certificate_expires: m.cert_expires.clone(),
            joined_at: None,
        })
        .collect();

    Ok(Json(ApiResponse::new(PondStatusResponse {
        active,
        locked: status.ca_initialized && status.ca_locked,
        cornerstone,
        stones,
        profile: format!("{:?}", status.profile),
        ca_fingerprint: status.ca_fingerprint,
        enrollment_state: format!("{:?}", status.enrollment_state),
    })))
}

/// POST /api/v1/pond/join — Join pond with TOTP code
///
/// This stone enrolls into the pond by providing a valid TOTP code.
/// On success, the stone receives a certificate issued by the CA.
pub async fn pond_join_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondJoinRequest>,
) -> PondResult<PondJoinResponse> {
    let core = get_certmesh_core(&state)?;

    let hostname = payload.hostname.unwrap_or_else(|| state.stone_name.clone());

    let join_req = koi_certmesh::protocol::JoinRequest {
        hostname: hostname.clone(),
        auth: koi_crypto::auth::AuthResponse::Totp { code: payload.code },
        sans: payload.sans,
    };

    let join_resp = core.enroll(&join_req).await.map_err(certmesh_err)?;

    // Update pond state
    refresh_pond_active(&state).await;

    // Re-register mDNS with pond TXT properties
    if let Some(ref mdns) = state.mdns_handle {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            let _ = mdns.reregister(&ip, mac.as_deref()).await;
        }
    }

    tracing::info!(
        stone = %join_resp.hostname,
        fingerprint = %join_resp.ca_fingerprint,
        "Stone enrolled in pond"
    );

    // Determine cornerstone from the roster
    let status = core.certmesh_status().await;
    let cornerstone = status
        .members
        .iter()
        .find(|m| m.role == "primary")
        .map(|m| m.hostname.clone());

    Ok(Json(ApiResponse::new(PondJoinResponse {
        stone_name: join_resp.hostname,
        cornerstone,
        certificate_expires: "30 days".to_string(),
        status: "active".to_string(),
        ca_fingerprint: join_resp.ca_fingerprint,
    })))
}

/// POST /api/v1/pond/invite — Open enrollment and rotate auth
///
/// Rotates the TOTP secret (generating a fresh one), opens the enrollment
/// window for a limited duration, and returns the new TOTP URI for the
/// invitee to add to their authenticator app.
pub async fn pond_invite_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondInviteRequest>,
) -> PondResult<PondInviteResponse> {
    let core = get_certmesh_core(&state)?;

    // Rotate auth to get a fresh TOTP URI
    let auth_setup = core
        .rotate_auth(&payload.passphrase, None)
        .await
        .map_err(certmesh_err)?;

    let totp_uri = match auth_setup {
        koi_crypto::auth::AuthSetup::Totp { totp_uri } => totp_uri,
        _ => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "UNEXPECTED_AUTH_METHOD",
                "Expected TOTP auth setup but got a different method.",
                None,
            ))
        }
    };

    // Open enrollment window with deadline
    let ttl_minutes = payload.ttl_minutes.unwrap_or(30);
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(ttl_minutes as i64);
    core.open_enrollment(Some(deadline))
        .await
        .map_err(certmesh_err)?;

    tracing::info!(
        inviter = %state.stone_name,
        ttl_minutes = ttl_minutes,
        "Pond enrollment opened with fresh invitation"
    );

    Ok(Json(ApiResponse::new(PondInviteResponse {
        totp_uri,
        expires_at: Some(deadline.to_rfc3339()),
        ttl_seconds: Some(ttl_minutes * 60),
        inviter_stone: state.stone_name.clone(),
        enrollment_state: "open".to_string(),
    })))
}

/// POST /api/v1/pond/unlock — Unlock pond after restart
///
/// After a reboot, the CA private key is locked (encrypted at rest).
/// This endpoint decrypts the key so certmesh can issue/renew certificates.
pub async fn pond_unlock_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondUnlockRequest>,
) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    core.unlock(&payload.passphrase)
        .await
        .map_err(certmesh_err)?;

    // Update pond state
    refresh_pond_active(&state).await;

    // Re-register mDNS with pond TXT properties
    if let Some(ref mdns) = state.mdns_handle {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            let _ = mdns.reregister(&ip, mac.as_deref()).await;
        }
    }

    tracing::info!("Pond unlocked — CA key decrypted");

    Ok(Json(ApiResponse::new(serde_json::json!({
        "unlocked": true,
    }))))
}

/// DELETE /api/v1/pond — Drain the pond (destroy CA)
///
/// Irreversibly destroys the CA and all certificates.
/// All enrolled stones lose their trust relationship.
pub async fn pond_remove_v1(State(state): State<AppState>) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    core.destroy().await.map_err(certmesh_err)?;

    // Update pond state
    state.pond_active.store(false, Ordering::Relaxed);

    // Re-register mDNS without pond TXT properties
    if let Some(ref mdns) = state.mdns_handle {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            let _ = mdns.reregister(&ip, mac.as_deref()).await;
        }
    }

    tracing::warn!("Pond drained — CA destroyed, all certificates invalidated");

    Ok(Json(ApiResponse::new(serde_json::json!({
        "destroyed": true,
    }))))
}

/// DELETE /api/v1/pond/stones/{stone_name} — Untrust a stone (revoke certificate)
pub async fn pond_untrust_v1(
    State(state): State<AppState>,
    Path(stone_name): Path<String>,
) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    core.revoke_member(&stone_name, None, Some("Untrusted via API".to_string()))
        .await
        .map_err(certmesh_err)?;

    tracing::info!(stone = %stone_name, "Stone revoked from pond");

    Ok(Json(ApiResponse::new(serde_json::json!({
        "revoked": true,
        "stone_name": stone_name,
    }))))
}

/// POST /api/v1/pond/promote — Promote this stone to standby CA
pub async fn pond_promote_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondPromoteRequest>,
) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    let _resp = core
        .promote(&payload.passphrase)
        .await
        .map_err(certmesh_err)?;

    tracing::info!(
        stone = %state.stone_name,
        "Stone promoted — received CA key material"
    );

    Ok(Json(ApiResponse::new(serde_json::json!({
        "promoted": true,
        "ca_fingerprint": koi_certmesh::ca::ca_fingerprint_from_disk()
            .unwrap_or_else(|_| "unavailable".to_string()),
    }))))
}

/// GET /api/v1/pond/ca.pem — Download CA public certificate
///
/// Serves the CA public certificate for manual trust installation
/// on non-enrolled machines (e.g., browsers, phones).
pub async fn pond_ca_cert_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, [(String, String); 1], String), (StatusCode, Json<ApiErrorResponse>)> {
    let core = get_certmesh_core(&state)?;
    let status = core.certmesh_status().await;

    if !status.ca_initialized {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "POND_NOT_INITIALIZED",
            "No pond CA exists. Run 'garden-rake place keystone' first.",
            None,
        ));
    }

    // Read CA cert from disk
    let ca_cert_path = koi_certmesh::ca::ca_cert_path();
    let ca_pem = tokio::fs::read_to_string(&ca_cert_path)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CA_READ_ERROR",
                format!("Failed to read CA certificate: {e}"),
                None,
            )
        })?;

    Ok((
        StatusCode::OK,
        [(
            "content-type".to_string(),
            "application/x-pem-file".to_string(),
        )],
        ca_pem,
    ))
}
