//! Stone registration business logic
//!
//! Handles heartbeat registration and TTL-based offline detection.
//! Pure domain logic — no I/O, operates on GardenTopology directly.
//! Uses TopologyEntry from garden_common as the canonical stone type.

use chrono::{Duration, Utc};
use garden_common::types::topology::TopologyEntry;
use garden_common::{PeerAddress, RegisterServiceInfo, StoneStatus, TopologyServiceEntry};

use super::topology::GardenTopology;
use crate::domain::events::RegistrationEvent;

/// TTL for stone heartbeat (seconds). Stones that miss this are marked offline.
/// Set to 3x the heartbeat interval (45s) to tolerate network jitter and missed beats.
const STONE_TTL_SECONDS: i64 = 135;

/// Register or update a stone in the topology.
///
/// Creates a TopologyEntry on first registration, updates it on heartbeat.
/// Returns a domain event describing what happened.
pub fn register_stone(
    topology: &mut GardenTopology,
    stone_id: Option<&str>,
    stone_name: &str,
    address: &PeerAddress,
    services: Vec<RegisterServiceInfo>,
) -> RegistrationEvent {
    let now = Utc::now();
    let cache_key = stone_id.unwrap_or(stone_name).to_string();
    let is_new = !topology.stones.contains_key(&cache_key);

    // Convert RegisterServiceInfo → TopologyServiceEntry
    let topo_services: Vec<TopologyServiceEntry> = services
        .into_iter()
        .map(|svc| TopologyServiceEntry {
            offering_id: String::new(),
            name: garden_common::offerings::OfferingFqn::parse(&svc.name)
                .unwrap_or_else(|_| garden_common::offerings::OfferingFqn {
                    source: None,
                    offering: svc.name.clone(),
                    instance: None,
                    image_ref: None,
                }),
            offering: svc.service_type,
            category: String::new(),
            status: svc.status,
            role: None,
            ports: std::collections::HashMap::new(),
        })
        .collect();

    if let Some(entry) = topology.stones.get_mut(&cache_key) {
        // Update existing stone
        entry.last_seen = now;
        entry.status = StoneStatus::Online;
        entry.address = address.clone();

        if entry.stone_name != stone_name {
            tracing::info!(
                stone_id = ?stone_id,
                old_name = %entry.stone_name,
                new_name = %stone_name,
                "Stone hostname changed"
            );
            entry.stone_name = stone_name.to_string();
        }

        entry.services = topo_services;
    } else {
        // New stone
        let entry = TopologyEntry {
            stone_id: stone_id.unwrap_or(&cache_key).to_string(),
            stone_name: stone_name.to_string(),
            address: address.clone(),
            moss_version: String::new(),
            services: topo_services,
            mac: None,
            health: garden_common::constants::VITALITY_THRIVING.to_string(),
            capabilities: None,
            status: StoneStatus::Online,
            discovered_at: now,
            last_seen: now,
            tags: Vec::new(),
            gateways: Vec::new(),
        };

        topology.stones.insert(cache_key, entry);
    }

    topology.last_updated = now;

    if is_new {
        RegistrationEvent::stone_registered(
            stone_id.map(|s| s.to_string()),
            stone_name,
            address.http_base(),
        )
    } else {
        RegistrationEvent::stone_heartbeat(stone_name)
    }
}

/// Mark a stone offline by name (mDNS goodbye).
///
/// Scans topology for a matching stone_name and sets it offline.
/// Returns an event if the stone was online, None if already offline or unknown.
pub fn mark_stone_offline(
    topology: &mut GardenTopology,
    stone_name: &str,
) -> Option<RegistrationEvent> {
    for entry in topology.stones.values_mut() {
        if entry.stone_name == stone_name && entry.status == StoneStatus::Online {
            entry.status = StoneStatus::Offline;

            tracing::warn!(
                stone_name = %stone_name,
                "Stone marked offline (mDNS goodbye)"
            );

            return Some(RegistrationEvent::stone_offline(stone_name));
        }
    }

    None
}

/// Run TTL check: mark stones offline if heartbeat expired.
///
/// Returns events for each stone that went offline.
pub fn check_ttl(topology: &mut GardenTopology) -> Vec<RegistrationEvent> {
    let now = Utc::now();
    let ttl = Duration::seconds(STONE_TTL_SECONDS);
    let mut events = Vec::new();

    for entry in topology.stones.values_mut() {
        if entry.status == StoneStatus::Online {
            let elapsed = now.signed_duration_since(entry.last_seen);
            if elapsed > ttl {
                entry.status = StoneStatus::Offline;

                tracing::warn!(
                    stone_name = %entry.stone_name,
                    last_seen = %entry.last_seen,
                    "Stone marked offline (TTL expired)"
                );

                events.push(RegistrationEvent::stone_offline(&entry.stone_name));
            }
        }
    }

    events
}
