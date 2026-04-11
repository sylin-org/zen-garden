//! Pond security API handlers
//!
//! Translates zen-garden Pond vocabulary to koi-certmesh operations.
//! All cryptographic work is delegated to certmesh — these handlers
//! are a thin domain-vocabulary facade.
//!
//! Security model:
//! - CA (keystone) creation requires a passphrase
//! - Enrollment uses TOTP codes shared out-of-band
//! - Auto-unlock: JustMe/MyTeam profiles save passphrase to disk and
//!   unlock automatically on reboot. MyOrganization profiles stay locked.
//! - All cert lifecycle managed by certmesh (issue, renew, revoke)

use crate::{
    AppState, bad_gateway, bad_request, conflict, error_response, forbidden, internal, not_found,
    unavailable,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Html,
};
use garden_common::api_utils::ApiErrorResponse;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

/// Embedded pond ceremony UI — single-page app served at `/pond`
const POND_HTML: &str = include_str!("../../../assets/pond.html");

// ============================================================================
// Request types
// ============================================================================

#[derive(Deserialize)]
pub struct PondInitRequest {
    /// Passphrase to encrypt the CA private key at rest.
    /// If empty and `generate_passphrase` is true, an XKCD-style passphrase is generated.
    #[serde(default)]
    pub passphrase: String,
    /// Generate an XKCD-style passphrase (word-word-word-NN) instead of requiring one.
    /// The generated passphrase is returned in the response.
    #[serde(default)]
    pub generate_passphrase: bool,
    /// Trust profile: "just-me" (default), "my-team", "my-organization"
    #[serde(default)]
    pub profile: Option<String>,
    /// Optional pond name (auto-generated if omitted)
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct PondRenameRequest {
    /// New pond name (must match pond-{word}-{word} format, or omit to auto-generate)
    #[serde(default)]
    pub name: Option<String>,
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
    /// Passphrase to decrypt the CA private key (passphrase unlock)
    #[serde(default)]
    pub passphrase: Option<String>,
    /// TOTP code for authenticator-based unlock
    #[serde(default)]
    pub totp_code: Option<String>,
    /// FIDO2 credential ID (base64) for security key unlock.
    /// The caller must have already verified the WebAuthn assertion.
    #[serde(default)]
    pub fido2_credential_id: Option<String>,
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
    /// Client's X25519 public key for key exchange (hex-encoded, 32 bytes)
    pub client_public_key: String,
}

#[derive(Deserialize)]
pub struct ClientEnrollRequest {
    /// Client machine's hostname
    pub hostname: String,
    /// TOTP code from authenticator app
    pub code: String,
    /// Additional SANs for the certificate (e.g. IPs, .local aliases)
    #[serde(default)]
    pub sans: Vec<String>,
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
    pub name: String,
    /// Generated passphrase (only present when `generate_passphrase` was true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_passphrase: Option<String>,
    /// Memorization hint for the generated passphrase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memorization_hint: Option<String>,
}

#[derive(Serialize)]
pub struct PondInviteResponse {
    pub totp_uri: String,
    pub expires_at: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub inviter_stone: String,
    pub enrollment_state: String,
}

#[derive(Serialize, Deserialize)]
pub struct PondJoinResponse {
    pub stone_name: String,
    pub cornerstone: Option<String>,
    pub certificate_expires: String,
    pub status: String,
    pub ca_fingerprint: String,
    /// PEM-encoded CA certificate (populated by cornerstone for proxy enrollment)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub ca_cert: Option<String>,
    /// PEM-encoded service certificate (populated by cornerstone for proxy enrollment)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub service_cert: Option<String>,
    /// PEM-encoded private key (populated by cornerstone for proxy enrollment)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub service_key: Option<String>,
}

#[derive(Serialize)]
pub struct ClientEnrollResponse {
    /// PEM-encoded CA public certificate (for trust store + ca.pem)
    pub ca_cert: String,
    /// PEM-encoded service certificate (for mTLS client identity)
    pub service_cert: String,
    /// PEM-encoded private key (for mTLS client identity)
    pub service_key: String,
    /// CA certificate fingerprint for verification
    pub ca_fingerprint: String,
    /// Enrolled hostname (echoed back)
    pub hostname: String,
    /// Certificate expiry (ISO 8601)
    pub cert_expires: String,
}

#[derive(Serialize)]
pub struct PondStatusResponse {
    pub active: bool,
    pub locked: bool,
    pub name: Option<String>,
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

type PondResult<T> = crate::api::ApiResult<T>;

/// Map koi CertmeshError to HTTP error response
fn certmesh_err(e: koi_certmesh::CertmeshError) -> (StatusCode, Json<ApiErrorResponse>) {
    use koi_certmesh::CertmeshError;
    match e {
        CertmeshError::CaNotInitialized => conflict(
            "POND_NOT_INITIALIZED",
            "Pond not initialized. Run 'garden-rake place keystone' first.",
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
        CertmeshError::EnrollmentClosed => forbidden(
            "ENROLLMENT_CLOSED",
            "Enrollment is closed. Ask the keystone operator to run 'garden-rake pond invite'.",
        ),
        CertmeshError::AlreadyEnrolled(hostname) => conflict(
            "ALREADY_ENROLLED",
            format!("Stone '{hostname}' is already enrolled in the pond."),
        ),
        CertmeshError::NotFound(what) => not_found("NOT_FOUND", format!("Not found: {what}")),
        CertmeshError::Revoked(hostname) => forbidden(
            "REVOKED",
            format!("Stone '{hostname}' has been revoked from the pond."),
        ),
        CertmeshError::ApprovalDenied => forbidden(
            "APPROVAL_DENIED",
            "Enrollment request was denied by the operator.",
        ),
        CertmeshError::NoSlotFound(detail) => bad_request("NO_SLOT_FOUND", detail.to_string()),
        _ => {
            // Sanitise: strip internal Rust error chains, log the full detail
            let full = format!("{e}");
            tracing::warn!(error = %full, "Certmesh operation failed");
            internal(
                "CERTMESH_ERROR",
                "An internal error occurred. Check server logs for details.",
            )
        }
    }
}

/// Get the CertmeshCore from AppState, returning appropriate HTTP errors
fn get_certmesh_core(
    state: &AppState,
) -> Result<std::sync::Arc<koi_certmesh::CertmeshCore>, (StatusCode, Json<ApiErrorResponse>)> {
    let handle = state.discovery.koi.certmesh().map_err(|e| {
        unavailable(
            "CERTMESH_UNAVAILABLE",
            format!("Certmesh subsystem not available: {e}"),
        )
    })?;
    handle.core().map_err(|e| {
        unavailable(
            "CERTMESH_CORE_UNAVAILABLE",
            format!("Certmesh core not ready: {e}"),
        )
    })
}

/// Refresh the pond_active flag from certmesh state.
///
/// Delegates to domain function. Kept as a thin wrapper so existing callers
/// in this module continue to work without a path change.
async fn refresh_pond_active(state: &AppState) {
    crate::domain::security::pond_lifecycle::refresh_pond_active(state).await;
}

/// Notify the system that enrollment state changed.
///
/// Delegates to domain function. Kept as a thin wrapper so existing callers
/// in this module continue to work without a path change.
async fn notify_enrollment_changed(state: &AppState, enrolled: bool, cornerstone: Option<String>) {
    crate::domain::security::pond_lifecycle::notify_enrollment_changed(
        state,
        enrolled,
        cornerstone,
    )
    .await;
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /pond — Serve the embedded pond ceremony web UI
pub async fn get_pond_page() -> (
    StatusCode,
    [(&'static str, &'static str); 1],
    Html<&'static str>,
) {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        Html(POND_HTML),
    )
}

/// POST /api/v1/pond/init — Place keystone (create CA)
///
/// Creates the certificate authority for this garden. The calling stone
/// becomes the cornerstone (primary CA holder). Returns a TOTP URI for
/// the authenticator app — this is used to authorize future stone enrollments.
pub async fn pond_init_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondInitRequest>,
) -> PondResult<PondInitResponse> {
    use crate::domain::security::pond_lifecycle::{self, PondInitInput};

    let core = get_certmesh_core(&state)?;

    // Generate passphrase if requested (or if passphrase is empty)
    let (passphrase, generated_passphrase, memorization_hint) =
        if payload.generate_passphrase || payload.passphrase.is_empty() {
            let entropy = koi_certmesh::entropy::collect_entropy(
                koi_certmesh::entropy::EntropyMode::AutoGenerate,
            )
            .map_err(|e| internal("ENTROPY_FAILED", format!("Entropy collection failed: {e}")))?;
            let generated = koi_certmesh::entropy::generate_passphrase(&entropy);
            let hint = koi_certmesh::entropy::memorization_hint(&generated);
            (generated.clone(), Some(generated), Some(hint))
        } else {
            (payload.passphrase, None, None)
        };

    let input = PondInitInput {
        passphrase,
        profile: payload.profile,
        name: payload.name,
    };

    let result = pond_lifecycle::init(&state, core, input)
        .await
        .map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("Unknown trust profile") {
                bad_request("INVALID_PROFILE", msg)
            } else {
                internal("POND_INIT_FAILED", msg)
            }
        })?;

    crate::api::ok(PondInitResponse {
        cornerstone: result.cornerstone,
        keystone_path: result.keystone_path,
        certificate_expires: "30 days".to_string(),
        status: "active".to_string(),
        totp_uri: result.totp_uri,
        ca_fingerprint: result.ca_fingerprint,
        name: result.pond_name,
        generated_passphrase,
        memorization_hint,
    })
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

    crate::api::ok(PondStatusResponse {
        active,
        locked: status.ca_initialized && status.ca_locked,
        name: state.security.pond.state.name().await,
        cornerstone,
        stones,
        profile: format!("{:?}", status.profile),
        ca_fingerprint: status.ca_fingerprint,
        enrollment_state: format!("{:?}", status.enrollment_state),
    })
}

/// POST /api/v1/pond/join — Join pond with TOTP code
///
/// Two modes of operation:
/// - **Cornerstone** (has CA): enrolls the stone directly via certmesh
/// - **Non-cornerstone**: discovers the cornerstone via topology, proxies the
///   enrollment request, stores the returned certificates locally, and
///   activates HTTPS + chirp signing without a restart.
///
/// Rake always sends `POST /api/v1/pond/join` to the tended stone.
/// Rake never contacts another stone directly.
pub async fn pond_join_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondJoinRequest>,
) -> PondResult<PondJoinResponse> {
    // Determine if this stone is the cornerstone (has CA initialized)
    let is_cornerstone = if let Ok(handle) = state.discovery.koi.certmesh() {
        if let Ok(core) = handle.core() {
            core.certmesh_status().await.ca_initialized
        } else {
            false
        }
    } else {
        false
    };

    if is_cornerstone {
        local_enrollment(&state, payload).await
    } else {
        proxy_enrollment(&state, payload).await
    }
}

/// Handle enrollment locally — this stone IS the cornerstone.
async fn local_enrollment(
    state: &AppState,
    payload: PondJoinRequest,
) -> PondResult<PondJoinResponse> {
    let core = get_certmesh_core(state)?;

    let hostname = payload
        .hostname
        .unwrap_or_else(|| state.current.stone.name.clone());

    let join_req = koi_certmesh::protocol::JoinRequest {
        hostname: hostname.clone(),
        auth: koi_crypto::auth::AuthResponse::Totp { code: payload.code },
        sans: payload.sans,
    };

    let join_resp = core.enroll(&join_req).await.map_err(certmesh_err)?;

    // Determine cornerstone from the roster
    let status = core.certmesh_status().await;
    let cornerstone = status
        .members
        .iter()
        .find(|m| m.role == "primary")
        .map(|m| m.hostname.clone());

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(state, true, cornerstone.clone()).await;

    tracing::info!(
        stone = %join_resp.hostname,
        fingerprint = %join_resp.ca_fingerprint,
        "Stone enrolled in pond (local)"
    );

    crate::api::ok(PondJoinResponse {
        stone_name: join_resp.hostname,
        cornerstone,
        certificate_expires: "30 days".to_string(),
        status: "active".to_string(),
        ca_fingerprint: join_resp.ca_fingerprint,
        // Include cert material so proxying stones can store it locally
        ca_cert: Some(join_resp.ca_cert),
        service_cert: Some(join_resp.service_cert),
        service_key: Some(join_resp.service_key),
    })
}

/// Handle enrollment by proxying to the cornerstone.
///
/// Flow: Rake → this stone → cornerstone → cert issued → stored locally.
async fn proxy_enrollment(
    state: &AppState,
    payload: PondJoinRequest,
) -> PondResult<PondJoinResponse> {
    // Discover cornerstone address via topology
    let cornerstone_addr = discover_cornerstone(state).await?;

    // Forward the join request with our hostname
    let proxy_payload = serde_json::json!({
        "code": payload.code,
        "hostname": state.current.stone.name,
        "sans": payload.sans,
    });

    tracing::info!(
        cornerstone = %cornerstone_addr,
        stone = %state.current.stone.name,
        "Proxying pond join to cornerstone"
    );

    let resp = state
        .security
        .stone_client
        .post(&cornerstone_addr, "/api/v1/pond/join")
        .timeout(garden_common::constants::timeouts::pond_join_timeout())
        .json(&proxy_payload)
        .send()
        .await
        .map_err(|e| {
            bad_gateway(
                "CORNERSTONE_UNREACHABLE",
                format!("Failed to reach cornerstone at {cornerstone_addr}: {e}"),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Try to forward the structured error from the cornerstone
        if let Ok(err_resp) = serde_json::from_str::<ApiErrorResponse>(&body) {
            return Err((status, Json(err_resp)));
        }
        return Err(bad_gateway(
            "CORNERSTONE_ERROR",
            format!("Cornerstone returned {status}: {body}"),
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        bad_gateway(
            "CORNERSTONE_PARSE_ERROR",
            format!("Failed to parse cornerstone response: {e}"),
        )
    })?;

    // Extract the join response from ApiResponse wrapper
    let data = body.get("data").ok_or_else(|| {
        bad_gateway(
            "CORNERSTONE_RESPONSE_INVALID",
            "Cornerstone response missing 'data' field".to_string(),
        )
    })?;

    let join_resp: PondJoinResponse = serde_json::from_value(data.clone()).map_err(|e| {
        bad_gateway(
            "CORNERSTONE_PARSE_ERROR",
            format!("Failed to parse join response: {e}"),
        )
    })?;

    // Extract and store certificates locally
    let (ca_cert, service_cert, service_key) = match (
        &join_resp.ca_cert,
        &join_resp.service_cert,
        &join_resp.service_key,
    ) {
        (Some(ca), Some(cert), Some(key)) => (ca.clone(), cert.clone(), key.clone()),
        _ => {
            return Err(bad_gateway(
                "MISSING_CERT_DATA",
                "Cornerstone did not return certificate material".to_string(),
            ));
        }
    };

    // Write certs to local filesystem
    write_enrollment_certs(
        &state.current.stone.name,
        &ca_cert,
        &service_cert,
        &service_key,
    )
    .await?;

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(state, true, join_resp.cornerstone.clone()).await;

    tracing::info!(
        stone = %state.current.stone.name,
        cornerstone = ?join_resp.cornerstone,
        fingerprint = %join_resp.ca_fingerprint,
        "Stone enrolled in pond (proxied via cornerstone)"
    );

    // Return clean response to Rake (without cert material)
    crate::api::ok(PondJoinResponse {
        stone_name: join_resp.stone_name,
        cornerstone: join_resp.cornerstone,
        certificate_expires: join_resp.certificate_expires,
        status: join_resp.status,
        ca_fingerprint: join_resp.ca_fingerprint,
        ca_cert: None,
        service_cert: None,
        service_key: None,
    })
}

/// Discover the cornerstone's address via the topology cache.
///
/// Queries online peers for `/api/v1/pond/status` to find which stone
/// holds the CA (role = "primary"). Returns the cornerstone's `PeerAddress`.
async fn discover_cornerstone(
    state: &AppState,
) -> Result<garden_common::PeerAddress, (StatusCode, Json<ApiErrorResponse>)> {
    let cache = state.current.topology.cache.read().await;

    // Collect online peers, most recently seen first
    let mut candidates: Vec<_> = cache
        .values()
        .filter(|e| e.stone_name != state.current.stone.name)
        .filter(|e| e.status == garden_common::types::StoneStatus::Online)
        .collect();
    candidates.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

    if candidates.is_empty() {
        return Err(unavailable(
            "NO_PEERS_FOUND",
            "No online peers discovered. Cannot find cornerstone. \
             Ensure other stones are running and on the same network.",
        ));
    }

    for entry in &candidates {
        let resp = match state
            .security
            .stone_client
            .get(&entry.address, "/api/v1/pond/status")
            .timeout(garden_common::constants::timeouts::pond_operation_timeout())
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };

        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(_) => continue,
        };

        let cornerstone_name = body
            .get("data")
            .and_then(|d| d.get("cornerstone"))
            .and_then(|c| c.as_str());

        if let Some(name) = cornerstone_name {
            // Found the cornerstone hostname — look up its address
            for e in cache.values() {
                if e.stone_name == name {
                    tracing::info!(
                        cornerstone = %name,
                        endpoint = %e.address,
                        via = %entry.stone_name,
                        "Cornerstone discovered via peer"
                    );
                    return Ok(e.address.clone());
                }
            }
            // Cornerstone identified but not in our topology cache
            return Err(unavailable(
                "CORNERSTONE_NOT_DISCOVERED",
                format!(
                    "Cornerstone '{name}' identified but not found in topology. \
                     Wait for discovery or check network."
                ),
            ));
        }
    }

    Err(unavailable(
        "NO_POND_FOUND",
        "No active pond discovered in the garden. \
         Ensure a keystone has been placed on another stone.",
    ))
}

/// Write enrollment certificates to the local filesystem.
///
/// Creates `{data_dir}/koi/certs/{hostname}/` with cert.pem, key.pem,
/// ca.pem, and fullchain.pem. Also stores the CA cert at the certmesh
/// CA cert path so chirp verification works on non-cornerstone stones.
async fn write_enrollment_certs(
    hostname: &str,
    ca_cert: &str,
    service_cert: &str,
    service_key: &str,
) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(hostname);

    tokio::fs::create_dir_all(&certs_dir).await.map_err(|e| {
        internal(
            "CERT_WRITE_ERROR",
            format!("Failed to create cert directory: {e}"),
        )
    })?;

    tokio::fs::write(certs_dir.join("cert.pem"), service_cert)
        .await
        .map_err(|e| internal("CERT_WRITE_ERROR", format!("Failed to write cert.pem: {e}")))?;

    tokio::fs::write(certs_dir.join("key.pem"), service_key)
        .await
        .map_err(|e| internal("CERT_WRITE_ERROR", format!("Failed to write key.pem: {e}")))?;

    // Restrict key.pem permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(
            certs_dir.join("key.pem"),
            std::fs::Permissions::from_mode(0o600),
        )
        .await;
    }

    tokio::fs::write(certs_dir.join("ca.pem"), ca_cert)
        .await
        .map_err(|e| internal("CERT_WRITE_ERROR", format!("Failed to write ca.pem: {e}")))?;

    let fullchain = format!("{service_cert}{ca_cert}");
    tokio::fs::write(certs_dir.join("fullchain.pem"), &fullchain)
        .await
        .map_err(|e| {
            internal(
                "CERT_WRITE_ERROR",
                format!("Failed to write fullchain.pem: {e}"),
            )
        })?;

    // Also store CA cert at the certmesh CA cert path so chirp verification
    // works on non-cornerstone stones (activate_pond_security reads from there).
    let ca_cert_dest = koi_certmesh::CertmeshPaths::default().ca_cert_path();
    if let Some(parent) = ca_cert_dest.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&ca_cert_dest, ca_cert).await;

    tracing::info!(dir = %certs_dir.display(), "Enrollment certificates stored locally");

    Ok(())
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
            return Err(internal(
                "UNEXPECTED_AUTH_METHOD",
                "Expected TOTP auth setup but got a different method.",
            ));
        }
    };

    // Open enrollment window with deadline
    let ttl_minutes = payload.ttl_minutes.unwrap_or(30);
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(ttl_minutes as i64);
    core.open_enrollment(Some(deadline))
        .await
        .map_err(certmesh_err)?;

    tracing::info!(
        inviter = %state.current.stone.name,
        ttl_minutes = ttl_minutes,
        "Pond enrollment opened with fresh invitation"
    );

    crate::api::ok(PondInviteResponse {
        totp_uri,
        expires_at: Some(deadline.to_rfc3339()),
        ttl_seconds: Some(ttl_minutes * 60),
        inviter_stone: state.current.stone.name.clone(),
        enrollment_state: "open".to_string(),
    })
}

/// POST /api/v1/pond/unlock — Unlock pond after restart
///
/// After a reboot, the CA private key is locked (encrypted at rest).
/// This endpoint decrypts the key so certmesh can issue/renew certificates.
///
/// Supports three unlock methods (provide exactly one):
/// - `passphrase`: traditional passphrase-based unlock
/// - `totp_code`: authenticator app code (requires TOTP unlock slot)
/// - `fido2_credential_id`: security key (requires FIDO2 unlock slot, assertion pre-verified)
pub async fn pond_unlock_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondUnlockRequest>,
) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    // Check if already unlocked — return idempotent no-op
    {
        let status = core.certmesh_status().await;
        if status.ca_initialized && !status.ca_locked {
            return crate::api::ok(serde_json::json!({
                "unlocked": true,
                "message": "Pond is already unlocked."
            }));
        }
    }

    if let Some(ref totp_code) = payload.totp_code {
        // TOTP-based unlock
        core.unlock_with_totp(totp_code)
            .await
            .map_err(certmesh_err)?;
        tracing::info!("Pond unlocked via TOTP code");
    } else if let Some(ref credential_id_b64) = payload.fido2_credential_id {
        // FIDO2-based unlock (assertion already verified by caller)
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let credential_id = b64.decode(credential_id_b64).map_err(|e| {
            bad_request(
                "INVALID_CREDENTIAL",
                format!("Invalid base64 credential ID: {e}"),
            )
        })?;
        core.unlock_with_fido2(&credential_id)
            .await
            .map_err(certmesh_err)?;
        tracing::info!("Pond unlocked via FIDO2 security key");
    } else if let Some(ref passphrase) = payload.passphrase {
        // Passphrase-based unlock (original path)
        core.unlock(passphrase).await.map_err(certmesh_err)?;
        tracing::info!("Pond unlocked via passphrase");
    } else {
        return Err(bad_request(
            "NO_UNLOCK_METHOD",
            "Provide one of: passphrase, totp_code, or fido2_credential_id",
        ));
    }

    // Re-derive enrolled state (CA is now unlocked → enrolled)
    refresh_pond_active(&state).await;

    // Determine cornerstone
    let cornerstone = {
        let status = core.certmesh_status().await;
        status
            .members
            .iter()
            .find(|m| m.role == "primary")
            .map(|m| m.hostname.clone())
    };

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(&state, true, cornerstone).await;

    tracing::info!("Pond unlocked — CA key decrypted");

    crate::api::ok(serde_json::json!({
        "unlocked": true,
    }))
}

/// DELETE /api/v1/pond — Drain the pond (destroy CA)
///
/// Irreversibly destroys the CA and all certificates.
/// All enrolled stones lose their trust relationship.
pub async fn pond_remove_v1(State(state): State<AppState>) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    core.destroy().await.map_err(certmesh_err)?;

    // Notify enrollment change (unenrolled) — listener stops HTTPS
    notify_enrollment_changed(&state, false, None).await;

    tracing::warn!("Pond drained — CA destroyed, all certificates invalidated");

    // Clean up pond metadata
    let path = std::path::PathBuf::from(garden_common::constants::paths::pond_metadata_file());
    let _ = std::fs::remove_file(&path);

    // Clean up auto-unlock key if present
    koi_certmesh::CertmeshCore::delete_auto_unlock_key();

    crate::api::ok(serde_json::json!({
        "destroyed": true,
    }))
}

/// PUT /api/v1/pond/name — Rename the pond
///
/// Changes the decorative pond name without any cryptographic consequences.
/// If no name is provided, a new random name is generated.
pub async fn pond_rename_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondRenameRequest>,
) -> PondResult<serde_json::Value> {
    if !state.security.pond.state.enrolled() {
        return Err(conflict(
            "POND_NOT_INITIALIZED",
            "No pond to rename. Initialize with 'garden-rake place keystone' first.",
        ));
    }

    let new_name = match payload.name {
        Some(ref name) if crate::domain::naming::is_valid_pond_name(name) => name.clone(),
        Some(ref name) if !name.is_empty() => {
            return Err(bad_request(
                "INVALID_POND_NAME",
                format!("Pond name must match 'pond-{{word}}-{{word}}' format, got: '{name}'"),
            ));
        }
        _ => crate::domain::naming::generate_pond_name(),
    };

    state.security.pond.state.set_name(new_name.clone()).await;
    let metadata = crate::domain::PondMetadata {
        name: Some(new_name.clone()),
    };
    if let Err(e) = crate::domain::save_pond_metadata(&metadata) {
        tracing::warn!(error = %e, "Failed to persist pond metadata");
    }

    tracing::info!(name = %new_name, "Pond renamed");

    crate::api::ok(serde_json::json!({
        "name": new_name,
    }))
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

    crate::api::ok(serde_json::json!({
        "revoked": true,
        "stone_name": stone_name,
    }))
}

/// POST /api/v1/pond/promote — Promote this stone to standby CA
pub async fn pond_promote_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondPromoteRequest>,
) -> PondResult<serde_json::Value> {
    let core = get_certmesh_core(&state)?;

    let key_bytes: [u8; 32] = hex::decode(&payload.client_public_key)
        .map_err(|e| {
            certmesh_err(koi_certmesh::CertmeshError::Internal(format!(
                "invalid client_public_key hex: {e}"
            )))
        })?
        .try_into()
        .map_err(|_| {
            certmesh_err(koi_certmesh::CertmeshError::Internal(
                "client_public_key must be exactly 32 bytes".to_string(),
            ))
        })?;

    let _resp = core.promote(&key_bytes).await.map_err(certmesh_err)?;

    tracing::info!(
        stone = %state.current.stone.name,
        "Stone promoted — received CA key material"
    );

    crate::api::ok(serde_json::json!({
        "promoted": true,
        "ca_fingerprint": koi_certmesh::ca::ca_fingerprint_from_disk(&koi_certmesh::CertmeshPaths::default())
            .unwrap_or_else(|_| "unavailable".to_string()),
    }))
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
        return Err(not_found(
            "POND_NOT_INITIALIZED",
            "No pond CA exists. Run 'garden-rake place keystone' first.",
        ));
    }

    // Read CA cert from disk
    let ca_cert_path = koi_certmesh::CertmeshPaths::default().ca_cert_path();
    let ca_pem = tokio::fs::read_to_string(&ca_cert_path)
        .await
        .map_err(|e| {
            internal(
                "CA_READ_ERROR",
                format!("Failed to read CA certificate: {e}"),
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

// ============================================================================
// Ceremony
// ============================================================================

/// POST /api/v1/pond/ceremony — Step through a pond ceremony
///
/// This endpoint drives pond ceremonies (init, join, invite, unlock)
/// using the koi-common ceremony protocol. Each request either starts
/// a new ceremony or continues an existing session.
///
/// When the "init" ceremony completes, the handler automatically
/// creates the CA using the collected bag data.
pub async fn pond_ceremony_v1(
    State(state): State<AppState>,
    Json(request): Json<koi_common::ceremony::CeremonyRequest>,
) -> Result<Json<koi_common::ceremony::CeremonyResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let host = &state.security.pond.ceremony.host;

    // Pre-fill hostname for TOTP personalization
    let mut req = request;
    if req.ceremony.as_deref() == Some("init") && req.session_id.is_none() {
        req.data
            .entry("_self_hostname".to_string())
            .or_insert_with(|| serde_json::json!(state.current.stone.name));
    }

    let response = host
        .step(req)
        .map_err(|e| bad_request("CEREMONY_ERROR", format!("{e}")))?;

    // When an init ceremony completes, execute the CA creation
    if response.complete && response.error.is_none() {
        let is_init = response
            .result_data
            .as_ref()
            .map(|d| d.contains_key("_effective_profile") && d.contains_key("passphrase"))
            .unwrap_or(false);
        if is_init {
            return execute_pond_init_from_ceremony(&state, response).await;
        }

        // When an unlock ceremony completes, execute the unlock
        let is_unlock = response
            .result_data
            .as_ref()
            .map(|d| {
                d.contains_key("passphrase")
                    || d.contains_key("_unlock_totp_input")
                    || d.contains_key("_unlock_fido2_assertion")
            })
            .unwrap_or(false);
        if is_unlock {
            return execute_pond_unlock_from_ceremony(&state, response).await;
        }
    }

    Ok(Json(response))
}

/// Execute pond initialization using data collected by the ceremony,
/// then return the ceremony response with creation details attached.
async fn execute_pond_init_from_ceremony(
    state: &AppState,
    mut response: koi_common::ceremony::CeremonyResponse,
) -> Result<Json<koi_common::ceremony::CeremonyResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let bag = response.result_data.as_ref().ok_or_else(|| {
        internal(
            "CEREMONY_ERROR",
            "Init ceremony completed with no result data",
        )
    })?;
    let core = get_certmesh_core(state)?;

    let effective_profile = bag
        .get("_effective_profile")
        .and_then(|v| v.as_str())
        .unwrap_or("just_me");
    let profile = match effective_profile {
        "just_me" | "JustMe" => koi_certmesh::profiles::TrustProfile::JustMe,
        "my_team" | "MyTeam" => koi_certmesh::profiles::TrustProfile::MyTeam,
        "my_organization" | "MyOrganization" => {
            koi_certmesh::profiles::TrustProfile::MyOrganization
        }
        _ => koi_certmesh::profiles::TrustProfile::JustMe,
    };

    let passphrase = bag
        .get("passphrase")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let entropy_hex = bag
        .get("_entropy_seed")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let operator = bag
        .get("operator")
        .and_then(|v| v.as_str())
        .map(String::from);
    let enrollment_open = bag.get("_enrollment_open").and_then(|v| v.as_bool());
    let requires_approval = bag.get("_requires_approval").and_then(|v| v.as_bool());
    let totp_secret_hex = bag
        .get("_totp_secret_hex")
        .and_then(|v| v.as_str())
        .map(String::from);

    let create_req = koi_certmesh::protocol::CreateCaRequest {
        passphrase,
        entropy_hex,
        profile,
        operator,
        enrollment_open,
        requires_approval,
        totp_secret_hex,
    };

    let body = serde_json::to_vec(&create_req).map_err(|e| {
        internal(
            "SERIALIZE_ERROR",
            format!("Failed to build certmesh request: {e}"),
        )
    })?;

    let http_req = axum::http::Request::builder()
        .method("POST")
        .uri("/create")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body))
        .expect("valid request");

    let http_resp = core
        .http_routes()
        .oneshot(http_req)
        .await
        .expect("Router is infallible");

    let status_code = http_resp.status();
    let resp_bytes = axum::body::to_bytes(http_resp.into_body(), 1024 * 1024)
        .await
        .map_err(|e| {
            internal(
                "RESPONSE_READ_ERROR",
                format!("Failed to read certmesh response: {e}"),
            )
        })?;

    if !status_code.is_success() {
        let error_text = String::from_utf8_lossy(&resp_bytes);
        tracing::error!(
            status = %status_code,
            body = %error_text,
            "Certmesh CA creation failed (ceremony)"
        );
        response.error = Some(format!("Pond creation failed: {error_text}"));
        return Ok(Json(response));
    }

    let create_resp: koi_certmesh::protocol::CreateCaResponse = serde_json::from_slice(&resp_bytes)
        .map_err(|e| {
            internal(
                "PARSE_ERROR",
                format!("Failed to parse certmesh response: {e}"),
            )
        })?;

    // Update pond state
    refresh_pond_active(state).await;

    // ── Unlock method: domain-driven decision ───────────────────────
    // The trust profile determines whether auto-unlock is configured.
    // Token-based unlock (TOTP/FIDO2) is handled separately below
    // because it requires ceremony-specific data from the bag.
    let unlock_method = bag
        .get("_unlock_method")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");
    let passphrase_for_file = bag.get("passphrase").and_then(|v| v.as_str()).unwrap_or("");

    match unlock_method {
        "auto" => {
            // Delegate to the domain — single source of truth
            if let Err(e) = koi_certmesh::CertmeshCore::configure_auto_unlock_for_profile(
                profile,
                passphrase_for_file,
            ) {
                tracing::warn!(error = %e, "Failed to configure auto-unlock (pond will require manual unlock on reboot)");
            }
        }
        "token" => {
            let token_type = bag
                .get("unlock_token_type")
                .and_then(|v| v.as_str())
                .unwrap_or("totp");

            let slot_table_path = koi_certmesh::CertmeshPaths::default().slot_table_path();
            if !slot_table_path.exists() {
                tracing::error!(
                    "Slot table not found after CA creation — cannot register unlock token"
                );
            } else {
                match koi_crypto::unlock_slots::SlotTable::load(&slot_table_path) {
                    Ok(mut table) => {
                        // Unwrap master key with passphrase to add the new slot
                        match table.unwrap_with_passphrase(passphrase_for_file) {
                            Ok(master_key) => {
                                match token_type {
                                    "totp" => {
                                        // Read the unlock TOTP secret from the ceremony bag
                                        if let Some(secret_hex) =
                                            bag.get("_unlock_totp_secret").and_then(|v| v.as_str())
                                        {
                                            match koi_common::encoding::hex_decode(secret_hex) {
                                                Ok(secret_bytes) => {
                                                    match table
                                                        .add_totp_slot(&master_key, &secret_bytes)
                                                    {
                                                        Ok(()) => {
                                                            if let Err(e) =
                                                                table.save(&slot_table_path)
                                                            {
                                                                tracing::error!(error = %e, "Failed to save slot table after adding TOTP slot");
                                                            } else {
                                                                tracing::info!(
                                                                    "TOTP unlock slot registered — pond can be unlocked with authenticator code"
                                                                );
                                                            }
                                                        }
                                                        Err(e) => {
                                                            tracing::error!(error = %e, "Failed to add TOTP unlock slot")
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!(error = %e, "Invalid _unlock_totp_secret hex")
                                                }
                                            }
                                        } else {
                                            tracing::error!(
                                                "Token type is TOTP but _unlock_totp_secret is missing from ceremony bag"
                                            );
                                        }
                                    }
                                    "fido2" => {
                                        // Read FIDO2 credential from the ceremony bag
                                        if let Some(fido2_data) = bag.get("_fido2_registered") {
                                            let credential_id = fido2_data
                                                .get("credential_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");
                                            let public_key = fido2_data
                                                .get("public_key")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");
                                            let rp_id = fido2_data
                                                .get("rp_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("localhost");

                                            if credential_id.is_empty() || public_key.is_empty() {
                                                tracing::error!(
                                                    "FIDO2 credential data incomplete — skipping slot creation"
                                                );
                                            } else {
                                                use base64::Engine;
                                                let b64 = base64::engine::general_purpose::STANDARD;
                                                let cred_bytes =
                                                    b64.decode(credential_id).unwrap_or_default();
                                                let pk_bytes =
                                                    b64.decode(public_key).unwrap_or_default();

                                                match table.add_fido2_slot(
                                                    &master_key,
                                                    &cred_bytes,
                                                    &pk_bytes,
                                                    rp_id,
                                                ) {
                                                    Ok(()) => {
                                                        if let Err(e) = table.save(&slot_table_path)
                                                        {
                                                            tracing::error!(error = %e, "Failed to save slot table after adding FIDO2 slot");
                                                        } else {
                                                            tracing::info!(
                                                                "FIDO2 unlock slot registered — pond can be unlocked with security key"
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        tracing::error!(error = %e, "Failed to add FIDO2 unlock slot")
                                                    }
                                                }
                                            }
                                        } else {
                                            tracing::error!(
                                                "Token type is FIDO2 but _fido2_registered is missing from ceremony bag"
                                            );
                                        }
                                    }
                                    other => {
                                        tracing::warn!(
                                            token_type = other,
                                            "Unknown unlock token type — no slot created"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to unwrap master key for token slot creation")
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to load slot table for token registration")
                    }
                }
            }
        }
        // "passphrase" or anything else → no auto-unlock, no token
        _ => {
            tracing::info!(
                "Unlock method is '{unlock_method}' — no auto-unlock key or token registered"
            );
        }
    }

    let pond_name = crate::domain::naming::generate_pond_name();
    state.security.pond.state.set_name(pond_name.clone()).await;
    let metadata = crate::domain::PondMetadata {
        name: Some(pond_name.clone()),
    };
    if let Err(e) = crate::domain::save_pond_metadata(&metadata) {
        tracing::warn!(error = %e, "Failed to persist pond metadata");
    }

    notify_enrollment_changed(state, true, Some(state.current.stone.name.clone())).await;

    tracing::info!(
        cornerstone = %state.current.stone.name,
        pond_name = %pond_name,
        profile = ?profile,
        fingerprint = %create_resp.ca_fingerprint,
        "Pond initialized via ceremony — keystone placed"
    );

    // Sanitize result_data — strip secrets, add creation results
    let mut safe_data = serde_json::Map::new();
    safe_data.insert("pond_name".into(), serde_json::json!(pond_name));
    safe_data.insert(
        "ca_fingerprint".into(),
        serde_json::json!(create_resp.ca_fingerprint),
    );
    safe_data.insert(
        "cornerstone".into(),
        serde_json::json!(state.current.stone.name),
    );
    safe_data.insert("profile".into(), serde_json::json!(effective_profile));
    response.result_data = Some(safe_data);

    Ok(Json(response))
}

// ============================================================================
// Ceremony-driven unlock
// ============================================================================

/// Execute pond unlock using data collected by the unlock ceremony.
async fn execute_pond_unlock_from_ceremony(
    state: &AppState,
    mut response: koi_common::ceremony::CeremonyResponse,
) -> Result<Json<koi_common::ceremony::CeremonyResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let bag = response.result_data.as_ref().ok_or_else(|| {
        internal(
            "CEREMONY_ERROR",
            "Unlock ceremony completed with no result data",
        )
    })?;
    let core = get_certmesh_core(state)?;

    let unlock_result =
        if let Some(totp_code) = bag.get("_unlock_totp_input").and_then(|v| v.as_str()) {
            core.unlock_with_totp(totp_code).await
        } else if let Some(fido2_data) = bag.get("_unlock_fido2_assertion") {
            // FIDO2 — the ceremony collected assertion data
            let credential_id = fido2_data
                .get("credential_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if credential_id.is_empty() {
                return Err(bad_request(
                    "INVALID_CREDENTIAL",
                    "FIDO2 assertion missing credential_id",
                ));
            }
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD;
            let cred_bytes = b64.decode(credential_id).map_err(|e| {
                bad_request(
                    "INVALID_CREDENTIAL",
                    format!("Invalid base64 credential ID: {e}"),
                )
            })?;
            core.unlock_with_fido2(&cred_bytes).await
        } else if let Some(passphrase) = bag.get("passphrase").and_then(|v| v.as_str()) {
            core.unlock(passphrase).await
        } else {
            return Err(bad_request(
                "NO_UNLOCK_METHOD",
                "Unlock ceremony completed without any unlock credential",
            ));
        };

    match unlock_result {
        Ok(()) => {
            refresh_pond_active(state).await;

            // Determine cornerstone
            let cornerstone = {
                let status = core.certmesh_status().await;
                status
                    .members
                    .iter()
                    .find(|m| m.role == "primary")
                    .map(|m| m.hostname.clone())
            };
            notify_enrollment_changed(state, true, cornerstone).await;

            tracing::info!("Pond unlocked via ceremony");

            // Sanitize result_data — strip secrets
            let mut safe_data = serde_json::Map::new();
            safe_data.insert("unlocked".into(), serde_json::json!(true));
            response.result_data = Some(safe_data);
        }
        Err(e) => {
            tracing::warn!(error = %e, "Pond unlock failed via ceremony");
            response.error = Some(format!("Unlock failed: {e}"));
        }
    }

    Ok(Json(response))
}

// Auto-unlock key management now lives in koi-certmesh domain:
//   CertmeshCore::save_auto_unlock_key()
//   CertmeshCore::delete_auto_unlock_key()
//   CertmeshCore::try_auto_unlock()
//   CertmeshCore::configure_auto_unlock_for_profile()

// ============================================================================
// Client Enrollment
// ============================================================================

/// POST /api/v1/pond/enroll-client — Client enrollment (no stone state mutation)
///
/// Issues a certificate for a non-Moss client (Rake on a workstation).
/// Only works on the cornerstone (the stone with the active CA).
/// Auth-gated by TOTP code in the request body.
///
/// Unlike `/api/v1/pond/join`, this does NOT trigger `notify_enrollment_changed()`,
/// start HTTPS, or emit `PondEvent`. It calls `CertmeshCore::enroll()` directly —
/// same crypto, same auth verification, but without stone lifecycle side effects.
pub async fn pond_enroll_client_v1(
    State(state): State<AppState>,
    Json(payload): Json<ClientEnrollRequest>,
) -> PondResult<ClientEnrollResponse> {
    // Only the cornerstone can issue certificates
    let is_cornerstone = if let Ok(handle) = state.discovery.koi.certmesh() {
        if let Ok(core) = handle.core() {
            core.certmesh_status().await.ca_initialized
        } else {
            false
        }
    } else {
        false
    };

    if !is_cornerstone {
        return Err(conflict(
            "NOT_CORNERSTONE",
            "This stone is not the CA. Discover the cornerstone via _certmesh._tcp mDNS.",
        ));
    }

    let core = get_certmesh_core(&state)?;

    let join_req = koi_certmesh::protocol::JoinRequest {
        hostname: payload.hostname.clone(),
        auth: koi_crypto::auth::AuthResponse::Totp { code: payload.code },
        sans: payload.sans,
    };

    let join_resp = core.enroll(&join_req).await.map_err(certmesh_err)?;

    // Update the member's role to Client in the roster
    if let Ok(handle) = state.discovery.koi.certmesh()
        && let Ok(core) = handle.core()
    {
        let _ = core
            .set_member_role(&payload.hostname, koi_certmesh::roster::MemberRole::Client)
            .await;
    }

    // Determine cert expiry from roster
    let cert_expires = {
        let status = core.certmesh_status().await;
        status
            .members
            .iter()
            .find(|m| m.hostname == payload.hostname)
            .map(|m| m.cert_expires.clone())
            .unwrap_or_default()
    };

    tracing::info!(
        hostname = %payload.hostname,
        fingerprint = %join_resp.ca_fingerprint,
        "Client enrolled in pond"
    );

    crate::api::ok(ClientEnrollResponse {
        ca_cert: join_resp.ca_cert,
        service_cert: join_resp.service_cert,
        service_key: join_resp.service_key,
        ca_fingerprint: join_resp.ca_fingerprint,
        hostname: join_resp.hostname,
        cert_expires,
    })
}
