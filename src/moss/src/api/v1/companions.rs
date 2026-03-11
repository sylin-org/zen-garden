//! Companion management endpoints for Moss
//! Provides Companion registry and command proxy functionality

use crate::app_state::AppState;
use crate::error_response;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::command_manifest::{CommandManifest, CommandResponse, CompanionCommandRequest};
use serde::{Deserialize, Serialize};

/// Summary of a registered Companion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub command_count: usize,
    pub running: bool,
    pub pid: Option<u32>,
}

/// Response for Companion listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionListResponse {
    pub companions: Vec<CompanionSummary>,
}

/// GET /api/v1/stone/Companions
/// Returns list of available Companions with running status
pub async fn get_companions(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CompanionListResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let companions = state.companion.registry.list().await;

    let mut summaries = Vec::new();
    for a in companions {
        let running = state.companion.registry.is_running(&a.id).await;
        summaries.push(CompanionSummary {
            id: a.manifest.id.clone(),
            name: a.manifest.name.clone(),
            version: a.manifest.version.clone(),
            description: a.manifest.description.clone(),
            command_count: a.manifest.commands.len(),
            running,
            pid: if running { a.pid() } else { None },
        });
    }

    Ok(Json(ApiResponse::new(CompanionListResponse {
        companions: summaries,
    })))
}

/// GET /api/v1/stone/companions/:id
/// Returns Companion manifest with running status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionDetailResponse {
    #[serde(flatten)]
    pub manifest: CommandManifest,
    pub running: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
}

pub async fn get_companion_manifest(
    State(state): State<AppState>,
    Path(companion_id): Path<String>,
) -> Result<Json<ApiResponse<CompanionDetailResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state.companion.registry.get(&companion_id).await {
        Some(c) => {
            let running = c.is_running();
            Ok(Json(ApiResponse::new(CompanionDetailResponse {
                manifest: c.manifest.clone(),
                running,
                pid: if running { c.pid() } else { None },
                port: c.port(),
            })))
        }
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "COMPANION_NOT_FOUND",
            format!("Companion '{}' not found", companion_id),
            None,
        )),
    }
}

/// POST /api/v1/stone/companions/:id/command
/// Proxy command to Companion (5s timeout)
///
/// If the first arg is "all", broadcasts to all stones in topology AND runs locally.
/// The "all" keyword is stripped before forwarding to the Companion.
pub async fn send_companion_command(
    State(state): State<AppState>,
    Path(companion_id): Path<String>,
    Json(request): Json<CompanionCommandRequest>,
) -> Result<Json<CommandResponse>, (StatusCode, Json<CommandResponse>)> {
    // Check for "all" broadcast modifier
    let is_broadcast = request
        .raw_args
        .first()
        .map(|s| s == "all")
        .unwrap_or(false);

    // Strip "all" from args if present
    let local_args: Vec<String> = if is_broadcast {
        request.raw_args.iter().skip(1).cloned().collect()
    } else {
        request.raw_args.clone()
    };

    // Build local request (without "all")
    let local_request = CompanionCommandRequest::new(&companion_id, local_args);

    // Execute locally first
    let local_result = execute_companion_command_local(&state, &companion_id, &local_request).await;

    // If broadcast, fan out to all other stones
    if is_broadcast {
        broadcast_to_topology(&state, &companion_id, &local_request).await;
    }

    local_result
}

/// Execute Companion command on this stone only
async fn execute_companion_command_local(
    state: &AppState,
    companion_id: &str,
    request: &CompanionCommandRequest,
) -> Result<Json<CommandResponse>, (StatusCode, Json<CommandResponse>)> {
    // Get Companion and its assigned port
    let companion = match state.companion.registry.get(companion_id).await {
        Some(a) => a,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(CommandResponse::error(format!(
                    "Companion '{}' not found",
                    companion_id
                ))),
            ));
        }
    };

    // Auto-start Companion if not running
    if !companion.is_running() {
        tracing::info!(companion_id = %companion_id, "Companion not running, auto-starting before command execution");

        // Get moss endpoint for Companion to connect to
        let self_entry = state.self_entry.read().await;
        let moss_endpoint = self_entry.address.http_base();
        drop(self_entry);

        if let Err(e) = state
            .companion.registry
            .start(companion_id, &moss_endpoint)
            .await
        {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(CommandResponse::error(format!(
                    "Failed to auto-start Companion '{}': {}",
                    companion_id, e
                ))),
            ));
        }

        // Give the Companion a moment to initialize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    // Get the pre-assigned port
    let port = companion.port().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse::error(format!(
                "Companion '{}' has no assigned port",
                companion_id
            ))),
        )
    })?;

    tracing::info!(
        companion_id = %companion_id,
        port = port,
        args = ?request.raw_args,
        "Forwarding command to Companion"
    );

    // Forward command to Companion's command server
    let url = format!("http://127.0.0.1:{}/command", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommandResponse::error(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))),
            )
        })?;

    match client.post(&url).json(&request).send().await {
        Ok(resp) => {
            let status = resp.status();
            match resp.json::<CommandResponse>().await {
                Ok(cmd_response) => {
                    if status.is_success() {
                        Ok(Json(cmd_response))
                    } else {
                        Err((
                            StatusCode::from_u16(status.as_u16())
                                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                            Json(cmd_response),
                        ))
                    }
                }
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CommandResponse::error(format!(
                        "Failed to parse Companion response: {}",
                        e
                    ))),
                )),
            }
        }
        Err(e) => {
            if e.is_connect() {
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(CommandResponse::error(format!(
                        "Companion '{}' is not responding on port {}. Is it running?",
                        companion_id, port
                    ))),
                ))
            } else if e.is_timeout() {
                Err((
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(CommandResponse::error(format!(
                        "Companion '{}' command timed out (5s)",
                        companion_id
                    ))),
                ))
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CommandResponse::error(format!(
                        "Failed to reach Companion: {}",
                        e
                    ))),
                ))
            }
        }
    }
}

/// Broadcast Companion command to all other stones in topology
///
/// Runs in parallel with best-effort delivery. Errors are logged but not propagated.
async fn broadcast_to_topology(
    state: &AppState,
    companion_id: &str,
    request: &CompanionCommandRequest,
) {
    use crate::domain::topology;

    // Get our own stone_id to exclude from broadcast
    let self_id = {
        let self_entry = state.self_entry.read().await;
        self_entry.stone_id.clone()
    };

    // Get all online stones except self
    let stones = topology::get_online_stones(&state.topology_cache).await;
    let other_stones: Vec<_> = stones
        .into_iter()
        .filter(|s| s.stone_id != self_id)
        .collect();

    if other_stones.is_empty() {
        tracing::debug!(companion_id = %companion_id, "No other stones to broadcast to");
        return;
    }

    tracing::info!(
        companion_id = %companion_id,
        stone_count = other_stones.len(),
        args = ?request.raw_args,
        "Broadcasting Companion command to all stones"
    );

    // Fan out requests in parallel
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let futures: Vec<_> = other_stones
        .iter()
        .map(|stone| {
            let client = client.clone();
            let url = format!(
                "{}/api/v1/stone/companions/{}/command",
                stone.address.http_base().trim_end_matches('/'),
                companion_id
            );
            let request = request.clone();
            let stone_name = stone.stone_name.clone();

            async move {
                match client.post(&url).json(&request).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::debug!(stone = %stone_name, "Broadcast succeeded");
                    }
                    Ok(resp) => {
                        tracing::warn!(
                            stone = %stone_name,
                            status = %resp.status(),
                            "Broadcast failed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            stone = %stone_name,
                            error = %e,
                            "Broadcast error"
                        );
                    }
                }
            }
        })
        .collect();

    // Execute all in parallel, don't wait for completion to avoid blocking
    tokio::spawn(async move {
        futures_util::future::join_all(futures).await;
    });
}

/// Response for Companion lifecycle operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionLifecycleResponse {
    pub companion_id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub message: String,
}

/// POST /api/v1/stone/companions/:id/up
/// Start an Companion process and enable auto-start
///
/// When user explicitly starts an Companion, it should also be marked
/// to auto-start on boot.
pub async fn start_companion(
    State(state): State<AppState>,
    Path(companion_id): Path<String>,
) -> Result<Json<ApiResponse<CompanionLifecycleResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Build this Moss's endpoint for the Companion to connect to
    let self_entry = state.self_entry.read().await;
    let moss_endpoint = self_entry.address.http_base();
    drop(self_entry);

    // Enable the Companion (mark for auto-start on boot)
    if let Err(e) = state.companion.registry.enable(&companion_id).await {
        tracing::warn!(companion_id = %companion_id, error = %e, "Failed to enable Companion");
    }

    match state
        .companion.registry
        .start(&companion_id, &moss_endpoint)
        .await
    {
        Ok(pid) => Ok(Json(ApiResponse::new(CompanionLifecycleResponse {
            companion_id: companion_id.clone(),
            running: true,
            pid: Some(pid),
            message: format!(
                "Companion '{}' started and enabled for auto-start (PID {})",
                companion_id, pid
            ),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPANION_START_FAILED",
            format!("Failed to start Companion '{}': {}", companion_id, e),
            None,
        )),
    }
}

/// POST /api/v1/stone/companions/:id/down
/// Stop an Companion process and disable auto-start
///
/// When user explicitly stops an Companion, it should stay off until
/// manually started again. This persists the disabled state.
pub async fn stop_companion(
    State(state): State<AppState>,
    Path(companion_id): Path<String>,
) -> Result<Json<ApiResponse<CompanionLifecycleResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state
        .companion.registry
        .stop_and_disable(&companion_id)
        .await
    {
        Ok(()) => Ok(Json(ApiResponse::new(CompanionLifecycleResponse {
            companion_id: companion_id.clone(),
            running: false,
            pid: None,
            message: format!(
                "Companion '{}' stopped and disabled (will not auto-start)",
                companion_id
            ),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPANION_STOP_FAILED",
            format!("Failed to stop Companion '{}': {}", companion_id, e),
            None,
        )),
    }
}

/// POST /api/v1/stone/companions/refresh
/// Re-scan Companions directory
pub async fn refresh_companions(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CompanionListResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state.companion.registry.refresh_all().await {
        Ok(_) => {
            // Return updated list with running status
            let companions = state.companion.registry.list().await;
            let mut summaries = Vec::new();
            for a in companions {
                let running = state.companion.registry.is_running(&a.id).await;
                summaries.push(CompanionSummary {
                    id: a.manifest.id.clone(),
                    name: a.manifest.name.clone(),
                    version: a.manifest.version.clone(),
                    description: a.manifest.description.clone(),
                    command_count: a.manifest.commands.len(),
                    running,
                    pid: if running { a.pid() } else { None },
                });
            }
            Ok(Json(ApiResponse::new(CompanionListResponse {
                companions: summaries,
            })))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMPANION_REFRESH_FAILED",
            e.to_string(),
            None,
        )),
    }
}
