use crate::domain::tools::{stream_event_type_for_delta, ToolQuery, ToolsSnapshotPayload};
use crate::{error_response, AppState};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::{self, Stream, StreamExt};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::tools::event_types;
use garden_common::tools::{CapabilitySelector, GardenTool, ToolDelta};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ToolsQueryParams {
    /// Filter by fqid: bare name (type match) or instance name (exact match).
    #[serde(default)]
    pub fqid: Option<String>,
    /// Filter by category: "orchestrator", "offering", "storage".
    #[serde(default)]
    pub category: Option<String>,
    /// Filter by status: "running", "degraded", "stopped".
    #[serde(default)]
    pub status: Option<String>,
    /// Capability filter: "model:llama3" or "model:llama3,model:nomic".
    #[serde(default)]
    pub capability: Option<String>,
    /// Resume cursor for delta replay / SSE resume.
    #[serde(default)]
    pub since: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ToolsSnapshotResponse {
    pub cursor: u64,
    pub tools: Vec<GardenTool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replay: Vec<ToolDelta>,
}

pub async fn list_garden_tools_v1(
    State(state): State<AppState>,
    Query(query): Query<ToolsQueryParams>,
) -> Result<Json<ApiResponse<ToolsSnapshotResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let filter = parse_query(&query)?;
    let since = query.since.unwrap_or(0);

    let (cursor, tools, replay) = {
        let reg = state.registry.read().await;
        let (cursor, tools) = reg.snapshot(&filter);
        let replay = if since > 0 {
            reg.deltas_since(since, &filter)
        } else {
            Vec::new()
        };
        (cursor, tools, replay)
    };

    Ok(Json(ApiResponse::new(ToolsSnapshotResponse {
        cursor,
        tools,
        replay,
    })))
}

pub async fn stream_garden_tools_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ToolsQueryParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ApiErrorResponse>)>
{
    let filter = parse_query(&query)?;
    let mut resume_cursor = query.since.unwrap_or(0);

    // MOSS-0004: child token for cooperative shutdown
    let token = state.shutdown_token.child_token();
    let rx = state.tools_tx.subscribe();

    let (snapshot_cursor, snapshot_tools, replay) = {
        let reg = state.registry.read().await;
        if resume_cursor == 0 {
            if let Some(last_event_id) = extract_last_event_id(&headers) {
                resume_cursor = parse_resume_cursor(last_event_id, &reg);
            }
        }

        let (cursor, tools) = reg.snapshot(&filter);
        let replay = if resume_cursor > 0 {
            reg.deltas_since(resume_cursor, &filter)
        } else {
            Vec::new()
        };
        (cursor, tools, replay)
    };

    let snapshot_payload = ToolsSnapshotPayload {
        cursor: snapshot_cursor,
        tools: snapshot_tools,
    };
    let snapshot_json = serde_json::to_string(&snapshot_payload).unwrap_or_else(|_| {
        serde_json::json!({ "cursor": snapshot_cursor, "tools": [] }).to_string()
    });

    let snapshot_event = Event::default()
        .id(snapshot_cursor.to_string())
        .event(event_types::TOOLS_SNAPSHOT)
        .data(snapshot_json);
    let snapshot_stream = stream::once(async move { Ok(snapshot_event) });

    let replay_filter = filter.clone();
    let replay_stream = stream::iter(replay.into_iter().filter_map(move |delta| {
        let replay_filter = replay_filter.clone();
        delta_to_event(&delta, &replay_filter).map(Ok::<Event, Infallible>)
    }));

    let live_filter = filter.clone();
    let live_stream = BroadcastStream::new(rx).filter_map(move |result| {
        let live_filter = live_filter.clone();
        async move {
            match result {
                Ok(delta) => {
                    if delta.cursor <= snapshot_cursor {
                        return None;
                    }
                    delta_to_event(&delta, &live_filter).map(Ok::<Event, Infallible>)
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "Tools stream receiver lagged");
                    None
                }
            }
        }
    });

    let heartbeat_stream =
        IntervalStream::new(tokio::time::interval(Duration::from_secs(15))).map(move |_| {
            let data = serde_json::json!({
                "cursor": snapshot_cursor,
                "timestamp": chrono::Utc::now(),
            });
            Ok::<Event, Infallible>(
                Event::default()
                    .event(event_types::TOOLS_HEARTBEAT)
                    .data(data.to_string()),
            )
        });

    // MOSS-0004: Wrap in cancellation-aware stream — ends on shutdown
    let inner = stream::select(
        snapshot_stream.chain(replay_stream).chain(live_stream),
        heartbeat_stream,
    );
    let stream = async_stream::stream! {
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield event,
                        None => break,
                    }
                }
                _ = token.cancelled() => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn parse_query(
    query: &ToolsQueryParams,
) -> Result<ToolQuery, (StatusCode, Json<ApiErrorResponse>)> {
    let capabilities = query
        .capability
        .as_deref()
        .map(parse_capability_selectors)
        .transpose()?;

    Ok(ToolQuery {
        fqid: query
            .fqid
            .as_deref()
            .map(|fqid| fqid.trim().to_ascii_lowercase())
            .filter(|fqid| !fqid.is_empty()),
        category: query
            .category
            .as_deref()
            .map(|c| c.trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty()),
        status: query
            .status
            .as_deref()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty()),
        capabilities: capabilities.unwrap_or_default(),
    })
}

fn parse_capability_selectors(
    raw: &str,
) -> Result<Vec<CapabilitySelector>, (StatusCode, Json<ApiErrorResponse>)> {
    let mut parsed = Vec::new();
    for token in raw.split([',', '|']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cap_type, item)) = token.split_once(':') else {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_CAPABILITY_FILTER",
                "capability must be '<type>:<item>' (comma-separated for multiple)".to_string(),
                None,
            ));
        };
        let cap_type = cap_type.trim().to_ascii_lowercase();
        let item = item.trim().to_string();
        if cap_type.is_empty() || item.is_empty() {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_CAPABILITY_FILTER",
                "capability must be '<type>:<item>' (comma-separated for multiple)".to_string(),
                None,
            ));
        }
        parsed.push(CapabilitySelector { cap_type, item });
    }

    if parsed.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_CAPABILITY_FILTER",
            "capability must include at least one '<type>:<item>' selector".to_string(),
            None,
        ));
    }

    Ok(parsed)
}

fn extract_last_event_id(headers: &HeaderMap) -> Option<&str> {
    headers.get("last-event-id").and_then(|h| h.to_str().ok())
}

fn parse_resume_cursor(last_event_id: &str, reg: &crate::domain::garden_registry::GardenRegistryInner) -> u64 {
    if let Ok(parsed) = last_event_id.trim().parse::<u64>() {
        return parsed;
    }
    reg.cursor_for_event_id(last_event_id).unwrap_or(0)
}

fn delta_to_event(delta: &ToolDelta, filter: &ToolQuery) -> Option<Event> {
    if !filter.matches_delta(delta) {
        return None;
    }

    let event_type = stream_event_type_for_delta(delta);
    let payload = serde_json::to_string(delta).ok()?;

    Some(
        Event::default()
            .id(delta.event_id.clone())
            .event(event_type)
            .data(payload),
    )
}
