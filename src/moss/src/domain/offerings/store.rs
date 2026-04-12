//! Persistence port for the Offerings aggregate.
//!
//! The aggregate depends on `Arc<dyn OfferingStore>` so it can be tested with
//! an in-memory fake and kept free of direct `crate::infra::*` calls.
//!
//! Uses the `Pin<Box<Future>>` pattern (same as `BackgroundTask` in ARCH-0015)
//! rather than `async-trait`, which was removed in ARCH-0007.

use anyhow::Result;
use garden_common::Offering;
use std::future::Future;
use std::pin::Pin;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port for persisting the merged offerings set (active + candidates).
pub trait OfferingStore: Send + Sync {
    /// Load all offerings from persistence. Caller splits into active /
    /// candidates by `Offering::is_adopted()`.
    fn load(&self) -> BoxFut<'_, Result<Vec<Offering>>>;

    /// Save the full merged offerings set. Called after every successful
    /// mutation by the aggregate's `finalize` step.
    fn save<'a>(&'a self, all: &'a [Offering]) -> BoxFut<'a, Result<()>>;
}

/// No-op `OfferingStore` for unit tests — load returns empty, save is a no-op.
pub struct NoopOfferingStore;

impl OfferingStore for NoopOfferingStore {
    fn load(&self) -> BoxFut<'_, Result<Vec<Offering>>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn save<'a>(&'a self, _all: &'a [Offering]) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// File-backed `OfferingStore` that delegates to the existing
/// `crate::infra::{load_offerings, save_offerings}` helpers.
pub struct FileOfferingStore;

impl OfferingStore for FileOfferingStore {
    fn load(&self) -> BoxFut<'_, Result<Vec<Offering>>> {
        Box::pin(async { crate::infra::load_offerings().await })
    }

    fn save<'a>(&'a self, all: &'a [Offering]) -> BoxFut<'a, Result<()>> {
        Box::pin(async move { crate::infra::save_offerings(all).await })
    }
}
