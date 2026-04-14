//! SeedBank — a managed-storage summary in the garden's ubiquitous language.

use super::load::Percent;
use crate::presence::StoragePresence;
use serde::{Deserialize, Serialize};

/// Compact summary of a seed-bank (managed storage) attached to a stone.
///
/// Derived data comes through helper methods (`free_gb`, `fill_percent`)
/// rather than being stored — keeps the domain value small and avoids
/// the possibility of storing an inconsistent capacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedBank {
    pub name: String,
    pub used_gb: u64,
    pub total_gb: u64,
}

impl SeedBank {
    /// Free capacity in gigabytes. Saturates at 0 if `used_gb > total_gb`
    /// (defensive — shouldn't happen but we won't panic either).
    pub fn free_gb(&self) -> u64 {
        self.total_gb.saturating_sub(self.used_gb)
    }

    /// Fill level as a [`Percent`]. Returns 0% if `total_gb == 0`
    /// (rather than dividing by zero).
    pub fn fill_percent(&self) -> Percent {
        if self.total_gb == 0 {
            return Percent::MIN;
        }
        let v = (self.used_gb as f64 / self.total_gb as f64) * 100.0;
        Percent::new(v)
    }

    /// True if utilisation ≥ 90% — a common threshold for operator action.
    pub fn is_nearly_full(&self) -> bool {
        self.fill_percent().value() >= 90.0
    }
}

impl From<&StoragePresence> for SeedBank {
    fn from(p: &StoragePresence) -> Self {
        Self {
            name: p.name.clone(),
            used_gb: p.used_gb,
            total_gb: p.total_gb,
        }
    }
}

impl From<StoragePresence> for SeedBank {
    fn from(p: StoragePresence) -> Self {
        Self::from(&p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_gb_computes_remaining_capacity() {
        let bank = SeedBank {
            name: "primary".into(),
            used_gb: 120,
            total_gb: 500,
        };
        assert_eq!(bank.free_gb(), 380);
    }

    #[test]
    fn free_gb_saturates_when_used_exceeds_total() {
        let bank = SeedBank {
            name: "weird".into(),
            used_gb: 600,
            total_gb: 500,
        };
        assert_eq!(bank.free_gb(), 0);
    }

    #[test]
    fn fill_percent_returns_zero_for_empty_capacity() {
        let bank = SeedBank {
            name: "uninit".into(),
            used_gb: 0,
            total_gb: 0,
        };
        assert_eq!(bank.fill_percent(), Percent::MIN);
    }

    #[test]
    fn fill_percent_computes_fraction() {
        let bank = SeedBank {
            name: "half".into(),
            used_gb: 50,
            total_gb: 100,
        };
        assert_eq!(bank.fill_percent().value(), 50.0);
    }

    #[test]
    fn is_nearly_full_matches_threshold() {
        let under = SeedBank {
            name: "x".into(),
            used_gb: 89,
            total_gb: 100,
        };
        let at = SeedBank {
            name: "x".into(),
            used_gb: 90,
            total_gb: 100,
        };
        let over = SeedBank {
            name: "x".into(),
            used_gb: 99,
            total_gb: 100,
        };
        assert!(!under.is_nearly_full());
        assert!(at.is_nearly_full());
        assert!(over.is_nearly_full());
    }

    #[test]
    fn from_storage_presence_copies_fields() {
        let presence = StoragePresence {
            name: "backup".into(),
            used_gb: 200,
            total_gb: 1000,
        };
        let bank = SeedBank::from(&presence);
        assert_eq!(bank.name, "backup");
        assert_eq!(bank.used_gb, 200);
        assert_eq!(bank.total_gb, 1000);
    }
}
