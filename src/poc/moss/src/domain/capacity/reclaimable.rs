//! The [`Reclaimable`] port and admission-control types (STORAGE-0020).
//!
//! The governor owns *policy* (when and how hard to reclaim); each domain
//! owns *mechanism* (what it is safe to delete and in what order) behind
//! this port. The governor never reaches into a domain's storage — it asks.

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;

use super::budget::ReclaimLevel;

/// How willingly the governor evicts from a consumer. Lower priority is
/// reclaimed first — pure junk before anything resembling real data.
///
/// Ordered: `Eager < Normal`. Consumers that hold live data or identity
/// (offering volumes, the pond keystone) are simply not registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReclaimPriority {
    /// Reclaimed first — leaked artifacts with no value (e.g. orphaned images).
    Eager,
    /// Reclaimed under genuine pressure — bounded but real backups.
    Normal,
}

/// What a single reclaimer freed in one pass. Byte figures are best-effort
/// estimates; the governor measures actual free space between reclaimers
/// rather than trusting these.
#[derive(Debug, Clone, Serialize)]
pub struct Reclaimed {
    /// The reclaimer's name.
    pub reclaimer: &'static str,
    /// Count of items removed (directories, images, …).
    pub items_removed: usize,
    /// Human-readable summary lines for the reclaim report / sweep history.
    pub notes: Vec<String>,
}

impl Reclaimed {
    /// A pass that freed nothing — the common, healthy case.
    pub fn none(reclaimer: &'static str) -> Self {
        Self {
            reclaimer,
            items_removed: 0,
            notes: Vec::new(),
        }
    }
}

/// A consumer of disk space the governor can ask to free some.
///
/// `dyn`-compatible: `reclaim` returns a boxed future rather than using
/// `async fn`, mirroring [`crate::tasks::task_trait::BackgroundTask`] so the
/// governor can hold `Vec<Arc<dyn Reclaimable>>` without an `async-trait`
/// dependency.
pub trait Reclaimable: Send + Sync {
    /// Stable name for logs, metrics, and reclaim reports.
    fn name(&self) -> &'static str;

    /// Eviction order. The governor reclaims `Eager` consumers first.
    fn priority(&self) -> ReclaimPriority;

    /// Free space, best-effort, at the requested aggressiveness. Must be
    /// idempotent and must never delete below the consumer's own floor.
    fn reclaim<'a>(
        &'a self,
        level: ReclaimLevel,
    ) -> Pin<Box<dyn Future<Output = Reclaimed> + Send + 'a>>;
}

/// A request to write a large amount of data, submitted to the governor
/// before the write begins.
#[derive(Debug, Clone)]
pub struct ReserveRequest {
    /// What is being written, for the denial message and tracing.
    pub purpose: String,
    /// Estimated write size in bytes, if known. Subtracted from available
    /// space before the floor check; `None` means "unknown, check current
    /// free space only".
    pub estimated_bytes: Option<u64>,
}

impl ReserveRequest {
    /// A reservation of unknown size for `purpose`.
    pub fn new(purpose: impl Into<String>) -> Self {
        Self {
            purpose: purpose.into(),
            estimated_bytes: None,
        }
    }
}

/// The governor's answer to a [`ReserveRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// There is room — proceed with the write.
    Allow,
    /// Disk pressure forbids the write; `reason` explains why.
    Deny { reason: String },
}

impl Verdict {
    /// Whether the write was allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allow)
    }

    /// Construct a denial with a formatted reason.
    pub fn deny(reason: impl Into<String>) -> Self {
        Verdict::Deny {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eager_reclaimed_before_normal() {
        let mut order = [ReclaimPriority::Normal, ReclaimPriority::Eager];
        order.sort();
        assert_eq!(order, [ReclaimPriority::Eager, ReclaimPriority::Normal]);
    }

    #[test]
    fn verdict_allow_is_allowed() {
        assert!(Verdict::Allow.is_allowed());
        assert!(!Verdict::deny("full").is_allowed());
    }
}
