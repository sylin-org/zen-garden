//! BackgroundTask: mdns-lurk-listener (ARCH-0015)
//!
//! Passive topology discovery via mDNS. Listens for neighbor stone
//! announcements and populates the topology cache.
//!
//! Pattern C: carries the mDNS lurk broadcast receiver and the stone name
//! captured at registration time.

use std::future::Future;
use std::pin::Pin;

use garden_common::infra::koi_client::DiscoveredStone;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct MdnsLurkListenerTask {
    pub rx: tokio::sync::broadcast::Receiver<DiscoveredStone>,
    pub self_stone_name: String,
}

impl BackgroundTask for MdnsLurkListenerTask {
    fn name(&self) -> &'static str {
        "mdns-lurk-listener"
    }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        let mut rx = self.rx;
        let self_stone_name = self.self_stone_name;

        Box::pin(async move {
            ctx.ready.signal();

            let topology_cache = ctx.state.current.topology.cache.clone();
            let topology_dirty = ctx.state.current.topology.dirty.clone();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => break,
                    result = rx.recv() => {
                        match result {
                            Ok(discovered) => {
                                // Skip self-announcements
                                if discovered.stone_name == self_stone_name {
                                    continue;
                                }

                                tracing::debug!(
                                    stone_id = ?discovered.stone_id,
                                    stone_name = %discovered.stone_name,
                                    address = %discovered.address,
                                    mac = ?discovered.mac,
                                    "mDNS: Neighbor stone discovered and cached"
                                );

                                // Add to topology cache (only if stone_id is present)
                                if let Some(sid) = discovered.stone_id {
                                    let entry = garden_common::TopologyEntry {
                                        stone_id: sid,
                                        stone_name: discovered.stone_name,
                                        address: discovered.address,
                                        moss_version: discovered
                                            .version
                                            .unwrap_or_else(|| "unknown".to_string()),
                                        services: vec![],
                                        mac: discovered.mac,
                                        health: discovered.health.unwrap_or_else(|| {
                                            garden_common::constants::STONE_INITIALIZING.to_string()
                                        }),
                                        capabilities: None,
                                        status: garden_common::StoneStatus::Online,
                                        discovered_at: chrono::Utc::now(),
                                        last_seen: chrono::Utc::now(),
                                        tags: vec![],
                                        gateways: vec![],
                                    };
                                    crate::domain::topology::upsert_from_chirp_dirty(
                                        &topology_cache,
                                        entry,
                                        &topology_dirty,
                                    )
                                    .await;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(missed = n, "mDNS lurk-listener: missed events");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::debug!("mDNS lurk-listener channel closed");
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
