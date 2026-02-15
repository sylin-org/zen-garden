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
    pub name: String,
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

/// Refresh the pond_active flag from certmesh state.
///
/// A stone is "pond active" if either:
/// 1. It is the cornerstone (CA initialized and unlocked), OR
/// 2. It is an enrolled member (has cert + key from a prior enrollment)
async fn refresh_pond_active(state: &AppState) {
    // Cornerstone path: CA initialized and unlocked
    if let Ok(handle) = state.koi_handle.certmesh() {
        if let Ok(core) = handle.core() {
            let status = core.certmesh_status().await;
            if status.ca_initialized && !status.ca_locked {
                state.pond_active.store(true, Ordering::Relaxed);
                return;
            }
        }
    }

    // Enrolled member path: check for enrollment certs on disk
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(&state.stone_name);
    if certs_dir.join("cert.pem").exists() && certs_dir.join("key.pem").exists() {
        state.pond_active.store(true, Ordering::Relaxed);
    }
}

/// Notify the system that enrollment state changed.
///
/// Updates `pond_active`, updates `PondState`, emits `PondEvent::EnrollmentChanged`
/// on the EventBus, and re-registers mDNS. The enrollment-change listener
/// (spawned at boot) reacts by starting/stopping HTTPS + chirp signing.
async fn notify_enrollment_changed(state: &AppState, enrolled: bool, cornerstone: Option<String>) {
    // Update flags
    state.pond_active.store(enrolled, Ordering::Relaxed);
    if enrolled {
        state.pond.set_enrolled(cornerstone.clone()).await;
    } else {
        state.pond.set_unenrolled().await;
    }

    // Emit domain event — listener handles HTTPS + chirps
    state
        .event_bus
        .emit(crate::domain::PondEvent::enrollment_changed(
            enrolled,
            cornerstone,
        ));

    // Re-register mDNS with/without pond TXT properties
    if let Some(ref mdns) = state.mdns_handle {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            let _ = mdns.reregister(&ip, mac.as_deref()).await;
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

    // Generate or use provided pond name
    let pond_name = match payload.name {
        Some(ref name) if garden_common::naming::is_valid_pond_name(name) => name.clone(),
        Some(ref name) if !name.is_empty() => {
            // User gave a name but not in pond-x-y format — prefix it
            format!("pond-{}", name.to_lowercase().replace(' ', "-"))
        }
        _ => garden_common::naming::generate_pond_name(),
    };

    // Persist pond metadata and update state
    state.pond.set_name(pond_name.clone()).await;
    let metadata = crate::domain::PondMetadata {
        name: Some(pond_name.clone()),
    };
    if let Err(e) = crate::domain::save_pond_metadata(&metadata) {
        tracing::warn!(error = %e, "Failed to persist pond metadata");
    }

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(&state, true, Some(state.stone_name.clone())).await;

    tracing::info!(
        cornerstone = %state.stone_name,
        pond_name = %pond_name,
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
        name: pond_name,
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
        name: state.pond.name().await,
        cornerstone,
        stones,
        profile: format!("{:?}", status.profile),
        ca_fingerprint: status.ca_fingerprint,
        enrollment_state: format!("{:?}", status.enrollment_state),
    })))
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
    let is_cornerstone = if let Ok(handle) = state.koi_handle.certmesh() {
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

    let hostname = payload.hostname.unwrap_or_else(|| state.stone_name.clone());

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

    Ok(Json(ApiResponse::new(PondJoinResponse {
        stone_name: join_resp.hostname,
        cornerstone,
        certificate_expires: "30 days".to_string(),
        status: "active".to_string(),
        ca_fingerprint: join_resp.ca_fingerprint,
        // Include cert material so proxying stones can store it locally
        ca_cert: Some(join_resp.ca_cert),
        service_cert: Some(join_resp.service_cert),
        service_key: Some(join_resp.service_key),
    })))
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
        "hostname": state.stone_name,
        "sans": payload.sans,
    });

    tracing::info!(
        cornerstone = %cornerstone_addr,
        stone = %state.stone_name,
        "Proxying pond join to cornerstone"
    );

    let resp = state
        .stone_client
        .post(&cornerstone_addr, "/api/v1/pond/join")
        .timeout(std::time::Duration::from_secs(15))
        .json(&proxy_payload)
        .send()
        .await
        .map_err(|e| {
            error_response(
                StatusCode::BAD_GATEWAY,
                "CORNERSTONE_UNREACHABLE",
                format!("Failed to reach cornerstone at {cornerstone_addr}: {e}"),
                None,
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Try to forward the structured error from the cornerstone
        if let Ok(err_resp) = serde_json::from_str::<ApiErrorResponse>(&body) {
            return Err((status, Json(err_resp)));
        }
        return Err(error_response(
            StatusCode::BAD_GATEWAY,
            "CORNERSTONE_ERROR",
            format!("Cornerstone returned {status}: {body}"),
            None,
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "CORNERSTONE_PARSE_ERROR",
            format!("Failed to parse cornerstone response: {e}"),
            None,
        )
    })?;

    // Extract the join response from ApiResponse wrapper
    let data = body.get("data").ok_or_else(|| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "CORNERSTONE_RESPONSE_INVALID",
            "Cornerstone response missing 'data' field".to_string(),
            None,
        )
    })?;

    let join_resp: PondJoinResponse = serde_json::from_value(data.clone()).map_err(|e| {
        error_response(
            StatusCode::BAD_GATEWAY,
            "CORNERSTONE_PARSE_ERROR",
            format!("Failed to parse join response: {e}"),
            None,
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
            return Err(error_response(
                StatusCode::BAD_GATEWAY,
                "MISSING_CERT_DATA",
                "Cornerstone did not return certificate material".to_string(),
                None,
            ))
        }
    };

    // Write certs to local filesystem
    write_enrollment_certs(&state.stone_name, &ca_cert, &service_cert, &service_key).await?;

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(state, true, join_resp.cornerstone.clone()).await;

    tracing::info!(
        stone = %state.stone_name,
        cornerstone = ?join_resp.cornerstone,
        fingerprint = %join_resp.ca_fingerprint,
        "Stone enrolled in pond (proxied via cornerstone)"
    );

    // Return clean response to Rake (without cert material)
    Ok(Json(ApiResponse::new(PondJoinResponse {
        stone_name: join_resp.stone_name,
        cornerstone: join_resp.cornerstone,
        certificate_expires: join_resp.certificate_expires,
        status: join_resp.status,
        ca_fingerprint: join_resp.ca_fingerprint,
        ca_cert: None,
        service_cert: None,
        service_key: None,
    })))
}

/// Discover the cornerstone's address via the topology cache.
///
/// Queries online peers for `/api/v1/pond/status` to find which stone
/// holds the CA (role = "primary"). Returns the cornerstone's `PeerAddress`.
async fn discover_cornerstone(
    state: &AppState,
) -> Result<garden_common::PeerAddress, (StatusCode, Json<ApiErrorResponse>)> {
    let cache = state.topology_cache.read().await;

    // Collect online peers, most recently seen first
    let mut candidates: Vec<_> = cache
        .values()
        .filter(|e| e.stone_name != state.stone_name)
        .filter(|e| e.status == garden_common::types::StoneStatus::Online)
        .collect();
    candidates.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

    if candidates.is_empty() {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_PEERS_FOUND",
            "No online peers discovered. Cannot find cornerstone. \
             Ensure other stones are running and on the same network.",
            None,
        ));
    }

    for entry in &candidates {
        let resp = match state
            .stone_client
            .get(&entry.address, "/api/v1/pond/status")
            .timeout(std::time::Duration::from_secs(5))
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
            return Err(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "CORNERSTONE_NOT_DISCOVERED",
                format!(
                    "Cornerstone '{name}' identified but not found in topology. \
                     Wait for discovery or check network."
                ),
                None,
            ));
        }
    }

    Err(error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "NO_POND_FOUND",
        "No active pond discovered in the garden. \
         Ensure a keystone has been placed on another stone.",
        None,
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
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CERT_WRITE_ERROR",
            format!("Failed to create cert directory: {e}"),
            None,
        )
    })?;

    tokio::fs::write(certs_dir.join("cert.pem"), service_cert)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CERT_WRITE_ERROR",
                format!("Failed to write cert.pem: {e}"),
                None,
            )
        })?;

    tokio::fs::write(certs_dir.join("key.pem"), service_key)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CERT_WRITE_ERROR",
                format!("Failed to write key.pem: {e}"),
                None,
            )
        })?;

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
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CERT_WRITE_ERROR",
                format!("Failed to write ca.pem: {e}"),
                None,
            )
        })?;

    let fullchain = format!("{service_cert}{ca_cert}");
    tokio::fs::write(certs_dir.join("fullchain.pem"), &fullchain)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CERT_WRITE_ERROR",
                format!("Failed to write fullchain.pem: {e}"),
                None,
            )
        })?;

    // Also store CA cert at the certmesh CA cert path so chirp verification
    // works on non-cornerstone stones (activate_pond_security reads from there).
    let ca_cert_dest = koi_certmesh::ca::ca_cert_path();
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

    // Notify enrollment change (unenrolled) — listener stops HTTPS
    notify_enrollment_changed(&state, false, None).await;

    tracing::warn!("Pond drained — CA destroyed, all certificates invalidated");

    // Clean up pond metadata
    let path = std::path::PathBuf::from(garden_common::constants::paths::pond_metadata_file());
    let _ = std::fs::remove_file(&path);

    Ok(Json(ApiResponse::new(serde_json::json!({
        "destroyed": true,
    }))))
}

/// PUT /api/v1/pond/name — Rename the pond
///
/// Changes the decorative pond name without any cryptographic consequences.
/// If no name is provided, a new random name is generated.
pub async fn pond_rename_v1(
    State(state): State<AppState>,
    Json(payload): Json<PondRenameRequest>,
) -> PondResult<serde_json::Value> {
    if !state.pond.enrolled() {
        return Err(error_response(
            StatusCode::CONFLICT,
            "POND_NOT_INITIALIZED",
            "No pond to rename. Initialize with 'garden-rake place keystone' first.",
            None,
        ));
    }

    let new_name = match payload.name {
        Some(ref name) if garden_common::naming::is_valid_pond_name(name) => name.clone(),
        Some(ref name) if !name.is_empty() => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_POND_NAME",
                format!("Pond name must match 'pond-{{word}}-{{word}}' format, got: '{name}'"),
                None,
            ));
        }
        _ => garden_common::naming::generate_pond_name(),
    };

    state.pond.set_name(new_name.clone()).await;
    let metadata = crate::domain::PondMetadata {
        name: Some(new_name.clone()),
    };
    if let Err(e) = crate::domain::save_pond_metadata(&metadata) {
        tracing::warn!(error = %e, "Failed to persist pond metadata");
    }

    tracing::info!(name = %new_name, "Pond renamed");

    Ok(Json(ApiResponse::new(serde_json::json!({
        "name": new_name,
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
