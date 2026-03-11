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
use garden_common::offerings::OfferingFqn;
use garden_common::tools::{GardenTool, ServiceInfo, Stone, ToolIdentity};
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

    // Build GardenTool for gateway registration
    let fqn = OfferingFqn::parse(&registration.fqn).ok();
    let fqid = fqn.as_ref().map(|f| f.fqn()).unwrap_or_else(|| registration.fqn.clone());
    let tool_type = fqn
        .as_ref()
        .map(|f| f.offering.clone())
        .unwrap_or_else(|| offering.clone());
    let instance = fqn.as_ref().and_then(|f| f.instance.clone()).unwrap_or_default();

    let tool = GardenTool {
        fqid: fqid.clone(),
        tool: ToolIdentity {
            name: instance,
            tool_type,
            category: registration
                .category
                .clone()
                .unwrap_or_else(|| "orchestrator".to_string()),
            id: String::new(),
            tags: registration.tags.clone(),
        },
        stone: Stone {
            id: state.stone_id.clone(),
            name: state.stone_name.clone(),
            endpoint: state.self_entry.read().await.address.http_base(),
        },
        service: ServiceInfo {
            status: garden_common::SERVICE_RUNNING.to_string(),
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
            // Preserve source fields — don't lose them in URI composition
            hostname: Some(registration.hostname.clone()),
            ip: Some(registration.ip.clone()),
            port: Some(registration.port),
            uri_template: registration.uri_template.clone(),
        },
        capabilities: Vec::new(),
        storage: None,
    };

    let delta = {
        let mut reg = state.fqn_handler.registry.write().await;
        reg.upsert(&offering, tool, registration.handler_for.clone())
    };

    // Broadcast via tools beacon so remote registries get the entry
    if let Some(delta) = delta {
        state.publish_tool_deltas(vec![delta], true).await;
    }

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
    let delta = {
        let mut reg = state.fqn_handler.registry.write().await;
        reg.remove(&offering, &state.stone_id)
    };

    if let Some(delta) = delta {
        tracing::info!(offering = %offering, "Gateway deregistered");
        state.publish_tool_deltas(vec![delta], true).await;
    } else {
        tracing::debug!(offering = %offering, "Gateway not found for deregistration");
    }

    StatusCode::OK
}
