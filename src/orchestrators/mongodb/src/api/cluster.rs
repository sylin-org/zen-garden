//! Cluster management API endpoints.

use crate::app_state::AppState;
use crate::domain::types::{derive_replica_set_name, InstanceHealth, MongoInstance, PendingAction, ReplicaRole};
use crate::infra::mongo_client::MongoClient;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use garden_common::offerings::OfferingFqn;
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
    let catalog = state.catalog.read().await;

    // Group instances by FQN
    let mut fqn_groups: std::collections::HashMap<OfferingFqn, Vec<&MongoInstance>> =
        std::collections::HashMap::new();
    for instance in instances.values() {
        fqn_groups
            .entry(instance.fqn.clone())
            .or_default()
            .push(instance);
    }

    let mut logical_sets: Vec<Value> = Vec::new();

    for (fqn, group) in &fqn_groups {
        let rs = replica_sets.get(fqn);
        let rs_name = rs
            .map(|r| r.rs_name.clone())
            .unwrap_or_else(|| derive_replica_set_name(fqn));
        let initialized = rs.map(|r| r.initialized).unwrap_or(false);
        let connection_string = rs.and_then(|r| r.connection_string.clone());

        let members: Vec<Value> = group
            .iter()
            .map(|inst| {
                // Try to find matching RS member for role/lag overlay.
                // Uses catalog to resolve endpoint format mismatches between
                // the instance's mongo_endpoint and RS member endpoint strings.
                let rs_member = rs.and_then(|r| {
                    r.members.iter().find(|m| {
                        // Direct match
                        m.endpoint == inst.mongo_endpoint
                            // Catalog-resolved: both resolve to same stone
                            || catalog.resolve_name(&m.endpoint)
                                == Some(inst.stone_name.as_str())
                            // stone_name match (from health monitor)
                            || m.stone_name == inst.stone_name
                    })
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
                    "server_version": inst.server_version,
                    "wire_version_range": inst.wire_version_range.map(|(min, max)| json!([min, max])),
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
    let fqn_str = req.fqn.as_deref().unwrap_or("mongodb");
    let fqn = OfferingFqn::parse(fqn_str).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(StepdownResponse {
                success: false,
                message: format!("invalid FQN '{}': {}", fqn_str, e),
            }),
        )
    })?;
    let seconds = req.seconds.unwrap_or(60);

    let rs = state.replica_set_for(&fqn).await.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(StepdownResponse {
                success: false,
                message: format!("replica set for FQN '{}' not found", fqn),
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
        Some(suffix) if !suffix.is_empty() => {
            OfferingFqn::with_instance("mongodb", suffix).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "success": false, "message": format!("invalid FQN suffix: {e}") })),
                )
            })?
        }
        _ => OfferingFqn::new("mongodb").unwrap(),
    };

    // Reject if this FQN is already installed on the target stone.
    let fqn_str = fqn.to_string();
    let target_ep = req.moss_endpoint.trim_end_matches('/');
    let instances = state.instances.read().await;
    let duplicate = instances.values().any(|i| {
        i.moss_endpoint.trim_end_matches('/') == target_ep && i.fqn.to_string() == fqn_str
    });
    drop(instances);

    if duplicate {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "success": false,
                "message": format!("{fqn_str} is already installed on this stone"),
            })),
        ));
    }

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
        .json(&json!({ "offering": fqn.to_string() }))
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

    // Wake the conductor — new install will eventually produce an instance
    state.conductor_notify.notify_one();

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

    // Resolve endpoint to stone_name via catalog, then look up instance for FQN
    let (stone_name, fqn, resolved_endpoint) = {
        let cat = state.catalog.read().await;
        let reg = state.instances.read().await;

        if let Some(identity) = cat.resolve(&mongo_endpoint) {
            let sn = identity.stone_name.clone();
            let inst_data = reg.get(&sn).map(|i| (i.fqn.clone(), i.mongo_endpoint.clone()));
            inst_data.map(|(fqn, ep)| (sn, fqn, ep))
        } else {
            // Fallback: search by mongo_endpoint field directly
            reg.values()
                .find(|i| i.mongo_endpoint == mongo_endpoint)
                .map(|i| (i.stone_name.clone(), i.fqn.clone(), i.mongo_endpoint.clone()))
        }
    }
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "message": format!("instance '{}' not found in registry", mongo_endpoint),
            })),
        )
    })?;
    // Use the canonical mongo_endpoint from the registry for RS operations
    let mongo_endpoint = resolved_endpoint;

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

    // Wake the conductor — pending removal needs processing
    state.conductor_notify.notify_one();

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
        "stone_name": stone_name,
        "endpoint": mongo_endpoint,
        "fqn": fqn,
    })))
}

/// `DELETE /api/cluster/instances/:stone_name` — remove a stone from the registry.
///
/// For offline/dead stones the user wants to stop tracking:
/// - If the instance is still in the RS, queues `rs.remove()` first.
/// - Removes the instance from the registry immediately (no waiting for RS eviction).
pub async fn delete_instance(
    State(state): State<AppState>,
    Path(stone_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let stone_name = urlencoding::decode(&stone_name)
        .map(|s| s.into_owned())
        .unwrap_or(stone_name);

    // Look up the instance
    let instance_data = {
        let reg = state.instances.read().await;
        reg.get(&stone_name)
            .map(|i| (i.mongo_endpoint.clone(), i.fqn.clone()))
    };

    let (mongo_endpoint, fqn) = instance_data.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "message": format!("instance '{}' not found in registry", stone_name),
            })),
        )
    })?;

    // If the instance is in the RS, queue rs.remove() so the RS config stays clean
    let in_rs = {
        let rss = state.replica_sets.read().await;
        rss.get(&fqn)
            .map(|rs| rs.members.iter().any(|m| m.endpoint == mongo_endpoint))
            .unwrap_or(false)
    };

    if in_rs && !state.has_pending_removal(&mongo_endpoint).await {
        state
            .queue_action(PendingAction::RemoveMember {
                mongo_endpoint: mongo_endpoint.clone(),
                fqn: fqn.clone(),
                requested_at: chrono::Utc::now(),
            })
            .await;
        tracing::info!(
            stone = %stone_name,
            endpoint = %mongo_endpoint,
            "queued rs.remove() for dismissed instance"
        );
    }

    // Remove from registry immediately — no more probing
    state.remove_instance(&stone_name).await;

    // Wake the conductor — registry changed
    state.conductor_notify.notify_one();

    tracing::info!(stone = %stone_name, "instance dismissed from registry");

    state
        .emit_event(
            "instance.dismissed",
            &json!({ "stone_name": stone_name, "endpoint": mongo_endpoint, "fqn": fqn })
                .to_string(),
        )
        .await;

    Ok(Json(json!({
        "success": true,
        "message": format!("{} removed from registry", stone_name),
        "stone_name": stone_name,
        "endpoint": mongo_endpoint,
        "in_rs": in_rs,
    })))
}

/// `GET /api/cluster/actions` — list pending membership actions.
pub async fn get_pending_actions(State(state): State<AppState>) -> Json<Value> {
    let actions = state.pending_actions_snapshot().await;
    Json(json!({ "pending_actions": actions }))
}

/// Request body for reassigning a stone's MongoDB offering to a different FQN.
#[derive(Deserialize)]
pub struct ReassignRequest {
    /// Stone name to reassign.
    pub stone_name: String,
    /// New FQN to assign (e.g. "mongodb::legacy").
    pub new_fqn: String,
}

/// `POST /api/cluster/reassign` — reassign a stone's MongoDB to a different FQN pool.
///
/// Calls the stone's Moss API to non-destructively reassign the service FQN.
/// Moss stops the container, renames it, updates the manifest, and restarts.
/// Volumes survive because they're bound by container ID.
///
/// The tools stream will emit a Remove(old) + Upsert(new) delta, and discovery
/// will re-register the instance under the new FQN.
pub async fn post_reassign(
    State(state): State<AppState>,
    Json(req): Json<ReassignRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let new_fqn = OfferingFqn::parse(&req.new_fqn).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "success": false, "message": format!("invalid FQN '{}': {e}", req.new_fqn) })),
        )
    })?;

    // Look up stone's current instance data
    let (moss_endpoint, old_fqn, mongo_endpoint) = {
        let reg = state.instances.read().await;
        reg.get(&req.stone_name)
            .map(|i| (i.moss_endpoint.clone(), i.fqn.clone(), i.mongo_endpoint.clone()))
    }
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "message": format!("instance '{}' not found", req.stone_name),
            })),
        )
    })?;

    let base_url = moss_endpoint.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "message": format!("HTTP client error: {e}") })),
            )
        })?;

    // Call Moss reassign endpoint (non-destructive: stop → rename → start)
    let old_fqn_str = old_fqn.to_string();
    let encoded_old = urlencoding::encode(&old_fqn_str);
    let reassign_url =
        format!("{base_url}/api/v1/stone/services/{encoded_old}/reassign");

    let resp = client
        .post(&reassign_url)
        .json(&json!({ "new_fqn": new_fqn.to_string() }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "success": false, "message": format!("cannot reach stone: {e}") })),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(json!({}));
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "success": false,
                "message": format!("reassign failed ({}): {}", status, body),
            })),
        ));
    }

    // Queue RS removal for the old endpoint+FQN (member is in the old RS)
    if !state.has_pending_removal_for_fqn(&mongo_endpoint, &old_fqn).await {
        state
            .queue_action(PendingAction::RemoveMember {
                mongo_endpoint: mongo_endpoint.clone(),
                fqn: old_fqn.clone(),
                requested_at: chrono::Utc::now(),
            })
            .await;
    }

    // Update the instance's FQN in the local registry immediately
    // (discovery will also update it when the tools stream delta arrives)
    {
        let mut reg = state.instances.write().await;
        if let Some(inst) = reg.get_mut(&req.stone_name) {
            inst.fqn = new_fqn.clone();
            inst.health = InstanceHealth::Unknown;
        }
    }

    tracing::info!(
        stone = %req.stone_name,
        from = %old_fqn,
        to = %new_fqn,
        "instance reassigned to new FQN pool"
    );

    state
        .emit_event(
            "instance.reassigned",
            &json!({
                "stone_name": req.stone_name,
                "from_fqn": old_fqn,
                "to_fqn": new_fqn,
            })
            .to_string(),
        )
        .await;

    // Wake the conductor — FQN changed, need to reconcile sets
    state.conductor_notify.notify_one();

    Ok(Json(json!({
        "success": true,
        "message": format!("{} reassigned from {} to {}", req.stone_name, old_fqn, new_fqn),
        "stone_name": req.stone_name,
        "from_fqn": old_fqn,
        "to_fqn": new_fqn,
    })))
}
