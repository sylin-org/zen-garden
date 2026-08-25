//! Pond lifecycle operations (ARCH-0005 extraction).
//!
//! Domain logic for pond initialization (place keystone), extracted from
//! the API handler. No HTTP response types — the handler maps results
//! to API responses.

use anyhow::{Context, Result};
use std::sync::Arc;
use tower::ServiceExt;

use crate::Moss;

// ============================================================================
// Types
// ============================================================================

/// Input for pond initialization (domain-facing, no HTTP concerns).
pub struct PondInitInput {
    pub passphrase: String,
    pub profile: Option<String>,
    pub name: Option<String>,
}

/// Result of a successful pond initialization.
pub struct PondInitResult {
    pub cornerstone: String,
    pub keystone_path: String,
    pub totp_uri: Option<String>,
    pub ca_fingerprint: String,
    pub pond_name: String,
}

// ============================================================================
// Trust profile translation
// ============================================================================

/// Resolve a trust-profile preset name into the three booleans certmesh stores.
///
/// koi flattened trust profiles to `(enrollment_open, requires_approval,
/// auto_unlock)`; the named presets ("just-me", "my-team", "my-organization",
/// numeric "1"/"2"/"3") survive only as UX labels, resolved here via koi's
/// `preset_bools`. `None` defaults to "just-me" (the single-stone pond).
pub fn parse_trust_profile(input: Option<&str>) -> Result<koi_certmesh::profiles::PresetBools> {
    let name = input.unwrap_or("just-me");
    koi_certmesh::profiles::preset_bools(name).ok_or_else(|| {
        anyhow::anyhow!("Unknown trust profile: '{name}'. Valid: just-me, my-team, my-organization")
    })
}

// ============================================================================
// Pond init
// ============================================================================

/// Initialize the pond (place keystone) — creates the CA on this stone.
///
/// This stone becomes the cornerstone (primary CA holder). Returns a
/// TOTP URI for the authenticator app used to authorize future enrollments.
///
/// The certmesh CA creation is invoked via tower::Service (in-process HTTP)
/// to avoid code path divergence with certmesh's own endpoint.
pub async fn init(
    state: &Moss,
    core: Arc<koi_certmesh::CertmeshCore>,
    input: PondInitInput,
) -> Result<PondInitResult> {
    let (enrollment_open, requires_approval, auto_unlock) =
        parse_trust_profile(input.profile.as_deref())?;

    // Generate cryptographic entropy for CA creation
    let entropy = {
        use rand::RngCore;
        let mut buf = [0u8; 32];
        rand::rng().fill_bytes(&mut buf);
        hex::encode(buf)
    };

    // Build certmesh CreateCA request. The security posture is the two real
    // booleans; `auto_unlock` is the create-time vault decision (configured
    // inside CertmeshCore::create from this flag).
    let create_req = koi_certmesh::protocol::CreateCaRequest {
        passphrase: input.passphrase.clone(),
        entropy_hex: entropy,
        operator: None,
        enrollment_open,
        requires_approval,
        auto_unlock,
        totp_secret_hex: None,
    };

    let body = serde_json::to_vec(&create_req).context("Failed to serialize certmesh request")?;

    // Invoke certmesh via in-process HTTP (tower::Service)
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
        .context("Failed to read certmesh response")?;

    if !status_code.is_success() {
        let error_text = String::from_utf8_lossy(&resp_bytes);
        tracing::error!(status = %status_code, body = %error_text, "Certmesh CA creation failed");
        anyhow::bail!("Failed to create CA: {error_text}");
    }

    let create_resp: koi_certmesh::protocol::CreateCaResponse =
        serde_json::from_slice(&resp_bytes).context("Failed to parse certmesh response")?;

    // Extract TOTP URI from auth setup (TOTP is the only auth method).
    let koi_crypto::auth::AuthSetup::Totp { totp_uri } = &create_resp.auth_setup;
    let totp_uri = Some(totp_uri.clone());

    // Update pond state
    refresh_pond_active(state).await;

    // Auto-unlock is configured inside CertmeshCore::create() from the request's
    // `auto_unlock` flag (koi's single source of truth) — no separate call here.

    // Generate or use provided pond name
    let pond_name = resolve_pond_name(input.name.as_deref());

    // Persist pond metadata and update state
    state.security.set_pond_name(pond_name.clone()).await;
    let metadata = crate::domain::PondMetadata {
        name: Some(pond_name.clone()),
    };
    if let Err(e) = crate::domain::save_pond_metadata(&metadata) {
        tracing::warn!(error = %e, "Failed to persist pond metadata");
    }

    // Notify enrollment change — listener starts HTTPS + chirp signing
    notify_enrollment_changed(state, true, Some(state.current.stone.name.clone())).await;

    tracing::info!(
        cornerstone = %state.current.stone.name,
        pond_name = %pond_name,
        enrollment_open,
        requires_approval,
        auto_unlock,
        fingerprint = %create_resp.ca_fingerprint,
        "Pond initialized — keystone placed"
    );

    Ok(PondInitResult {
        cornerstone: state.current.stone.name.clone(),
        keystone_path: core.paths().ca_dir().display().to_string(),
        totp_uri,
        ca_fingerprint: create_resp.ca_fingerprint,
        pond_name,
    })
}

// ============================================================================
// Helpers (moved from api::v1::pond, used by multiple handlers)
// ============================================================================

/// Resolve a pond name from user input, normalizing or auto-generating.
pub fn resolve_pond_name(input: Option<&str>) -> String {
    match input {
        Some(name) if crate::domain::naming::is_valid_pond_name(name) => name.to_string(),
        Some(name) if !name.is_empty() => {
            // User gave a name but not in pond-x-y format — prefix it
            format!("pond-{}", name.to_lowercase().replace(' ', "-"))
        }
        _ => crate::domain::naming::generate_pond_name(),
    }
}

/// Refresh the pond_active flag from certmesh state.
///
/// A stone is "pond active" if either:
/// 1. It is the cornerstone (CA initialized and unlocked), OR
/// 2. It is an enrolled member (has cert + key from a prior enrollment)
pub async fn refresh_pond_active(state: &Moss) {
    // Cornerstone path: CA initialized and unlocked
    if let Ok(handle) = state.discovery.koi().certmesh()
        && let Ok(core) = handle.core()
    {
        let status = core.certmesh_status().await;
        if status.ca_initialized && !status.ca_locked {
            state.security.refresh_active(true);
            return;
        }
    }

    // Enrolled member path: check for enrollment certs on disk
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(&state.current.stone.name);
    if certs_dir.join("cert.pem").exists() && certs_dir.join("key.pem").exists() {
        state.security.refresh_active(true);
    }
}

/// Notify the system that enrollment state changed.
///
/// Updates `pond_active`, updates `PondState`, emits `PondEvent::EnrollmentChanged`
/// on the EventBus, and re-registers mDNS. The enrollment-change listener
/// (spawned at boot) reacts by starting/stopping HTTPS + chirp signing.
pub async fn notify_enrollment_changed(
    state: &Moss,
    enrolled: bool,
    cornerstone: Option<String>,
) {
    // Update enrollment state via aggregate commands
    if enrolled {
        state.security.mark_enrolled(cornerstone.clone()).await;
    } else {
        state.security.mark_unenrolled().await;
    }

    // Emit domain event on EventBus — listener handles HTTPS + chirps
    // (dual stream: SecurityChanged via aggregate + PondEvent via EventBus)
    state
        .event_bus
        .emit(crate::domain::PondEvent::enrollment_changed(
            enrolled,
            cornerstone,
        ));

    // Re-register mDNS with/without pond TXT properties
    {
        let (ip, mac) = garden_common::infra::network::get_local_ip_and_mac();
        if ip != "127.0.0.1" && !ip.is_empty() {
            state.discovery.reregister(&ip, mac.as_deref()).await;
        }
    }

    // Register certmesh CA service on mDNS if this is the cornerstone
    if enrolled {
        state
            .discovery
            .register_certmesh(garden_common::constants::MOSS_HTTP)
            .await;
    }
}
