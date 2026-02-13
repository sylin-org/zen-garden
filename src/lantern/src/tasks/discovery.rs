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
        KoiClient, KoiDiscoveryEvent, run_koi_discovery_loop_with_removals,
    };

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

    let (tx, mut rx) = tokio::sync::broadcast::channel(64);
    let koi = std::sync::Arc::new(koi);

    tokio::spawn(run_koi_discovery_loop_with_removals(
        koi,
        garden_common::constants::MDNS_SERVICE_TYPE,
        tx,
    ));

    loop {
        match rx.recv().await {
            Ok(KoiDiscoveryEvent::Resolved(stone)) => {
                upsert_discovered_stone(&state, &stone).await;
            }
            Ok(KoiDiscoveryEvent::Removed(stone_name)) => {
                mark_discovered_stone_offline(&state, &stone_name).await;
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped = skipped, "Lantern discovery lagged on Koi events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::warn!("Lantern discovery channel closed");
                break;
            }
        }
    }

    Ok(())
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
