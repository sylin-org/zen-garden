//! SSE client for the Moss Tools API stream.
//!
//! Subscribes to `GET /api/v1/garden/tools/stream` and filters events using
//! a caller-provided `fqid_filter` predicate, so each orchestrator can select
//! only its own offerings (e.g. `offering:ollama`, `offering:mongodb`).

use crate::http::check_response;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

/// Events from the Tools API stream relevant to an orchestrator.
#[derive(Debug, Clone)]
pub enum ToolStreamEvent {
    /// An offering instance discovered or updated.
    OfferingDiscovered {
        stone_id: String,
        stone_name: String,
        /// Fully constructed endpoint URL using hostname, e.g. `http://stone-quartz-fen.local:27017`.
        endpoint: String,
        /// Fully qualified ID from tools, e.g. `"offering:mongodb"` or `"offering:mongodb:analytics"`.
        tool_fqid: String,
        /// Whether the offering is automation-ready (Running + Healthy).
        /// `false` means the container is stopped, degraded, or unavailable.
        ready: bool,
    },
    /// An offering instance disappeared.
    OfferingRemoved {
        stone_id: String,
        stone_name: String,
    },
    /// Heartbeat from the stream (keep-alive).
    Heartbeat,
}

/// Connect to the Tools API SSE stream and yield filtered events.
///
/// `fqid_filter` decides which `tool_fqid` values are relevant (e.g.
/// `|fqid| fqid.starts_with("offering:mongodb")`).
///
/// This function blocks until the stream ends or an error occurs.
/// The caller should reconnect on failure.
pub async fn subscribe_tools_stream(
    stone_endpoint: &str,
    fqid_filter: impl Fn(&str) -> bool,
    mut on_event: impl FnMut(ToolStreamEvent),
) -> Result<()> {
    let url = format!("{stone_endpoint}/api/v1/garden/tools/stream");

    tracing::info!(url = %url, "connecting to Tools API stream");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        // No .timeout() — SSE streams are long-lived
        .build()?;

    let response = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .with_context(|| format!("connect to Tools API stream at {url}"))?;
    let response = check_response(response, "Tools API stream").await?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut event_type = String::new();
    let mut data_lines = Vec::<String>::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read SSE chunk")?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
            buffer.drain(..=newline_pos);

            if line.is_empty() {
                // Empty line = event boundary
                if !data_lines.is_empty() {
                    let data = data_lines.join("\n");
                    for event in parse_sse_event(&event_type, &data, &fqid_filter) {
                        on_event(event);
                    }
                    data_lines.clear();
                    event_type.clear();
                }
            } else if let Some(rest) = line.strip_prefix("event:") {
                event_type = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim().to_string());
            } else if line.starts_with("id:") || line.starts_with("retry:") {
                // Ignore id and retry fields
            }
        }
    }

    Ok(())
}

/// Parse an SSE event into zero or more `ToolStreamEvent`s.
fn parse_sse_event(
    event_type: &str,
    data: &str,
    fqid_filter: &impl Fn(&str) -> bool,
) -> Vec<ToolStreamEvent> {
    match event_type {
        "tools.snapshot" => parse_snapshot_tools(data, fqid_filter),
        "tool.upsert" => parse_upsert(data, fqid_filter).into_iter().collect(),
        "tool.remove" => parse_remove(data).into_iter().collect(),
        "tools.heartbeat" => vec![ToolStreamEvent::Heartbeat],
        _ => {
            tracing::trace!(event = event_type, "ignoring SSE event type");
            vec![]
        }
    }
}

/// Parse a tool.upsert event.
fn parse_upsert(data: &str, fqid_filter: &impl Fn(&str) -> bool) -> Option<ToolStreamEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let projection = json.get("projection")?;
    extract_offering_tool(projection, fqid_filter)
}

/// Parse a tool.remove event.
fn parse_remove(data: &str) -> Option<ToolStreamEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let stone_id = json.get("stone_id")?.as_str()?.to_string();
    let stone_name = json.get("stone_name")?.as_str()?.to_string();
    Some(ToolStreamEvent::OfferingRemoved {
        stone_id,
        stone_name,
    })
}

/// Extract an offering endpoint from a tool projection JSON, applying the
/// caller's fqid filter.
fn extract_offering_tool(
    tool: &serde_json::Value,
    fqid_filter: &impl Fn(&str) -> bool,
) -> Option<ToolStreamEvent> {
    let fqid = tool.get("tool_fqid")?.as_str()?;
    if !fqid_filter(fqid) {
        return None;
    }

    let stone_id = tool.get("stone_id")?.as_str()?.to_string();
    let stone_name = tool.get("stone_name")?.as_str()?.to_string();

    let connection = tool.get("connection")?;
    let ip = connection.get("ip").and_then(|v| v.as_str());
    let hostname = connection.get("hostname").and_then(|v| v.as_str());
    // Prefer IP over hostname — .local mDNS resolution is unreliable
    // inside Docker containers on Windows.
    let host = ip.or(hostname)?;
    let port = connection.get("port").and_then(|v| v.as_u64())?;

    let protocol = connection
        .get("protocol")
        .and_then(|v| v.as_str())
        .unwrap_or("http");

    let endpoint = format!("{protocol}://{host}:{port}");

    let ready = tool
        .get("ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Some(ToolStreamEvent::OfferingDiscovered {
        stone_id,
        stone_name,
        endpoint,
        tool_fqid: fqid.to_string(),
        ready,
    })
}

/// Parse a full snapshot JSON to extract all matching tools.
pub fn parse_snapshot_tools(
    data: &str,
    fqid_filter: &impl Fn(&str) -> bool,
) -> Vec<ToolStreamEvent> {
    let json: serde_json::Value = match serde_json::from_str(data) {
        Ok(j) => j,
        Err(_) => return vec![],
    };

    let mut events = Vec::new();
    if let Some(tools) = json.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            if let Some(event) = extract_offering_tool(tool, fqid_filter) {
                events.push(event);
            }
        }
    }
    events
}
