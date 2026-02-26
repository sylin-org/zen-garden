//! Cluster management API endpoints.

use crate::app_state::AppState;
use crate::domain::types::{derive_replica_set_name, MongoInstance, PendingAction, ReplicaRole};
use crate::infra::mongo_client::MongoClient;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// `GET /api/cluster/status` — overview of all replica sets.
pub async fn get_cluster_status(State(state): State<AppState>) -> Json<Value> {
    let replica_sets = state.replica_sets.read().await;
    let instances = state.instances.read().await;

    let rs_list: Vec<Value> = replica_sets
        .iter()
        .map(|(fqn, rs)| {
            let primary = rs
                .members
                .iter()
                .find(|m| m.role == ReplicaRole::Primary)
                .map(|m| m.endpoint.clone());

            let secondary_count = rs
                .members
                .iter()
                .filter(|m| m.role == ReplicaRole::Secondary)
                .count();

            let max_lag = rs
                .members
                .iter()
                .filter_map(|m| m.lag_seconds)
                .fold(0.0_f64, f64::max);

            json!({
                "fqn": fqn,
                "rs_name": rs.rs_name,
                "initialized": rs.initialized,
                "primary": primary,
                "secondaries": secondary_count,
                "total_members": rs.members.len(),
                "max_lag_seconds": max_lag,
                "connection_string": rs.connection_string,
                "last_updated": rs.last_updated.to_rfc3339(),
            })
        })
        .collect();

    Json(json!({
        "replica_sets": rs_list,
        "total_instances": instances.len(),
    }))
}

/// `GET /api/cluster/members` — all instances grouped by FQN ("logical sets").
///
/// Shows every discovered MongoDB instance, including singletons that haven't
/// been initialized into a replica set yet. RS state (role, lag) is overlaid
/// when available.
pub async fn get_cluster_members(State(state): State<AppState>) -> Json<Value> {
    let instances = state.instances.read().await;
    let replica_sets = state.replica_sets.read().await;

    // Group instances by FQN
    let mut fqn_groups: std::collections::HashMap<String, Vec<&MongoInstance>> =
        std::collections::HashMap::new();
    for instance in instances.values() {
        fqn_groups
            .entry(instance.fqn.clone())
            .or_default()
            .push(instance);
    }

    let mut logical_sets: Vec<Value> = Vec::new();

    for (fqn, group) in &fqn_groups {
        let rs = replica_sets.get(fqn.as_str());
        let rs_name = rs
            .map(|r| r.rs_name.clone())
            .unwrap_or_else(|| derive_replica_set_name(fqn));
        let initialized = rs.map(|r| r.initialized).unwrap_or(false);
        let connection_string = rs.and_then(|r| r.connection_string.clone());

        let members: Vec<Value> = group
            .iter()
            .map(|inst| {
                // Try to find matching RS member for role/lag overlay
                let rs_member = rs.and_then(|r| {
                    r.members
                        .iter()
                        .find(|m| m.endpoint == inst.mongo_endpoint)
                });

                json!({
                    "endpoint": inst.mongo_endpoint,
                    "stone_name": inst.stone_name,
                    "health": inst.health,
                    "role": rs_member.map(|m| &m.role).or(inst.role.as_ref()),
                    "healthy": rs_member.map(|m| m.healthy),
                    "lag_seconds": rs_member.and_then(|m| m.lag_seconds),
                    "last_heartbeat": rs_member
                        .and_then(|m| m.last_heartbeat)
                        .map(|dt| dt.to_rfc3339()),
                })
            })
            .collect();

        logical_sets.push(json!({
            "fqn": fqn,
            "rs_name": rs_name,
            "initialized": initialized,
            "connection_string": connection_string,
            "members": members,
        }));
    }

    // Sort by FQN for stable output
    logical_sets.sort_by(|a, b| {
        a.get("fqn")
            .and_then(|v| v.as_str())
            .cmp(&b.get("fqn").and_then(|v| v.as_str()))
    });

    Json(json!({ "logical_sets": logical_sets }))
}

/// Query parameters for connection string endpoint.
#[derive(Deserialize)]
pub struct ConnectQuery {
    /// Optional: return connection string relative to a specific stone.
    pub from: Option<String>,
}

/// `GET /api/cluster/connect` — connection strings for all replica sets.
pub async fn get_connection_strings(
    State(state): State<AppState>,
    Query(_query): Query<ConnectQuery>,
) -> Json<Value> {
    let replica_sets = state.replica_sets.read().await;

    let connections: Vec<Value> = replica_sets
        .iter()
        .filter(|(_, rs)| rs.initialized)
        .map(|(fqn, rs)| {
            json!({
                "fqn": fqn,
                "rs_name": rs.rs_name,
                "connection_string": rs.connection_string,
                "members": rs.members.len(),
            })
        })
        .collect();

    Json(json!({ "connections": connections }))
}

/// Request body for stepdown.
#[derive(Deserialize)]
pub struct StepdownRequest {
    /// FQN of the replica set to step down (default: "mongodb").
    pub fqn: Option<String>,
    /// Seconds to step down for (default: 60).
    pub seconds: Option<u32>,
}

/// Response for stepdown.
#[derive(Serialize)]
pub struct StepdownResponse {
    pub success: bool,
    pub message: String,
}

/// `POST /api/cluster/stepdown` — ask the primary to step down.
pub async fn post_stepdown(
    State(state): State<AppState>,
    Json(req): Json<StepdownRequest>,
) -> Result<Json<StepdownResponse>, (StatusCode, Json<StepdownResponse>)> {
    let fqn = req.fqn.as_deref().unwrap_or("mongodb");
    let seconds = req.seconds.unwrap_or(60);

    let rs = state.replica_set_for(fqn).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(StepdownResponse {
                success: false,
                message: format!("replica set for FQN '{fqn}' not found"),
            }),
        )
    })?;

    let primary = rs
        .members
        .iter()
        .find(|m| m.role == ReplicaRole::Primary)
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                Json(StepdownResponse {
                    success: false,
                    message: "no primary found in replica set".to_string(),
                }),
            )
        })?;

    let client =
        MongoClient::connect(&primary.endpoint)
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(StepdownResponse {
                        success: false,
                        message: format!("cannot connect to primary: {e}"),
                    }),
                )
            })?;

    client.rs_step_down(seconds).await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(StepdownResponse {
                success: false,
                message: format!("stepdown command failed: {e}"),
            }),
        )
    })?;

    tracing::info!(
        fqn = %fqn,
        primary = %primary.endpoint,
        seconds = seconds,
        "primary stepdown requested"
    );

    state
        .emit_event(
            "rs.stepdown",
            &json!({ "fqn": fqn, "primary": primary.endpoint, "seconds": seconds }).to_string(),
        )
        .await;

    Ok(Json(StepdownResponse {
        success: true,
        message: format!(
            "stepdown requested on {} for {}s",
            primary.endpoint, seconds
        ),
    }))
}

/// Request body for installing MongoDB on a stone.
#[derive(Deserialize)]
pub struct InstallRequest {
    /// Moss endpoint of the target stone (e.g. `http://192.168.1.5:7185`).
    pub moss_endpoint: String,
    /// Optional FQN suffix. Empty or absent = default pool ("mongodb").
    /// A value like "dev" creates "mongodb:dev".
    pub fqn_suffix: Option<String>,
}

/// `POST /api/cluster/install` — install MongoDB on a stone via its Moss API.
///
/// Proxies a `POST /api/v1/stone/services` to the target stone, requesting
/// the `mongodb` offering with an optional FQN.
pub async fn post_install(
    State(state): State<AppState>,
    Json(req): Json<InstallRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let fqn = match &req.fqn_suffix {
        Some(suffix) if !suffix.is_empty() => format!("mongodb:{suffix}"),
        _ => "mongodb".to_string(),
    };

    let url = format!(
        "{}/api/v1/stone/services",
        req.moss_endpoint.trim_end_matches('/')
    );

    // Accept self-signed certs for internal stone-to-stone communication
    // (pond CA issues certs that are not in the system trust store).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "message": format!("HTTP client error: {e}") })),
            )
        })?;

    let resp = client
        .post(&url)
        .json(&json!({ "offering": fqn }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "success": false, "message": format!("Cannot reach stone: {e}") })),
            )
        })?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));

    if !status.is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "success": false,
                "message": format!("Stone returned {}: {}", status, body),
            })),
        ));
    }

    tracing::info!(
        stone = %req.moss_endpoint,
        fqn = %fqn,
        "install requested via dashboard"
    );

    state
        .emit_event(
            "install.requested",
            &json!({ "stone": req.moss_endpoint, "fqn": fqn }).to_string(),
        )
        .await;

    Ok(Json(json!({
        "success": true,
        "message": format!("MongoDB installation requested on {}", req.moss_endpoint),
        "fqn": fqn,
    })))
}

/// `DELETE /api/cluster/members/:endpoint` — remove a member from the logical set.
///
/// Queues a `PendingAction::RemoveMember` that will execute `rs.remove()` when
/// the target is reachable, then removes the instance from the registry.
/// If the instance comes back with the same FQN, it will be auto-readmitted.
pub async fn delete_member(
    State(state): State<AppState>,
    Path(endpoint): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // URL-decode the endpoint (it may contain colons, dots)
    let mongo_endpoint = urlencoding::decode(&endpoint)
        .map(|s| s.into_owned())
        .unwrap_or(endpoint);

    // Look up the instance to get its FQN
    let fqn = {
        let reg = state.instances.read().await;
        reg.get(&mongo_endpoint).map(|i| i.fqn.clone())
    };

    let fqn = fqn.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "message": format!("instance '{}' not found in registry", mongo_endpoint),
            })),
        )
    })?;

    // Check if there's already a pending removal
    if state.has_pending_removal(&mongo_endpoint).await {
        return Ok(Json(json!({
            "success": true,
            "message": "removal already pending",
            "endpoint": mongo_endpoint,
            "fqn": fqn,
        })));
    }

    // Queue the removal action
    let action = PendingAction::RemoveMember {
        mongo_endpoint: mongo_endpoint.clone(),
        fqn: fqn.clone(),
        requested_at: chrono::Utc::now(),
    };

    state.queue_action(action).await;

    tracing::info!(
        endpoint = %mongo_endpoint,
        fqn = %fqn,
        "member removal queued"
    );

    state
        .emit_event(
            "rs.member.removal_queued",
            &json!({ "endpoint": mongo_endpoint, "fqn": fqn }).to_string(),
        )
        .await;

    Ok(Json(json!({
        "success": true,
        "message": format!("removal queued for {}", mongo_endpoint),
        "endpoint": mongo_endpoint,
        "fqn": fqn,
    })))
}

/// `GET /api/cluster/actions` — list pending membership actions.
pub async fn get_pending_actions(State(state): State<AppState>) -> Json<Value> {
    let actions = state.pending_actions_snapshot().await;
    Json(json!({ "pending_actions": actions }))
}
