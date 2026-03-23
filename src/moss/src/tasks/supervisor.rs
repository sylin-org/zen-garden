//! Task supervisor for structured concurrency.
//!
//! Wraps `tokio::task::JoinSet` to provide:
//! - Named background task tracking with tracing spans
//! - Panic detection with task name in messages
//! - Clean shutdown via `JoinSet::shutdown()` on cancellation
//!
//! Each spawned task gets a `tracing::info_span!("task", name)` so the
//! task name is visible in panic messages, structured logs, and
//! tokio-console.

use std::future::Future;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Tracks all background tasks for structured shutdown and panic detection.
pub(crate) struct TaskSupervisor {
    tasks: JoinSet<()>,
}

impl TaskSupervisor {
    pub fn new() -> Self {
        Self {
            tasks: JoinSet::new(),
        }
    }

    /// Spawn a named background task.
    ///
    /// The task name is attached as a `tracing::info_span` so it appears in
    /// structured logs, panic backtraces, and tokio-console. When tokio
    /// stabilizes `JoinSet::build_task().name()`, this method should migrate
    /// to that API for native runtime-level naming.
    pub fn spawn(&mut self, name: &'static str, future: impl Future<Output = ()> + Send + 'static) {
        let span = tracing::info_span!("task", name);
        self.tasks.spawn(tracing::Instrument::instrument(
            async move {
                tracing::debug!(task = name, "task started");
                future.await;
                tracing::debug!(task = name, "task completed");
            },
            span,
        ));
    }

    /// Run the supervisor — monitors tasks and logs panics.
    /// Returns when all tasks complete or the token is cancelled.
    pub async fn run(mut self, token: CancellationToken) {
        loop {
            tokio::select! {
                result = self.tasks.join_next() => {
                    match result {
                        Some(Ok(())) => {}
                        Some(Err(e)) if e.is_panic() => {
                            tracing::error!(error = %e, "Background task panicked!");
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "Background task cancelled");
                        }
                        None => break,
                    }
                }
                _ = token.cancelled() => {
                    tracing::info!("Shutting down all supervised background tasks");
                    self.tasks.shutdown().await;
                    break;
                }
            }
        }
        tracing::info!("Task supervisor stopped");
    }
}
