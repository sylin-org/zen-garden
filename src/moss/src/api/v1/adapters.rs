//! Adapter management endpoints for Moss
//! Provides adapter registry and command proxy functionality

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use garden_common::command_manifest::{AdapterCommandRequest, CommandManifest, CommandResponse};
use garden_common::api_utils::{ApiResponse, ApiErrorResponse};
use crate::app_state::AppState;
use crate::error_response;
use serde::{Deserialize, Serialize};

/// Summary of a registered adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub command_count: usize,
    pub running: bool,
    pub pid: Option<u32>,
}

/// Response for adapter listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterListResponse {
    pub adapters: Vec<AdapterSummary>,
}

/// GET /api/v1/stone/adapters
/// Returns list of available adapters with running status
pub async fn get_adapters(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AdapterListResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let adapters = state.adapter_registry.list().await;
    
    let mut summaries = Vec::new();
    for a in adapters {
        let running = state.adapter_registry.is_running(&a.id).await;
        summaries.push(AdapterSummary {
            id: a.manifest.id.clone(),
            name: a.manifest.name.clone(),
            version: a.manifest.version.clone(),
            description: a.manifest.description.clone(),
            command_count: a.manifest.commands.len(),
            running,
            pid: if running { a.pid() } else { None },
        });
    }
    
    Ok(Json(ApiResponse::new(AdapterListResponse { adapters: summaries })))
}

/// GET /api/v1/stone/adapters/:id
/// Returns adapter manifest with running status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterDetailResponse {
    #[serde(flatten)]
    pub manifest: CommandManifest,
    pub running: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
}

pub async fn get_adapter_manifest(
    State(state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterDetailResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state.adapter_registry.get(&adapter_id).await {
        Some(adapter) => {
            let running = adapter.is_running();
            Ok(Json(ApiResponse::new(AdapterDetailResponse {
                manifest: adapter.manifest.clone(),
                running,
                pid: if running { adapter.pid() } else { None },
                port: adapter.port(),
            })))
        }
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "ADAPTER_NOT_FOUND",
            format!("Adapter '{}' not found", adapter_id),
            None,
        )),
    }
}

/// POST /api/v1/stone/adapters/:id/command
/// Proxy command to adapter (5s timeout)
/// 
/// If the first arg is "all", broadcasts to all stones in topology AND runs locally.
/// The "all" keyword is stripped before forwarding to the adapter.
pub async fn send_adapter_command(
    State(state): State<AppState>,
    Path(adapter_id): Path<String>,
    Json(request): Json<AdapterCommandRequest>,
) -> Result<Json<CommandResponse>, (StatusCode, Json<CommandResponse>)> {
    // Check for "all" broadcast modifier
    let is_broadcast = request.raw_args.first().map(|s| s == "all").unwrap_or(false);
    
    // Strip "all" from args if present
    let local_args: Vec<String> = if is_broadcast {
        request.raw_args.iter().skip(1).cloned().collect()
    } else {
        request.raw_args.clone()
    };
    
    // Build local request (without "all")
    let local_request = AdapterCommandRequest::new(&adapter_id, local_args);
    
    // Execute locally first
    let local_result = execute_adapter_command_local(&state, &adapter_id, &local_request).await;
    
    // If broadcast, fan out to all other stones
    if is_broadcast {
        broadcast_to_topology(&state, &adapter_id, &local_request).await;
    }
    
    local_result
}

/// Execute adapter command on this stone only
async fn execute_adapter_command_local(
    state: &AppState,
    adapter_id: &str,
    request: &AdapterCommandRequest,
) -> Result<Json<CommandResponse>, (StatusCode, Json<CommandResponse>)> {
    // Get adapter and its assigned port
    let adapter = match state.adapter_registry.get(&adapter_id).await {
        Some(a) => a,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(CommandResponse::error(format!("Adapter '{}' not found", adapter_id))),
            ));
        }
    };
    
    // Auto-start adapter if not running
    if !adapter.is_running() {
        tracing::info!(adapter_id = %adapter_id, "Adapter not running, auto-starting before command execution");
        
        // Get moss endpoint for adapter to connect to
        let self_entry = state.self_entry.read().await;
        let moss_endpoint = self_entry.endpoint.clone();
        drop(self_entry);
        
        if let Err(e) = state.adapter_registry.start(&adapter_id, &moss_endpoint).await {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(CommandResponse::error(format!(
                    "Failed to auto-start adapter '{}': {}", 
                    adapter_id, e
                ))),
            ));
        }
        
        // Give the adapter a moment to initialize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    
    // Get the pre-assigned port
    let port = adapter.port().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse::error(format!(
                "Adapter '{}' has no assigned port", 
                adapter_id
            ))),
        )
    })?;
    
    tracing::info!(
        adapter_id = %adapter_id,
        port = port,
        args = ?request.raw_args,
        "Forwarding command to adapter"
    );
    
    // Forward command to adapter's command server
    let url = format!("http://127.0.0.1:{}/command", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommandResponse::error(format!("Failed to create HTTP client: {}", e))),
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
                        Err((StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), Json(cmd_response)))
                    }
                }
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CommandResponse::error(format!("Failed to parse adapter response: {}", e))),
                )),
            }
        }
        Err(e) => {
            if e.is_connect() {
                Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(CommandResponse::error(format!(
                        "Adapter '{}' is not responding on port {}. Is it running?", 
                        adapter_id, port
                    ))),
                ))
            } else if e.is_timeout() {
                Err((
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(CommandResponse::error(format!(
                        "Adapter '{}' command timed out (5s)", 
                        adapter_id
                    ))),
                ))
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(CommandResponse::error(format!("Failed to reach adapter: {}", e))),
                ))
            }
        }
    }
}

/// Broadcast adapter command to all other stones in topology
/// 
/// Runs in parallel with best-effort delivery. Errors are logged but not propagated.
async fn broadcast_to_topology(
    state: &AppState,
    adapter_id: &str,
    request: &AdapterCommandRequest,
) {
    use crate::domain::topology;
    
    // Get our own stone_id to exclude from broadcast
    let self_id = {
        let self_entry = state.self_entry.read().await;
        self_entry.stone_id.clone()
    };
    
    // Get all online stones except self
    let stones = topology::get_online_stones(&state.topology_cache).await;
    let other_stones: Vec<_> = stones.into_iter()
        .filter(|s| s.stone_id != self_id)
        .collect();
    
    if other_stones.is_empty() {
        tracing::debug!(adapter_id = %adapter_id, "No other stones to broadcast to");
        return;
    }
    
    tracing::info!(
        adapter_id = %adapter_id,
        stone_count = other_stones.len(),
        args = ?request.raw_args,
        "Broadcasting adapter command to all stones"
    );
    
    // Fan out requests in parallel
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    
    let futures: Vec<_> = other_stones.iter().map(|stone| {
        let client = client.clone();
        let url = format!("{}/api/v1/stone/adapters/{}/command", 
            stone.endpoint.trim_end_matches('/'), 
            adapter_id
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
    }).collect();
    
    // Execute all in parallel, don't wait for completion to avoid blocking
    tokio::spawn(async move {
        futures_util::future::join_all(futures).await;
    });
}

/// Response for adapter lifecycle operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterLifecycleResponse {
    pub adapter_id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub message: String,
}

/// POST /api/v1/stone/adapters/:id/up
/// Start an adapter process and enable auto-start
/// 
/// When user explicitly starts an adapter, it should also be marked
/// to auto-start on boot.
pub async fn start_adapter(
    State(state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterLifecycleResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Build this Moss's endpoint for the adapter to connect to
    let self_entry = state.self_entry.read().await;
    let moss_endpoint = self_entry.endpoint.clone();
    drop(self_entry);
    
    // Enable the adapter (mark for auto-start on boot)
    if let Err(e) = state.adapter_registry.enable(&adapter_id).await {
        tracing::warn!(adapter_id = %adapter_id, error = %e, "Failed to enable adapter");
    }
    
    match state.adapter_registry.start(&adapter_id, &moss_endpoint).await {
        Ok(pid) => Ok(Json(ApiResponse::new(AdapterLifecycleResponse {
            adapter_id: adapter_id.clone(),
            running: true,
            pid: Some(pid),
            message: format!("Adapter '{}' started and enabled for auto-start (PID {})", adapter_id, pid),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ADAPTER_START_FAILED",
            format!("Failed to start adapter '{}': {}", adapter_id, e),
            None,
        )),
    }
}

/// POST /api/v1/stone/adapters/:id/down
/// Stop an adapter process and disable auto-start
/// 
/// When user explicitly stops an adapter, it should stay off until
/// manually started again. This persists the disabled state.
pub async fn stop_adapter(
    State(state): State<AppState>,
    Path(adapter_id): Path<String>,
) -> Result<Json<ApiResponse<AdapterLifecycleResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state.adapter_registry.stop_and_disable(&adapter_id).await {
        Ok(()) => Ok(Json(ApiResponse::new(AdapterLifecycleResponse {
            adapter_id: adapter_id.clone(),
            running: false,
            pid: None,
            message: format!("Adapter '{}' stopped and disabled (will not auto-start)", adapter_id),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ADAPTER_STOP_FAILED",
            format!("Failed to stop adapter '{}': {}", adapter_id, e),
            None,
        )),
    }
}

/// POST /api/v1/stone/adapters/refresh
/// Re-scan adapters directory
pub async fn refresh_adapters(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AdapterListResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match state.adapter_registry.refresh_all().await {
        Ok(_) => {
            // Return updated list with running status
            let adapters = state.adapter_registry.list().await;
            let mut summaries = Vec::new();
            for a in adapters {
                let running = state.adapter_registry.is_running(&a.id).await;
                summaries.push(AdapterSummary {
                    id: a.manifest.id.clone(),
                    name: a.manifest.name.clone(),
                    version: a.manifest.version.clone(),
                    description: a.manifest.description.clone(),
                    command_count: a.manifest.commands.len(),
                    running,
                    pid: if running { a.pid() } else { None },
                });
            }
            Ok(Json(ApiResponse::new(AdapterListResponse { adapters: summaries })))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "ADAPTER_REFRESH_FAILED",
            e.to_string(),
            None,
        )),
    }
}
