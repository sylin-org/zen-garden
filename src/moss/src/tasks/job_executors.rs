//! Background job executors for service installation
//!
//! Non-blocking async tasks for:
//! - Single service installation
//! - Batch service installation
//!
//! These tasks:
//! - Run in background via tokio::spawn()
//! - Update job status in shared state
//! - Emit events for progress tracking
//! - Don't block the HTTP response

use crate::api::v1::events::{
    emit_job_completed, emit_job_failed, emit_job_progress, emit_job_started,
};
use crate::domain::events::OfferingEvent;
use crate::domain::network::NetworkMode;
use crate::domain::{connection, get_compiled_offering};
use crate::infra::config::MossConfig;
use crate::infra::network::{apply_static_from_pool, load_network_state};
use crate::infra::TaskStore;
use crate::{AppState, JobStatus};
use garden_common::console;
use garden_common::templates::{render_template, TemplateContext};
use garden_common::utils::ids::generate_guidv7;
use garden_common::{
    offerings::parse_offering_fqn, ManagedData, Offering, OfferingGuidance, OfferingLocation,
    OfferingModeData, OfferingStatus, ServiceHealthStatus,
};

/// Substitute template variables in guidance markdown
///
/// Supports full template syntax including conditionals:
/// - `{{port}}` - The default service port (host-side)
/// - `{{<name>-port}}` - Named port (e.g., `{{admin-port}}`, `{{management-port}}`)
/// - `{{server-name}}` - The stone's name/hostname
/// - `{{offering}}` - The offering type (e.g., "mongodb")
/// - `{{name}}` - The service instance name
/// - `{{static-ip}}` - Assigned static IP (empty if DHCP)
/// - `{{#if var}}...{{/if}}` - Conditional blocks
/// - `{{#if var}}...{{#else}}...{{/if}}` - If-else blocks
fn substitute_guidance_templates(
    template: &str,
    name: &str,
    offering: &str,
    ports: &std::collections::HashMap<String, (u16, u16)>,
    stone_name: &str,
    static_ip: Option<&str>,
) -> String {
    let mut ctx = TemplateContext::new();

    // Set basic variables
    ctx.set("server-name", stone_name);
    ctx.set("offering", offering);
    ctx.set("name", name);
    ctx.set("os", std::env::consts::OS);
    ctx.set("arch", std::env::consts::ARCH);

    // Set port variables: "default" → {{port}}, others → {{<name>-port}}
    for (port_name, (host_port, _)) in ports {
        if port_name == "default" {
            ctx.set("port", host_port.to_string());
        } else {
            ctx.set(format!("{}-port", port_name), host_port.to_string());
        }
    }

    // Set static-ip if available (enables conditionals)
    if let Some(ip) = static_ip {
        ctx.set("static-ip", ip);
    }

    render_template(template, &ctx)
}

/// Build OfferingGuidance from manifest, with template substitution
///
/// This is used during installation and for backfilling guidance on boot.
/// Pass `static_ip` if the stone has a static IP assigned.
pub fn build_guidance(
    state: &AppState,
    name: &str,
    offering: &str,
    ports: &std::collections::HashMap<String, (u16, u16)>,
    static_ip: Option<&str>,
) -> Option<OfferingGuidance> {
    let default_port = ports.get("default").map(|(h, _)| *h).unwrap_or(30000);

    tracing::debug!(
        offering = %offering,
        name = %name,
        default_port = default_port,
        port_count = ports.len(),
        static_ip = ?static_ip,
        "build_guidance: starting"
    );

    let manifest = match state.manifest_registry.sw.get(offering) {
        Some(m) => {
            tracing::debug!(
                offering = %offering,
                has_guidance = m.guidance.is_some(),
                guidance_len = m.guidance.as_ref().map(|g| g.len()).unwrap_or(0),
                "build_guidance: found manifest"
            );
            m
        }
        None => {
            tracing::debug!(
                offering = %offering,
                manifest_count = state.manifest_registry.sw.len(),
                "build_guidance: no manifest found for offering"
            );
            return None;
        }
    };

    let template = match manifest.guidance.as_ref() {
        Some(t) => {
            tracing::debug!(
                offering = %offering,
                template_len = t.len(),
                "build_guidance: found guidance template"
            );
            t
        }
        None => {
            tracing::debug!(
                offering = %offering,
                "build_guidance: manifest has no guidance template"
            );
            return None;
        }
    };

    let content = substitute_guidance_templates(
        template,
        name,
        offering,
        ports,
        &state.stone_name,
        static_ip,
    );

    // Build variables map for API consumers
    let mut variables = std::collections::HashMap::new();
    // Default port as "port"
    variables.insert("port".to_string(), default_port.to_string());
    // Named ports as "<name>-port"
    for (port_name, (host_port, _)) in ports {
        if port_name != "default" {
            variables.insert(format!("{}-port", port_name), host_port.to_string());
        }
    }
    variables.insert("server-name".to_string(), state.stone_name.clone());
    variables.insert("offering".to_string(), offering.to_string());
    variables.insert("name".to_string(), name.to_string());
    variables.insert("os".to_string(), std::env::consts::OS.to_string());
    variables.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    if let Some(ip) = static_ip {
        variables.insert("static-ip".to_string(), ip.to_string());
    }

    tracing::info!(
        offering = %offering,
        content_len = content.len(),
        "build_guidance: successfully built guidance"
    );

    Some(OfferingGuidance { content, variables })
}

/// Build guidance for adopted offerings (uses adopted guidance template)
pub fn build_adopted_guidance(
    state: &AppState,
    name: &str,
    offering: &str,
    port: u16,
    static_ip: Option<&str>,
) -> Option<OfferingGuidance> {
    let manifest = state.manifest_registry.sw.get(offering)?;
    let template = manifest
        .adopted
        .as_ref()
        .and_then(|a| a.guidance.as_ref())?;

    let mut ports = std::collections::HashMap::new();
    ports.insert("default".to_string(), (port, port));

    let content = substitute_guidance_templates(
        template,
        name,
        offering,
        &ports,
        &state.stone_name,
        static_ip,
    );

    let mut variables = std::collections::HashMap::new();
    variables.insert("port".to_string(), port.to_string());
    variables.insert("server-name".to_string(), state.stone_name.clone());
    variables.insert("offering".to_string(), offering.to_string());
    variables.insert("name".to_string(), name.to_string());
    variables.insert("os".to_string(), std::env::consts::OS.to_string());
    variables.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    if let Some(ip) = static_ip {
        variables.insert("static-ip".to_string(), ip.to_string());
    }

    Some(OfferingGuidance { content, variables })
}

/// Backfill missing guidance for services in the registry
///
/// Called at boot time to ensure any service that:
/// 1. Has no cached guidance
/// 2. Has a manifest with guidance template
///
/// Gets the guidance generated and cached.
///
/// Returns the number of services that were updated.
pub async fn backfill_missing_guidance(state: &AppState) -> usize {
    tracing::info!("Backfill: starting guidance backfill check");
    let mut updated = 0;

    // Get static IP if assigned
    let network_state = load_network_state().await;
    let static_ip_str = match &network_state.mode {
        NetworkMode::Static { address, .. } => Some(address.to_string()),
        _ => None,
    };
    let static_ip = static_ip_str.as_deref();

    // Log all manifests that have guidance templates
    let manifests_with_guidance: Vec<(String, usize)> = state
        .manifest_registry
        .sw
        .entries
        .iter()
        .filter(|(_, entry)| {
            entry.guidance.is_some()
                || entry
                    .adopted
                    .as_ref()
                    .and_then(|a| a.guidance.as_ref())
                    .is_some()
        })
        .map(|(name, entry)| {
            let managed_len = entry.guidance.as_ref().map(|g| g.len()).unwrap_or(0);
            let adopted_len = entry
                .adopted
                .as_ref()
                .and_then(|a| a.guidance.as_ref())
                .map(|g| g.len())
                .unwrap_or(0);
            (name.clone(), managed_len.max(adopted_len))
        })
        .collect();

    tracing::info!(
        total_manifests = state.manifest_registry.sw.len(),
        with_guidance = manifests_with_guidance.len(),
        guidance_offerings = ?manifests_with_guidance,
        "Backfill: manifest registry state"
    );

    // First pass: collect offerings that need guidance
    // For backfilling, we use the manifest's ports since existing offerings may only have a single port stored
    #[allow(clippy::type_complexity)]
    let offerings_needing_guidance: Vec<(
        String,
        String,
        String,
        std::collections::HashMap<String, (u16, u16)>,
    )> = {
        let offerings = state.offerings.read().await;
        tracing::info!(
            offering_count = offerings.len(),
            "Backfill: checking offerings in registry"
        );

        offerings
            .iter()
            .filter(|o| o.is_managed())
            .filter(|o| {
                let has_guidance = o
                    .managed_data()
                    .map(|m| m.guidance.is_some())
                    .unwrap_or(false);
                let manifest_has_guidance = state
                    .manifest_registry
                    .sw
                    .get(&o.offering)
                    .map(|m| m.guidance.is_some())
                    .unwrap_or(false);

                tracing::info!(
                    offering = %o.name,
                    offering_type = %o.offering,
                    has_guidance = has_guidance,
                    manifest_has_guidance = manifest_has_guidance,
                    "Backfill: checking offering"
                );

                // Only consider offerings without guidance where manifest has guidance
                !has_guidance && manifest_has_guidance
            })
            .filter_map(|o| {
                // Get ports from the manifest template for proper template substitution
                let ports = state
                    .manifest_registry
                    .sw
                    .get(&o.offering)
                    .and_then(|m| m.parse_template().ok())
                    .map(|t| t.ports)?;
                Some((
                    o.offering_id.clone(),
                    o.name.clone(),
                    o.offering.clone(),
                    ports,
                ))
            })
            .collect()
    };

    let offerings_needing_adopted_guidance: Vec<(String, String, String, u16)> = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .filter(|o| o.is_adopted())
            .filter(|o| {
                let has_guidance = o
                    .adopted_data()
                    .map(|a| a.guidance.is_some())
                    .unwrap_or(false);
                let manifest_has_guidance = state
                    .manifest_registry
                    .sw
                    .get(&o.offering)
                    .and_then(|m| m.adopted.as_ref())
                    .and_then(|a| a.guidance.as_ref())
                    .is_some();
                !has_guidance && manifest_has_guidance
            })
            .map(|o| {
                (
                    o.offering_id.clone(),
                    o.name.clone(),
                    o.offering.clone(),
                    o.location.port,
                )
            })
            .collect()
    };

    if offerings_needing_guidance.is_empty() && offerings_needing_adopted_guidance.is_empty() {
        tracing::info!("Backfill: no offerings need guidance");
        return 0;
    }

    tracing::info!(
        managed = offerings_needing_guidance.len(),
        adopted = offerings_needing_adopted_guidance.len(),
        "Backfilling missing guidance for offerings"
    );

    // Second pass: update offerings with generated guidance
    {
        let mut offerings = state.offerings.write().await;
        for (offering_id, name, offering_type, ports) in offerings_needing_guidance {
            if let Some(guidance) = build_guidance(state, &name, &offering_type, &ports, static_ip)
            {
                if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
                    if let Some(ref mut managed) = o.managed_data_mut() {
                        managed.guidance = Some(guidance);
                        updated += 1;
                        tracing::debug!(offering = %name, "Backfilled guidance");
                    }
                }
            }
        }

        for (offering_id, name, offering_type, port) in offerings_needing_adopted_guidance {
            if let Some(guidance) =
                build_adopted_guidance(state, &name, &offering_type, port, static_ip)
            {
                if let Some(o) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
                    if let Some(ref mut adopted) = o.adopted_data_mut() {
                        adopted.guidance = Some(guidance);
                        updated += 1;
                        tracing::debug!(offering = %name, "Backfilled adopted guidance");
                    }
                }
            }
        }
    }

    // Persist if we made changes
    if updated > 0 {
        if let Err(e) = state.persist_offerings().await {
            tracing::error!(error = ?e, "Failed to persist offerings after guidance backfill");
        } else {
            tracing::info!(
                count = updated,
                "Guidance backfill complete, offerings persisted"
            );
        }
    }

    updated
}

/// Execute single service installation in background
///
/// This is a long-running task that should be spawned with tokio::spawn().
/// It:
/// 1. Updates job status to Running
/// 2. Resolves offering from offerings index
/// 3. Validates compatibility
/// 4. Pulls Docker image and creates container
/// 5. Adds service to registry
/// 6. Updates job status to Completed/Failed
///
/// # Non-Blocking
/// This function is designed to run in the background. The HTTP endpoint
/// should spawn this task and immediately return the job ID to the client.
///
/// # Parameters
/// - `state`: Application state (cloned, cheap due to Arc)
/// - `job_id`: Job ID for tracking
/// - `offering_type`: Offering template name to install
/// - `service_name`: Fully-qualified service name (FQN)
///
/// # Example
/// ```rust,ignore
/// let state_clone = state.clone();
/// let job_id = job_id.to_string();
/// let offering_type = offering.to_string();
/// let service_name = offering.to_string();
/// tokio::spawn(async move {
///     install_service_task(&state_clone, &job_id, &offering_type, &service_name).await;
/// });
/// ```
pub async fn install_service_task(
    state: &AppState,
    job_id: &str,
    offering_type: &str,
    service_name: &str,
) {
    let offering = service_name;
    // Update job status to Running
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
        }
    }

    // Emit job started event
    state.console.emit(console::ConsoleEvent::new(
        console::EventCategory::Jobs,
        console::EventStatus::Started,
        format!("Install {} (job: {})", offering, &job_id[..8]),
    ));

    emit_job_started(state, job_id, offering, "install");
    tracing::info!(job_id, offering, "Starting service installation");

    tracing::debug!(
        offering,
        offering_type,
        "Resolving compiled offering config"
    );
    let compiled = match get_compiled_offering(state, offering_type).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            state.console.emit(console::ConsoleEvent::new(
                console::EventCategory::Jobs,
                console::EventStatus::Failed,
                format!("Offering not found: {}", offering),
            ));
            emit_job_failed(state, job_id, offering, "Offering not found");
            // Remove Installing entry from registry
            remove_installing_entry(state, offering).await;
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = JobStatus::Failed;
                job.failed
                    .insert(offering.to_string(), "Offering not found".to_string());
                job.completed_at = Some(std::time::SystemTime::now());
            }
            return;
        }
        Err(e) => {
            emit_job_failed(
                state,
                job_id,
                offering,
                &format!("Failed to read offerings index: {}", e),
            );
            // Remove Installing entry from registry
            remove_installing_entry(state, offering).await;
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = JobStatus::Failed;
                job.failed.insert(
                    offering.to_string(),
                    format!("Offerings index error: {}", e),
                );
                job.completed_at = Some(std::time::SystemTime::now());
            }
            return;
        }
    };

    if compiled.compatibility.decision == "fail" {
        let reason = compiled
            .compatibility
            .reason
            .clone()
            .unwrap_or_else(|| "Incompatible".to_string());
        state.console.emit(console::ConsoleEvent::new(
            console::EventCategory::Jobs,
            console::EventStatus::Failed,
            format!("Compatibility: {}", offering),
        ));
        emit_job_failed(
            state,
            job_id,
            offering,
            &format!("Compatibility validation failed: {}", reason),
        );

        // Remove Installing entry from registry
        remove_installing_entry(state, offering).await;
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.failed.insert(
                offering.to_string(),
                format!("Compatibility failed: {}", reason),
            );
            job.completed_at = Some(std::time::SystemTime::now());
        }
        return;
    }

    if compiled.compatibility.decision.as_str() == "fallback" {
        emit_job_progress(
            state,
            "warn",
            format!(
                "Compatibility fallback: {}",
                compiled.compatibility.reason.clone().unwrap_or_default()
            ),
            job_id,
            offering,
        );
    }

    // Handle static IP requirements for this offering
    // SSE events only for actual state changes, verbose info goes to tracing only
    // Track assigned/existing static IP for guidance template rendering
    let mut assigned_static_ip: Option<String> = None;

    if compiled.network.wants_static_ip() {
        tracing::info!(
            offering = %offering,
            preference = if compiled.network.requires_static_ip() { "required" } else { "preferred" },
            reason = ?compiled.network.static_ip_reason,
            "Offering wants static IP"
        );

        // Get static IP pool (from config or auto-detected defaults)
        let config = MossConfig::load();
        let pool_config = MossConfig::get_static_ip_pool(config.as_ref());

        match pool_config {
            Some(ref pool) => {
                // Load current network state
                let mut network_state = load_network_state().await;

                // Check if we already have static IP (another offering requested it)
                if network_state.mode.is_static() {
                    // Capture existing static IP for guidance rendering
                    if let Some(existing_ip) = network_state.mode.static_address() {
                        assigned_static_ip = Some(existing_ip.to_string());
                    }

                    // Just register as additional requester (no SSE - internal bookkeeping)
                    let is_first = network_state.add_requester(offering);
                    if !is_first {
                        let existing_ip = network_state
                            .mode
                            .static_address()
                            .unwrap_or_else(|| "0.0.0.0".parse().unwrap());
                        tracing::info!(
                            offering = %offering,
                            ip = %existing_ip,
                            requesters = network_state.requester_count(),
                            "Registered as additional static IP requester"
                        );
                        // Save updated state with new requester
                        if let Err(e) =
                            crate::infra::network::save_network_state(&network_state).await
                        {
                            tracing::warn!(error = ?e, "Failed to save network state");
                        }
                    }
                } else {
                    // Apply static IP from pool - this is an actual state change
                    match apply_static_from_pool(pool, offering, &mut network_state).await {
                        Ok(ip) => {
                            // Capture assigned static IP for guidance rendering
                            assigned_static_ip = Some(ip.to_string());

                            // SSE: meaningful state change
                            emit_job_progress(
                                state,
                                "info",
                                format!("Switching to static IP {}", ip),
                                job_id,
                                offering,
                            );
                        }
                        Err(e) => {
                            tracing::warn!(offering = %offering, error = ?e, "Static IP assignment failed");

                            if compiled.network.requires_static_ip() {
                                // Required - fail installation (SSE: error is meaningful)
                                let error_msg =
                                    format!("Static IP required but assignment failed: {}", e);
                                state.console.emit(console::ConsoleEvent::new(
                                    console::EventCategory::Jobs,
                                    console::EventStatus::Failed,
                                    format!("Static IP required: {}", offering),
                                ));
                                emit_job_failed(state, job_id, offering, &error_msg);
                                remove_installing_entry(state, offering).await;
                                let mut jobs = state.jobs.write().await;
                                if let Some(job) = jobs.get_mut(job_id) {
                                    job.status = JobStatus::Failed;
                                    job.failed.insert(offering.to_string(), error_msg);
                                    job.completed_at = Some(std::time::SystemTime::now());
                                }
                                return;
                            }
                            // Preferred - continue silently (just logged above)
                        }
                    }
                }
            }
            None => {
                // Pool explicitly disabled or auto-detection failed
                tracing::info!(offering = %offering, "Static IP pool unavailable (disabled or auto-detection failed)");

                if compiled.network.requires_static_ip() {
                    // Required - fail installation (SSE: error is meaningful)
                    let error_msg = "Static IP required but unavailable";
                    state.console.emit(console::ConsoleEvent::new(
                        console::EventCategory::Jobs,
                        console::EventStatus::Failed,
                        format!("Static IP required: {}", offering),
                    ));
                    emit_job_failed(state, job_id, offering, error_msg);
                    remove_installing_entry(state, offering).await;
                    let mut jobs = state.jobs.write().await;
                    if let Some(job) = jobs.get_mut(job_id) {
                        job.status = JobStatus::Failed;
                        job.failed
                            .insert(offering.to_string(), error_msg.to_string());
                        job.completed_at = Some(std::time::SystemTime::now());
                    }
                    return;
                }
                // Preferred - continue silently (just logged above)
            }
        }
    }

    // Extract values before install_service consumes compiled
    let native_port = compiled.default_host_port();
    let offering_protocol = connection::infer_protocol_from_manifest_metadata(
        offering_type,
        &compiled.category,
        state
            .manifest_registry
            .get_offering(offering_type)
            .and_then(|entry| entry.connection.as_ref()),
    );
    let guidance = build_guidance(
        state,
        offering,
        offering_type,
        &compiled.ports,
        assigned_static_ip.as_deref(),
    );
    let image_full = compiled.image.clone();
    let image_version = image_full
        .split(':')
        .next_back()
        .unwrap_or("latest")
        .to_string();

    // Extract seed files (initial configuration) into volume directories.
    // Must happen before container creation so configs are present at first boot.
    match crate::infra::embedded::extract_seeds(offering_type, &compiled.volumes) {
        Ok(n) if n > 0 => {
            emit_job_progress(
                state,
                "info",
                format!("Seeded {} config file(s)", n),
                job_id,
                offering,
            );
        }
        Err(e) => {
            tracing::warn!(offering, error = ?e, "Failed to extract seed files (non-fatal)");
        }
        _ => {}
    }

    // Install via Docker
    emit_job_progress(
        state,
        "info",
        format!("Pulling image: {}", compiled.image),
        job_id,
        offering,
    );
    let spec = crate::docker::ContainerSpec {
        image: compiled.image.clone(),
        command: None,
        ports: compiled.ports_vec(),
        environment: compiled.environment,
        volumes: compiled.volumes,
        config_files: vec![],
    };
    let actual_ports = match state
        .docker
        .install_service(offering, &spec, Some(&state.console))
        .await
    {
        Ok(resolved) => resolved,
        Err(e) => {
            state.console.emit(console::ConsoleEvent::new(
                console::EventCategory::Jobs,
                console::EventStatus::Failed,
                format!("Install failed: {}", offering),
            ));
            emit_job_failed(
                state,
                job_id,
                offering,
                &format!("Installation failed: {}", e),
            );
            tracing::error!(job_id, offering, error = ?e, "Docker install failed");
            // Remove Installing entry from registry
            remove_installing_entry(state, offering).await;
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = JobStatus::Failed;
                job.failed
                    .insert(offering.to_string(), format!("Install failed: {}", e));
                job.completed_at = Some(std::time::SystemTime::now());
            }
            return;
        }
    };

    // Use actual Docker-bound port. Prefer the manifest's default (native_port) if it
    // appears in Docker's response; fall back to first port only when remapped.
    let actual_port = actual_ports
        .iter()
        .find(|(h, _)| *h == native_port)
        .or(actual_ports.first())
        .map(|(h, _)| *h)
        .unwrap_or(native_port);

    emit_job_progress(
        state,
        "info",
        format!("Creating container for {}", offering),
        job_id,
        offering,
    );

    // Update existing offering entry (created with Installing status before job started)
    // Change status from Installing to Running and clear job_id
    let offering_id = {
        let mut offerings = state.offerings.write().await;
        if let Some(existing) = offerings.iter_mut().find(|o| o.name == offering) {
            existing.status = OfferingStatus::Running;
            existing.health = ServiceHealthStatus::Healthy;
            existing.version = image_version.clone();
            existing.location.port = actual_port;
            existing.location.protocol = offering_protocol.clone();
            if let Some(ref mut managed) = existing.managed_data_mut() {
                managed.job_id = None;
                managed.guidance = guidance.clone();
            }
            existing.offering_id.clone()
        } else {
            // Fallback: entry was somehow removed, recreate it
            let new_id = generate_guidv7();
            let unified = Offering {
                offering_id: new_id.clone(),
                name: offering.to_string(),
                offering: offering_type.to_string(),
                version: image_version.clone(),
                status: OfferingStatus::Running,
                health: ServiceHealthStatus::Healthy,
                sub_capabilities: Vec::new(),
                location: OfferingLocation {
                    host: "localhost".to_string(),
                    port: actual_port,
                    protocol: offering_protocol.clone(),
                    agnostic_port: None,
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
            offerings.push(unified);
            new_id
        }
    };

    let _ = state.persist_offerings().await;

    // Sync services to self_entry and broadcast chirp so topology reflects the change immediately
    state.sync_self_services(true).await;

    // Emit offering lifecycle event (triggers listeners: chirp debounce, SSE, timers)
    state.event_bus.emit(OfferingEvent::deployed(
        &offering_id,
        offering,
        state.stone_name(),
        &image_full,
    ));

    // Assign initial orchestration role for elected offerings (ORCH-0006)
    if compiled.coordination.is_elected() {
        if let Err(e) =
            crate::tasks::offering_orchestration::assign_initial_role(state, &offering_id, offering)
                .await
        {
            tracing::warn!(
                offering = %offering,
                error = ?e,
                "Failed to assign initial orchestration role (non-fatal)"
            );
        }
    }

    // Register scheduled tasks from manifest
    if !compiled.tasks.is_empty() {
        let task_store = TaskStore::new();
        match task_store
            .register_tasks(&offering_id, offering, &compiled.tasks)
            .await
        {
            Ok(count) if count > 0 => {
                tracing::info!(
                    offering = %offering,
                    task_count = count,
                    "Registered scheduled tasks"
                );
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    offering = %offering,
                    error = ?e,
                    "Failed to register scheduled tasks (non-fatal)"
                );
            }
        }
    }

    emit_job_completed(state, job_id, offering);

    // Mark job as completed
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.completed.push(offering.to_string());
            job.completed_at = Some(std::time::SystemTime::now());
        }
    }

    state.console.emit(console::ConsoleEvent::new(
        console::EventCategory::Jobs,
        console::EventStatus::Completed,
        format!("Install {} (job: {})", offering, &job_id[..8]),
    ));

    tracing::info!(job_id, offering, "Service installation completed");
}

/// Execute batch service installation in background
///
/// This is a long-running task that should be spawned with tokio::spawn().
/// It installs multiple services sequentially, tracking success/failure for each.
///
/// # Non-Blocking
/// This function is designed to run in the background. The HTTP endpoint
/// should spawn this task and immediately return the job ID to the client.
///
/// # Parameters
/// - `state`: Application state (cloned, cheap due to Arc)
/// - `job_id`: Job ID for tracking
/// - `offerings`: List of offering names to install
///
/// # Example
/// ```rust,ignore
/// let state_clone = state.clone();
/// let job_id = job_id.to_string();
/// let offerings = vec!["nginx".to_string(), "postgres".to_string()];
/// tokio::spawn(async move {
///     install_batch_task(&state_clone, &job_id, offerings).await;
/// });
/// ```
pub async fn install_batch_task(state: &AppState, job_id: &str, offerings: Vec<String>) {
    let offerings_count = offerings.len();

    // Load network state to get any existing static IP for guidance rendering
    let network_state = load_network_state().await;
    let static_ip = match &network_state.mode {
        NetworkMode::Static { address, .. } => Some(address.to_string()),
        _ => None,
    };

    // Update job status to Running
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
        }
    }

    state.console.emit(console::ConsoleEvent::new(
        console::EventCategory::Jobs,
        console::EventStatus::Started,
        format!(
            "Batch install {} services (job: {})",
            offerings_count,
            &job_id[..8]
        ),
    ));

    tracing::info!(
        job_id,
        count = offerings_count,
        "Starting batch installation"
    );

    for offering in offerings {
        tracing::info!(job_id, offering, "Installing service");

        let offering_fqn = match parse_offering_fqn(&offering) {
            Ok(fqn) => fqn,
            Err(e) => {
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed
                        .insert(offering.clone(), format!("Invalid offering name: {}", e));
                }
                continue;
            }
        };

        let service_name = offering_fqn.fqn();
        let offering_type = offering_fqn.offering.clone();

        let compiled = match get_compiled_offering(state, &offering_type).await {
            Ok(Some(o)) => o,
            Ok(None) => {
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed
                        .insert(service_name.clone(), "Offering not found".to_string());
                }
                continue;
            }
            Err(e) => {
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed.insert(
                        service_name.clone(),
                        format!("Offerings index error: {}", e),
                    );
                }
                continue;
            }
        };

        if compiled.compatibility.decision == "fail" {
            let reason = compiled
                .compatibility
                .reason
                .clone()
                .unwrap_or_else(|| "Incompatible".to_string());
            tracing::error!(
                job_id,
                service = %service_name,
                reason = %reason,
                "Compatibility validation failed"
            );
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.failed.insert(
                    service_name.clone(),
                    format!("Compatibility failed: {}", reason),
                );
            }
            continue;
        }

        // Extract values before install_service consumes compiled
        let native_port = compiled.default_host_port();
        let offering_protocol = connection::infer_protocol_from_manifest_metadata(
            &offering_type,
            &compiled.category,
            state
                .manifest_registry
                .get_offering(&offering_type)
                .and_then(|entry| entry.connection.as_ref()),
        );
        let guidance = build_guidance(
            state,
            &service_name,
            &offering_type,
            &compiled.ports,
            static_ip.as_deref(),
        );
        let image_full = compiled.image.clone();
        let image_version = image_full
            .split(':')
            .next_back()
            .unwrap_or("latest")
            .to_string();

        // Extract seed files (initial configuration) into volume directories.
        // Must happen before container creation so configs are present at first boot.
        match crate::infra::embedded::extract_seeds(&offering_type, &compiled.volumes) {
            Ok(n) if n > 0 => {
                tracing::info!(service = %service_name, count = n, "Seeded config files");
            }
            Err(e) => {
                tracing::warn!(service = %service_name, error = ?e, "Failed to extract seed files (non-fatal)");
            }
            _ => {}
        }

        // Install via Docker
        let spec = crate::docker::ContainerSpec {
            image: compiled.image.clone(),
            command: None,
            ports: compiled.ports_vec(),
            environment: compiled.environment,
            volumes: compiled.volumes,
            config_files: vec![],
        };
        let actual_ports = match state
            .docker
            .install_service(&service_name, &spec, Some(&state.console))
            .await
        {
            Ok(resolved) => resolved,
            Err(e) => {
                tracing::error!(job_id, service = %service_name, error = ?e, "Docker install failed");
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed
                        .insert(service_name.clone(), format!("Install failed: {}", e));
                }
                continue;
            }
        };

        // Use actual Docker-bound port. Prefer the manifest's default (native_port) if it
        // appears in Docker's response; fall back to first port only when remapped.
        let actual_port = actual_ports
            .iter()
            .find(|(h, _)| *h == native_port)
            .or(actual_ports.first())
            .map(|(h, _)| *h)
            .unwrap_or(native_port);

        // Add to offerings registry
        let offering_id = generate_guidv7();
        let unified = Offering {
            offering_id: offering_id.clone(),
            name: service_name.clone(),
            offering: offering_type.to_string(),
            version: image_version,
            status: OfferingStatus::Running,
            health: ServiceHealthStatus::Healthy,
            sub_capabilities: Vec::new(),
            location: OfferingLocation {
                host: "localhost".to_string(),
                port: actual_port,
                protocol: offering_protocol,
                agnostic_port: None,
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

        {
            let mut offerings = state.offerings.write().await;
            if let Some(existing) = offerings.iter_mut().find(|o| o.name == service_name) {
                *existing = unified;
            } else {
                offerings.push(unified);
            }
        }

        let _ = state.persist_offerings().await;

        // Sync services to self_entry and broadcast chirp so topology reflects the change immediately
        state.sync_self_services(true).await;

        // Emit offering lifecycle event (triggers listeners: chirp debounce, SSE, timers)
        state.event_bus.emit(OfferingEvent::deployed(
            &offering_id,
            &service_name,
            state.stone_name(),
            &image_full,
        ));

        // Mark offering as completed
        {
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.completed.push(service_name.clone());
            }
        }

        tracing::info!(job_id, service = %service_name, "Service installed");
    }

    // Mark job as completed (or failed if some services failed)
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            let failed = !job.failed.is_empty();
            job.status = if failed {
                JobStatus::Failed
            } else {
                JobStatus::Completed
            };
            job.completed_at = Some(std::time::SystemTime::now());

            // Emit completion event
            if failed {
                state.console.emit(console::ConsoleEvent::new(
                    console::EventCategory::Jobs,
                    console::EventStatus::Failed,
                    format!(
                        "Batch install {} failed, {} succeeded (job: {})",
                        job.failed.len(),
                        job.completed.len(),
                        &job_id[..8]
                    ),
                ));
            } else {
                state.console.emit(console::ConsoleEvent::new(
                    console::EventCategory::Jobs,
                    console::EventStatus::Completed,
                    format!(
                        "Batch install {} services (job: {})",
                        offerings_count,
                        &job_id[..8]
                    ),
                ));
            }
        }
    }

    tracing::info!(job_id, "Batch installation completed");
}

/// Remove an Installing entry from the offerings registry on failure
///
/// Called when a service installation fails to clean up the placeholder entry
/// that was created before the installation job started.
async fn remove_installing_entry(state: &AppState, offering: &str) {
    let mut offerings = state.offerings.write().await;
    if let Some(pos) = offerings
        .iter()
        .position(|o| o.name == offering && o.status == OfferingStatus::Installing)
    {
        offerings.remove(pos);
        tracing::debug!(
            offering,
            "Removed Installing entry from offerings after failure"
        );
    }
    drop(offerings);
    let _ = state.persist_offerings().await;

    // Sync services to self_entry to reflect the removal
    state.sync_self_services(true).await;
}

// =============================================================================
// Capabilities Refresh Task
// =============================================================================

/// Background task for refreshing capabilities (models, extensions, etc.)
///
/// Refreshes all capabilities for an offering by re-running the "add" operation
/// for each existing capability. For Ollama, this pulls the latest version of
/// each model.
///
/// # Arguments
/// * `state` - Application state
/// * `job_id` - Job ID for tracking progress
/// * `offering` - The offering name (e.g., "ollama")
/// * `cap_type` - Optional capability type filter (e.g., "model")
///
/// # Progress Tracking
/// Progress is tracked in the Job struct:
/// - `offerings` field holds capability names (not offering names)
/// - `completed` accumulates successfully refreshed capabilities
/// - `failed` maps failed capability names to error messages
pub async fn refresh_capabilities_task(
    state: &AppState,
    job_id: &str,
    offering: &str,
    cap_type: Option<&str>,
) {
    use crate::domain::CapabilityExecutor;
    use crate::infra::manifests::get_capability_manifest;

    // Update job status to Running
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
        }
    }

    // Emit job started event
    state.console.emit(console::ConsoleEvent::new(
        console::EventCategory::Jobs,
        console::EventStatus::Started,
        format!("Refresh capabilities {} (job: {})", offering, &job_id[..8]),
    ));

    emit_job_started(state, job_id, offering, "refresh-capabilities");
    tracing::info!(job_id, offering, "Starting capabilities refresh");

    // Find the offering
    let (service, mode) = {
        let offerings = state.offerings.read().await;
        match offerings
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(offering))
        {
            Some(o) => {
                let mode = o.mode();
                let service = offering_to_service_info_for_refresh(o, state).await;
                (service, mode)
            }
            None => {
                let error = format!("Offering '{}' not found", offering);
                emit_job_failed(state, job_id, offering, &error);
                mark_job_failed(state, job_id, offering, &error).await;
                return;
            }
        }
    };

    // Get capability manifest
    let manifest = match get_capability_manifest(&service.offering) {
        Some(m) => m,
        None => {
            let error = format!("No capability manifest found for '{}'", offering);
            emit_job_failed(state, job_id, offering, &error);
            mark_job_failed(state, job_id, offering, &error).await;
            return;
        }
    };

    // List current capabilities
    let executor = CapabilityExecutor::new();
    let collections = match executor.list_capabilities(&service, manifest, mode).await {
        Ok(c) => c,
        Err(e) => {
            let error = format!("Failed to list capabilities: {}", e);
            emit_job_failed(state, job_id, offering, &error);
            mark_job_failed(state, job_id, offering, &error).await;
            return;
        }
    };

    // Filter by type if specified
    let filtered_collections: Vec<_> = if let Some(cap_type_filter) = cap_type {
        collections
            .into_iter()
            .filter(|c| c.cap_type == cap_type_filter)
            .collect()
    } else {
        collections
    };

    // Build list of capabilities to refresh
    let mut capabilities: Vec<(String, String)> = Vec::new(); // (name, type)
    for collection in &filtered_collections {
        let cap_config = manifest.get_capability_type(&collection.cap_type);
        let can_refresh = cap_config
            .and_then(|c| c.add.as_ref())
            .map(|a| a.available)
            .unwrap_or(false);

        if can_refresh {
            for item in &collection.items {
                capabilities.push((item.name.clone(), collection.cap_type.clone()));
            }
        }
    }

    let total = capabilities.len();
    tracing::info!(job_id, offering, total, "Refreshing capabilities");

    // Process each capability
    for (idx, (cap_name, cap_type_str)) in capabilities.iter().enumerate() {
        emit_job_progress(
            state,
            "info",
            format!("Refreshing {}/{}: {}", idx + 1, total, cap_name),
            job_id,
            offering,
        );

        match executor
            .add_capability(&service, manifest, mode, cap_type_str, cap_name)
            .await
        {
            Ok(result) if result.success => {
                // Mark as completed
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.completed.push(cap_name.clone());
                }
                tracing::debug!(job_id, capability = %cap_name, "Capability refreshed successfully");
            }
            Ok(result) => {
                // Add operation returned but reported failure
                let error = result.error.unwrap_or_else(|| "Unknown error".to_string());
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed.insert(cap_name.clone(), error.clone());
                }
                tracing::warn!(job_id, capability = %cap_name, error = %error, "Capability refresh failed");
            }
            Err(e) => {
                // Add operation threw an error
                let error = e.to_string();
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.failed.insert(cap_name.clone(), error.clone());
                }
                tracing::warn!(job_id, capability = %cap_name, error = %error, "Capability refresh error");
            }
        }
    }

    // Mark job as completed
    let (succeeded, failed_count) = {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.completed_at = Some(std::time::SystemTime::now());
            (job.completed.len(), job.failed.len())
        } else {
            (0, 0)
        }
    };

    // Emit completion event
    if failed_count == 0 {
        emit_job_completed(state, job_id, offering);
        state.console.emit(console::ConsoleEvent::new(
            console::EventCategory::Jobs,
            console::EventStatus::Completed,
            format!(
                "Refreshed {} capabilities for {} (job: {})",
                succeeded,
                offering,
                &job_id[..8]
            ),
        ));
    } else {
        emit_job_progress(
            state,
            "warn",
            format!(
                "Refresh completed: {} succeeded, {} failed",
                succeeded, failed_count
            ),
            job_id,
            offering,
        );
        state.console.emit(console::ConsoleEvent::new(
            console::EventCategory::Jobs,
            console::EventStatus::Completed,
            format!(
                "Refresh {}: {} ok, {} failed (job: {})",
                offering,
                succeeded,
                failed_count,
                &job_id[..8]
            ),
        ));
    }

    tracing::info!(
        job_id,
        offering,
        succeeded,
        failed = failed_count,
        "Capabilities refresh completed"
    );
}

/// Convert Offering to ServiceInfo for capability executor (internal helper)
async fn offering_to_service_info_for_refresh(
    offering: &Offering,
    state: &AppState,
) -> garden_common::ServiceInfo {
    use crate::domain::get_offering_port;
    use garden_common::{Ports, ServiceInfo, ServiceStatus};

    let port = if offering.location.port > 0 {
        offering.location.port
    } else {
        get_offering_port(&offering.offering, state).await
    };

    ServiceInfo {
        offering_id: offering.offering_id.clone(),
        name: offering.name.clone(),
        offering: offering.offering.clone(),
        version: offering.version.clone(),
        status: match offering.status {
            OfferingStatus::Running => ServiceStatus::Running,
            OfferingStatus::Stopped => ServiceStatus::Stopped,
            OfferingStatus::Installing => ServiceStatus::Installing,
            OfferingStatus::Degraded => ServiceStatus::Degraded,
            OfferingStatus::Maintenance => ServiceStatus::Maintenance,
            OfferingStatus::Unknown => ServiceStatus::Unknown,
        },
        health: offering.health.clone(),
        ports: Ports {
            native: port,
            agnostic: offering.location.agnostic_port,
        },
        resources: offering.managed_data().and_then(|m| m.resources.clone()),
        job_id: offering.managed_data().and_then(|m| m.job_id.clone()),
        sub_capabilities: offering.sub_capabilities.clone(),
        guidance: offering
            .managed_data()
            .and_then(|m| m.guidance.clone())
            .or_else(|| offering.adopted_data().and_then(|a| a.guidance.clone())),
        customized_by: offering
            .managed_data()
            .map(|m| {
                crate::domain::config_compose::patch_owners(&m.config_patches)
            })
            .unwrap_or_default(),
    }
}

/// Mark a job as failed (helper for background tasks)
async fn mark_job_failed(state: &AppState, job_id: &str, key: &str, error: &str) {
    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = JobStatus::Failed;
        job.failed.insert(key.to_string(), error.to_string());
        job.completed_at = Some(std::time::SystemTime::now());
    }
}

/// Background task for adding a single capability
///
/// Creates a job, executes the add command, and updates job status.
pub async fn add_capability_task(
    state: &AppState,
    job_id: &str,
    offering: &str,
    cap_type: &str,
    capability_name: &str,
) {
    use crate::domain::CapabilityExecutor;
    use crate::infra::manifests::get_capability_manifest;

    // Update job status to Running
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
        }
    }

    // Emit job started event
    state.console.emit(console::ConsoleEvent::new(
        console::EventCategory::Jobs,
        console::EventStatus::Started,
        format!(
            "Add {} {} to {} (job: {})",
            cap_type,
            capability_name,
            offering,
            &job_id[..8.min(job_id.len())]
        ),
    ));

    emit_job_started(state, job_id, offering, "add-capability");
    tracing::info!(
        job_id,
        offering,
        cap_type,
        capability_name,
        "Starting capability add"
    );

    // Find the offering
    let (service, mode) = {
        let offerings = state.offerings.read().await;
        match offerings
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(offering))
        {
            Some(o) => {
                let mode = o.mode();
                let service = offering_to_service_info_for_refresh(o, state).await;
                (service, mode)
            }
            None => {
                let error = format!("Offering '{}' not found", offering);
                emit_job_failed(state, job_id, offering, &error);
                mark_job_failed(state, job_id, capability_name, &error).await;
                return;
            }
        }
    };

    // Get capability manifest
    let manifest = match get_capability_manifest(&service.offering) {
        Some(m) => m,
        None => {
            let error = format!("No capability manifest found for '{}'", offering);
            emit_job_failed(state, job_id, offering, &error);
            mark_job_failed(state, job_id, capability_name, &error).await;
            return;
        }
    };

    // Execute add operation
    let executor = CapabilityExecutor::new();

    emit_job_progress(
        state,
        "info",
        format!("Adding {} '{}'...", cap_type, capability_name),
        job_id,
        offering,
    );

    match executor
        .add_capability(&service, manifest, mode, cap_type, capability_name)
        .await
    {
        Ok(result) if result.success => {
            if let Err(e) = crate::domain::tools::capability_orchestrator::record_capability_added(
                state,
                offering,
                cap_type,
                capability_name,
            )
            .await
            {
                let error = format!("Capability added but state update failed: {}", e);
                emit_job_failed(state, job_id, offering, &error);
                mark_job_failed(state, job_id, capability_name, &error).await;
                tracing::error!(
                    job_id,
                    offering,
                    cap_type,
                    capability_name,
                    error = %error,
                    "Capability add state update failed"
                );
                return;
            }

            // Mark as completed
            {
                let mut jobs = state.jobs.write().await;
                if let Some(job) = jobs.get_mut(job_id) {
                    job.status = JobStatus::Completed;
                    job.completed.push(capability_name.to_string());
                    job.completed_at = Some(std::time::SystemTime::now());
                }
            }

            emit_job_completed(state, job_id, offering);
            state.console.emit(console::ConsoleEvent::new(
                console::EventCategory::Jobs,
                console::EventStatus::Completed,
                format!(
                    "Added {} '{}' to {} (job: {})",
                    cap_type,
                    capability_name,
                    offering,
                    &job_id[..8.min(job_id.len())]
                ),
            ));

            tracing::info!(
                job_id,
                offering,
                cap_type,
                capability_name,
                "Capability add completed"
            );
        }
        Ok(result) => {
            // Operation returned but reported failure
            let error = result.error.unwrap_or_else(|| "Unknown error".to_string());
            emit_job_failed(state, job_id, offering, &error);
            mark_job_failed(state, job_id, capability_name, &error).await;

            state.console.emit(console::ConsoleEvent::new(
                console::EventCategory::Jobs,
                console::EventStatus::Failed,
                format!(
                    "Failed to add {} '{}': {} (job: {})",
                    cap_type,
                    capability_name,
                    error,
                    &job_id[..8.min(job_id.len())]
                ),
            ));

            tracing::warn!(job_id, offering, cap_type, capability_name, error = %error, "Capability add failed");
        }
        Err(e) => {
            let error = e.to_string();
            emit_job_failed(state, job_id, offering, &error);
            mark_job_failed(state, job_id, capability_name, &error).await;

            state.console.emit(console::ConsoleEvent::new(
                console::EventCategory::Jobs,
                console::EventStatus::Failed,
                format!(
                    "Error adding {} '{}': {} (job: {})",
                    cap_type,
                    capability_name,
                    error,
                    &job_id[..8.min(job_id.len())]
                ),
            ));

            tracing::error!(job_id, offering, cap_type, capability_name, error = %error, "Capability add error");
        }
    }
}
