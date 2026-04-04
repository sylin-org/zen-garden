//! BackgroundTask: ip-change-handler (ARCH-0015)
//!
//! Monitors network events and re-announces resolution when the stone's IP
//! changes. Only registered when `ZG_STONE_HOST` is not set (dynamic host).

use std::future::Future;
use std::pin::Pin;

use crate::tasks::network_monitor::NetworkEvent;
use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct IpChangeHandlerTask;

impl BackgroundTask for IpChangeHandlerTask {
    fn name(&self) -> &'static str {
        "ip-change-handler"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            ctx.ready.signal();

            let mut network_rx = ctx.state.platform.network.subscribe();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => break,
                    result = network_rx.recv() => {
                        let Ok(event) = result else { break };

                        let new_ip = match &event {
                            NetworkEvent::IpChanged { new, .. } => Some(new.clone()),
                            NetworkEvent::Reconnected { new } => Some(new.clone()),
                            NetworkEvent::Disconnected { .. } => None,
                        };

                        if let Some(ip) = new_ip {
                            // Reinitialize P2P sender sockets when network becomes available.
                            // Critical on Linux where interfaces may not be ready at boot.
                            if matches!(event, NetworkEvent::Reconnected { .. }) {
                                tracing::info!("Network reconnected, reinitializing P2P senders");
                                garden_common::infra::communications::p2p::reinit_senders().await;
                            }

                            // Delegate all resolution change handling to AppState
                            ctx.state.announce_resolution_change(&ip).await;
                        }
                    }
                }
            }

            TaskOutcome::Cancelled
        })
    }
}
