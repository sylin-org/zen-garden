//! Compiled offering — the per-offering shape published by the
//! `Catalog` aggregate.
//!
//! Moved from `domain/offerings/catalog.rs` in Ch2 of ARCH-0022
//! (Book V of ARCH-0017). Type name and public shape are preserved —
//! 8 non-module files across the crate reference `CompiledOffering`
//! directly (`placement.rs`, `api/v1/offerings.rs`, `api/v1/updates.rs`,
//! `services_internal.rs`, `service_lifecycle.rs`, `offering_resolution
//! .rs`, `ceremony/phases/nourish.rs`, `job_executors.rs`), so renaming
//! would cascade without architectural benefit.

use crate::domain::compatibility::CompiledCompatibility;
use garden_common::TaskDefinition;
use garden_common::manifests::NetworkRequirements;

/// Compiled offering ready for API consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompiledOffering {
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Effective image after compatibility evaluation.
    pub image: String,
    /// Manifest-level command override (e.g., whisper-server args).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Config file mappings for file-based configuration injection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_files: Vec<garden_common::manifests::ConfigFileMapping>,
    /// Named ports: `name → (host_port, container_port)`.
    /// Convention: `"default"` is the primary service port.
    pub ports: std::collections::HashMap<String, (u16, u16)>,
    pub environment: Vec<String>,
    pub volumes: Vec<(String, String)>,
    pub compatibility: CompiledCompatibility,
    /// Scheduled tasks: `name → definition`.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tasks: std::collections::HashMap<String, TaskDefinition>,
    /// Network requirements (static IP preference).
    #[serde(default)]
    pub network: NetworkRequirements,
    /// How instances coordinate across stones (ORCH-0006).
    /// `Independent` (default) = no election. `Elected` = Primary/Dormant roles.
    #[serde(default)]
    pub coordination: garden_common::CoordinationMode,
    /// GPU device requests from manifest `deploy.resources.reservations.devices`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_requests: Vec<garden_common::manifests::GpuDeviceRequest>,
}

impl CompiledOffering {
    /// Get the default (primary) port mapping, if any.
    pub fn default_port(&self) -> Option<&(u16, u16)> {
        self.ports.get("default")
    }

    /// Get the default host port (for registry/guidance).
    pub fn default_host_port(&self) -> u16 {
        self.default_port().map(|(host, _)| *host).unwrap_or(30000)
    }

    /// Get ports as named tuples: `(name, host_port, container_port)`.
    /// Order: `"default"` first, then remaining ports sorted by name.
    pub fn ports_vec_named(&self) -> Vec<(String, u16, u16)> {
        let mut ports = Vec::with_capacity(self.ports.len());

        // Default port first (if present).
        if let Some(&(h, c)) = self.ports.get("default") {
            ports.push(("default".to_string(), h, c));
        }

        // Then other ports sorted by name.
        let mut other_ports: Vec<_> = self.ports.iter().filter(|(k, _)| *k != "default").collect();
        other_ports.sort_by_key(|(k, _)| *k);

        for (name, &(h, c)) in other_ports {
            ports.push((name.clone(), h, c));
        }

        ports
    }

    /// Get ports as a flat `Vec` for Docker (port order: default first, then sorted by name).
    pub fn ports_vec(&self) -> Vec<(u16, u16)> {
        self.ports_vec_named()
            .into_iter()
            .map(|(_, h, c)| (h, c))
            .collect()
    }

    /// Remap volume host paths for FQN-specific isolation.
    ///
    /// The compiled offering caches volumes under the offering name directory.
    /// When deploying a named instance (e.g., `comfyui::prod`), this remaps
    /// the paths to use the FQN-encoded directory instead:
    /// - `comfyui`       → `{volumes_dir}/comfyui/...`
    /// - `comfyui::prod` → `{volumes_dir}/comfyui--prod/...`
    pub fn volumes_for_fqn(
        &self,
        fqn: &garden_common::offerings::OfferingFqn,
    ) -> Vec<(String, String)> {
        let encoded = fqn.encoded_for_container();
        if encoded == self.name {
            return self.volumes.clone();
        }

        let base = garden_common::constants::paths::volumes_dir();
        let old_prefix = format!("{}/{}/", base, self.name);
        let new_prefix = format!("{}/{}/", base, encoded);

        self.volumes
            .iter()
            .map(|(host, container)| {
                let remapped = if host.starts_with(&old_prefix) {
                    format!("{}{}", new_prefix, &host[old_prefix.len()..])
                } else {
                    host.clone()
                };
                (remapped, container.clone())
            })
            .collect()
    }
}
