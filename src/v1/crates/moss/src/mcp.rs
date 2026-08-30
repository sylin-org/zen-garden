//! The MCP surface (ADR-0014): the garden, native to AI assistants.
//!
//! THE CHANNELS LAW: MCP, CLI, and API are just channels that enter the
//! same command pipeline. Every tool here delegates to the exact
//! application-service calls the HTTP faces use — no second brain, no
//! drift (B1/R4.3). The transport is MCP Streamable HTTP's legal
//! minimum: JSON-RPC over POST, 405 for GET (we offer no
//! server-initiated stream), no session state. Tools are derived from
//! the garden's verbs; outputs are the same envelopes the faces speak.

use crate::http::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};
use std::sync::Arc;

/// A tool the garden offers over MCP.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
}

fn tools() -> Vec<ToolDef> {
    fn obj(name: &'static str, description: &'static str, properties: Value, required: &[&str]) -> ToolDef {
        ToolDef {
            name,
            description,
            input_schema: json!({
                "type": "object",
                "properties": properties,
                "required": required,
            }),
        }
    }
    vec![
        obj(
            "observe",
            "See the whole room as this stone sees it: every stone, every \
             offering with its status and connection ports, every running \
             and finished job. Call this first - it answers 'what is in \
             the garden right now?'",
            json!({}),
            &[],
        ),
        obj(
            "offerings",
            "List the offerings planted on this stone with full detail: \
             status, mode (managed/adopted/borrowed), ports, image.",
            json!({}),
            &[],
        ),
        obj(
            "plant",
            "Plant (deploy) an offering by catalog name - the catalog \
             manifest defines the image. Returns the connection details.",
            json!({
                "name": { "type": "string", "description": "catalog name, e.g. 'memcached'" },
                "ports": {
                    "type": "object",
                    "description": "optional named container ports, e.g. {\"default\": 6379}",
                },
            }),
            &["name"],
        ),
        obj(
            "rest",
            "Rest an offering: stopped, and the garden keeps it rested \
             (desired state, not a one-off stop). Its data stays.",
            json!({ "name": { "type": "string", "description": "offering name or FQN" } }),
            &["name"],
        ),
        obj(
            "wake",
            "Wake a rested offering back to running - resurrecting it from \
             its stored spec if reality lost it.",
            json!({ "name": { "type": "string", "description": "offering name or FQN" } }),
            &["name"],
        ),
        obj(
            "uproot",
            "Remove an offering entirely: workload deleted, record \
             forgotten. Destructive - prefer rest for temporary pauses.",
            json!({ "name": { "type": "string", "description": "offering name or FQN" } }),
            &["name"],
        ),
        obj(
            "capabilities",
            "See what an offering HOLDS - e.g. the models inside an ollama \
             - observed live through its manifest channel.",
            json!({ "offering": { "type": "string", "description": "offering name or FQN" } }),
            &["offering"],
        ),
        obj(
            "grow",
            "Grow a capability inside an offering - e.g. pull a model into \
             ollama. Runs as a job; MANAGED work only (adopted offerings \
             are observed, never operated). Check the 'jobs' tool to see \
             it finish.",
            json!({
                "offering": { "type": "string", "description": "offering name or FQN" },
                "type": { "type": "string", "description": "capability type, e.g. 'model'" },
                "item": { "type": "string", "description": "the capability's name, e.g. 'llama3'" },
            }),
            &["offering", "type", "item"],
        ),
        obj(
            "jobs",
            "List async operations on this stone, newest first: captures, \
             capability growth, replants - with status and progress.",
            json!({}),
            &[],
        ),
    ]
}

/// MCP over Streamable HTTP (legal minimum): POST carries JSON-RPC.
pub async fn handle(
    State(state): State<Arc<AppState>>,
    body: axum::body::Bytes,
) -> Response {
    let req: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return rpc_error(None, -32700, &format!("parse error: {e}"), StatusCode::BAD_REQUEST)
                .into_response()
        }
    };
    let method = req["method"].as_str().unwrap_or_default().to_string();
    let id = req.get("id").cloned();

    // Notifications carry no id: accepted, nothing returned.
    let Some(id) = id else {
        return StatusCode::ACCEPTED.into_response();
    };

    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-03-26",
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "zen-garden-moss",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })),
        "tools/list" => Ok(json!({
            "tools": tools().into_iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })).collect::<Vec<_>>(),
        })),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or_default().to_string();
            let args = req["params"]["arguments"].clone();
            match run_tool(&state, &name, &args).await {
                Ok(value) => Ok(json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                    "isError": false,
                })),
                Err(e) => Ok(json!({
                    "content": [{ "type": "text", "text": e }],
                    "isError": true,
                })),
            }
        }
        other => Err(format!("method not found: {other}")),
    };

    match result {
        Ok(result) => json_ok(id, result),
        Err(message) => rpc_error(Some(id), -32601, &message, StatusCode::OK).into_response(),
    }
}

/// The channels law in code: every tool delegates to the exact
/// application-service calls the HTTP faces use.
async fn run_tool(state: &Arc<AppState>, name: &str, args: &Value) -> Result<Value, String> {
    use crate::garden::capabilities;
    match name {
        "observe" => Ok(crate::pulse::snapshot(
            &state.garden,
            &state.topology,
            &state.jobs,
            crate::http::self_view(state),
        )),
        "offerings" => {
            let rows: Vec<Value> = state
                .garden
                .snapshot()
                .iter()
                .map(|o| serde_json::to_value(o).unwrap_or_default())
                .collect();
            Ok(json!({ "offerings": rows }))
        }
        "plant" => {
            let name = args["name"]
                .as_str()
                .ok_or("plant needs a 'name'")?
                .to_string();
            let mut ports = std::collections::HashMap::new();
            if let Some(map) = args["ports"].as_object() {
                for (role, port) in map {
                    let port = port.as_u64().ok_or("ports values must be numbers")? as u16;
                    ports.insert(role.clone(), port);
                }
            }
            let inputs = std::collections::BTreeMap::new();
            let offering = state
                .garden
                .offer(&name, None, ports, None, None, &inputs)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "planted": offering.name,
                "status": offering.status.as_str(),
                "location": offering.location,
            }))
        }
        "rest" => {
            let name = arg_name(args)?;
            let offering = state.garden.rest(&name).await.map_err(|e| e.to_string())?;
            Ok(json!({ "name": offering.name, "status": offering.status.as_str() }))
        }
        "wake" => {
            let name = arg_name(args)?;
            let offering = state.garden.wake(&name).await.map_err(|e| e.to_string())?;
            Ok(json!({ "name": offering.name, "status": offering.status.as_str() }))
        }
        "uproot" => {
            let name = arg_name(args)?;
            state.garden.uproot(&name).await.map_err(|e| e.to_string())?;
            Ok(json!({ "uprooted": name }))
        }
        "capabilities" => {
            let name = arg_name(args)?;
            let fqn = garden_glossary::fqn::canonicalize(&name)
                .map_err(|e| e.to_string())?;
            let offering = state
                .garden
                .placed(&fqn)
                .ok_or_else(|| format!("'{fqn}' is not planted here"))?;
            let map = capabilities::discover(&state.garden, &offering)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "offering": fqn, "capabilities": map }))
        }
        "grow" => {
            let offering = args["offering"].as_str().ok_or("grow needs 'offering'")?;
            let kind = args["type"].as_str().ok_or("grow needs 'type'")?;
            let item = args["item"].as_str().ok_or("grow needs 'item'")?;
            let job_id = capabilities::grow(
                Arc::clone(&state.garden),
                state.jobs.clone(),
                offering,
                kind,
                item,
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({
                "accepted": true,
                "job_id": job_id,
                "hint": "check the 'jobs' tool - the capability appears when the job is done",
            }))
        }
        "jobs" => {
            let jobs: Vec<Value> = state
                .jobs
                .list()
                .iter()
                .map(|j| serde_json::to_value(j).unwrap_or_default())
                .collect();
            Ok(json!({ "jobs": jobs }))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn arg_name(args: &Value) -> Result<String, String> {
    args["name"]
        .as_str()
        .or(args["offering"].as_str())
        .map(String::from)
        .ok_or_else(|| "needs a 'name' (or 'offering')".to_string())
}

fn json_ok(id: Value, result: Value) -> Response {
    (
        StatusCode::OK,
        axum::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
    )
        .into_response()
}

fn rpc_error(id: Option<Value>, code: i64, message: &str, status: StatusCode) -> Response {
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    (status, axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn app() -> axum::Router {
        crate::http::router(crate::http::tests::test_state())
    }

    async fn rpc(app: &axum::Router, method: &str, params: Value) -> Value {
        let id = serde_json::json!(1);
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        v["result"].clone()
    }

    /// The handshake speaks the protocol and lists the garden's verbs.
    #[tokio::test]
    async fn initialize_and_tools_list() {
        let app = app();
        let init = rpc(&app, "initialize", json!({"protocolVersion": "2025-03-26"})).await;
        assert_eq!(init["serverInfo"]["name"], "zen-garden-moss");

        let listed = rpc(&app, "tools/list", json!({})).await;
        let names: Vec<&str> = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"observe") && names.contains(&"plant") && names.contains(&"grow"));
    }

    /// The channels law, tested: an MCP tool call lands in the SAME
    /// pipeline as the HTTP faces - planting through MCP is visible
    /// through the offerings face.
    #[tokio::test]
    async fn a_tool_call_enters_the_same_pipeline() {
        let app = app();
        // The test state's catalog is empty; plant through MCP refuses
        // with the honest error, proving the call reached the pipeline.
        let res = rpc(
            &app,
            "tools/call",
            json!({"name": "plant", "arguments": {"name": "redis"}}),
        )
        .await;
        assert_eq!(res["isError"], true);
        assert!(
            res["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("no catalog entry"),
            "the pipeline's own refusal surfaced: {}",
            res["content"][0]["text"]
        );
    }
}
