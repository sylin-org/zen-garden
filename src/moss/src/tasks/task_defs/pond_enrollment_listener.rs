//! BackgroundTask: pond-enrollment-listener (ARCH-0015)
//!
//! Reacts to PondEvent::EnrollmentChanged by starting/stopping HTTPS + chirp signing.

use std::future::Future;
use std::pin::Pin;

use crate::domain::{DomainEvent, PondEvent};
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct PondEnrollmentListenerTask;

impl BackgroundTask for PondEnrollmentListenerTask {
    fn name(&self) -> &'static str {
        "pond-enrollment-listener"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let console = ctx.state.console.clone();
            let mut pond_rx = ctx.state.event_bus.subscribe();

            loop {
                tokio::select! {
                    result = pond_rx.recv() => {
                        match result {
                            Ok(DomainEvent::Pond(PondEvent::EnrollmentChanged { enrolled, .. })) => {
                                // Reload inter-stone TLS client with fresh cert material
                                ctx.state.security.stone_client.reload_tls();

                                if enrolled {
                                    crate::bootstrap::run::activate_pond_security(
                                        &ctx.state,
                                        &console,
                                    )
                                    .await;
                                } else {
                                    ctx.state
                                        .security
                                        .https
                                        .store(false, std::sync::atomic::Ordering::Relaxed);
                                    tracing::info!(
                                        "Pond unenrolled — HTTPS deactivated (flag cleared)"
                                    );
                                }
                            }
                            Ok(_) => {} // Ignore non-pond events
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    missed = n,
                                    "Pond enrollment listener: missed events"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::debug!("Pond enrollment listener: event bus closed");
                                break;
                            }
                        }
                    }
                    _ = ctx.token.cancelled() => {
                        tracing::debug!("Pond enrollment listener shutting down");
                        break;
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
