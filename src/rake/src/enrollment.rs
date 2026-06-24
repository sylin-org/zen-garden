//! Client enrollment — certificate storage and mTLS configuration
//!
//! Handles writing enrollment certificates to disk and loading them
//! for future mTLS connections. Certificate paths match Moss's expected
//! layout so a future Moss installation inherits enrollment automatically.

use std::path::PathBuf;

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
/// Delegates to `os-truststore` (the published crate koi 0.5.0 adopted in place
/// of its old koi-truststore crate, ADR-019) which handles platform differences:
/// - Linux: `update-ca-certificates`
/// - Windows: `certutil -addstore Root`
/// - macOS: `security add-trusted-cert`
///
/// Requires administrator/root privileges. Returns Ok(()) on success,
/// or a warning message if installation fails (non-fatal — mTLS still works
/// for Rake even without system trust store, browsers won't trust though).
pub fn install_ca_in_trust_store(ca_cert_pem: &str) -> Result<(), String> {
    let cert = os_truststore::Cert::from_pem(ca_cert_pem)
        .map_err(|e| format!("Invalid CA certificate: {e}"))?;
    os_truststore::Install::new(&cert)
        .label("zen-garden-pond")
        .run()
        .map(|_| ())
        .map_err(|e| format!("CA trust store installation failed: {e}"))
}

/// Check if the CA is already installed in the system trust store.
///
/// `os-truststore` queries by certificate (there is no label-based lookup), so
/// this takes the CA PEM. Any parse or query failure is treated as "not
/// installed" — the caller then falls through to an idempotent (re)install.
pub fn is_ca_installed(ca_cert_pem: &str) -> bool {
    os_truststore::Cert::from_pem(ca_cert_pem)
        .and_then(|cert| os_truststore::is_installed(&cert))
        .unwrap_or(false)
}
