//! Transport port for the Tool aggregate.
//!
//! The aggregate depends on `Arc<dyn ToolsBeaconTransport>` to publish
//! UDP tools beacons so it can be tested with an in-memory fake and
//! kept free of direct `crate::infra::*` calls.
//!
//! Uses the `Pin<Box<Future>>` pattern (same as `OfferingStore` and
//! `BackgroundTask` in ARCH-0015) rather than `async-trait`, which was
//! removed in ARCH-0007.

use anyhow::Result;
use garden_common::tools::ToolDelta;
use std::future::Future;
use std::pin::Pin;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port for publishing tool-delta announcements over the garden's UDP
/// transport (`TOOLS_BEACON` announcements).
///
/// The aggregate calls `broadcast_incremental` after every command
/// that produces non-empty deltas. `broadcast_snapshot` is used by
/// bootstrap / announcer paths (Ch5) that want to publish an
/// authoritative full set to remote stones — receivers reconcile any
/// previously-announced entries from this stone that are absent from
/// the snapshot.
pub trait ToolsBeaconTransport: Send + Sync {
    /// Publish an incremental beacon carrying the given deltas. Caller
    /// guarantees `deltas` is non-empty. Implementations must not block
    /// the calling task on network I/O beyond a bounded timeout.
    fn broadcast_incremental<'a>(
        &'a self,
        stone_id: &'a str,
        stone_name: &'a str,
        endpoint: &'a str,
        deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>>;

    /// Publish a snapshot beacon (authoritative full set). Empty
    /// snapshots are valid — they tell receivers "I have no entries,
    /// clear what you've learned about me."
    fn broadcast_snapshot<'a>(
        &'a self,
        stone_id: &'a str,
        stone_name: &'a str,
        endpoint: &'a str,
        deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>>;
}

/// No-op transport used in unit tests where beacon I/O is out of scope.
///
/// Dropping a delta is never the right production behaviour; tests that
/// need to observe outbound beacons should use a recording fake
/// instead.
pub struct NoopBeaconTransport;

impl ToolsBeaconTransport for NoopBeaconTransport {
    fn broadcast_incremental<'a>(
        &'a self,
        _stone_id: &'a str,
        _stone_name: &'a str,
        _endpoint: &'a str,
        _deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn broadcast_snapshot<'a>(
        &'a self,
        _stone_id: &'a str,
        _stone_name: &'a str,
        _endpoint: &'a str,
        _deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}
