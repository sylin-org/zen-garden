//! Multi-offering discovery task.
//!
//! Discovers ALL AI offering instances across the garden, not just one type.
//!
//! # Discovery flow
//!
//! 1. **Resolve a stone** — explicit override -> cached tending -> Koi mDNS browse.
//! 2. **Topology query** — `GET /api/v1/garden/topology` on the tended stone.
//!    Parses every stone's services and registers those matching any known AI
//!    offering type (ollama, comfyui, speaches, infinity, etc.).
//! 3. **Tools API stream** — subscribe to `GET /api/v1/garden/tools/stream` for
//!    real-time `tool.upsert` / `tool.remove` events so the orchestrator reacts
//!    to new AI instances coming online or going away after the initial load.
//! 4. On stream failure -> clear tending, re-discover from step 1.

use crate::app_state::{AppState, TendedStone};
use crate::domain::types::{
    Capability, ComputeType, Gpu, InstanceHealth, ModelFqn, ModelMetadata,
    OfferingKind, ServiceInstance, Stone, Vram,
};
use orchestrator_common::tools_stream::{self, ToolStreamEvent};
use orchestrator_common::topology;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How often to re-query the topology to catch stones the SSE stream missed.
const TOPOLOGY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Run the multi-offering discovery loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        if shutdown.is_cancelled() {
            return;
        }

        // ── Phase 1: Resolve a stone endpoint ────────────────────
        let stone_endpoint = match resolve_stone(&state, &shutdown).await {
            Some(ep) => ep,
            None => return,
        };

        // ── Phase 2: Topology query — authoritative initial load ─
        discover_from_topology(&stone_endpoint, &state).await;

        // ── Phase 3: SSE stream + periodic topology refresh ──────
        tracing::info!(
            endpoint = %stone_endpoint,
            "subscribing to Tools API stream + topology refresh"
        );

        let refresh_handle = {
            let state = state.clone();
            let endpoint = stone_endpoint.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                topology_refresh_loop(endpoint, state, shutdown).await;
            })
        };

        let state_for_stream = state.clone();
        let result = tools_stream::subscribe_tools_stream(
            &stone_endpoint,
            |fqid| is_ai_offering_fqid(fqid),
            |event| {
                handle_tool_event(&state_for_stream, event);
            },
        )
        .await;

        // ── Stream ended — stop refresh loop and reconnect ───────
        match result {
            Ok(()) => tracing::warn!("tools stream ended normally, will re-discover"),
            Err(e) => tracing::warn!(error = %e, "tools stream error, will re-discover"),
        }
        refresh_handle.abort();

        state.clear_tending().await;

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Check if a tool FQID matches any AI offering type.
fn is_ai_offering_fqid(fqid: &str) -> bool {
    OfferingKind::LOCAL_OFFERING_NAMES
        .iter()
        .any(|name| fqid.starts_with(&format!("offering:{name}")))
}

/// Handle a single tool stream event.
fn handle_tool_event(state: &AppState, event: ToolStreamEvent) {
    match event {
        ToolStreamEvent::OfferingDiscovered {
            stone_id,
            stone_name,
            endpoint,
            tool_fqid,
            ready,
        } => {
            if !ready {
                tracing::debug!(
                    stone = %stone_name,
                    fqid = %tool_fqid,
                    "SSE: AI instance not ready (container stopped), skipping"
                );
                return;
            }

            // Parse offering kind from the tool FQID (e.g. "offering:ollama" -> Ollama)
            let offering_name = tool_fqid
                .strip_prefix("offering:")
                .unwrap_or(&tool_fqid)
                .split(':')
                .next()
                .unwrap_or("");

            let kind = match OfferingKind::from_topology_name(offering_name) {
                Some(k) => k,
                None => {
                    tracing::debug!(fqid = %tool_fqid, "SSE: unrecognized AI offering, skipping");
                    return;
                }
            };

            tracing::info!(
                stone = %stone_name,
                kind = %kind,
                endpoint = %endpoint,
                "SSE: discovered AI instance"
            );

            let state = state.clone();
            tokio::spawn(async move {
                let instance = build_instance_from_discovery(
                    stone_id,
                    stone_name,
                    endpoint.clone(),
                    kind,
                    0,
                    None,
                );
                { let cfg = state.config.read().await; state.registry.upsert_instance(instance, &cfg).await; drop(cfg); };
                profile_instance(&state, &endpoint, kind).await;
            });
        }
        ToolStreamEvent::OfferingRemoved {
            stone_id: _,
            stone_name,
        } => {
            tracing::info!(stone = %stone_name, "SSE: AI instance removed");
            let state = state.clone();
            tokio::spawn(async move {
                let endpoint = {
                    let snap = state.registry.snapshot().clone();
                    snap.instances
                        .values()
                        .find(|i| i.stone.name == stone_name)
                        .map(|i| i.endpoint.clone())
                };
                if let Some(ep) = endpoint {
                    state.registry.remove_instance(&ep).await;
                }
            });
        }
        ToolStreamEvent::Heartbeat => {
            tracing::trace!("tools stream heartbeat");
        }
    }
}

/// Query topology for all AI offering instances and register them.
async fn discover_from_topology(stone_endpoint: &str, state: &AppState) {
    for offering_name in OfferingKind::LOCAL_OFFERING_NAMES {
        match topology::query_topology_for_offering(stone_endpoint, offering_name).await {
            Ok(stones) => {
                if stones.is_empty() {
                    continue;
                }
                let kind = match OfferingKind::from_topology_name(offering_name) {
                    Some(k) => k,
                    None => continue,
                };
                tracing::debug!(
                    count = stones.len(),
                    offering = %offering_name,
                    "topology: discovered AI instances"
                );
                for topo_stone in &stones {
                    let (vram_total, gpu_name) = extract_hw_from_caps(&topo_stone.capabilities);

                    // Prefer actual detected port from topology, fall back to manifest default
                    let service_port = topo_stone
                        .ports
                        .get("default")
                        .copied()
                        .unwrap_or_else(|| kind.default_service_port().unwrap_or(0));
                    let endpoint = format!(
                        "http://{}:{}",
                        topo_stone.ip, service_port
                    );

                    tracing::debug!(
                        stone = %topo_stone.stone_name,
                        kind = %kind,
                        endpoint = %endpoint,
                        "topology: registering AI instance"
                    );

                    let instance = build_instance_from_discovery(
                        topo_stone.stone_id.clone(),
                        topo_stone.stone_name.clone(),
                        endpoint.clone(),
                        kind,
                        vram_total,
                        gpu_name,
                    );
                    { let cfg = state.config.read().await; state.registry.upsert_instance(instance, &cfg).await; drop(cfg); };
                    profile_instance(state, &endpoint, kind).await;
                }
            }
            Err(e) => {
                tracing::debug!(
                    offering = %offering_name,
                    error = %e,
                    "topology query failed for offering"
                );
            }
        }
    }
}

/// Extract VRAM total bytes and GPU name from hardware capabilities.
fn extract_hw_from_caps(
    caps: &Option<garden_common::types::HardwareCapabilities>,
) -> (u64, Option<String>) {
    let Some(caps) = caps else {
        return (0, None);
    };

    let vram_mb = caps
        .hardware
        .ai_capabilities
        .as_ref()
        .map(|ai| ai.total_vram_mb)
        .unwrap_or(0);

    let gpu_name = caps.hardware.gpus.first().map(|g| g.model.clone());

    (vram_mb * 1_048_576, gpu_name)
}

/// Build a `ServiceInstance` from discovery data with defaults.
fn build_instance_from_discovery(
    stone_id: String,
    stone_name: String,
    endpoint: String,
    kind: OfferingKind,
    vram_total_bytes: u64,
    gpu_name: Option<String>,
) -> ServiceInstance {
    let priority = if kind.is_cloud() { -10 } else { 0 };
    let compute = if vram_total_bytes > 0 {
        ComputeType::Gpu
    } else {
        ComputeType::Cpu
    };

    ServiceInstance {
        stone: Stone {
            id: stone_id,
            name: stone_name,
        },
        endpoint,
        kind,
        gpu: Gpu {
            name: gpu_name,
            compute,
        },
        vram: Vram {
            total_bytes: vram_total_bytes,
            budget_bytes: vram_total_bytes,
            free_bytes: None,
        },
        health: InstanceHealth::Profiling,
        models_available: vec![],
        models_loaded: vec![],
        capabilities: vec![],
        queue_depth: 0,
        last_seen: Instant::now(),
        metadata: serde_json::Value::Null,
        priority,
    }
}

/// Profile an instance through the Offering trait: probe + enumerate.
///
/// Called after every instance registration (topology, SSE, refresh).
/// Transitions health to Healthy, populates models_available, and
/// upserts model metadata into the global registry.
async fn profile_instance(state: &AppState, endpoint: &str, kind: OfferingKind) {
    let adapter = match state.providers.get(kind) {
        Some(a) => a,
        None => return, // no provider registered for this offering type
    };

    let ctx = crate::catalog::ProviderContext {
        endpoint: endpoint.to_string(),
        model: None,
        api_key: None,
    };

    // Probe for liveness
    match adapter.probe(&ctx).await {
        Ok(probe) => {
            state
                .registry
                .set_instance_health(endpoint, InstanceHealth::Healthy)
                .await;

            // Store probe results: capabilities always, metadata when available
            state
                .registry
                .update_instance_capabilities(
                    endpoint,
                    probe.capabilities,
                    probe.version,
                )
                .await;
        }
        Err(e) => {
            // Only log WARN on the first failure; subsequent failures are debug
            // to avoid flooding the console with dead endpoint spam.
            let already_unhealthy = {
                let snap = state.registry.snapshot().clone();
                snap.instances.get(endpoint)
                    .map(|i| !i.is_routable())
                    .unwrap_or(false)
            };
            if already_unhealthy {
                tracing::debug!(
                    endpoint = %endpoint,
                    kind = %kind,
                    "probe still failing (already marked unhealthy)"
                );
            } else {
                tracing::warn!(
                    endpoint = %endpoint,
                    kind = %kind,
                    error = %e,
                    "probe failed during profiling"
                );
            }
            state
                .registry
                .set_instance_health(
                    endpoint,
                    InstanceHealth::Unhealthy {
                        since: std::time::Instant::now(),
                        reason: format!("probe failed: {e}"),
                    },
                )
                .await;
            return;
        }
    }

    // Resolve the stone name for building FQNs (needed for directory)
    let stone_name = {
        let snap = state.registry.snapshot().clone();
        snap.instances
            .get(endpoint)
            .map(|i| i.stone.name.clone())
            .unwrap_or_default()
    };

    // Enumerate models/resources
    match adapter.enumerate(&ctx).await {
        Ok(service_models) => {
            let model_names: Vec<String> = service_models.iter().map(|m| m.name.clone()).collect();
            let count = model_names.len();

            // Update instance model inventory
            state
                .registry
                .update_instance_models(endpoint, model_names, vec![])
                .await;

            // Upsert each model into both the legacy registry and the directory
            for sm in &service_models {
                let quantization = sm
                    .metadata
                    .get("quantization_level")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let parameter_size = sm
                    .metadata
                    .get("parameter_size")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let family = sm
                    .metadata
                    .get("family")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let families: Vec<String> = sm
                    .metadata
                    .get("families")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let format = sm
                    .metadata
                    .get("format")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let size_disk = sm
                    .metadata
                    .get("size_disk")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let context_length = sm
                    .metadata
                    .get("context_length")
                    .and_then(|v| v.as_u64());
                let parameter_count = sm
                    .metadata
                    .get("parameter_count")
                    .and_then(|v| v.as_u64());

                // ORCH-0015: contribute to model directory
                let fqn = ModelFqn::new(
                    kind.as_str(),
                    &stone_name,
                    &sm.name,
                    quantization.clone(),
                );
                let caps: Vec<Capability> = sm.capabilities.clone();
                let meta = ModelMetadata {
                    parameter_count,
                    parameter_size,
                    quantization_level: quantization,
                    family,
                    families,
                    format,
                    size_disk,
                    vram_bytes: sm.vram_bytes,
                    context_length,
                };
                state
                    .directory
                    .upsert(fqn, caps, sm.specializations.clone(), meta)
                    .await;
            }

            tracing::debug!(
                endpoint = %endpoint,
                kind = %kind,
                models = count,
                "profiled instance"
            );
        }
        Err(e) => {
            tracing::warn!(
                endpoint = %endpoint,
                kind = %kind,
                error = %e,
                "enumerate failed during profiling"
            );
        }
    }

    // Provision skills for this instance (ORCH-0022)
    // Skills are already loaded from disk. For each skill matching this provider,
    // ensure dependencies are cached locally and pushed to the instance.
    let moss_endpoint = derive_moss_endpoint(endpoint);
    let offering_fqn = kind.as_str().to_string();
    let cache_paths = crate::skills::cache::CachePaths::new(
        std::path::Path::new(&state.data_dir),
        kind.as_str(),
    );

    // Get all registered skills for this provider kind
    let skills_snapshot = state.skills.snapshot().clone();
    let provider_skills: Vec<_> = skills_snapshot
        .skills
        .iter()
        .filter(|sv| sv.definition.provider_kind == kind)
        .collect();

    if provider_skills.is_empty() {
        return;
    }

    let stone_name = endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("unknown")
        .to_string();
    let instance_vram_mb = {
        let snap = state.registry.snapshot().clone();
        snap.instances
            .get(endpoint)
            .map(|i| i.vram.total_bytes / 1_048_576)
            .unwrap_or(0)
    };

    // Shared HTTP client for readiness checks (one per profile cycle, not per skill)
    let http = &state.http;

    for skill_view in provider_skills {
        let skill = &skill_view.definition;
        let skill_name = skill.name.clone();

        // Check if all models are already on the instance
        let manifest = crate::skills::cache::DependencyManifest::load(&cache_paths.manifest_path).await;
        let readiness = crate::skills::provisioner::check_instance_readiness(
            http, skill, &manifest, &moss_endpoint, &offering_fqn, "comfyui-models",
        ).await;

        if readiness.ready {
            state.skills.set_readiness(
                &skill_name, endpoint,
                crate::domain::skill::SkillInstanceView {
                    stone_name: stone_name.clone(),
                    endpoint: endpoint.to_string(),
                    ready: true,
                    reason: readiness.reason,
                    vram_mb: instance_vram_mb,
                },
            ).await;
            continue;
        }

        // Not ready — submit to provisioning queue (ORCH-0024).
        // The queue handles dedup, backoff, and bounded concurrency.
        let target = crate::domain::provisioning::ProvisioningTarget {
            skill: skill_name.clone(),
            endpoint: endpoint.to_string(),
        };

        let submitted = state.provisioning.submit(
            target,
            crate::domain::provisioning::Priority::Discovery,
            stone_name.clone(),
            kind.as_str().to_string(),
        ).await;

        if submitted {
            tracing::info!(skill = %skill_name, endpoint = %endpoint, "queued skill provisioning");
        }
    }
}

/// Derive the Moss HTTP endpoint from a service endpoint.
///
/// Replaces the port in the URL with 7185 (Moss default).
fn derive_moss_endpoint(service_endpoint: &str) -> String {
    if let Some(colon_pos) = service_endpoint.rfind(':') {
        format!(
            "{}:{}",
            &service_endpoint[..colon_pos],
            garden_common::constants::MOSS_HTTP
        )
    } else {
        format!("{service_endpoint}:{}", garden_common::constants::MOSS_HTTP)
    }
}

/// Periodically re-query the topology to catch stones the SSE stream missed.
async fn topology_refresh_loop(
    stone_endpoint: String,
    state: AppState,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(TOPOLOGY_REFRESH_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        // Hot-reload: rescan skills directory for new/removed skills
        let skills_dir = std::path::PathBuf::from(&state.data_dir).join("skills");
        let disk_skills = crate::skills::loader::load_skills(&skills_dir).await;

        // Register new skills, update existing ones
        let current_snapshot = state.skills.snapshot().clone();
        let current_names: std::collections::HashSet<String> = current_snapshot
            .skills
            .iter()
            .map(|sv| sv.definition.name.clone())
            .collect();
        let disk_names: std::collections::HashSet<String> = disk_skills
            .iter()
            .map(|s| s.name.clone())
            .collect();

        // Only register NEW skills — don't re-register existing ones
        for skill in disk_skills {
            if !current_names.contains(&skill.name) {
                tracing::info!(skill = %skill.name, "hot-reload: new skill detected");
                state.skills.register(skill).await;
            }
        }

        // Unregister removed skills + GC their cached models
        let mut any_removed = false;
        for name in &current_names {
            if !disk_names.contains(name) {
                tracing::info!(skill = %name, "hot-reload: skill removed from disk");
                state.skills.unregister(name).await;
                any_removed = true;
            }
        }

        if any_removed {
            // Run GC for each provider that might have orphaned models
            for kind in crate::domain::types::OfferingKind::LOCAL_OFFERING_NAMES {
                let cache_paths = crate::skills::cache::CachePaths::new(
                    std::path::Path::new(&state.data_dir),
                    kind,
                );
                if let Ok(removed) = crate::skills::cache::garbage_collect(&skills_dir, &cache_paths).await {
                    if removed > 0 {
                        tracing::info!(provider = kind, removed, "GC: cleaned unreferenced models");
                    }
                }
            }
        }

        discover_from_topology(&stone_endpoint, &state).await;
    }
}

/// Resolve a stone endpoint through the priority cascade:
///
/// 1. Explicit `--stone` / `GARDEN_STONE` override
/// 2. Cached tending (persisted `.tending` file, validated via health check)
/// 3. Koi mDNS discovery (browse for `_moss._tcp`, pick first healthy stone)
///
/// Returns `None` only if shutdown is requested.
async fn resolve_stone(state: &AppState, shutdown: &CancellationToken) -> Option<String> {
    // ── 1. Explicit stone override (preferred hint, not mandatory) ─
    if let Some(ref explicit) = state.explicit_stone {
        if orchestrator_common::discovery::check_stone_health(explicit).await {
            tracing::info!(endpoint = %explicit, "using explicit stone override (healthy)");
            let tended = TendedStone {
                stone_name: "explicit".to_string(),
                stone_id: None,
                endpoint: explicit.clone(),
                last_seen: chrono::Utc::now(),
            };
            state.tend_to(tended).await;
            return Some(explicit.clone());
        }
        tracing::warn!(
            endpoint = %explicit,
            "explicit stone unreachable, falling through to discovery"
        );
    }

    // ── 2. Local Moss (same machine) ──────────────────────────────
    // The AI orchestrator typically runs alongside Moss. Local Moss has
    // the most complete topology view (it sees its own services). Prefer
    // it over remote stones discovered via mDNS.
    //
    // Try both localhost (native) and host.docker.internal (Docker).
    {
        let moss_port = garden_common::constants::MOSS_HTTP;
        let candidates = [
            format!("http://localhost:{moss_port}"),
            format!("http://host.docker.internal:{moss_port}"),
        ];

        for local in &candidates {
            if orchestrator_common::discovery::check_stone_health(local).await {
                tracing::info!(endpoint = %local, "using local Moss for topology");
                let tended = TendedStone {
                    stone_name: "local".to_string(),
                    stone_id: None,
                    endpoint: local.clone(),
                    last_seen: chrono::Utc::now(),
                };
                state.tend_to(tended).await;
                return Some(local.clone());
            }
        }
        tracing::debug!("local Moss not reachable, trying cached/mDNS");
    }

    // ── 3. Cached tending state ──────────────────────────────────
    {
        let tended = state.tended_stone.read().await;
        if let Some(ref stone) = *tended {
            tracing::info!(
                stone = %stone.stone_name,
                endpoint = %stone.endpoint,
                "checking cached tending state"
            );
            if orchestrator_common::discovery::check_stone_health(&stone.endpoint).await {
                tracing::info!(stone = %stone.stone_name, "cached stone is healthy");
                return Some(stone.endpoint.clone());
            }
            tracing::warn!(
                stone = %stone.stone_name,
                "cached stone unreachable, will re-discover"
            );
        }
    }
    state.clear_tending().await;

    // ── 4. Koi mDNS discovery ────────────────────────────────────
    loop {
        if shutdown.is_cancelled() {
            return None;
        }

        if !orchestrator_common::discovery::check_koi_health(&state.koi_endpoint).await {
            tracing::warn!(
                koi = %state.koi_endpoint,
                "Koi not reachable, retrying in 5s"
            );
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
                _ = shutdown.cancelled() => return None,
            }
        }

        tracing::info!(koi = %state.koi_endpoint, "discovering stones via Koi mDNS");

        match orchestrator_common::discovery::discover_stones(&state.koi_endpoint).await {
            Ok(stones) if stones.is_empty() => {
                tracing::warn!("no stones found on the network, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Ok(stones) => {
                for stone in &stones {
                    let endpoint = stone.endpoint();
                    tracing::info!(
                        stone = %stone.stone_name,
                        endpoint = %endpoint,
                        "checking discovered stone health"
                    );
                    if orchestrator_common::discovery::check_stone_health(&endpoint).await {
                        let tended = TendedStone {
                            stone_name: stone.stone_name.clone(),
                            stone_id: stone.stone_id.clone(),
                            endpoint: endpoint.clone(),
                            last_seen: chrono::Utc::now(),
                        };
                        state.tend_to(tended).await;
                        return Some(endpoint);
                    }
                }
                tracing::warn!(
                    "discovered {} stone(s) but none are healthy, retrying in 10s",
                    stones.len()
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "mDNS discovery failed, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
        }
    }
}
