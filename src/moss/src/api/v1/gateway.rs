//! Gateway registration API — ORCH-0004
//!
//! Orchestrators register as gateways for offerings they front.
//! PUT upserts (idempotent), DELETE removes. Both trigger auto-chirp
//! so the gateway entry propagates through topology.

use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use garden_common::GatewayRegistration;
use serde::{Deserialize, Serialize};

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
        "Gateway registered"
    );

    {
        let mut gateways = state.gateways.write().await;
        gateways.insert(offering, registration);
    }

    // Auto-chirp: gateways changed → propagate via topology
    state.sync_self_services(true).await;

    // Refresh tools projection so gateways appear in /api/v1/garden/tools
    state.refresh_local_tools_projection().await;

    Ok(Json(PutGatewayResponse {
        lease_id,
        ttl_seconds: 60,
    }))
}

/// DELETE /api/v1/garden/gateway/{offering}
///
/// Deregister a gateway. Triggers auto-chirp to remove from topology.
pub async fn delete_gateway(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> StatusCode {
    let removed = {
        let mut gateways = state.gateways.write().await;
        gateways.remove(&offering).is_some()
    };

    if removed {
        tracing::info!(offering = %offering, "Gateway deregistered");
        state.sync_self_services(true).await;
        state.refresh_local_tools_projection().await;
    } else {
        tracing::debug!(offering = %offering, "Gateway not found for deregistration");
    }

    StatusCode::OK
}
