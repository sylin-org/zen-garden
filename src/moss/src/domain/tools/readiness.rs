use garden_common::storage::SeedBankAnnouncement;
use garden_common::{Offering, OfferingStatus, ServiceHealthStatus};

/// Returns `(status_string, ready)` for an offering.
pub fn offering_readiness(offering: &Offering) -> (&'static str, bool) {
    match offering.status {
        OfferingStatus::Running => match offering.health {
            ServiceHealthStatus::Healthy => ("running", true),
            ServiceHealthStatus::Degraded => ("degraded", false),
            ServiceHealthStatus::Offline => ("stopped", false),
        },
        OfferingStatus::Degraded => ("degraded", false),
        OfferingStatus::Installing
        | OfferingStatus::Stopped
        | OfferingStatus::Maintenance
        | OfferingStatus::Unknown => ("stopped", false),
    }
}

pub fn seed_bank_readiness(seed_bank: &SeedBankAnnouncement) -> (&'static str, bool) {
    match seed_bank.health.to_ascii_lowercase().as_str() {
        "healthy" => ("running", true),
        "read-only" | "degraded" => ("degraded", false),
        _ => ("stopped", false),
    }
}
