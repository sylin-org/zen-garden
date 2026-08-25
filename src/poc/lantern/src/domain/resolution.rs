//! Service resolution logic
//!
//! Finds online stones that provide a requested service type.
//! Pure domain logic — reads from GardenTopology, no I/O.

use garden_common::constants::SERVICE_RUNNING;
use garden_common::{ResolveResponse, ResolveServiceInfo, StoneStatus};

use super::topology::GardenTopology;

/// Resolve a service type to an online stone endpoint.
///
/// Returns the first online stone that has a running service of the given type.
pub fn resolve_service(topology: &GardenTopology, service_type: &str) -> Option<ResolveResponse> {
    for entry in topology.stones.values() {
        if entry.status != StoneStatus::Online {
            continue;
        }

        for svc in &entry.services {
            if svc.offering == service_type && svc.status == SERVICE_RUNNING {
                return Some(ResolveResponse {
                    stone_name: entry.stone_name.clone(),
                    endpoint: entry.address.http_base(),
                    service: ResolveServiceInfo {
                        name: svc.name.to_string(),
                        service_type: svc.offering.clone(),
                        connection_string: String::new(),
                    },
                });
            }
        }
    }

    None
}
