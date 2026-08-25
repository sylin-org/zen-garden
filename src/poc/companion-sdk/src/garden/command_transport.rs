//! Command transport — HTTP commands in, command results out.
//!
//! [`CommandTransport`] serves three endpoints — `POST /command`,
//! `POST /shutdown`, `GET /health` — and translates commands into the
//! event mesh. Each command invocation is published as a
//! [`CommandInvocation`] event with a fresh correlation ID; adapters that
//! handle the command publish matching [`CommandResult`] events; the
//! transport aggregates the results within a timeout and synthesizes the
//! HTTP response.
//!
//! See [COMPANION-0004] for the book ADR.
//!
//! # Three flows in one transport
//!
//! 1. **Outbound**: HTTP handler → publish [`CommandInvocation`] to Pulse.
//! 2. **Correlation**: a background task subscribes to Pulse, filters for
//!    [`CommandResult`] events, routes them to the correct HTTP handler
//!    via an in-memory correlation map.
//! 3. **Response**: HTTP handler collects results until timeout, aggregates
//!    them into a [`CommandResponse`], and returns.
//!
//! # Result aggregation
//!
//! - **Zero results** within the timeout → `CommandResponse::error("No handler ...")`.
//! - **One result** → echo the adapter's outcome as-is.
//! - **Multiple results** → join outputs / errors with adapter-id prefixes.
//!
//! [COMPANION-0004]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0004-transport.md
//! [`CommandResponse`]: garden_common::command_manifest::CommandResponse

use super::event::{Event, EventId, EventPayload, new_event_id};
use super::pulse::Pulse;
use super::transport::{BoxFuture, Transport};
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use garden_common::command_manifest::CommandResponse;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Kind constants
// ---------------------------------------------------------------------------

/// Kind for [`CommandInvocation`] events.
pub const KIND_COMMAND_INVOCATION: &str = "core.command.invocation";

/// Kind for [`CommandResult`] events.
pub const KIND_COMMAND_RESULT: &str = "core.command.result";

/// Every kind [`CommandTransport`] emits. Used by [`Transport::emitted_kinds`].
const COMMAND_EMITTED_KINDS: &[&str] = &[KIND_COMMAND_INVOCATION];

// ---------------------------------------------------------------------------
// Command event payloads
// ---------------------------------------------------------------------------

/// Published when `POST /command` is invoked. Adapters that handle the
/// command publish a matching [`CommandResult`] (correlated via
/// `correlation_id`).
#[derive(Debug, Clone)]
pub struct CommandInvocation {
    /// Correlates this invocation with its result(s). GUIDv7.
    pub correlation_id: EventId,

    /// Raw positional args. `raw_args[0]` is the command name;
    /// subsequent entries are parameters.
    pub raw_args: Vec<String>,
}

impl EventPayload for CommandInvocation {
    const KIND: &'static str = KIND_COMMAND_INVOCATION;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Published by an adapter in response to a [`CommandInvocation`]. The
/// `correlation_id` must equal the invocation's id; otherwise the result
/// is dropped by the transport's correlation collector.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub correlation_id: EventId,
    pub outcome: CommandOutcome,

    /// Identifier of the adapter that produced this result — used for
    /// observability and for multi-adapter aggregation prefixes.
    pub from: String,
}

impl EventPayload for CommandResult {
    const KIND: &'static str = KIND_COMMAND_RESULT;
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The outcome of an adapter handling a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// Command handled successfully; `output` may contain a printable
    /// summary.
    Success { output: Option<String> },

    /// Command failed; `message` is the human-readable error.
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Public transport type
// ---------------------------------------------------------------------------

const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Serves HTTP commands, translating them into the event mesh.
///
/// # Construction
///
/// ```
/// use garden_companion_sdk::garden::CommandTransport;
/// use std::time::Duration;
///
/// let transport = CommandTransport::new(7188);
/// let with_custom_timeout = CommandTransport::new(7188)
///     .with_timeout(Duration::from_secs(10));
/// ```
pub struct CommandTransport {
    port: u16,
    response_timeout: Duration,
}

impl CommandTransport {
    /// Construct with default 5-second response timeout.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
        }
    }

    /// Override the per-request response-collection timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.response_timeout = timeout;
        self
    }
}

impl Transport for CommandTransport {
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(run_command_transport(
            self.port,
            self.response_timeout,
            pulse,
            shutdown,
        ))
    }

    fn emitted_kinds(&self) -> &'static [&'static str] {
        COMMAND_EMITTED_KINDS
    }
}

// ---------------------------------------------------------------------------
// Correlation map + collector task
// ---------------------------------------------------------------------------

type CorrelationMap = Arc<Mutex<HashMap<EventId, mpsc::UnboundedSender<CommandResult>>>>;

#[derive(Clone)]
struct TransportState {
    pulse: Arc<Pulse>,
    correlations: CorrelationMap,
    response_timeout: Duration,
    shutdown: CancellationToken,
}

async fn run_command_transport(
    port: u16,
    response_timeout: Duration,
    pulse: Arc<Pulse>,
    shutdown: CancellationToken,
) {
    let correlations: CorrelationMap = Arc::new(Mutex::new(HashMap::new()));

    // Correlation collector task
    let collector_handle = {
        let correlations = correlations.clone();
        let pulse = pulse.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            run_correlation_collector(pulse, correlations, shutdown).await;
        })
    };

    let state = TransportState {
        pulse,
        correlations,
        response_timeout,
        shutdown: shutdown.clone(),
    };

    let app = Router::new()
        .route("/command", post(handle_command))
        .route("/shutdown", post(handle_shutdown))
        .route("/health", axum::routing::get(handle_health))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(port, "CommandTransport listening");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, port, "CommandTransport failed to bind");
            collector_handle.abort();
            return;
        }
    };

    let shutdown_for_axum = shutdown.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        shutdown_for_axum.cancelled().await;
    });

    if let Err(e) = server.await {
        tracing::warn!(error = %e, "CommandTransport server exited with error");
    }

    collector_handle.abort();
    let _ = collector_handle.await;
}

async fn run_correlation_collector(
    pulse: Arc<Pulse>,
    correlations: CorrelationMap,
    shutdown: CancellationToken,
) {
    let mut rx = pulse.subscribe();

    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(event) => {
                    if let Some(result) = event.payload::<CommandResult>() {
                        let sender_opt = {
                            let map = correlations.lock().expect("correlation map poisoned");
                            map.get(&result.correlation_id).cloned()
                        };
                        if let Some(tx) = sender_opt {
                            let _ = tx.send(result.clone());
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "CommandTransport collector lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            _ = shutdown.cancelled() => break,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CommandRequest {
    #[serde(default)]
    raw_args: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn handle_command(
    State(state): State<TransportState>,
    Json(req): Json<CommandRequest>,
) -> (StatusCode, Json<CommandResponse>) {
    let correlation_id = new_event_id();
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Register in the correlation map before publishing so no result is missed.
    {
        let mut map = state.correlations.lock().expect("correlation map poisoned");
        map.insert(correlation_id, tx);
    }

    // Publish the invocation
    let invocation = CommandInvocation {
        correlation_id,
        raw_args: req.raw_args,
    };
    let _ = state.pulse.ingest(Event::new(invocation));

    // Collect results until the timeout deadline.
    let deadline = tokio::time::sleep(state.response_timeout);
    tokio::pin!(deadline);

    let mut results: Vec<CommandResult> = Vec::new();

    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => break,
            maybe = rx.recv() => match maybe {
                Some(result) => results.push(result),
                None => break,
            }
        }
    }

    // Remove from the map (receivers in the map hold tx clones; dropping
    // here ensures the correlation map doesn't leak entries).
    {
        let mut map = state.correlations.lock().expect("correlation map poisoned");
        map.remove(&correlation_id);
    }

    let response = aggregate_results(results, state.response_timeout);
    (StatusCode::OK, Json(response))
}

async fn handle_shutdown(State(state): State<TransportState>) -> StatusCode {
    tracing::info!("CommandTransport received /shutdown");
    state.shutdown.cancel();
    StatusCode::ACCEPTED
}

async fn handle_health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy" })
}

// ---------------------------------------------------------------------------
// Response aggregation
// ---------------------------------------------------------------------------

/// Collapse zero, one, or many [`CommandResult`]s into a single
/// [`CommandResponse`] to return to the HTTP caller.
pub(crate) fn aggregate_results(results: Vec<CommandResult>, timeout: Duration) -> CommandResponse {
    match results.len() {
        0 => CommandResponse::error(format!(
            "No handler responded within {}ms",
            timeout.as_millis()
        )),
        1 => single_result_to_response(results.into_iter().next().unwrap()),
        _ => aggregate_many(results),
    }
}

fn single_result_to_response(result: CommandResult) -> CommandResponse {
    match result.outcome {
        CommandOutcome::Success { output } => match output {
            Some(s) => CommandResponse::success(s),
            None => CommandResponse::success("OK"),
        },
        CommandOutcome::Error { message } => CommandResponse::error(message),
    }
}

fn aggregate_many(results: Vec<CommandResult>) -> CommandResponse {
    let has_error = results
        .iter()
        .any(|r| matches!(r.outcome, CommandOutcome::Error { .. }));

    let lines: Vec<String> = results
        .iter()
        .map(|r| match &r.outcome {
            CommandOutcome::Success { output } => {
                format!("{}: {}", r.from, output.as_deref().unwrap_or("OK"))
            }
            CommandOutcome::Error { message } => format!("{}: ERROR {}", r.from, message),
        })
        .collect();

    let joined = lines.join("\n");
    if has_error {
        CommandResponse::error(joined)
    } else {
        CommandResponse::success(joined)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::{Event, IngestResult, Pulse, PulseConfig};

    fn core_pulse() -> Arc<Pulse> {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 16,
            broadcast_capacity: 64,
        }));
        pulse.register_namespace("core");
        pulse
    }

    // --- Payload typing ---

    #[test]
    fn invocation_and_result_round_trip_through_envelope() {
        let id = new_event_id();
        let inv = CommandInvocation {
            correlation_id: id,
            raw_args: vec!["brightness".into(), "50".into()],
        };
        let evt = Event::new(inv);
        assert_eq!(evt.kind, "core.command.invocation");
        let back = evt.payload::<CommandInvocation>().unwrap();
        assert_eq!(back.correlation_id, id);
        assert_eq!(back.raw_args, vec!["brightness", "50"]);

        let result = CommandResult {
            correlation_id: id,
            outcome: CommandOutcome::Success {
                output: Some("done".into()),
            },
            from: "firefly-matrix".into(),
        };
        let result_evt = Event::new(result);
        assert_eq!(result_evt.kind, "core.command.result");
        assert_eq!(
            result_evt.payload::<CommandResult>().unwrap().from,
            "firefly-matrix"
        );
    }

    #[test]
    fn invocation_is_not_coalescing() {
        const { assert!(!CommandInvocation::COALESCING) };
        const { assert!(!CommandResult::COALESCING) };
    }

    // --- Emitted kinds ---

    #[test]
    fn command_transport_emits_only_invocation_kind() {
        let t = CommandTransport::new(7188);
        assert_eq!(t.emitted_kinds(), &["core.command.invocation"]);
    }

    // --- Response aggregation ---

    #[test]
    fn zero_results_produces_timeout_error() {
        let resp = aggregate_results(vec![], Duration::from_millis(200));
        assert!(!resp.is_success());
        assert!(resp.message.contains("200ms"));
    }

    #[test]
    fn single_success_result_becomes_success_response() {
        let id = new_event_id();
        let result = CommandResult {
            correlation_id: id,
            outcome: CommandOutcome::Success {
                output: Some("brightness=50".into()),
            },
            from: "firefly-matrix".into(),
        };
        let resp = aggregate_results(vec![result], Duration::from_secs(1));
        assert!(resp.is_success());
        assert!(resp.message.contains("brightness=50"));
    }

    #[test]
    fn single_error_result_becomes_error_response() {
        let id = new_event_id();
        let result = CommandResult {
            correlation_id: id,
            outcome: CommandOutcome::Error {
                message: "device not connected".into(),
            },
            from: "firefly-matrix".into(),
        };
        let resp = aggregate_results(vec![result], Duration::from_secs(1));
        assert!(!resp.is_success());
        assert!(resp.message.contains("device not connected"));
    }

    #[test]
    fn multiple_successes_join_with_adapter_prefixes() {
        let id = new_event_id();
        let results = vec![
            CommandResult {
                correlation_id: id,
                outcome: CommandOutcome::Success {
                    output: Some("cleared".into()),
                },
                from: "firefly-matrix".into(),
            },
            CommandResult {
                correlation_id: id,
                outcome: CommandOutcome::Success {
                    output: Some("cleared".into()),
                },
                from: "firefly-oled-v2".into(),
            },
        ];
        let resp = aggregate_results(results, Duration::from_secs(1));
        assert!(resp.is_success());
        assert!(resp.message.contains("firefly-matrix:"));
        assert!(resp.message.contains("firefly-oled-v2:"));
    }

    #[test]
    fn mixed_results_produce_error_with_adapter_prefixes() {
        let id = new_event_id();
        let results = vec![
            CommandResult {
                correlation_id: id,
                outcome: CommandOutcome::Success {
                    output: Some("OK".into()),
                },
                from: "firefly-matrix".into(),
            },
            CommandResult {
                correlation_id: id,
                outcome: CommandOutcome::Error {
                    message: "not connected".into(),
                },
                from: "firefly-oled-v2".into(),
            },
        ];
        let resp = aggregate_results(results, Duration::from_secs(1));
        assert!(!resp.is_success());
        assert!(resp.message.contains("ERROR not connected"));
        assert!(resp.message.contains("firefly-matrix:"));
    }

    // --- End-to-end: invocation → collector → result → aggregated response ---

    #[tokio::test]
    async fn end_to_end_single_adapter_response() {
        // Spin up transport with a short timeout so the test is quick
        // even if nothing responds.
        let pulse = core_pulse();
        let token = CancellationToken::new();

        // Find a free port by binding to 0.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let transport = Box::new(
            CommandTransport::new(port).with_timeout(Duration::from_millis(500)),
        );

        let pulse_for_run = pulse.clone();
        let token_for_run = token.clone();
        let server = tokio::spawn(async move {
            transport.run(pulse_for_run, token_for_run).await;
        });

        // Give the server time to bind.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Fake "adapter": subscribe to pulse, react to CommandInvocation
        // by publishing a CommandResult.
        let fake_adapter = {
            let pulse = pulse.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let mut rx = pulse.subscribe();
                loop {
                    tokio::select! {
                        recv = rx.recv() => match recv {
                            Ok(evt) => {
                                if let Some(inv) = evt.payload::<CommandInvocation>() {
                                    let _ = pulse.ingest(Event::new(CommandResult {
                                        correlation_id: inv.correlation_id,
                                        outcome: CommandOutcome::Success {
                                            output: Some(format!("handled {:?}", inv.raw_args)),
                                        },
                                        from: "test-adapter".into(),
                                    }));
                                }
                            }
                            Err(_) => break,
                        },
                        _ = token.cancelled() => break,
                    }
                }
            })
        };

        // Send HTTP command
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{}/command", port))
            .json(&serde_json::json!({ "raw_args": ["hello", "world"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: CommandResponse = resp.json().await.unwrap();
        assert!(body.is_success());
        assert!(body.message.contains("handled"));

        // Shut down
        token.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), fake_adapter).await;
    }

    #[tokio::test]
    async fn end_to_end_no_handler_times_out() {
        let pulse = core_pulse();
        let token = CancellationToken::new();

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let transport = Box::new(
            CommandTransport::new(port).with_timeout(Duration::from_millis(200)),
        );

        let pulse_for_run = pulse.clone();
        let token_for_run = token.clone();
        let server = tokio::spawn(async move {
            transport.run(pulse_for_run, token_for_run).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{}/command", port))
            .json(&serde_json::json!({ "raw_args": ["unhandled"] }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: CommandResponse = resp.json().await.unwrap();
        assert!(!body.is_success());
        assert!(body.message.contains("No handler responded"));

        token.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }

    #[tokio::test]
    async fn health_endpoint_returns_healthy() {
        let pulse = core_pulse();
        let token = CancellationToken::new();

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let transport = Box::new(CommandTransport::new(port));
        let server = tokio::spawn({
            let pulse = pulse.clone();
            let token = token.clone();
            async move {
                transport.run(pulse, token).await;
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{}/health", port))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "healthy");

        token.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    }

    #[tokio::test]
    async fn shutdown_endpoint_cancels_token() {
        let pulse = core_pulse();
        let token = CancellationToken::new();

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let transport = Box::new(CommandTransport::new(port));
        let token_for_run = token.clone();
        let server = tokio::spawn(async move {
            transport.run(pulse, token_for_run).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://127.0.0.1:{}/shutdown", port))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 202);

        // The server should exit within a couple of seconds.
        let _ = tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server did not exit on /shutdown");
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn ingest_invocation_publishes_to_pulse() {
        // This test verifies the CommandInvocation payload flows through
        // Pulse without going through the HTTP server.
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let correlation_id = new_event_id();
        let evt = Event::new(CommandInvocation {
            correlation_id,
            raw_args: vec!["test".into()],
        });
        assert!(matches!(
            pulse.ingest(evt),
            IngestResult::Accepted { subscribers: 1 }
        ));

        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.command.invocation");
        let inv = delivered.payload::<CommandInvocation>().unwrap();
        assert_eq!(inv.correlation_id, correlation_id);
        assert_eq!(inv.raw_args, vec!["test"]);
    }
}
