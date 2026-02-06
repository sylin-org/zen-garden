use garden_common::storage::SeedBankAnnouncement;
use garden_common::tools::ToolState;
use garden_common::{Offering, OfferingStatus, ServiceHealthStatus};

pub fn offering_readiness(offering: &Offering) -> (ToolState, bool) {
    match offering.status {
        OfferingStatus::Running => match offering.health {
            ServiceHealthStatus::Healthy => (ToolState::Ready, true),
            ServiceHealthStatus::Degraded => (ToolState::Degraded, false),
            ServiceHealthStatus::Offline => (ToolState::Unavailable, false),
        },
        OfferingStatus::Degraded => (ToolState::Degraded, false),
        OfferingStatus::Installing
        | OfferingStatus::Stopped
        | OfferingStatus::Maintenance
        | OfferingStatus::Unknown => (ToolState::Unavailable, false),
    }
}

pub fn seed_bank_readiness(seed_bank: &SeedBankAnnouncement) -> (ToolState, bool) {
    match seed_bank.health.to_ascii_lowercase().as_str() {
        "healthy" => (ToolState::Ready, true),
        "read-only" | "degraded" => (ToolState::Degraded, false),
        _ => (ToolState::Unavailable, false),
    }
}
