//! Scenario: HTTP command round-trip through CommandTransport.
//!
//! Simulates the full command lifecycle:
//! 1. `POST /command { raw_args }` → CommandTransport publishes
//!    `CommandInvocation` event.
//! 2. A custom adapter subscribed to `core.command.invocation` handles
//!    the command and publishes `CommandResult` with matching
//!    correlation_id.
//! 3. CommandTransport correlates, aggregates, and returns the JSON
//!    response to the HTTP caller.

use garden_common::command_manifest::CommandResponse;
use garden_companion_sdk::adapters::{
    Adapter, AdapterInfo, AdapterProfile, adapter::BoxFuture,
};
use garden_companion_sdk::moss_client::MossLocalClient;
use garden_companion_sdk::testing::{FakeFactory, TestHarness};
use garden_companion_sdk::garden::{
    CommandInvocation, CommandOutcome, CommandResult, CommandTransport, Event, Pulse,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Adapter that echoes whatever args it receives as a success result.
struct EchoAdapter {
    id: String,
}

impl Adapter for EchoAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            kind: "test.echo",
            id: self.id.clone(),
            device: None,
        }
    }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: &["core.command.invocation"],
            ..AdapterProfile::default()
        }
    }

    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        _moss: Arc<MossLocalClient>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            loop {
                tokio::select! {
                    maybe = events.recv() => match maybe {
                        Some(event) => {
                            if let Some(inv) = event.payload::<CommandInvocation>() {
                                let output = format!("echo: {}", inv.raw_args.join(" "));
                                let _ = pulse.ingest(Event::new(CommandResult {
                                    correlation_id: inv.correlation_id,
                                    outcome: CommandOutcome::Success { output: Some(output) },
                                    from: self.id.clone(),
                                }));
                            }
                        }
                        None => break,
                    },
                    _ = shutdown.cancelled() => break,
                }
            }
        })
    }
}

#[tokio::test]
async fn http_post_command_round_trips_via_event_mesh() {
    // Pick an ephemeral port.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let transport = CommandTransport::new(port).with_timeout(std::time::Duration::from_millis(500));

    let factory = FakeFactory::new("test.echo", || {
        Box::new(EchoAdapter {
            id: "only".to_string(),
        })
    });

    let harness = TestHarness::new("scenario-command-round-trip")
        .with_transport(transport)
        .with_adapter_factory(factory)
        .start()
        .await;

    // Let the HTTP server bind and supervisor spawn the adapter.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/command", port))
        .json(&serde_json::json!({ "raw_args": ["hello", "world"] }))
        .send()
        .await
        .expect("HTTP request failed");
    assert_eq!(resp.status(), 200);

    let body: CommandResponse = resp.json().await.expect("response not JSON");
    assert!(body.is_success(), "expected success; got: {:?}", body);
    assert!(
        body.message.contains("echo: hello world"),
        "unexpected body: {}",
        body.message
    );

    let _ = harness.shutdown().await;
}
