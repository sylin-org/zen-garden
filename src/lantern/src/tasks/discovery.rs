//! Stone discovery via mDNS (Koi on Windows, mdns-sd on Linux)
//!
//! Passively listens for `_moss._tcp` service announcements on the local
//! network and registers discovered stones into Lantern's topology.
//! This gives Lantern the same discovery capability as Moss — stones appear
//! automatically without requiring `LANTERN_ENDPOINT` to be configured.
//!
//! Unlike Moss, Lantern also handles `removed` / goodbye events to mark
//! stones offline immediately. TTL cleanup remains as a fallback for
//! ungraceful disconnects (crash, network loss).

use garden_common::infra::koi_client::DiscoveredStone;

use crate::domain::registration::{mark_stone_offline, register_stone};
use crate::AppState;

/// Spawn the mDNS discovery listener.
///
/// On Windows, connects to Koi's SSE events stream for `_moss._tcp`.
/// On Linux, uses mdns-sd native browse. Discovered stones are registered
/// into Lantern's topology cache and trigger domain events.
pub fn spawn_discovery(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_discovery(state).await {
            tracing::error!(error = %e, "Discovery task failed");
        }
    })
}

async fn run_discovery(state: AppState) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        run_koi_discovery(state).await
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_mdns_discovery(state).await
    }
}

/// Windows: Discover stones via Koi mDNS proxy SSE stream.
///
/// Reads the raw SSE stream directly to handle both `resolved` (online)
/// and `removed` (offline/goodbye) events. Does NOT delegate to the common
/// `run_koi_discovery_loop` because that only emits `DiscoveredStone`.
#[cfg(target_os = "windows")]
async fn run_koi_discovery(state: AppState) -> anyhow::Result<()> {
    use garden_common::infra::koi_client::{
        extract_service_info, is_lan_routable, KoiClient, KoiEventData,
    };
    use std::time::Duration;

    let koi = match KoiClient::try_connect().await {
        Some(client) => {
            tracing::info!(
                base_url = %client.base_url(),
                "Lantern discovery: connected to Koi mDNS proxy"
            );
            client
        }
        None => {
            tracing::info!(
                "Lantern discovery: Koi not available, mDNS discovery disabled. \
                 Stones must use LANTERN_ENDPOINT heartbeat to register."
            );
            return Ok(());
        }
    };

    tracing::info!("Lantern mDNS discovery started via Koi SSE (passive topology discovery)");

    let mut backoff = Duration::from_secs(1);
    let max_backoff = KoiClient::max_reconnect_backoff();

    loop {
        match koi
            .open_events_stream(garden_common::constants::MDNS_SERVICE_TYPE)
            .await
        {
            Ok(mut resp) => {
                backoff = Duration::from_secs(1);
                let mut buffer = String::new();

                while let Some(chunk) = resp.chunk().await.unwrap_or(None) {
                    let text = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find("\n\n") {
                        let event_block = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        // Parse SSE event header and data
                        let mut event_type = String::new();
                        let mut data_line = String::new();

                        for line in event_block.lines() {
                            if let Some(value) = line.strip_prefix("event:") {
                                event_type = value.trim().to_string();
                            } else if let Some(value) = line.strip_prefix("data:") {
                                data_line = value.trim().to_string();
                            }
                        }

                        if data_line.is_empty() {
                            continue;
                        }

                        let event_data: KoiEventData = match serde_json::from_str(&data_line) {
                            Ok(d) => d,
                            Err(_) => continue,
                        };

                        let json_event = event_data.event.as_deref().unwrap_or("");
                        let is_removed = event_type == "removed" || json_event == "removed";
                        let is_resolved = event_type == "resolved" || json_event == "resolved";

                        if is_removed {
                            let stone_name = extract_stone_name_from_event(&event_data);
                            if let Some(name) = stone_name {
                                mark_discovered_stone_offline(&state, &name).await;
                            }
                        } else if is_resolved {
                            if let Some((_, ip, port, txt)) = extract_service_info(&event_data) {
                                if !is_lan_routable(&ip) {
                                    continue;
                                }
                                if let Some(stone_name) = txt.get("stone_name").cloned() {
                                    let discovered = DiscoveredStone {
                                        stone_id: txt.get("stone_id").cloned(),
                                        stone_name,
                                        endpoint: format!("http://{}:{}", ip, port),
                                        mac: txt.get("mac").cloned(),
                                        version: txt.get("version").cloned(),
                                        health: txt.get("health").cloned(),
                                        discovered_at: chrono::Utc::now(),
                                    };
                                    upsert_discovered_stone(&state, &discovered).await;
                                }
                            }
                        }
                    }
                }

                tracing::debug!("Koi SSE stream ended, reconnecting");
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    backoff_secs = backoff.as_secs(),
                    "Koi SSE stream error, will reconnect"
                );
            }
        }

        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(max_backoff);
    }
}

/// Extract stone_name from a Koi SSE event (for removed/goodbye events)
#[cfg(target_os = "windows")]
fn extract_stone_name_from_event(
    event_data: &garden_common::infra::koi_client::KoiEventData,
) -> Option<String> {
    if let Some(ref svc) = event_data.service {
        svc.txt
            .as_ref()
            .and_then(|t| t.get("stone_name").cloned())
            .or_else(|| Some(svc.name.clone()))
    } else {
        event_data
            .txt
            .as_ref()
            .and_then(|t| t.get("stone_name").cloned())
            .or(event_data.name.clone())
    }
}

/// Linux: Discover stones via mdns-sd native browse
#[cfg(not(target_os = "windows"))]
async fn run_mdns_discovery(state: AppState) -> anyhow::Result<()> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse(garden_common::constants::MDNS_SERVICE_TYPE_LOCAL)?;

    tracing::info!("Lantern mDNS discovery started via mdns-sd (passive topology discovery)");

    enum MdnsEvent {
        Discovered(DiscoveredStone),
        Removed(String),
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<MdnsEvent>(32);

    std::thread::spawn(move || {
        loop {
            match receiver.recv() {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    if let Some(discovered) =
                        garden_common::infra::koi_client::extract_stone_from_service_info(&info)
                    {
                        let _ = tx.blocking_send(MdnsEvent::Discovered(discovered));
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    // Extract stone name from mDNS fullname (e.g. "stone-crystal-forest._moss._tcp.local.")
                    let stone_name = fullname.split('.').next().unwrap_or("").to_string();
                    if !stone_name.is_empty() {
                        let _ = tx.blocking_send(MdnsEvent::Removed(stone_name));
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    while let Some(event) = rx.recv().await {
        match event {
            MdnsEvent::Discovered(stone) => {
                upsert_discovered_stone(&state, &stone).await;
            }
            MdnsEvent::Removed(stone_name) => {
                mark_discovered_stone_offline(&state, &stone_name).await;
            }
        }
    }

    Ok(())
}

/// Register a discovered stone into Lantern's topology and emit domain event.
async fn upsert_discovered_stone(state: &AppState, stone: &DiscoveredStone) {
    let event = {
        let mut topology = state.topology.write().await;
        register_stone(
            &mut topology,
            stone.stone_id.as_deref(),
            &stone.stone_name,
            &stone.endpoint,
            vec![], // mDNS doesn't provide services — enrichment task fills those in
        )
    };

    tracing::info!(
        stone_name = %stone.stone_name,
        endpoint = %stone.endpoint,
        event_type = %event.event_type(),
        "mDNS discovery: stone registered in topology"
    );

    state.event_bus.emit(event);
}

/// Mark a stone offline from mDNS goodbye and emit domain event.
async fn mark_discovered_stone_offline(state: &AppState, stone_name: &str) {
    let event = {
        let mut topology = state.topology.write().await;
        mark_stone_offline(&mut topology, stone_name)
    };

    if let Some(event) = event {
        tracing::info!(
            stone_name = %stone_name,
            "mDNS discovery: stone marked offline (goodbye)"
        );
        state.event_bus.emit(event);
    }
}
