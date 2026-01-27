//! HTTP server for adapter command handling
//!
//! Provides standard endpoints:
//! - `POST /command` - Execute adapter commands
//! - `POST /shutdown` - Graceful shutdown
//! - `GET /health` - Health check

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use garden_common::command_manifest::CommandResponse;
use std::sync::Arc;
use tokio::sync::watch;

use crate::handler::CommandHandler;

/// Server state shared across handlers
pub(crate) struct ServerState<H: CommandHandler> {
    pub handler: Arc<H>,
    pub shutdown_tx: watch::Sender<bool>,
    pub adapter_name: String,
}

/// Command request from Moss
/// Note: Field is `raw_args` to match AdapterCommandRequest from garden_common
#[derive(Debug, serde::Deserialize)]
pub struct CommandRequest {
    /// Raw command arguments (matches AdapterCommandRequest.raw_args)
    #[serde(default)]
    pub raw_args: Vec<String>,
}

/// Start the adapter HTTP server
///
/// Returns the server task handle and a shutdown receiver.
/// The receiver signals `true` when `/shutdown` is called.
pub async fn start_server<H: CommandHandler>(
    port: u16,
    handler: Arc<H>,
    adapter_name: impl Into<String>,
) -> anyhow::Result<(tokio::task::JoinHandle<()>, watch::Receiver<bool>)> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let state = Arc::new(ServerState {
        handler,
        shutdown_tx,
        adapter_name: adapter_name.into(),
    });

    let app = Router::new()
        .route("/command", post(handle_command::<H>))
        .route("/shutdown", post(handle_shutdown::<H>))
        .route("/health", get(handle_health::<H>))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(port = port, "Starting adapter command server");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "Adapter server error");
        }
    });

    Ok((handle, shutdown_rx))
}

/// POST /command - Execute a command
async fn handle_command<H: CommandHandler>(
    State(state): State<Arc<ServerState<H>>>,
    Json(request): Json<CommandRequest>,
) -> Json<CommandResponse> {
    tracing::debug!(args = ?request.raw_args, "Received command");

    let response = state.handler.handle(&request.raw_args).await;

    Json(response)
}

/// POST /shutdown - Graceful shutdown
async fn handle_shutdown<H: CommandHandler>(
    State(state): State<Arc<ServerState<H>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    tracing::info!(adapter = %state.adapter_name, "Shutdown requested by Moss");

    // Notify handler of shutdown
    state.handler.on_shutdown().await;

    // Signal shutdown to runtime
    let _ = state.shutdown_tx.send(true);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "shutting_down",
            "adapter": state.adapter_name,
            "message": "Adapter is shutting down gracefully"
        })),
    )
}

/// GET /health - Health check
async fn handle_health<H: CommandHandler>(
    State(state): State<Arc<ServerState<H>>>,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy",
            "adapter": state.adapter_name
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct TestHandler;

    #[async_trait]
    impl CommandHandler for TestHandler {
        async fn handle(&self, args: &[String]) -> CommandResult {
            match args.first().map(|s| s.as_str()) {
                Some("hello") => CommandResult::success("Hello!"),
                Some(cmd) => CommandResult::error(format!("Unknown: {}", cmd)),
                None => CommandResult::error("No command"),
            }
        }
    }

    #[tokio::test]
    async fn test_server_starts() {
        let handler = Arc::new(TestHandler);
        let result = start_server(0, handler, "test").await; // Port 0 = random available
        assert!(result.is_ok());
        let (handle, _rx) = result.unwrap();
        handle.abort();
    }
}
