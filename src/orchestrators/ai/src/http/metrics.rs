//! `GET /metrics` — Prometheus text format exposing the live
//! `CapabilityDirectory` shape.
//!
//! # ORCH-0030 R2 M3 changes
//!
//! The legacy `Directory` aggregate and the `DemandLedger` have
//! been deleted. Metrics now read from
//! [`crate::services::directory_subscriber::CapabilityDirectory`]
//! and report:
//!
//! - `zg_orchestrator_directory_version` — monotonic snapshot version
//! - `zg_orchestrator_providers_total{enabled}` — number of registered
//!   providers, partitioned by enabled / disabled
//! - `zg_orchestrator_capabilities_total` — number of distinct
//!   (provider, primitive) pairs currently published
//! - `zg_orchestrator_skills_total` — number of distinct
//!   (provider, skill_id) pairs currently published
//!
//! Per-request demand counters are gone — adapters that need
//! per-model demand for their own scoring (Ollama) maintain their
//! own internal counters.

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::fmt::Write;

use crate::app_state::AppState;

pub async fn get_metrics(State(state): State<AppState>) -> Response {
    let mut body = String::new();

    // Directory shape.
    let version = state.capability_directory.version();
    let providers_map = state.capability_directory.providers().await;
    let provider_count = providers_map.len();
    let enabled_count = providers_map.values().filter(|p| p.enabled).count();
    let disabled_count = provider_count - enabled_count;

    let capability_count: usize = providers_map
        .values()
        .filter(|p| p.enabled)
        .map(|p| p.announcement.capabilities.len())
        .sum();

    let skill_count: usize = providers_map
        .values()
        .filter(|p| p.enabled)
        .map(|p| p.announcement.skills.len())
        .sum();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_directory_version Monotonic CapabilityDirectory version."
    )
    .ok();
    writeln!(&mut body, "# TYPE zg_orchestrator_directory_version gauge").ok();
    writeln!(&mut body, "zg_orchestrator_directory_version {}", version).ok();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_providers_total Number of registered providers, partitioned by enabled state."
    )
    .ok();
    writeln!(
        &mut body,
        "# TYPE zg_orchestrator_providers_total gauge"
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_providers_total{{enabled=\"true\"}} {}",
        enabled_count
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_providers_total{{enabled=\"false\"}} {}",
        disabled_count
    )
    .ok();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_capabilities_total Number of distinct (provider, primitive) capability declarations."
    )
    .ok();
    writeln!(
        &mut body,
        "# TYPE zg_orchestrator_capabilities_total gauge"
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_capabilities_total {}",
        capability_count
    )
    .ok();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_skills_total Number of distinct (provider, skill_id) skill declarations."
    )
    .ok();
    writeln!(&mut body, "# TYPE zg_orchestrator_skills_total gauge").ok();
    writeln!(&mut body, "zg_orchestrator_skills_total {}", skill_count).ok();

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}
