//! Composition helpers that bridge `AppState` and the `Topology`
//! aggregate.
//!
//! The aggregate itself holds no back-reference to `AppState` per
//! ARCH-0020. These free functions own the assembly of
//! [`SelfEntryInputs`] from the various AppState sub-contexts
//! (`current.address`, `current.health`, `current.mac`,
//! `current.capabilities`, `presence.notifications`, `offerings`,
//! `subsystems.network.ready`) and call the aggregate's typed
//! commands on behalf of AppState-bound consumers.
//!
//! Same composition-helper shape as
//! [`crate::domain::tool::projection::reproject_and_publish`] from
//! Book II (ARCH-0019 Ch5).

use super::aggregate::SelfEntryInputs;
use crate::AppState;
use garden_common::TopologyEntry;
use std::sync::atomic::Ordering;

/// Assemble `SelfEntryInputs` from the current AppState snapshot.
///
/// Reads from seven sources: stone identity, address, health, mac,
/// capabilities, presence tags, active offerings, and subsystems
/// readiness. Acquires each read lock independently; no lock is
/// held across another lock acquisition.
pub async fn self_entry_inputs(state: &AppState) -> SelfEntryInputs {
    let address = state.current.address.read().await.clone();
    let health = state.current.health.read().await.clone();
    let mac = state.current.mac.read().await.clone();
    let capabilities = state.current.capabilities.read().await.clone();
    let tags = state.presence.notifications.compile();
    let services = state
        .offerings
        .with_active(garden_common::TopologyServiceEntry::from_offerings)
        .await;
    let network_ready = state.subsystems.network.ready.load(Ordering::Relaxed);

    SelfEntryInputs {
        stone_id: state.current.stone.id.clone(),
        stone_name: state.current.stone.name.clone(),
        address,
        health,
        mac,
        capabilities,
        tags,
        services,
        moss_version: crate::version_string(),
        network_ready,
    }
}

/// Build a `TopologyEntry` from the current AppState snapshot.
///
/// Convenience wrapper: `self_entry_inputs(state) + Topology::build_self_entry`.
pub async fn build_self_entry(state: &AppState) -> TopologyEntry {
    let inputs = self_entry_inputs(state).await;
    state.topology.build_self_entry(inputs)
}

/// Sync services: assemble inputs, delegate to
/// `Topology::sync_services`, log failures.
pub async fn sync_services(state: &AppState, auto_chirp: bool) {
    let inputs = self_entry_inputs(state).await;
    if let Err(e) = state.topology.sync_services(inputs, auto_chirp).await {
        tracing::warn!(error = ?e, "Failed to auto-chirp after service sync");
    }
}

/// Sync capabilities: assemble inputs, delegate to
/// `Topology::sync_capabilities`.
pub async fn sync_capabilities(state: &AppState, auto_chirp: bool) {
    tracing::info!("Capabilities updated — Topology will read fresh data");
    let inputs = self_entry_inputs(state).await;
    if let Err(e) = state.topology.sync_capabilities(inputs, auto_chirp).await {
        tracing::warn!(error = ?e, "Failed to chirp after capabilities sync");
    }
}

/// Update stone health: mutate `current.health`, then assemble inputs
/// and delegate to `Topology::update_stone_health`.
pub async fn update_stone_health(state: &AppState, health: String, auto_chirp: bool) {
    {
        let mut h = state.current.health.write().await;
        *h = health.clone();
    }
    tracing::debug!(health = %health, "Updated stone health");

    let inputs = self_entry_inputs(state).await;
    if let Err(e) = state.topology.update_stone_health(inputs, auto_chirp).await {
        tracing::warn!(error = ?e, "Failed to chirp after health update");
    }
}

/// Announce a resolution change: mutate `current.address` and
/// `current.mac`, re-register mDNS (if present), then assemble
/// inputs and delegate to `Topology::announce_resolution_change`.
///
/// mDNS re-registration stays here rather than in the aggregate —
/// Discovery is Book X's scope, and the aggregate holds no handle
/// to the mDNS registry per ARCH-0020's "Alternative A rejected"
/// rationale.
pub async fn announce_resolution_change(state: &AppState, new_ip: &str) {
    let new_endpoint = format!("http://{}:{}", new_ip, state.current.api_port);
    tracing::info!(
        endpoint = %new_endpoint,
        "Announcing resolution change (IP/MAC)"
    );

    // Get fresh MAC address (may have changed with network)
    let (_, new_mac) = garden_common::infra::network::get_local_ip_and_mac();

    // Update current.address and current.mac (source fields)
    let new_ip_parsed: std::net::IpAddr = match new_ip.parse() {
        Ok(ip) => ip,
        Err(e) => {
            tracing::warn!(raw = %new_ip, error = %e, "Failed to parse new IP — skipping resolution change");
            return;
        }
    };
    {
        let old_tls_port = state.current.address.read().await.tls_port;
        let mut new_addr = garden_common::PeerAddress::new(new_ip_parsed, state.current.api_port);
        if let Some(tp) = old_tls_port {
            new_addr = new_addr.with_tls(tp);
        }
        *state.current.address.write().await = new_addr;
        *state.current.mac.write().await = new_mac.clone();
    }

    // Re-register mDNS with updated IP and MAC
    if let Some(ref mdns) = state.discovery.mdns
        && let Err(e) = mdns.reregister(new_ip, new_mac.as_deref()).await
    {
        tracing::warn!(error = ?e, "Failed to re-register mDNS after resolution change");
    }

    // Delegate the chirp to the aggregate.
    let inputs = self_entry_inputs(state).await;
    if let Err(e) = state.topology.announce_resolution_change(inputs).await {
        tracing::warn!(error = ?e, "Failed to chirp after resolution change");
    } else {
        tracing::info!("Resolution change announced (mDNS + UDP chirp)");
    }
}
