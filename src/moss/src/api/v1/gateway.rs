//! Gateway registration API — ORCH-0004
//!
//! Orchestrators register as gateways for offerings they front.
//! PUT upserts (idempotent), DELETE removes.
//!
//! Writes go directly to `tool.registry` with `EntryOrigin::Gateway` and a
//! TTL. The registry reaper removes expired entries. No separate registry.

use crate::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use garden_common::GatewayRegistration;
use garden_common::offerings::OfferingFqn;
use garden_common::tools::{GardenTool, ServiceInfo, Stone, ToolIdentity};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Gateway TTL — entries expire if not refreshed within this period.
const GATEWAY_TTL_SECS: u64 = 60;

/// Request body for PUT /api/v1/garden/gateway/{offering}
#[derive(Debug, Deserialize)]
pub struct PutGatewayRequest {
    pub fqn: String,
    pub hostname: String,
    pub ip: String,
    pub port: u16,
    pub handler_for: Vec<String>,
    pub protocol: String,
    #[serde(default)]
    pub uri_template: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source: String,
}

/// Response for PUT /api/v1/garden/gateway/{offering}
#[derive(Debug, Serialize)]
pub struct PutGatewayResponse {
    pub lease_id: String,
    pub ttl_seconds: u32,
}

/// PUT /api/v1/garden/gateway/{offering}
///
/// Register or refresh a gateway for an offering. Idempotent upsert.
/// The orchestrator calls this every 30s as a heartbeat.
pub async fn put_gateway(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(body): Json<PutGatewayRequest>,
) -> Result<Json<PutGatewayResponse>, StatusCode> {
    // Validate: handler_for must contain the path offering
    if !body.handler_for.contains(&offering) {
        tracing::warn!(
            offering = %offering,
            handler_for = ?body.handler_for,
            "Gateway handler_for does not contain path offering"
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    let registration = GatewayRegistration {
        fqn: body.fqn,
        handler_for: body.handler_for,
        hostname: body.hostname,
        ip: body.ip,
        port: body.port,
        protocol: body.protocol,
        uri_template: body.uri_template,
        category: body.category,
        tags: body.tags,
        source: body.source,
        registered_at: Utc::now(),
    };

    let lease_id = format!("gw-{}", offering);

    tracing::info!(
        offering = %offering,
        fqn = %registration.fqn,
        hostname = %registration.hostname,
        port = registration.port,
        "{} gateway registration for {}",
        offering,
        registration.fqn,
    );

    // Build GardenTool for registry
    let fqn = OfferingFqn::parse(&registration.fqn).ok();
    let fqid = fqn
        .as_ref()
        .map(|f| f.fqn())
        .unwrap_or_else(|| registration.fqn.clone());
    let tool_type = fqn
        .as_ref()
        .map(|f| f.offering.clone())
        .unwrap_or_else(|| offering.clone());
    let instance = fqn
        .as_ref()
        .and_then(|f| f.instance.clone())
        .unwrap_or_default();

    let tool = GardenTool {
        fqid: fqid.clone(),
        tool: ToolIdentity {
            name: instance,
            tool_type,
            category: registration
                .category
                .clone()
                .unwrap_or_else(|| garden_common::constants::CATEGORY_ORCHESTRATOR.to_string()),
            id: String::new(),
            tags: registration.tags.clone(),
            source: registration.source.clone(),
        },
        stone: Stone {
            id: state.current.stone.id.clone(),
            name: state.current.stone.name.clone(),
            endpoint: state.current.address.read().await.http_base(),
        },
        service: ServiceInfo {
            status: garden_common::constants::SERVICE_RUNNING.to_string(),
            ready: true,
            protocol: registration.protocol.clone(),
            uris: {
                let template = registration
                    .uri_template
                    .as_deref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| {
                        crate::domain::connection::default_template(&registration.protocol)
                    });
                crate::domain::connection::resolve_uris(
                    &template,
                    &registration.hostname,
                    &registration.ip,
                    registration.port,
                    &registration.protocol,
                )
            },
            hostname: Some(registration.hostname.clone()),
            ip: Some(registration.ip.clone()),
            port: Some(registration.port),
            uri_template: registration.uri_template.clone(),
        },
        capabilities: Vec::new(),
        storage: None,
    };

    let event = state
        .tool
        .register_gateway(tool, Duration::from_secs(GATEWAY_TTL_SECS))
        .await;

    if let Some(event) = event {
        tracing::info!(
            offering = %offering,
            fqn = %registration.fqn,
            "{} gateway entry committed",
            offering,
        );
        crate::domain::tool::projection::publish_events_for_state(&state, &[event]).await;
    } else {
        tracing::debug!(
            offering = %offering,
            fqn = %registration.fqn,
            "{} gateway TTL refreshed (no change)",
            offering,
        );
    }

    Ok(Json(PutGatewayResponse {
        lease_id,
        ttl_seconds: GATEWAY_TTL_SECS as u32,
    }))
}

/// DELETE /api/v1/garden/gateway/{offering}
///
/// Deregister a gateway. Removes from registry and broadcasts removal beacon.
pub async fn delete_gateway(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> StatusCode {
    let event = state
        .tool
        .deregister_gateway(&offering, &state.current.stone.id)
        .await;

    if let Some(event) = event {
        tracing::info!(
            offering = %offering,
            "{} gateway deregistered",
            offering,
        );
        crate::domain::tool::projection::publish_events_for_state(&state, &[event]).await;
    } else {
        tracing::debug!(offering = %offering, "Gateway not found for deregistration");
    }

    StatusCode::OK
}
