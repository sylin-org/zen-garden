//! Offerings index management
//!
//! Business logic for:
//! - Building offerings index from ManifestRegistry
//! - Caching compiled offerings with fingerprinting
//! - Template hashing for cache invalidation
//!
//! Composed with compatibility module for rule evaluation.

use crate::domain::compatibility::{compile_compatibility, CompiledCompatibility};
use crate::infra::ManifestRegistry;
use anyhow::Result;
use garden_common::manifests::NetworkRequirements;
use garden_common::TaskDefinition;

/// Compiled offering ready for API consumption
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompiledOffering {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub image: String, // effective image after compatibility evaluation
    /// Named ports: name -> (host_port, container_port)
    /// Convention: "default" is the primary service port
    pub ports: std::collections::HashMap<String, (u16, u16)>,
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,
    pub compatibility: CompiledCompatibility,
    /// Scheduled tasks: name -> definition
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tasks: std::collections::HashMap<String, TaskDefinition>,
    /// Network requirements (static IP preference)
    #[serde(default)]
    pub network: NetworkRequirements,
    /// Whether this offering supports replication across stones (ORCH-0001).
    #[serde(default = "default_replicable")]
    pub replicable: bool,
}

fn default_replicable() -> bool {
    true
}

impl CompiledOffering {
    /// Get the default (primary) port mapping, if any
    pub fn default_port(&self) -> Option<&(u16, u16)> {
        self.ports.get("default")
    }

    /// Get the default host port (for registry/guidance)
    pub fn default_host_port(&self) -> u16 {
        self.default_port().map(|(host, _)| *host).unwrap_or(30000)
    }

    /// Get ports as a flat Vec for Docker (port order: default first, then sorted by name)
    pub fn ports_vec(&self) -> Vec<(u16, u16)> {
        let mut ports = Vec::with_capacity(self.ports.len());

        // Default port first (if present)
        if let Some(p) = self.ports.get("default") {
            ports.push(*p);
        }

        // Then other ports sorted by name
        let mut other_ports: Vec<_> = self.ports.iter().filter(|(k, _)| *k != "default").collect();
        other_ports.sort_by_key(|(k, _)| *k);

        for (_, port) in other_ports {
            ports.push(*port);
        }

        ports
    }
}

/// Fingerprint for cache invalidation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OfferingsFingerprint {
    pub moss_version: String,
    pub capabilities_hash: String,
    pub templates_hash: String,
}

/// Cached offerings index with fingerprint
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OfferingsIndexCache {
    pub fingerprint: OfferingsFingerprint,
    pub generated_at: String,
    pub offerings: Vec<CompiledOffering>,
}

/// Get moss version string (from Cargo.toml + build number)
pub fn moss_version_string() -> String {
    // build.rs injects BUILD_NUMBER (see src/moss/src/discovery.rs)
    format!("{}.{}", env!("CARGO_PKG_VERSION"), env!("BUILD_NUMBER"))
}

/// Hash arbitrary bytes with BLAKE3
fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Generate capabilities hash for fingerprinting
///
/// Includes CPU, memory, GPU/AI capabilities.
/// Changes trigger offerings index rebuild.
pub fn current_capabilities_hash(
    cached_capabilities: Option<&garden_common::HardwareCapabilities>,
) -> String {
    let caps = crate::domain::compatibility::get_current_compat_capabilities(cached_capabilities);

    let payload = serde_json::json!({
        "cpu_model": caps.cpu_model,
        "cpu_features": caps.cpu_features,
        "architecture": caps.architecture,
        "total_memory_mb": caps.total_memory_mb,
        "has_cuda": caps.has_cuda,
        "has_rocm": caps.has_rocm,
        "has_directml": caps.has_directml,
        "has_openvino": caps.has_openvino,
        "gpu_vram_total_mb": caps.gpu_vram_total_mb,
    });
    blake3_hex(serde_json::to_vec(&payload).unwrap_or_default().as_slice())
}

/// Compute hash of all manifests for cache invalidation
///
/// Includes moss version, offering names, and all configuration.
/// Changes trigger offerings index rebuild.
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

/// Ensure offerings index is loaded (with caching)
///
/// Loads offerings index from cache or rebuilds if:
/// - Cache doesn't exist
/// - force_rebuild is true
/// - Fingerprint doesn't match (version/capabilities/manifests changed)
///
/// # Parameters
/// - `state`: Application state with manifest_registry and offerings_index
/// - `force_rebuild`: Skip cache and force rebuild
///
/// # Composability
/// This function manages AppState's offerings_index cache.
/// It delegates to:
/// - `load_offerings_cache()` for disk persistence (infra layer)
/// - `rebuild_offerings_index()` for index generation (domain layer)
/// - `save_offerings_cache()` for disk persistence (infra layer)
pub async fn ensure_offerings_index(state: &crate::AppState, force_rebuild: bool) -> Result<()> {
    if !force_rebuild {
        let existing = state.offerings_index.read().await;
        if existing.is_some() {
            return Ok(());
        }
    }

    // Snapshot cached capabilities once
    let cached_caps = state.capabilities.read().await.clone();
    let cached_caps_ref = cached_caps.as_ref();

    // Try disk cache first (best-effort)
    if !force_rebuild {
        if let Some(on_disk) = crate::infra::load_offerings_cache::<OfferingsIndexCache>().await? {
            let current = OfferingsFingerprint {
                moss_version: moss_version_string(),
                capabilities_hash: current_capabilities_hash(cached_caps_ref),
                templates_hash: manifests_hash(&state.manifest_registry)?,
            };

            if on_disk.fingerprint == current {
                *state.offerings_index.write().await = Some(on_disk);
                return Ok(());
            }
        }
    }

    let rebuilt = rebuild_offerings_index(&state.manifest_registry, cached_caps_ref)?;
    crate::infra::save_offerings_cache(&rebuilt).await?;
    *state.offerings_index.write().await = Some(rebuilt);
    Ok(())
}

/// Get a compiled offering by name
///
/// Ensures offerings index is loaded, then queries for specific offering.
///
/// # Returns
/// - `Ok(Some(offering))`: Offering found
/// - `Ok(None)`: Offering not found
/// - `Err(_)`: Failed to load offerings index
///
/// # Composability
/// This function ensures index is loaded before querying.
/// Delegates to `ensure_offerings_index()` for cache management.
pub async fn get_compiled_offering(
    state: &crate::AppState,
    offering: &str,
) -> Result<Option<CompiledOffering>> {
    ensure_offerings_index(state, false).await?;
    let guard = state.offerings_index.read().await;
    Ok(guard
        .as_ref()
        .and_then(|idx| idx.offerings.iter().find(|o| o.name == offering).cloned()))
}

/// Rebuild offerings index from ManifestRegistry
///
/// Evaluates compatibility rules and compiles all offerings.
/// Returns cache-ready index with fingerprint.
pub fn rebuild_offerings_index(
    registry: &ManifestRegistry,
    cached_capabilities: Option<&garden_common::HardwareCapabilities>,
) -> Result<OfferingsIndexCache> {
    let mut entries: Vec<_> = registry.sw.entries.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let fingerprint = OfferingsFingerprint {
        moss_version: moss_version_string(),
        capabilities_hash: current_capabilities_hash(cached_capabilities),
        templates_hash: manifests_hash(registry)?,
    };

    let mut offerings = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut template = entry.parse_template()?;
        let compatibility = compile_compatibility(&mut template, cached_capabilities);

        offerings.push(CompiledOffering {
            name: entry.name.clone(),
            category: entry.category.clone(),
            description: entry.description(),
            tags: entry.tags(),
            image: template.image,
            ports: template.ports,
            environment: template.environment,
            volumes: template.volumes,
            compatibility,
            tasks: template.tasks,
            network: template.network,
            replicable: entry.replicable,
        });
    }

    Ok(OfferingsIndexCache {
        fingerprint,
        generated_at: chrono::Utc::now().to_rfc3339(),
        offerings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moss_version_string() {
        let version = moss_version_string();
        assert!(version.contains('.'));
    }

    #[test]
    fn test_capabilities_hash_stable() {
        let hash1 = current_capabilities_hash(None);
        let hash2 = current_capabilities_hash(None);
        // Hash should be stable for same capabilities
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_empty());
    }

    #[test]
    fn test_fingerprint_equality() {
        let fp1 = OfferingsFingerprint {
            moss_version: "1.0.0".into(),
            capabilities_hash: "abc123".into(),
            templates_hash: "def456".into(),
        };
        let fp2 = fp1.clone();
        assert_eq!(fp1, fp2);
    }
}
