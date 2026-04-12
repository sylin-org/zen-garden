//! Discovery bounded context — mDNS registration, Koi embedded handle,
//! and peer discovery via lurk-listener.
//!
//! ## Aggregate pattern
//!
//! `Discovery` is an ephemeral aggregate (no persistence). It encapsulates:
//! - The Koi embedded handle (mDNS, DNS, certmesh, vault sub-handles)
//! - The mDNS service registration handle (`MdnsHandle`)
//! - The mDNS lurk-listener broadcast source
//!
//! ## Commands
//!
//! - `reregister(ip, mac)` — re-register mDNS `_moss._tcp` + `_http._tcp`
//! - `update_health(health)` — update mDNS TXT record
//! - `register_certmesh(port)` — register `_certmesh._tcp` service
//!
//! ## Events
//!
//! `DiscoveryChanged` with kinds: `Registered`, `HealthUpdated`.

mod aggregate;
mod event;
pub mod mdns;
#[cfg(test)]
mod tests;

pub use aggregate::Discovery;
pub use event::{DiscoveryChangeKind, DiscoveryChanged};
