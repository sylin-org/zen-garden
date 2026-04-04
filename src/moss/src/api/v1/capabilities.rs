//! Hardware capabilities API endpoints (ARCH-0014).
//!
//! Tier 1 (core): fast, always fresh, gates offering compatibility.
//! Tier 2 (topology): background probe, cached, delta-gated.
//!
//! Endpoints:
//!   GET  /api/v1/stone/capabilities          → FullCapabilities (both tiers)
//!   GET  /api/v1/stone/capabilities/core     → HardwareCapabilities (Tier 1)
//!   GET  /api/v1/stone/capabilities/topology → HardwareTopology (Tier 2)
//!   POST /api/v1/stone/capabilities/refresh  → 202 Accepted (trigger re-probe)

use crate::api::responses::ApiResponse;
use crate::domain::Current;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use garden_common::types::hardware_topology::{FullCapabilities, HardwareTopology};
use garden_common::HardwareCapabilities;
use std::sync::Arc;

/// GET /api/v1/stone/capabilities — Full capabilities (Tier 1 + Tier 2).
///
/// Returns `FullCapabilities` with `core` always populated and `topology`
/// set to `None` while the first Tier 2 probe is still running.
pub async fn get_capabilities(
    State(current): State<Arc<Current>>,
) -> Json<ApiResponse<FullCapabilities>> {
    let core = read_core(&current).await;
    let topology = current.hardware_topology.read().await.clone();

    Json(ApiResponse::new(FullCapabilities { core, topology }))
}

/// GET /api/v1/stone/capabilities/core — Tier 1 only (fast, offering compat).
///
/// Identical shape to the pre-ARCH-0014 `/capabilities` response.
/// Backwards compatible for existing consumers.
pub async fn get_capabilities_core(
    State(current): State<Arc<Current>>,
) -> Json<ApiResponse<HardwareCapabilities>> {
    let core = read_core(&current).await;
    Json(ApiResponse::new(core))
}

/// GET /api/v1/stone/capabilities/topology — Tier 2 only (deep, cached).
///
/// Returns `None` (204 No Content) while the probe is still running.
pub async fn get_capabilities_topology(
    State(current): State<Arc<Current>>,
) -> Result<Json<ApiResponse<HardwareTopology>>, StatusCode> {
    let guard = current.hardware_topology.read().await;
    match guard.as_ref() {
        Some(topo) => Ok(Json(ApiResponse::new(topo.clone()))),
        None => Err(StatusCode::NO_CONTENT),
    }
}

/// POST /api/v1/stone/capabilities/refresh — Trigger immediate re-probe.
///
/// Invalidates the cached topology and kicks a full background re-probe.
/// Returns 202 Accepted immediately — the probe runs asynchronously.
pub async fn refresh_capabilities(
    State(current): State<Arc<Current>>,
) -> StatusCode {
    // Clear cached topology to force re-probe
    {
        let mut guard = current.hardware_topology.write().await;
        *guard = None;
    }

    // Spawn re-probe in background (silent console — refresh is API-driven,
    // results are logged via tracing, not console events)
    let current_clone = current.clone();
    let console = Arc::new(garden_common::console::ConsolePrinter::new(
        garden_common::console::ConsoleMode::Silent,
    ));
    tokio::spawn(async move {
        crate::tasks::topology_probe::probe_now(
            current_clone,
            console,
        )
        .await;
    });

    StatusCode::ACCEPTED
}

/// Read Tier 1 capabilities from shared state, falling back to skeleton.
async fn read_core(current: &Current) -> HardwareCapabilities {
    let guard = current.capabilities.read().await;
    match guard.as_ref() {
        Some(caps) => caps.clone(),
        None => crate::infra::hardware::create_skeleton(current.stone.name.to_string()),
    }
}
