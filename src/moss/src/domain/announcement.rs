//! Active/passive announcement management.
//!
//! For offerings with `CoordinationMode::Elected`, only the Primary instance
//! is announced via mDNS/DNS. When the Primary role moves (election, failover,
//! manual promotion), the announcement follows.
//!
//! ## Flow
//!
//! 1. Offering deployed with `coordination: elected`
//! 2. Election determines Primary (first instance, or existing primary)
//! 3. Primary's FQN is announced via Koi mDNS: `{offering}.{instance}._type._tcp`
//! 4. On role change: old primary deannounced, new primary announced
//!
//! ## Announcement names
//!
//! FQN → announcement name uses dot separator for DNS subdomain alignment:
//! - `pihole` → `pihole` → `pihole.zengarden`
//! - `pihole::backup` → `pihole.backup` → `pihole.backup.zengarden`

use garden_common::offerings::OfferingFqn;
use garden_common::types::orchestration::{CoordinationMode, OfferingRole};

/// Determines whether an offering instance should be announced.
///
/// Returns `true` if:
/// - The offering is `Independent` (all instances announced), OR
/// - The offering is `Elected` AND this instance is `Primary`
pub fn should_announce(coordination: &CoordinationMode, role: &OfferingRole) -> bool {
    if !coordination.announce_primary_only() {
        return true; // Independent — always announce
    }
    role.is_announced()
}

/// Derive the mDNS/DNS announcement name for an offering FQN.
///
/// Delegates to `OfferingFqn::announcement_name()` which uses dot separator.
pub fn announcement_name(fqn: &OfferingFqn) -> String {
    fqn.announcement_name()
}

/// Infer the mDNS service type from a container port.
///
/// Matches the heuristic table in Koi's runtime adapter.
pub fn infer_service_type(port: u16) -> &'static str {
    match port {
        80 | 3000 | 5000 | 8000 | 8080 | 8888 | 9000 => "_http._tcp",
        443 | 8443 => "_https._tcp",
        5432 => "_postgresql._tcp",
        3306 => "_mysql._tcp",
        6379 => "_redis._tcp",
        27017 => "_mongodb._tcp",
        5672 => "_amqp._tcp",
        9092 => "_kafka._tcp",
        53 => "_dns._tcp",
        1883 => "_mqtt._tcp",
        _ => "_koi-managed._tcp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_always_announced() {
        assert!(should_announce(
            &CoordinationMode::Independent,
            &OfferingRole::Primary
        ));
        assert!(should_announce(
            &CoordinationMode::Independent,
            &OfferingRole::Replica
        ));
    }

    #[test]
    fn elected_only_primary_announced() {
        assert!(should_announce(
            &CoordinationMode::Elected,
            &OfferingRole::Primary
        ));
        assert!(!should_announce(
            &CoordinationMode::Elected,
            &OfferingRole::Replica
        ));
        assert!(!should_announce(
            &CoordinationMode::Elected,
            &OfferingRole::Degraded
        ));
        assert!(!should_announce(
            &CoordinationMode::Elected,
            &OfferingRole::Joining
        ));
    }

    #[test]
    fn announcement_name_default_instance() {
        let fqn = OfferingFqn::new("pihole").unwrap();
        assert_eq!(announcement_name(&fqn), "pihole");
    }

    #[test]
    fn announcement_name_with_instance() {
        let fqn = OfferingFqn::with_instance("searxng", "prod").unwrap();
        assert_eq!(announcement_name(&fqn), "searxng.prod");
    }

    #[test]
    fn announcement_name_hyphenated_offering() {
        let fqn = OfferingFqn::with_instance("home-assistant", "prod").unwrap();
        assert_eq!(announcement_name(&fqn), "home-assistant.prod");
    }

    #[test]
    fn service_type_heuristics() {
        assert_eq!(infer_service_type(53), "_dns._tcp");
        assert_eq!(infer_service_type(8080), "_http._tcp");
        assert_eq!(infer_service_type(5432), "_postgresql._tcp");
        assert_eq!(infer_service_type(6379), "_redis._tcp");
        assert_eq!(infer_service_type(27017), "_mongodb._tcp");
        assert_eq!(infer_service_type(12345), "_koi-managed._tcp");
    }
}
