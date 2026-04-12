//! Fingerprint helpers for catalog-cache invalidation.
//!
//! Moved from `domain/offerings/catalog.rs` in Ch2 of ARCH-0022
//! (Book V of ARCH-0017). The three functions compute stable hash
//! payloads that feed [`super::index::OfferingsFingerprint`]:
//!
//! - [`moss_version_string`] — build-time version + build number
//!   (env injected by `build.rs`).
//! - [`current_capabilities_hash`] — hash of the hardware facts
//!   relevant to compatibility rule evaluation.
//! - [`manifests_hash`] — hash of every manifest's effective compiled
//!   template, in stable order, including the moss version so
//!   schema/parsing changes invalidate every cache on upgrade.
//!
//! Visibility: `pub` during Ch2 so the remaining free functions in
//! `domain/offerings/catalog.rs` can still call them. Ch3 absorbs all
//! three into the aggregate's private `rebuild` command path and
//! flips them to `pub(super)`.

use anyhow::Result;
use garden_common::manifests::ManifestRegistry;

/// Get moss version string (from Cargo.toml + build number).
pub fn moss_version_string() -> String {
    // build.rs injects `BUILD_NUMBER` (see src/moss/src/discovery.rs).
    format!("{}.{}", env!("CARGO_PKG_VERSION"), env!("BUILD_NUMBER"))
}

/// Hash arbitrary bytes with BLAKE3.
fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Generate capabilities hash for fingerprinting.
///
/// Includes CPU, memory, GPU/AI capabilities. Changes trigger an
/// offerings-index rebuild.
pub fn current_capabilities_hash(
    cached_capabilities: Option<&garden_common::HardwareCapabilities>,
) -> String {
    let payload = if let Some(caps) = cached_capabilities {
        use garden_common::compatibility::FactSource;
        serde_json::json!({
            "architecture": caps.resolve_scalar(garden_common::compatibility::Fact::Architecture),
            "os_family": caps.resolve_scalar(garden_common::compatibility::Fact::OsFamily),
            "cpu_model": caps.resolve_scalar(garden_common::compatibility::Fact::CpuModel),
            "cpu_features": caps.resolve_set(garden_common::compatibility::Fact::CpuFeatures).into_iter().collect::<Vec<_>>(),
            "ram_total_mb": caps.resolve_numeric(garden_common::compatibility::Fact::RamTotalMb),
            "gpu_capabilities": caps.resolve_set(garden_common::compatibility::Fact::AiRuntime).into_iter().collect::<Vec<_>>(),
            "gpu_vram_total_mb": caps.resolve_numeric(garden_common::compatibility::Fact::GpuVramTotalMb),
        })
    } else {
        serde_json::json!({})
    };
    blake3_hex(serde_json::to_vec(&payload).unwrap_or_default().as_slice())
}

/// Compute hash of all manifests for cache invalidation.
///
/// Includes moss version, offering names, and all configuration.
/// Changes trigger an offerings-index rebuild.
pub fn manifests_hash(registry: &ManifestRegistry) -> Result<String> {
    let mut hasher = blake3::Hasher::new();

    // Include moss version in the hash so schema/parsing changes
    // can't accidentally reuse an old cache.
    hasher.update(moss_version_string().as_bytes());

    // Hash each offering's effective config in stable order.
    let mut entries: Vec<_> = registry.sw.entries.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    for entry in entries {
        let template = entry.parse_template()?;
        let payload = serde_json::json!({
            "name": entry.name,
            "category": entry.category,
            "description": entry.description(),
            "tags": entry.tags(),
            "image": template.image,
            "ports": template.ports,
            "environment": template.environment,
            "volumes": template.volumes,
            "compatibility": template.compatibility,
        });
        hasher.update(serde_json::to_vec(&payload).unwrap_or_default().as_slice());
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moss_version_string_is_stable() {
        let version = moss_version_string();
        assert!(version.contains('.'));
    }

    #[test]
    fn capabilities_hash_is_stable() {
        let hash1 = current_capabilities_hash(None);
        let hash2 = current_capabilities_hash(None);
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn fingerprint_equality() {
        let fp1 = super::super::index::OfferingsFingerprint {
            moss_version: "1.0.0".into(),
            capabilities_hash: "abc123".into(),
            templates_hash: "def456".into(),
        };
        let fp2 = fp1.clone();
        assert_eq!(fp1, fp2);
    }
}
