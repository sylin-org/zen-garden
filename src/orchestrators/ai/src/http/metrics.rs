//! `GET /metrics` — Prometheus text format exposing the
//! [`DemandLedger`] per-request counters and live Directory shape.
//!
//! This is the raw data source for a future demand-weighted advisor
//! (ADR §RecommendationEngine, §Demand ledger). v1 emits counters
//! passively; no decisions are made from them.

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::fmt::Write;

use crate::app_state::AppState;

pub async fn get_metrics(State(state): State<AppState>) -> Response {
    let mut body = String::new();

    // Directory shape.
    let snap = state.directory.snapshot();
    writeln!(
        &mut body,
        "# HELP zg_orchestrator_directory_version Monotonic Directory snapshot version."
    )
    .ok();
    writeln!(&mut body, "# TYPE zg_orchestrator_directory_version gauge").ok();
    writeln!(
        &mut body,
        "zg_orchestrator_directory_version {}",
        snap.version
    )
    .ok();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_providers_total Number of registered providers by health state."
    )
    .ok();
    writeln!(
        &mut body,
        "# TYPE zg_orchestrator_providers_total gauge"
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_providers_total{{health=\"healthy\"}} {}",
        snap.healthy_provider_count()
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_providers_total{{health=\"degraded\"}} {}",
        snap.degraded_provider_count()
    )
    .ok();
    writeln!(
        &mut body,
        "zg_orchestrator_providers_total{{health=\"offline\"}} {}",
        snap.offline_provider_count()
    )
    .ok();

    writeln!(
        &mut body,
        "# HELP zg_orchestrator_models_total Models currently registered in the Directory."
    )
    .ok();
    writeln!(&mut body, "# TYPE zg_orchestrator_models_total gauge").ok();
    writeln!(
        &mut body,
        "zg_orchestrator_models_total {}",
        snap.models.len()
    )
    .ok();

    // Demand ledger.
    writeln!(
        &mut body,
        "# HELP zg_orchestrator_requests_total Completed requests bucketed by primitive, provider, model, and outcome mode."
    )
    .ok();
    writeln!(
        &mut body,
        "# TYPE zg_orchestrator_requests_total counter"
    )
    .ok();

    let counters = state.recommendation.demand().snapshot().await;
    for (key, count) in counters {
        writeln!(
            &mut body,
            "zg_orchestrator_requests_total{{primitive=\"{}\",provider=\"{}\",model=\"{}\",outcome=\"{}\"}} {}",
            escape_label(key.primitive.dotted()),
            escape_label(&key.provider),
            escape_label(&key.model),
            escape_label(&key.outcome),
            count
        )
        .ok();
    }

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
}

/// Escape a label value per the Prometheus text exposition format.
fn escape_label(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}
