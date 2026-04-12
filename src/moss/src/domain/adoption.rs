//! Container adoption domain logic
//!
//! Handles discovery and adoption of existing Zen Garden containers:
//! - Validates containers against templates
//! - Evaluates compatibility and detects image mismatches
//! - Registers adopted containers in the service registry
//!
//! This is pure domain logic - delegates I/O to infra layer.

use crate::AppState;
use crate::docker::ContainerRuntime;
use crate::domain::compatibility::{CompatibilityDecision, evaluate_compatibility};
use crate::domain::connection;
use garden_common::manifests::ManifestRegistry;
use garden_common::offerings::OfferingFqn;
use garden_common::utils::ids::generate_guidv7;
use garden_common::{
    HardwareCapabilities, ManagedData, Offering, OfferingGuidance, OfferingLocation,
    OfferingModeData, OfferingStatus, ServiceHealthStatus,
};

/// Adopt a container for a specific offering into the registry
///
/// Validates that:
/// 1. The offering has a known template/manifest
/// 2. Compatibility rules are evaluated (may trigger fallback image)
/// 3. Running image matches expected image (or marks as degraded)
///
/// # Returns
/// - `Ok(Some(Offering))`: Container successfully adopted
/// - `Ok(None)`: No template found for offering (container left alone)
/// - `Err(_)`: Adoption failed (Client API error)
///
/// # Composability
/// This function is pure domain logic - it doesn't modify state directly.
/// Callers are responsible for:
/// - Adding returned Offering to registry
/// - Persisting registry changes
/// - Emitting events
pub async fn adopt_offering_container(
    docker: &ContainerRuntime,
    manifest_registry: &ManifestRegistry,
    offering: &str,
    stone_name: &str,
    cached_capabilities: Option<&HardwareCapabilities>,
) -> anyhow::Result<Option<Offering>> {
    let fqn = OfferingFqn::parse(offering)
        .map_err(|e| anyhow::anyhow!("Invalid offering name '{}': {}", offering, e))?;
    let offering_name = fqn.fqn();
    let offering_type = fqn.offering.clone();

    // Only adopt if the offering maps to a known template (valid manifest/template).
    let entry = match manifest_registry.sw.get(&offering_type) {
        Some(e) => e,
        None => return Ok(None),
    };

    let mut template = match entry.parse_template_for_fqn(&fqn) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    // Store guidance template for later (will be processed after we know the port)
    let guidance_template = entry.guidance.clone();

    // Compute expected image based on compatibility rules.
    if let Some(rules) = &template.compatibility
        && let Some(caps) = cached_capabilities
    {
        match evaluate_compatibility(rules, caps) {
            CompatibilityDecision::Pass => {}
            CompatibilityDecision::Warning { .. } => {
                // Warning: proceed with caution, but don't change image
            }
            CompatibilityDecision::Fallback { image, .. } => template.image = image,
            CompatibilityDecision::Fail { .. } => {
                // Leave container alone, but adopt it as degraded/incompatible.
            }
        }
    }

    let service_status = docker
        .get_service_status(&offering_name)
        .await
        .unwrap_or(garden_common::ServiceStatus::Unknown);
    let mut health = docker
        .get_service_health(&offering_name)
        .await
        .unwrap_or(ServiceHealthStatus::Offline);

    let actual_image = docker
        .get_service_image(&offering_name)
        .await
        .unwrap_or_else(|_| "<unknown>".to_string());
    let expected_image = template.image.clone();

    // If the running image doesn't match what we'd expect (including compatibility fallback), mark degraded.
    if actual_image != "<unknown>" && actual_image != expected_image {
        health = ServiceHealthStatus::Degraded;
    }

    let native_port = template.default_host_port();
    let version = actual_image
        .split(':')
        .next_back()
        .unwrap_or("latest")
        .to_string();

    // Query Client for actual port bindings (may differ from manifest if remapped)
    let docker_ports = docker
        .get_container_ports(&offering_name)
        .await
        .unwrap_or_default();
    let actual_port = docker_ports.first().map(|(h, _)| *h).unwrap_or(native_port);

    // Build a lookup from container_port → actual_host_port for guidance substitution
    let docker_port_map: std::collections::HashMap<u16, u16> =
        docker_ports.iter().map(|(h, c)| (*c, *h)).collect();

    // Build guidance with template substitution (if guidance template exists)
    let guidance = guidance_template.map(|tmpl| {
        // Substitute named ports using actual Client bindings (fall back to manifest)
        let mut content = tmpl.clone();
        for (port_name, (template_host, container_port)) in &template.ports {
            let host_port = docker_port_map
                .get(container_port)
                .copied()
                .unwrap_or(*template_host);
            let placeholder = if port_name == "default" {
                "{{port}}".to_string()
            } else {
                format!("{{{{{}-port}}}}", port_name)
            };
            content = content.replace(&placeholder, &host_port.to_string());
        }
        content = content
            .replace("{{server-name}}", stone_name)
            .replace("{{offering}}", &offering_type)
            .replace("{{name}}", &offering_name);

        // Build variables map for API consumers (using actual Client ports)
        let mut variables = std::collections::HashMap::new();
        variables.insert("port".to_string(), actual_port.to_string());
        for (port_name, (template_host, container_port)) in &template.ports {
            if port_name != "default" {
                let host_port = docker_port_map
                    .get(container_port)
                    .copied()
                    .unwrap_or(*template_host);
                variables.insert(format!("{}-port", port_name), host_port.to_string());
            }
        }
        variables.insert("server-name".to_string(), stone_name.to_string());
        variables.insert("offering".to_string(), offering_type.to_string());
        variables.insert("name".to_string(), offering_name.to_string());

        OfferingGuidance { content, variables }
    });

    // Convert ServiceStatus to OfferingStatus
    let status = match (&health, service_status) {
        (ServiceHealthStatus::Degraded, garden_common::ServiceStatus::Running) => {
            OfferingStatus::Degraded
        }
        (_, garden_common::ServiceStatus::Running) => OfferingStatus::Running,
        (_, garden_common::ServiceStatus::Stopped) => OfferingStatus::Stopped,
        (_, garden_common::ServiceStatus::Installing) => OfferingStatus::Installing,
        (_, garden_common::ServiceStatus::Degraded) => OfferingStatus::Degraded,
        (_, garden_common::ServiceStatus::Maintenance) => OfferingStatus::Maintenance,
        (_, garden_common::ServiceStatus::Unknown) => OfferingStatus::Unknown,
        (_, garden_common::ServiceStatus::Cordoned) => OfferingStatus::Cordoned,
    };
    let protocol = connection::infer_protocol_from_manifest_metadata(
        &offering_type,
        &entry.category,
        entry.connection.as_ref(),
    );

    let adopted = Offering {
        offering_id: generate_guidv7(),
        name: fqn,
        offering: offering_type,
        category: entry.category.clone(),
        version,
        status,
        health,
        sub_capabilities: Vec::new(),
        location: OfferingLocation {
            host: "localhost".to_string(),
            port: actual_port,
            protocol,
            agnostic_port: None,
            // Propagate detected ports so topology consumers know the actual port
            // (PORT-0001: only ports that differ from manifest defaults).
            port_map: {
                let mut pm = std::collections::HashMap::new();
                // Always include "default" so orchestrators can find the service
                pm.insert("default".to_string(), actual_port);
                // Include any named ports from the manifest with their actual bindings
                for (port_name, (template_host, container_port)) in &template.ports {
                    let host_port = docker_port_map
                        .get(container_port)
                        .copied()
                        .unwrap_or(*template_host);
                    pm.insert(port_name.clone(), host_port);
                }
                pm
            },
        },
        mode_data: OfferingModeData::Managed(ManagedData {
            resources: None,
            job_id: None,
            guidance,
            ..Default::default()
        }),
        registered_at: chrono::Utc::now(),
        updated_at: None,
        orchestration: None,
    };

    Ok(Some(adopted))
}

/// Adopt all existing Zen Garden containers that aren't already in the registry
///
/// This function:
/// 1. Lists all zen-offering-* containers
/// 2. Filters out containers already in the registry
/// 3. Attempts to adopt each container
/// 4. Returns adoption results for caller to handle
///
/// # Returns
/// `AdoptionResult` containing:
/// - `adopted`: Successfully adopted Offering entries
/// - `no_template`: Containers with no matching template
/// - `failed`: Containers that failed adoption with error messages
///
/// # Composability
/// This function is pure domain logic - it doesn't modify state.
/// Callers are responsible for:
/// - Adding adopted offerings to registry
/// - Persisting registry changes
/// - Emitting events
/// - Logging warnings for failed adoptions
pub async fn adopt_existing_containers(state: &AppState) -> AdoptionResult {
    let existing = match state.platform.container.list_zen_containers().await {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to list zen containers for adoption");
            return AdoptionResult::default();
        }
    };

    // Snapshot cached capabilities once for all adoptions (avoids N subprocess calls)
    let cached_caps = state.current.capabilities.read().await.clone();
    let cached_caps_ref = cached_caps.as_ref();

    let mut adopted = Vec::new();
    let mut no_template = Vec::new();
    let mut failed = Vec::new();

    for offering in existing {
        let already = {
            let offerings = state.offerings.read().await;
            offerings.iter().any(|o| o.name.to_string() == offering)
        };
        if already {
            continue;
        }

        match adopt_offering_container(
            &state.platform.container,
            state.catalog.manifests(),
            &offering,
            &state.current.stone.name,
            cached_caps_ref,
        )
        .await
        {
            Ok(Some(info)) => {
                tracing::info!(offering = %offering, "Adopting existing zen-offering container into registry");
                adopted.push(info);
            }
            Ok(None) => {
                tracing::warn!(offering = %offering, "Found zen-offering container but no matching template; leaving unregistered");
                no_template.push(offering);
            }
            Err(e) => {
                tracing::warn!(offering = %offering, error = ?e, "Failed to adopt existing container; leaving it alone");
                failed.push((offering, format!("{}", e)));
            }
        }
    }

    AdoptionResult {
        adopted,
        no_template,
        failed,
    }
}

/// Result of container adoption operation
#[derive(Debug, Default)]
pub struct AdoptionResult {
    /// Successfully adopted containers
    pub adopted: Vec<Offering>,
    /// Containers with no matching template (left unregistered)
    pub no_template: Vec<String>,
    /// Containers that failed adoption (offering, error message)
    pub failed: Vec<(String, String)>,
}
