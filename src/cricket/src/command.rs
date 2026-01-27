//! Command server for hey-tell commands
//! Receives commands from Moss and executes them

use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use garden_common::command_manifest::{AdapterCommandRequest, CommandResponse};
use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::manifest::TuneManager;
use crate::mixer::{Channel, Mixer};

/// Shared state for command handlers
struct AppState {
    mixer: Arc<Mixer>,
    tune_manager: Arc<TuneManager>,
}

/// Start the command server
pub async fn start_server(
    port: u16,
    mixer: Arc<Mixer>,
    tune_manager: Arc<TuneManager>,
) -> Result<JoinHandle<()>> {
    let state = Arc::new(AppState {
        mixer,
        tune_manager,
    });
    
    let app = Router::new()
        .route("/command", post(handle_command))
        .with_state(state);
    
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(port = port, "Starting command server");
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "Command server error");
        }
    });
    
    Ok(handle)
}

/// Handle incoming command from Moss
async fn handle_command(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdapterCommandRequest>,
) -> (StatusCode, Json<CommandResponse>) {
    tracing::debug!(
        adapter = %request.adapter,
        args = ?request.raw_args,
        "Received command"
    );
    
    // Parse command from raw_args
    let args = &request.raw_args;
    
    if args.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error("No command provided")
                .with_suggestions(vec![
                    "play <event>".to_string(),
                    "stop [channel]".to_string(),
                    "volume <0-100>".to_string(),
                    "tune <name>".to_string(),
                    "status".to_string(),
                ])),
        );
    }
    
    let cmd = args[0].to_lowercase();
    let cmd_args = &args[1..];
    
    match cmd.as_str() {
        "play" => handle_play(&state, cmd_args).await,
        "stop" => handle_stop(&state, cmd_args).await,
        "volume" | "vol" => handle_volume(&state, cmd_args).await,
        "tune" => handle_tune(&state, cmd_args).await,
        "status" => handle_status(&state).await,
        "test" => handle_test(&state, cmd_args).await,
        _ => (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error(format!("Unknown command: {}", cmd))
                .with_suggestions(vec![
                    "play <event>".to_string(),
                    "stop [channel]".to_string(),
                    "volume <0-100>".to_string(),
                    "tune <name>".to_string(),
                    "status".to_string(),
                ])),
        ),
    }
}

/// Play an event's audio
async fn handle_play(
    state: &AppState,
    args: &[String],
) -> (StatusCode, Json<CommandResponse>) {
    if args.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error("Usage: play <event>")
                .with_suggestions(get_event_suggestions(state))),
        );
    }
    
    let event = &args[0];
    
    // Get mapping from active tune
    let Some(mapping) = state.tune_manager.get_event_mapping(event) else {
        return (
            StatusCode::NOT_FOUND,
            Json(CommandResponse::error(format!("No mapping for event: {}", event))
                .with_suggestions(get_event_suggestions(state))),
        );
    };
    
    // Resolve channel and resource
    let Some(channel) = Channel::from_str(&mapping.channel) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse::error(format!("Invalid channel: {}", mapping.channel))),
        );
    };
    
    let active_name = state.tune_manager.active_name().unwrap_or_default();
    let Some(audio_data) = state.tune_manager.resolve_resource_bytes_with_fallback(&active_name, &mapping.resource) else {
        return (
            StatusCode::NOT_FOUND,
            Json(CommandResponse::error(format!(
                "Audio file not found: {} (no fallback defined)", 
                mapping.resource
            ))),
        );
    };
    
    // Play
    match state.mixer.play_bytes(channel, audio_data, mapping.looping).await {
        Ok(()) => (
            StatusCode::OK,
            Json(CommandResponse::success(format!(
                "Playing {} on {}", 
                event, 
                mapping.channel
            ))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CommandResponse::error(format!("Playback failed: {}", e))),
        ),
    }
}

/// Stop playback on channel(s)
async fn handle_stop(
    state: &AppState,
    args: &[String],
) -> (StatusCode, Json<CommandResponse>) {
    if args.is_empty() {
        // Stop all channels
        for channel in [Channel::Foreground, Channel::Midground, Channel::Ambient, Channel::Background] {
            state.mixer.stop(channel).await;
        }
        return (
            StatusCode::OK,
            Json(CommandResponse::success("Stopped all channels")),
        );
    }
    
    let channel_name = &args[0];
    let Some(channel) = Channel::from_str(channel_name) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error(format!("Invalid channel: {}", channel_name))
                .with_suggestions(vec![
                    "foreground".to_string(),
                    "midground".to_string(),
                    "ambient".to_string(),
                    "background".to_string(),
                ])),
        );
    };
    
    state.mixer.stop(channel).await;
    
    (
        StatusCode::OK,
        Json(CommandResponse::success(format!("Stopped {}", channel_name))),
    )
}

/// Set volume
async fn handle_volume(
    state: &AppState,
    args: &[String],
) -> (StatusCode, Json<CommandResponse>) {
    if args.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error("Usage: volume <0-100>")),
        );
    }
    
    let volume: u8 = match args[0].parse() {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CommandResponse::error("Volume must be 0-100")),
            );
        }
    };
    
    if volume > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error("Volume must be 0-100")),
        );
    }
    
    state.mixer.set_master_volume(volume as f32 / 100.0).await;
    
    (
        StatusCode::OK,
        Json(CommandResponse::success(format!("Volume set to {}%", volume))),
    )
}

/// Switch tune
async fn handle_tune(
    state: &AppState,
    args: &[String],
) -> (StatusCode, Json<CommandResponse>) {
    if args.is_empty() {
        // List available tunes
        let tunes = state.tune_manager.list_tunes();
        let active = state.tune_manager.active_name();
        
        let mut output = String::from("Available tunes:\n");
        for tune in tunes {
            let marker = if Some(&tune.name) == active.as_ref() { "→" } else { " " };
            output.push_str(&format!("{} {} (v{})\n", marker, tune.name, tune.version));
        }
        
        return (
            StatusCode::OK,
            Json(CommandResponse::success_with_output(
                "Current tune list",
                output,
            )),
        );
    }
    
    let tune_name = &args[0];
    
    match state.tune_manager.select(tune_name) {
        Ok(()) => (
            StatusCode::OK,
            Json(CommandResponse::success(format!("Switched to tune: {}", tune_name))),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(CommandResponse::error(format!("Failed to switch tune: {}", e))
                .with_suggestions(
                    state.tune_manager
                        .list_tunes()
                        .into_iter()
                        .map(|t| t.name)
                        .collect::<Vec<_>>()
                )),
        ),
    }
}

/// Show status
async fn handle_status(state: &AppState) -> (StatusCode, Json<CommandResponse>) {
    let active = state.tune_manager.active_name().unwrap_or_else(|| "(none)".to_string());
    let tune = state.tune_manager.active();
    
    let mut output = format!("Active tune: {}\n", active);
    
    if let Some(t) = tune {
        output.push_str(&format!("Version: {}\n", t.version));
        output.push_str(&format!("Events mapped: {}\n", t.events.len()));
    }
    
    (
        StatusCode::OK,
        Json(CommandResponse::success_with_output("Cricket status", output)),
    )
}

/// Test an event (trigger without SSE)
async fn handle_test(
    state: &AppState,
    args: &[String],
) -> (StatusCode, Json<CommandResponse>) {
    if args.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(CommandResponse::error("Usage: test <event>")
                .with_suggestions(get_event_suggestions(state))),
        );
    }
    
    // Reuse play logic
    handle_play(state, args).await
}

/// Get event suggestions from active tune
fn get_event_suggestions(state: &AppState) -> Vec<String> {
    state.tune_manager
        .active()
        .map(|t| t.events.keys().cloned().collect())
        .unwrap_or_default()
}
