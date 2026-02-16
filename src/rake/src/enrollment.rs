//! Client enrollment — certificate storage and mTLS configuration
//!
//! Handles writing enrollment certificates to disk and loading them
//! for future mTLS connections. Certificate paths match Moss's expected
//! layout so a future Moss installation inherits enrollment automatically.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Enrollment metadata persisted alongside certificates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PondEnrollment {
    pub pond_name: String,
    pub cornerstone: String,
    pub ca_fingerprint: String,
    pub enrolled_at: String,
    pub cert_expires: String,
    pub role: String,
}

/// Get the certificate directory for a given hostname.
///
/// Returns `{data_dir}/koi/certs/{hostname}/` — the same path Moss reads from.
/// On Linux: `/var/lib/zen-garden/koi/certs/{hostname}/`
/// On Windows: `.zen-garden/koi/certs/{hostname}/` (relative to cwd)
pub fn certs_dir(hostname: &str) -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(hostname)
}

/// Write enrollment certificates to disk.
///
/// Creates `{data_dir}/koi/certs/{hostname}/` with:
/// - `cert.pem`      — service certificate (0644)
/// - `key.pem`       — private key (0600 on Unix)
/// - `ca.pem`        — CA public certificate (0644)
/// - `fullchain.pem` — cert.pem + ca.pem concatenated (0644)
pub fn write_enrollment_certs(
    hostname: &str,
    ca_cert: &str,
    service_cert: &str,
    service_key: &str,
) -> Result<PathBuf> {
    let dir = certs_dir(hostname);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create cert directory: {}", dir.display()))?;

    std::fs::write(dir.join("cert.pem"), service_cert)
        .context("Failed to write cert.pem")?;

    std::fs::write(dir.join("key.pem"), service_key)
        .context("Failed to write key.pem")?;

    // Restrict key.pem permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.join("key.pem"),
            std::fs::Permissions::from_mode(0o600),
        )?;
    }

    std::fs::write(dir.join("ca.pem"), ca_cert)
        .context("Failed to write ca.pem")?;

    // fullchain = service cert + CA cert
    let fullchain = format!("{}\n{}", service_cert.trim(), ca_cert.trim());
    std::fs::write(dir.join("fullchain.pem"), fullchain)
        .context("Failed to write fullchain.pem")?;

    Ok(dir)
}

/// Write enrollment metadata to `.pond-enrollment.json` in the certs directory.
pub fn write_enrollment_metadata(hostname: &str, enrollment: &PondEnrollment) -> Result<()> {
    let dir = certs_dir(hostname);
    let path = dir.join(".pond-enrollment.json");
    let json = serde_json::to_string_pretty(enrollment)?;
    std::fs::write(&path, json)
        .with_context(|| format!("Failed to write enrollment metadata: {}", path.display()))?;
    Ok(())
}

/// Load enrollment metadata for a hostname, if it exists.
pub fn load_enrollment(hostname: &str) -> Option<PondEnrollment> {
    let path = certs_dir(hostname).join(".pond-enrollment.json");
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Check if this machine is enrolled in a pond (has valid cert files).
pub fn is_enrolled(hostname: &str) -> bool {
    let dir = certs_dir(hostname);
    dir.join("cert.pem").exists() && dir.join("key.pem").exists()
}

/// Load CA certificate and client identity for building an mTLS reqwest client.
///
/// Returns `(ca_cert_pem, client_cert_pem, client_key_pem)` if enrollment
/// certs exist, or `None` if not enrolled.
pub fn load_tls_materials(hostname: &str) -> Option<(String, String, String)> {
    let dir = certs_dir(hostname);
    let ca_cert = std::fs::read_to_string(dir.join("ca.pem")).ok()?;
    let client_cert = std::fs::read_to_string(dir.join("cert.pem")).ok()?;
    let client_key = std::fs::read_to_string(dir.join("key.pem")).ok()?;
    Some((ca_cert, client_cert, client_key))
}

/// Install the CA certificate into the system trust store.
///
/// Delegates to `koi_truststore` which handles platform differences:
/// - Linux: `update-ca-certificates`
/// - Windows: `certutil -addstore Root`
/// - macOS: `security add-trusted-cert`
///
/// Requires administrator/root privileges. Returns Ok(()) on success,
/// or a warning message if installation fails (non-fatal — mTLS still works
/// for Rake even without system trust store, browsers won't trust though).
pub fn install_ca_in_trust_store(ca_cert_pem: &str) -> Result<(), String> {
    koi_truststore::install_ca_cert(ca_cert_pem, "zen-garden-pond")
        .map_err(|e| format!("CA trust store installation failed: {e}"))
}

/// Check if the CA is already installed in the system trust store.
pub fn is_ca_installed() -> bool {
    koi_truststore::is_ca_installed("zen-garden-pond")
}
