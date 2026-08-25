//! `ChirpTransport` port for the Topology aggregate.
//!
//! Wraps the UDP `STONE_CHIRP` announcement path so the aggregate can
//! fire a chirp without importing `crate::announcement::*` directly.
//! Per ARCH-0020 (Book III of ARCH-0017).

use anyhow::Result;
use garden_common::TopologyEntry;
use std::future::Future;
use std::pin::Pin;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port for broadcasting a `STONE_CHIRP` announcement carrying this
/// stone's current topology entry.
pub trait ChirpTransport: Send + Sync {
    /// Broadcast a single chirp. Implementations should not block the
    /// calling task on network I/O beyond a bounded timeout.
    fn chirp<'a>(&'a self, entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>>;
}

/// Production adapter — wraps the existing `crate::announcement::announce`
/// free function, which sends via `garden_common::infra::communications::p2p`.
#[derive(Debug, Default, Clone, Copy)]
pub struct P2pChirpTransport;

impl ChirpTransport for P2pChirpTransport {
    fn chirp<'a>(&'a self, entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>> {
        Box::pin(async move { crate::announcement::announce(entry).await })
    }
}

/// No-op transport for unit tests where chirp I/O is out of scope.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopChirpTransport;

impl ChirpTransport for NoopChirpTransport {
    fn chirp<'a>(&'a self, _entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}
