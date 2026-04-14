---
audience: [developer]
doc_type: guide
status: current
last_verified: 2026-04-13
---

# Companion Integration Testing

How to write integration tests for a Companion built on `garden-companion-sdk`, using the `testing` module to stand up a full event mesh without external I/O.

---

## What You'll Need

- A Cargo crate that depends on `garden-companion-sdk` as a dev-dependency (or `tests/` in the SDK itself).
- `tokio` with the `macros` and `rt-multi-thread` features.
- Familiarity with the Adapter trait and Pulse/Garden aggregates (see [companion-development.md](companion-development.md)).

## What the Testing Module Provides

`garden_companion_sdk::testing` exports four primitives:

| Primitive | Purpose |
|-----------|---------|
| `TestHarness` | Fluent builder that constructs a `Companion` with short test-scope timings. |
| `MockTransport` | In-memory transport; publish events via its `handle().queue(event)`. |
| `RecordingAdapter` | Subscribes to declared kinds and stores every delivered event for assertion. |
| `FakeFactory` | Adapts a closure into an `AdapterFactory`, useful for bespoke test adapters. |

## Build a Scenario

The canonical shape of an integration test is: construct harness → start → drive events → assert → shut down.

```rust
use garden_companion_sdk::testing::{
    MockTransport, RecordingAdapter, TestHarness,
    recording_adapter::RecordingHandleExt,
};
use garden_companion_sdk::garden::Event;
use garden_common::presence::StoneHealthChangedPayload;
use std::time::Duration;

#[tokio::test]
async fn scenario() {
    let transport = MockTransport::new();
    let handle = transport.handle();

    let (records, factory) =
        RecordingAdapter::factory("test.record", "only", &["core.stone.health.changed"]);

    let harness = TestHarness::new("scenario-name")
        .with_flush_interval(Duration::from_millis(10))
        .with_transport(transport)
        .with_adapter_factory(factory)
        .start()
        .await;

    // Allow one discovery tick so the supervisor spawns the adapter.
    tokio::time::sleep(Duration::from_millis(100)).await;

    handle.queue(Event::new(StoneHealthChangedPayload {
        health: "thriving".into(),
        cpu_percent: 0.0,
        memory_percent: 0.0,
    }));

    // Wait past one flush window before asserting.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(records.kinds(), vec!["core.stone.health.changed"]);

    let _ = harness.shutdown().await;
}
```

## Choose the Right Harness Knob

| Knob | When to set | Default |
|------|-------------|---------|
| `with_flush_interval(d)` | Tests that need deterministic coalescing boundaries. Short intervals (10–20ms) keep tests fast. | 10ms |
| `with_transport(t)` | Attach `MockTransport` for scripted events, or `CommandTransport` / `SseTransport` for boundary tests. | none |
| `with_adapter_factory(f)` | Register each factory under test. Multiple factories share the same Pulse and Garden. | none |

## Write a Custom Adapter

When `RecordingAdapter` is not enough — for example, a round-trip test where the adapter must publish a response — implement `Adapter` directly and hand it to `FakeFactory`.

```rust
use garden_companion_sdk::adapters::{Adapter, AdapterInfo, AdapterProfile, adapter::BoxFuture};
use garden_companion_sdk::garden::{CommandInvocation, CommandOutcome, CommandResult, Event, Garden, Pulse};
use garden_companion_sdk::testing::FakeFactory;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct EchoAdapter { id: String }

impl Adapter for EchoAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo { kind: "test.echo", id: self.id.clone(), device: None }
    }
    fn profile(&self) -> AdapterProfile {
        AdapterProfile { subscriptions: &["core.command.invocation"], ..AdapterProfile::default() }
    }
    fn run(
        self: Box<Self>,
        mut events: mpsc::Receiver<Event>,
        _g: Arc<Garden>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            loop {
                tokio::select! {
                    maybe = events.recv() => match maybe {
                        Some(ev) => if let Some(inv) = ev.payload::<CommandInvocation>() {
                            let _ = pulse.ingest(Event::new(CommandResult {
                                correlation_id: inv.correlation_id,
                                outcome: CommandOutcome::Success { output: Some("ok".into()) },
                                from: self.id.clone(),
                            }));
                        },
                        None => break,
                    },
                    _ = shutdown.cancelled() => break,
                }
            }
        })
    }
}

let factory = FakeFactory::new("test.echo", || Box::new(EchoAdapter { id: "only".into() }));
```

## Timing Conventions

The harness defaults favour fast tests, but the supervisor still runs discovery on a 5-second cadence. Follow these rules:

- **Use `MockTransport` + `RecordingAdapter`** for most scenarios. A single 100ms sleep after `start()` is enough for discovery to spawn the adapter.
- **For lifecycle tests** (spawn → reap → bounce), expect runs in the 8–10 second range. Document this in the test comments.
- **For shutdown tests**, trust the bounded 5-second timeout inside `RunningHarness::shutdown()` — if it times out, a background task is leaking.

## Assert on the Right Surface

| Surface | API | Use for |
|---------|-----|---------|
| Delivered events | `records.kinds()`, `records.len()`, `records.is_empty()` | Verifying an adapter observed specific kinds. |
| Payload fields | `records.lock().unwrap().iter().filter_map(\|e\| e.payload::<P>())` | Asserting on coalesced values, correlation ids, etc. |
| Garden projection | `harness.garden().health()`, `.pond()`, `.offerings()` | End-to-end presence → projection. |
| Readiness | `harness.wait_ready(Duration::from_secs(2)).await` | Wait for the first `PresenceSnapshot` to project. |
| Supervisor state | `harness.adapters().active_count()` | Lifecycle assertions. |

## Verification

Run the integration tests:

```bash
cargo test --package garden-companion-sdk --tests
```

All scenarios in `src/companion-sdk/tests/` should pass. They also serve as reference implementations for each surface above.

## Troubleshooting

### Test hangs or times out in shutdown

A background task is not observing its cancellation token. Verify the adapter's `run` loop uses `tokio::select!` with `shutdown.cancelled()` and breaks on receiver close.

### Adapter never sees events

Check the subscription list matches the event kind exactly — the filter task drops anything not declared. Use `records.kinds()` on a receive-all adapter to confirm what Pulse is actually fanning out.

### `0` active adapters after `start()`

The first discovery tick has not fired yet. Sleep 100ms before asserting on `active_count()`.

## Next Steps

- [companion-development.md](companion-development.md) — write your first Companion.
- [companion-architecture.md](../specs/companion-architecture.md) — the pattern spec that the testing module exercises.
- [COMPANION-0001](../decisions/COMPANION-0001-companion-integration-epic.md) — epic context for the event mesh.
