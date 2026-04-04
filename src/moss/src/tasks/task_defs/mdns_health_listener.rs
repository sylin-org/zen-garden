//! BackgroundTask: mdns-health-listener (ARCH-0015, ARCH-0066)
//!
//! Re-registers the mDNS TXT record when stone health transitions.
//! Only registered when mDNS is available.

use std::future::Future;
use std::pin::Pin;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct MdnsHealthListenerTask;

impl BackgroundTask for MdnsHealthListenerTask {
    fn name(&self) -> &'static str {
        "mdns-health-listener"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mdns = match ctx.state.discovery.mdns.as_ref() {
                Some(m) => m.clone(),
                None => return TaskOutcome::Completed,
            };

            let mut health_rx = ctx.state.event_bus.subscribe();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => break,
                    result = health_rx.recv() => {
                        match result {
                            Ok(crate::domain::DomainEvent::Stone(
                                crate::domain::StoneEvent::HealthChanged { ref health, .. },
                            )) => {
                                mdns.update_health(health).await;
                            }
                            Ok(_) => {} // Ignore non-health events
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(missed = n, "mDNS health listener: missed events");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::debug!("mDNS health listener: event bus closed");
                                break;
                            }
                        }
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
