//! `ActivityStore` — bounded ring buffer feeding the Activity view.
//!
//! Entries arrive from the Announcer policy layer post-coalesce.
//! Capacity is fixed at construction time; the oldest entry falls off
//! when a new one would exceed it. Reads are cheap (a `Vec` clone of
//! the live deque); writes hold the lock briefly.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::event::ActivityEntry;

/// Default ring-buffer capacity. The Activity view shows recent
/// events; older history lives in Moss's persistent journals, not
/// in Pavilion's memory.
pub const DEFAULT_CAPACITY: usize = 200;

#[derive(Clone)]
pub struct ActivityStore {
    inner: Arc<RwLock<VecDeque<ActivityEntry>>>,
    capacity: usize,
}

impl ActivityStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    /// Append a new entry, dropping the oldest when over capacity.
    pub async fn push(&self, entry: ActivityEntry) {
        let mut buf = self.inner.write().await;
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// Snapshot the buffer in newest-first order. Used by the
    /// `get_activity` Tauri command and any test assertions.
    pub async fn snapshot(&self) -> Vec<ActivityEntry> {
        let buf = self.inner.read().await;
        buf.iter().rev().cloned().collect()
    }
}

impl Default for ActivityStore {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::announce::event::{GardenEvent, Severity};
    use chrono::Utc;

    fn entry(id: &str) -> ActivityEntry {
        ActivityEntry {
            id: id.to_string(),
            at: Utc::now(),
            event: GardenEvent::StoneJoined {
                stone_id: id.to_string(),
                stone_name: format!("stone-{id}"),
                endpoint: "http://localhost:7185".into(),
            },
            severity: Severity::Notice,
            promoted: false,
        }
    }

    #[tokio::test]
    async fn snapshot_is_newest_first() {
        let store = ActivityStore::new(8);
        store.push(entry("a")).await;
        store.push(entry("b")).await;
        store.push(entry("c")).await;
        let snap = store.snapshot().await;
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].id, "c");
        assert_eq!(snap[2].id, "a");
    }

    #[tokio::test]
    async fn capacity_drops_oldest() {
        let store = ActivityStore::new(2);
        store.push(entry("a")).await;
        store.push(entry("b")).await;
        store.push(entry("c")).await;
        let snap = store.snapshot().await;
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].id, "c");
        assert_eq!(snap[1].id, "b");
    }
}
