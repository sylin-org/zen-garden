//! Communications infrastructure layer
//!
//! Centralized transport handling for stone-to-stone communication:
//! - P2P: UDP broadcast on port 7184 (discovery, election, ceremonies)
//! - mDNS: Service discovery via multicast DNS

pub mod p2p;

pub use p2p::{send_announcement, subscribe_to_events, UdpEvent};
