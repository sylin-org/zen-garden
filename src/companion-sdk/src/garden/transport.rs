//! Transport trait — the extension point for event sources.
//!
//! A `Transport` is any long-running task that publishes events into
//! [`Pulse`]. The SDK ships two implementations in Book III:
//!
//! - [`SseTransport`] — consumes moss's `/presence/stream` SSE endpoint
//! - [`CommandTransport`] — serves HTTP commands, publishing invocations
//!   and correlating command results back into HTTP responses
//!
//! Future transports (MQTT subscriber, webhook receiver, file watcher, ...)
//! implement the same trait. No SDK modifications are required to add one.
//!
//! # The lifecycle
//!
//! 1. `Companion::new(...)` collects transports via `.with_transport(T)`.
//! 2. At `run()`, `Companion` walks every transport and calls
//!    [`Pulse::register_namespace`] for each kind's namespace (derived from
//!    [`Transport::emitted_kinds`]).
//! 3. For each transport, `Companion` spawns a task running
//!    [`Transport::run`] with a clone of the `Pulse` and a child
//!    [`CancellationToken`].
//! 4. On shutdown, `Companion` cancels the token; each transport's `run`
//!    future exits cleanly.
//!
//! [`SseTransport`]: crate::garden::SseTransport
//! [`CommandTransport`]: crate::garden::CommandTransport
//! [`Pulse::register_namespace`]: crate::garden::Pulse::register_namespace

use super::pulse::Pulse;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Pinned boxed future type alias used by [`Transport::run`].
///
/// Async-trait style: the trait method returns a `BoxFuture<'static, ()>`
/// rather than being `async fn` because async trait methods are not
/// object-safe in stable Rust when the trait also appears in
/// `Box<dyn Transport>`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A pluggable event source.
///
/// Transports run for the lifetime of the `Companion`. They ingest events
/// into [`Pulse`], and — optionally — subscribe to Pulse for response-style
/// patterns (see [`CommandTransport`] for correlation).
///
/// # Implementor contract
///
/// - `run` must return when `shutdown` is cancelled.
/// - `run` should be resilient to transient errors (reconnect, retry).
///   Fatal errors may be reported via `tracing` and cause `run` to return.
/// - `emitted_kinds` must return every `EventPayload::KIND` value this
///   transport may emit. `Companion` uses this list to register namespaces.
///
/// [`CommandTransport`]: crate::garden::CommandTransport
pub trait Transport: Send + 'static {
    /// Run until `shutdown` is cancelled.
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;

    /// All kinds this transport may emit. Used by `Companion` to
    /// auto-register namespaces on [`Pulse`].
    ///
    /// An empty slice is valid (e.g. for transports that only observe, not
    /// emit) but unusual.
    fn emitted_kinds(&self) -> &'static [&'static str];
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopTransport;
    impl Transport for NoopTransport {
        fn run(
            self: Box<Self>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> BoxFuture<'static, ()> {
            Box::pin(async move {
                shutdown.cancelled().await;
            })
        }

        fn emitted_kinds(&self) -> &'static [&'static str] {
            &["core.test.noop"]
        }
    }

    #[test]
    fn transport_is_object_safe_and_boxable() {
        let transports: Vec<Box<dyn Transport>> = vec![Box::new(NoopTransport)];
        assert_eq!(transports.len(), 1);
        assert_eq!(transports[0].emitted_kinds(), &["core.test.noop"]);
    }

    #[tokio::test]
    async fn noop_transport_exits_on_shutdown() {
        let pulse = Arc::new(Pulse::with_defaults());
        let token = CancellationToken::new();

        let token_cancel = token.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            token_cancel.cancel();
        });

        let transport: Box<dyn Transport> = Box::new(NoopTransport);
        transport.run(pulse, token).await;
        handle.await.unwrap();
    }
}
