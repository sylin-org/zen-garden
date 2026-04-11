use crate::domain::Tool;
use crate::domain::tool::{ToolQuery, ToolsSnapshotPayload, stream_event_type_for_delta};
use crate::{AppState, bad_request};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{self, Stream, StreamExt};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::tools::event_types;
use garden_common::tools::{CapabilitySelector, GardenTool, ToolDelta};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
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

/// `GET /api/v1/stone/tools/{fqid}` — single-tool lookup by fqid
/// across any origin (Local, Gateway, Announced). Returns 404 if no
/// registry entry matches the given fqid on any stone.
///
/// Added in Book II Ch6 (ARCH-0019) as the first singular tool
/// endpoint. Uses the `snapshot` query with an fqid filter and takes
/// the first match — `ToolQuery::matches_tool` honours the exact-fqid
/// matching semantics.
pub async fn get_tool_v1(
    State(tool): State<Arc<Tool>>,
    axum::extract::Path(fqid): axum::extract::Path<String>,
) -> Result<Json<GardenTool>, (StatusCode, Json<ApiErrorResponse>)> {
    let query = ToolQuery {
        fqid: Some(fqid.clone()),
        ..Default::default()
    };
    let (_, mut tools) = tool.snapshot(&query).await;
    match tools.pop() {
        Some(t) => Ok(Json(t)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiErrorResponse::new(
                "TOOL_NOT_FOUND",
                format!("No tool registered with fqid '{}'", fqid),
            )),
        )),
    }
}

pub async fn list_garden_tools_v1(
    State(tool): State<Arc<Tool>>,
    Query(query): Query<ToolsQueryParams>,
) -> crate::api::ApiResult<ToolsSnapshotResponse> {
    let filter = parse_query(&query)?;
    let since = query.since.unwrap_or(0);

    let (cursor, tools) = tool.snapshot(&filter).await;
    let replay = if since > 0 {
        tool.deltas_since(since, &filter).await
    } else {
        Vec::new()
    };

    crate::api::ok(ToolsSnapshotResponse {
        cursor,
        tools,
        replay,
    })
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
    let rx = state.tool.delta_stream();

    if resume_cursor == 0
        && let Some(last_event_id) = extract_last_event_id(&headers)
    {
        resume_cursor = if let Ok(parsed) = last_event_id.trim().parse::<u64>() {
            parsed
        } else {
            state
                .tool
                .cursor_for_event_id(last_event_id)
                .await
                .unwrap_or(0)
        };
    }

    let (snapshot_cursor, snapshot_tools) = state.tool.snapshot(&filter).await;
    let replay = if resume_cursor > 0 {
        state.tool.deltas_since(resume_cursor, &filter).await
    } else {
        Vec::new()
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
        stone_id: None,
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
            return Err(bad_request(
                "INVALID_CAPABILITY_FILTER",
                "capability must be '<type>:<item>' (comma-separated for multiple)",
            ));
        };
        let cap_type = cap_type.trim().to_ascii_lowercase();
        let item = item.trim().to_string();
        if cap_type.is_empty() || item.is_empty() {
            return Err(bad_request(
                "INVALID_CAPABILITY_FILTER",
                "capability must be '<type>:<item>' (comma-separated for multiple)",
            ));
        }
        parsed.push(CapabilitySelector { cap_type, item });
    }

    if parsed.is_empty() {
        return Err(bad_request(
            "INVALID_CAPABILITY_FILTER",
            "capability must include at least one '<type>:<item>' selector",
        ));
    }

    Ok(parsed)
}

fn extract_last_event_id(headers: &HeaderMap) -> Option<&str> {
    headers.get("last-event-id").and_then(|h| h.to_str().ok())
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
