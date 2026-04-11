//! Service infrastructure helpers (shared between lifecycle and API handlers).
//!
//! These functions bridge domain decisions with infra execution:
//! spec composition, container rebuild, compose-on-start.

use anyhow::Context;
use garden_common::offerings::OfferingFqn;

use crate::AppState;

/// Build a Docker container spec from the offering manifest + config patches.
///
/// Uses the **CompiledOffering** as the base (hardware-resolved image,
/// device_requests, command, env, volumes, ports), then applies config patch
/// overrides. Falls back to the raw template only when the compiled offering
/// is unavailable.
///
/// This matches the reference implementation in `install_service_task` which
/// also uses `compiled.*` for all spec fields.
pub async fn build_spec_from_manifest(
    state: &AppState,
    service_name: &str,
) -> anyhow::Result<crate::docker::ContainerSpec> {
    let patches = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.fqn_eq(service_name) && o.is_managed())
            .and_then(|o| o.managed_data())
            .map(|d| d.config_patches.clone())
            .unwrap_or_default()
    };

    // Resolve the offering type (strip instance suffix for FQN lookups)
    let fqn = OfferingFqn::parse(service_name).context("Invalid offering FQN")?;
    let offering_type = fqn.offering.clone();

    // Primary path: use the CompiledOffering (hardware-resolved image,
    // device_requests, environment, command, volumes, ports, config_files).
    // This is the same source that install_service_task uses.
    let compiled = crate::get_compiled_offering(
        state,
        &offering_type,
        &crate::infra::persistence::OsOfferingsCache,
    )
    .await;

    let (image, command, ports, environment, volumes, config_files, device_requests) =
        match compiled {
            Ok(Some(compiled)) => {
                // Resolve FQN-specific volume paths (e.g., comfyui::prod isolation)
                let fqn_volumes = compiled.volumes_for_fqn(&fqn);
                let ports = compiled.ports_vec();
                (
                    compiled.image,
                    compiled.command,
                    ports,
                    compiled.environment,
                    fqn_volumes,
                    compiled.config_files,
                    compiled.device_requests,
                )
            }
            Ok(None) | Err(_) => {
                // Fallback: use raw template (no hardware resolution).
                // This path should rarely execute — the compiled index is
                // always available in production.
                if compiled.is_err() {
                    tracing::warn!(
                        service = %service_name,
                        "Failed to read compiled offerings index, using raw manifest template"
                    );
                } else {
                    tracing::debug!(
                        service = %service_name,
                        "No compiled offering found, using raw manifest template"
                    );
                }
                let manifest = state
                    .manifest_registry
                    .get_offering(&offering_type)
                    .context("No manifest for offering")?;
                let template = manifest
                    .parse_template_for_fqn(&fqn)
                    .context("Failed to parse template")?;
                let ports = template.ports_vec();
                (
                    template.image,
                    template.command,
                    ports,
                    template.environment,
                    template.volumes,
                    template.config_files,
                    template.device_requests,
                )
            }
        };

    // Apply config patch overrides (command, env, volumes — same logic as compose)
    let mut effective_command = command;
    let mut effective_env = environment;
    let mut effective_volumes = volumes;

    let mut sorted_patches: Vec<&garden_common::types::ConfigPatch> = patches.iter().collect();
    sorted_patches.sort_by_key(|p| p.applied_at);

    for patch in sorted_patches {
        if let Some(ref cmd) = patch.command {
            effective_command = Some(cmd.clone());
        }
        for (key, value) in &patch.environment {
            effective_env.retain(|e| !e.starts_with(&format!("{}=", key)));
            effective_env.push(format!("{}={}", key, value));
        }
        for volume in &patch.volumes {
            let already_mounted = effective_volumes
                .iter()
                .any(|(_, container)| container == &volume.1);
            if !already_mounted {
                effective_volumes.push(volume.clone());
            }
        }
    }

    Ok(crate::docker::ContainerSpec {
        image,
        command: effective_command,
        ports,
        environment: effective_env,
        volumes: effective_volumes,
        config_files,
        device_requests,
    })
}

/// Result of a reconciliation — carries port change info for the caller
/// to decide whether the offering registry needs updating.
pub struct ReconcileResult {
    /// The actual port bindings from Docker after container creation.
    pub resolved_ports: Vec<(u16, u16)>,
    /// Whether any port binding differs from the stored offering state.
    pub ports_changed: bool,
    /// Primary host port (first/default port from resolved bindings).
    pub primary_port: Option<u16>,
    /// Named port map for remapped ports (name → actual_host_port).
    /// Only contains entries where the actual port differs from manifest default.
    pub port_map: std::collections::HashMap<String, u16>,
}

impl ReconcileResult {
    /// Apply port updates to the offering registry if ports were remapped
    /// during reconciliation. Centralizes the writeback logic so callers
    /// (health monitor, service_lifecycle::start) don't duplicate it.
    pub async fn apply_port_updates(&self, state: &AppState, offering_id: &str) {
        if !self.ports_changed {
            return;
        }
        let primary = self.primary_port;
        let new_port_map = self.port_map.clone();
        state
            .offerings
            .update(offering_id, |o| {
                if let Some(port) = primary {
                    o.location.port = port;
                }
                o.location.port_map = new_port_map;
                true
            })
            .await;
        tracing::info!(
            offering_id = %offering_id,
            "Reconciliation required port remap, registry updated"
        );
    }
}

/// Reconcile a missing container for a registered managed offering (OFFER-0008).
///
/// Unlike the old `rebuild_missing_container`, this function:
/// 1. Reads stored port mappings from the offering registry
/// 2. Builds the spec with stored ports (not manifest defaults)
/// 3. Passes through `install_service` (which handles port conflicts via remapping)
/// 4. Returns whether ports changed so the caller can update the registry
///
/// If the container already exists (partial prior attempt), it is started
/// rather than reinstalled.
pub async fn reconcile_offering(
    state: &AppState,
    service_name: &str,
) -> anyhow::Result<ReconcileResult> {
    // 1. Snapshot stored offering state (brief read lock, no await across it)
    let stored_port_map = {
        let offerings = state.offerings.read().await;
        let offering = offerings
            .iter()
            .find(|o| o.name.fqn_eq(service_name) && o.is_managed())
            .context("Managed offering not found in registry")?;
        offering.location.port_map.clone()
    };

    // 2. Handle partial prior attempt: container exists but is stopped
    if state
        .platform
        .docker
        .zen_container_exists(service_name)
        .await
        .unwrap_or(false)
    {
        tracing::info!(
            service = %service_name,
            "Container exists (partial prior attempt), starting it"
        );
        state
            .platform
            .docker
            .start_service(service_name, Some(&state.console))
            .await
            .context("Failed to start existing container")?;
        return Ok(ReconcileResult {
            resolved_ports: vec![],
            ports_changed: false,
            primary_port: None,
            port_map: std::collections::HashMap::new(),
        });
    }

    // 3. Build spec from manifest + config patches
    let mut spec = build_spec_from_manifest(state, service_name)
        .await
        .context("Failed to build container spec from manifest")?;

    // 4. Override spec ports with stored mappings (OFFER-0008)
    //    spec.ports contains the effective ports (manifest + patches). We keep
    //    the container ports as-is but replace host ports with stored values
    //    from the offering registry (so remapped ports survive a Docker wipe).
    let fqn =
        OfferingFqn::parse(service_name).context("Invalid offering FQN for reconciliation")?;
    let port_name_keys = resolve_port_name_keys(state, &fqn);
    if port_name_keys.len() == spec.ports.len() {
        // Name keys align with spec ports — apply stored overrides
        spec.ports = port_name_keys
            .iter()
            .zip(spec.ports.iter())
            .map(|(name, &(effective_host, container))| {
                let host = stored_port_map.get(name).copied().unwrap_or(effective_host);
                (host, container)
            })
            .collect();
    } else if !port_name_keys.is_empty() || !spec.ports.is_empty() {
        tracing::warn!(
            service = %service_name,
            port_names = port_name_keys.len(),
            spec_ports = spec.ports.len(),
            "Port name keys do not match spec ports — stored port mappings abandoned"
        );
    }

    // 5. Install container (port scanning handles conflicts via remapping)
    let resolved = state
        .platform
        .docker
        .install_service(service_name, &spec, Some(&state.console))
        .await
        .context("Failed to install container during reconciliation")?;

    // 6. Determine if ports changed from what was stored
    let ports_changed = resolved != spec.ports;

    // 7. Build primary port and named port map for registry update (PORT-0001)
    let primary_port = resolved.first().map(|(h, _)| *h);
    let port_map = if port_name_keys.len() == resolved.len() {
        // Build port_map: only entries where actual differs from effective default
        port_name_keys
            .iter()
            .zip(spec.ports.iter().zip(resolved.iter()))
            .filter_map(|(name, ((effective_host, _), (actual_host, _)))| {
                if actual_host != effective_host {
                    Some((name.clone(), *actual_host))
                } else {
                    None
                }
            })
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    Ok(ReconcileResult {
        resolved_ports: resolved,
        ports_changed,
        primary_port,
        port_map,
    })
}

/// Derive the ordered port name keys from the manifest template.
///
/// Returns names in the same order as `ServiceTemplate::ports_vec()`:
/// "default" first (if present), then remaining names alphabetically.
/// Used by `reconcile_offering` to map stored port_map overrides to spec
/// ports and to build the named port_map after reconciliation.
fn resolve_port_name_keys(state: &AppState, fqn: &OfferingFqn) -> Vec<String> {
    let Some(manifest) = state.manifest_registry.get_offering(&fqn.offering) else {
        return Vec::new();
    };
    let Ok(template) = manifest.parse_template_for_fqn(fqn) else {
        return Vec::new();
    };
    let mut keys = Vec::with_capacity(template.ports.len());
    if template.ports.contains_key("default") {
        keys.push("default".to_string());
    }
    let mut others: Vec<_> = template.ports.keys().filter(|k| *k != "default").collect();
    others.sort();
    keys.extend(others.into_iter().cloned());
    keys
}

/// Compose-on-start: check if a stopped container needs to be recreated
/// to match the effective config (manifest + patches).
///
/// This ensures config patches are applied even after a container restart or
/// Moss daemon restart. If the container spec doesn't match the desired config,
/// it is removed and recreated.
pub async fn compose_on_start(state: &AppState, service_name: &str) -> anyhow::Result<()> {
    // Only run if there are config patches to apply
    let has_patches = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.fqn_eq(service_name) && o.is_managed())
            .and_then(|o| o.managed_data())
            .map(|d| !d.config_patches.is_empty())
            .unwrap_or(false)
    };

    if !has_patches {
        return Ok(());
    }

    let desired_spec = build_spec_from_manifest(state, service_name)
        .await
        .context("Failed to build spec for compose-on-start")?;

    // Check if container needs cycling
    match state
        .platform
        .docker
        .needs_cycle(service_name, &desired_spec)
        .await
    {
        Ok(true) => {
            tracing::info!(
                service = %service_name,
                "Compose-on-start: container spec mismatch, cycling"
            );
            state
                .platform
                .docker
                .recreate_service(service_name, &desired_spec)
                .await
                .context("Failed to recreate container for compose-on-start")?;
        }
        Ok(false) => {
            tracing::debug!(
                service = %service_name,
                "Compose-on-start: container already matches desired config"
            );
        }
        Err(e) => {
            tracing::warn!(
                service = %service_name,
                error = ?e,
                "Compose-on-start: could not inspect container, will start as-is"
            );
        }
    }

    Ok(())
}
