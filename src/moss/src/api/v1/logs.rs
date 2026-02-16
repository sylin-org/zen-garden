//! Daemon log API endpoints
//!
//! - GET  /api/v1/stone/logs?lines=100&level=warn  — Recent log lines from file
//! - GET  /api/v1/stone/logs/stream                 — Live log stream (SSE)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::infra::error_response;
use crate::AppState;

/// Query parameters for GET /api/v1/stone/logs
#[derive(Debug, serde::Deserialize)]
pub struct LogsQuery {
    /// Number of recent lines to return (default: 100, max: 5000)
    pub lines: Option<usize>,
    /// Optional level filter (e.g., "warn", "error", "info")
    pub level: Option<String>,
}

/// GET /api/v1/stone/logs
///
/// Returns the last N lines from the current log file.
/// Reads from `{data_dir}/logs/garden-moss.log.{today}`.
pub async fn get_recent_logs(
    State(_state): State<AppState>,
    Query(params): Query<LogsQuery>,
) -> Result<Json<ApiResponse<Vec<String>>>, (StatusCode, Json<ApiErrorResponse>)> {
    let max_lines = params.lines.unwrap_or(100).min(5000);
    let level_filter = params.level.as_deref().map(|l| l.to_uppercase());

    let logs_dir = garden_common::constants::paths::logs_dir();
    let logs_path = std::path::Path::new(&logs_dir);

    if !logs_path.exists() {
        return Ok(Json(ApiResponse::new(Vec::new())));
    }

    // Find the most recent log file (today's or latest available)
    let log_file = find_current_log_file(logs_path).await;

    let log_file = match log_file {
        Some(f) => f,
        None => return Ok(Json(ApiResponse::new(Vec::new()))),
    };

    // Read the file and take last N lines
    let content = match tokio::fs::read_to_string(&log_file).await {
        Ok(c) => c,
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "log_read_failed",
                format!("Failed to read log file: {}", e),
                None,
            ));
        }
    };

    let mut lines: Vec<String> = content
        .lines()
        .filter(|line| {
            if let Some(ref level) = level_filter {
                line.contains(level.as_str())
            } else {
                true
            }
        })
        .map(|s| s.to_string())
        .collect();

    // Take last N lines
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }

    Ok(Json(ApiResponse::new(lines)))
}

/// GET /api/v1/stone/logs/stream
///
/// Live log stream via Server-Sent Events.
/// Subscribes to the log broadcast channel and streams new events.
pub async fn stream_logs(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    // MOSS-0004: child token for cooperative shutdown
    let token = state.shutdown_token.child_token();
    let rx = state.log_tx.subscribe();
    let inner = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(line) => Some(Event::default().data(line)),
        Err(_) => None,
    });

    // MOSS-0004: Cancellation-aware wrapper — ends stream on shutdown
    let stream = async_stream::stream! {
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield Ok::<Event, Infallible>(event),
                        None => break,
                    }
                }
                _ = token.cancelled() => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Find the most recent log file in the logs directory
///
/// tracing-appender creates files like `garden-moss.log.2026-02-09`.
/// Returns the most recently modified file matching the pattern.
async fn find_current_log_file(logs_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = tokio::fs::read_dir(logs_dir).await.ok()?;
    let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if !name.starts_with("garden-moss.log") {
            continue;
        }

        let modified = match tokio::fs::metadata(&path).await {
            Ok(m) => m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            Err(_) => continue,
        };

        match &best {
            Some((_, prev_time)) if modified > *prev_time => {
                best = Some((path, modified));
            }
            None => {
                best = Some((path, modified));
            }
            _ => {}
        }
    }

    best.map(|(path, _)| path)
}
