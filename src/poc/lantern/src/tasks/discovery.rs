//! Stone discovery via Koi embedded mDNS
//!
//! Passively listens for `_moss._tcp` service announcements on the local
//! network and registers discovered stones into Lantern's topology.
//! This gives Lantern the same discovery capability as Moss - stones appear
//! automatically without requiring `LANTERN_ENDPOINT` to be configured.
//!
//! Unlike Moss, Lantern also handles `removed` / goodbye events to mark
//! stones offline immediately. TTL cleanup remains as a fallback for
//! ungraceful disconnects (crash, network loss).

use garden_common::constants::MDNS_SERVICE_TYPE;

use crate::domain::registration::{mark_stone_offline, register_stone};
use crate::AppState;

/// Spawn the mDNS discovery listener via Koi embedded.
///
/// Uses `koi_handle.mdns().browse()` for unified cross-platform discovery.
/// Discovered stones are registered into Lantern's topology cache and trigger domain events.
pub fn spawn_discovery(state: &AppState) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_discovery(state).await {
            tracing::error!(error = %e, "Discovery task failed");
        }
    })
}

async fn run_discovery(state: AppState) -> anyhow::Result<()> {
    let mdns = state
        .koi_handle
        .mdns()
        .map_err(|e| anyhow::anyhow!("mDNS not available for discovery: {}", e))?;

    let browse = mdns
        .browse(MDNS_SERVICE_TYPE)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse: {}", e))?;

    tracing::info!("Lantern mDNS discovery started via koi-embedded (passive topology discovery)");

    while let Some(event) = browse.recv().await {
        match event {
            koi_embedded::MdnsEvent::Resolved(record) => {
                if let Some(discovered) = extract_stone_from_record(&record) {
                    upsert_discovered_stone(&state, &discovered).await;
                }
            }
            koi_embedded::MdnsEvent::Removed { ref name, .. } => {
                // Extract stone name from mDNS name (e.g. "stone-crystal-forest")
                let stone_name = name.split('.').next().unwrap_or("").to_string();
                if !stone_name.is_empty() {
                    mark_discovered_stone_offline(&state, &stone_name).await;
                }
            }
            _ => {}
        }
    }

    tracing::warn!("Lantern mDNS browse stream ended");
    Ok(())
}

/// Extract a discovered stone from a Koi `ServiceRecord`.
///
/// Returns `None` if the record has no LAN-routable IP address.
fn extract_stone_from_record(record: &koi_embedded::ServiceRecord) -> Option<DiscoveredStone> {
    let ip = record.ip.as_deref()?;

    if !garden_common::infra::koi_client::is_lan_routable(ip) {
        return None;
    }

    let ip_addr: std::net::IpAddr = ip.parse().ok()?;

    let port = record.port.unwrap_or(7185);
    let txt = &record.txt;

    let stone_name = txt
        .get("stone_name")
        .cloned()
        .unwrap_or_else(|| record.name.clone());

    let pond_active = txt.get("pond").map(|v| v == "active").unwrap_or(false);
    let https_port = txt.get("https_port").and_then(|v| v.parse::<u16>().ok());

    let mut address = garden_common::PeerAddress::new(ip_addr, port);
    if pond_active
        && let Some(tp) = https_port {
            address = address.with_tls(tp);
        }

    Some(DiscoveredStone {
        stone_id: txt.get("stone_id").cloned(),
        stone_name,
        address,
        mac: txt.get("mac").cloned(),
        version: txt.get("version").cloned(),
        health: txt.get("health").cloned(),
        discovered_at: chrono::Utc::now(),
    })
}

/// Register a discovered stone into Lantern's topology and emit domain event.
async fn upsert_discovered_stone(state: &AppState, stone: &DiscoveredStone) {
    let event = {
        let mut topology = state.topology.write().await;
        register_stone(
            &mut topology,
            stone.stone_id.as_deref(),
            &stone.stone_name,
            &stone.address,
            vec![], // mDNS doesn't provide services - enrichment task fills those in
        )
    };

    tracing::info!(
        stone_name = %stone.stone_name,
        address = %stone.address,
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

/// A stone discovered via mDNS.
///
/// Re-used from the common crate's canonical type.
use garden_common::infra::koi_client::DiscoveredStone;
