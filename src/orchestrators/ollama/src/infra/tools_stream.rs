//! SSE client for the Moss Tools API stream.
//!
//! Subscribes to `GET /api/v1/garden/tools/stream` and filters in-memory
//! for Ollama offerings (matching `tool_fqid` or aliases starting with
//! `offering:ollama`).
//!
//! TODO: Once Moss tool_fqid filtering supports alias matching, re-add
//! `?tool_fqid=offering:ollama` server-side filter to reduce traffic.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

/// Maximum bytes of error body to include in diagnostics.
const ERROR_BODY_MAX: usize = 512;

/// Check response status, preserving the response body on error.
async fn check_status(resp: reqwest::Response, label: &str) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let body_summary = if body.len() > ERROR_BODY_MAX {
        format!("{}…", &body[..ERROR_BODY_MAX])
    } else {
        body
    };
    tracing::warn!(
        label = %label,
        status = %status,
        body = %body_summary,
        "upstream HTTP error"
    );
    anyhow::bail!("{label} HTTP {status}: {body_summary}")
}

/// Events from the Tools API stream relevant to the router.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// An Ollama instance discovered or updated.
    OllamaDiscovered {
        stone_id: String,
        stone_name: String,
        endpoint: String, // http://<ip>:<port>
    },
    /// An Ollama instance disappeared.
    OllamaRemoved {
        stone_id: String,
        stone_name: String,
    },
    /// Heartbeat from the stream (keep-alive).
    Heartbeat,
}

/// Connect to the Tools API SSE stream and yield events.
///
/// This function blocks until the stream ends or an error occurs.
/// The caller should reconnect on failure.
pub async fn subscribe_tools_stream(
    stone_endpoint: &str,
    mut on_event: impl FnMut(ToolEvent),
) -> Result<()> {
    // TODO: Re-add `?tool_fqid=offering:ollama` once Moss alias-aware filtering is deployed.
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
    let response = check_status(response, "Tools API stream").await?;

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
                    for event in parse_sse_event(&event_type, &data) {
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

/// Parse an SSE event into zero or more `ToolEvent`s.
///
/// A `tools.snapshot` can contain multiple Ollama offerings (one per stone),
/// so this returns a `Vec` rather than a single `Option`.
fn parse_sse_event(event_type: &str, data: &str) -> Vec<ToolEvent> {
    match event_type {
        "tools.snapshot" => parse_snapshot_tools(data),
        "tool.upsert" => parse_upsert(data).into_iter().collect(),
        "tool.remove" => parse_remove(data).into_iter().collect(),
        "tools.heartbeat" => vec![ToolEvent::Heartbeat],
        _ => {
            tracing::trace!(event = event_type, "ignoring SSE event type");
            vec![]
        }
    }
}

/// Parse a tool.upsert event.
fn parse_upsert(data: &str) -> Option<ToolEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let projection = json.get("projection")?;
    extract_ollama_tool(projection)
}

/// Parse a tool.remove event.
fn parse_remove(data: &str) -> Option<ToolEvent> {
    let json: serde_json::Value = serde_json::from_str(data).ok()?;
    let stone_id = json.get("stone_id")?.as_str()?.to_string();
    let stone_name = json.get("stone_name")?.as_str()?.to_string();
    Some(ToolEvent::OllamaRemoved {
        stone_id,
        stone_name,
    })
}

/// Extract an Ollama endpoint from a tool projection JSON.
fn extract_ollama_tool(tool: &serde_json::Value) -> Option<ToolEvent> {
    let fqid = tool.get("tool_fqid")?.as_str()?;
    if !fqid.starts_with("offering:ollama") {
        return None;
    }

    let stone_id = tool.get("stone_id")?.as_str()?.to_string();
    let stone_name = tool.get("stone_name")?.as_str()?.to_string();

    let connection = tool.get("connection")?;
    let ip = connection
        .get("ip")
        .or_else(|| connection.get("hostname"))
        .and_then(|v| v.as_str())?;
    let port = connection.get("port").and_then(|v| v.as_u64())?;

    let protocol = connection
        .get("protocol")
        .and_then(|v| v.as_str())
        .unwrap_or("http");

    let endpoint = format!("{protocol}://{ip}:{port}");

    Some(ToolEvent::OllamaDiscovered {
        stone_id,
        stone_name,
        endpoint,
    })
}

/// Parse a full snapshot JSON to extract all Ollama tools.
pub fn parse_snapshot_tools(data: &str) -> Vec<ToolEvent> {
    let json: serde_json::Value = match serde_json::from_str(data) {
        Ok(j) => j,
        Err(_) => return vec![],
    };

    let mut events = Vec::new();
    if let Some(tools) = json.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            if let Some(event) = extract_ollama_tool(tool) {
                events.push(event);
            }
        }
    }
    events
}
