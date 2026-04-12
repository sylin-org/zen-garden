//! Unit tests for the `Catalog` aggregate.
//!
//! Ch3 of ARCH-0022 (Book V of ARCH-0017). Tests exercise the full
//! command + query + event surface against a real `Metrics` instance
//! and an in-memory `CatalogCache` stub. No mocks — the aggregate's
//! behaviour is identical in prod and under test.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use super::aggregate::Catalog;
use super::cache::CatalogCache;
use super::error::CatalogError;
use super::event::{CatalogChanged, ChangeKind, LoadSource};
use super::index::OfferingsIndex;
use crate::domain::Metrics;
use garden_common::manifests::ManifestRegistry;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ─── In-memory cache stub ─────────────────────────────────────────────

/// A test-only `CatalogCache` that stores the index in memory.
struct MemoryCatalogCache {
    inner: RwLock<Option<OfferingsIndex>>,
}

impl MemoryCatalogCache {
    fn new() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    fn with_index(index: OfferingsIndex) -> Self {
        Self {
            inner: RwLock::new(Some(index)),
        }
    }
}

impl CatalogCache for MemoryCatalogCache {
    fn load(&self) -> BoxFut<'_, Result<Option<OfferingsIndex>>> {
        Box::pin(async { Ok(self.inner.read().await.clone()) })
    }

    fn save<'a>(&'a self, cache: &'a OfferingsIndex) -> BoxFut<'a, Result<()>> {
        Box::pin(async move {
            *self.inner.write().await = Some(cache.clone());
            Ok(())
        })
    }
}

/// A cache that always fails on load.
struct FailingLoadCache;

impl CatalogCache for FailingLoadCache {
    fn load(&self) -> BoxFut<'_, Result<Option<OfferingsIndex>>> {
        Box::pin(async { Err(anyhow::anyhow!("disk read failed")) })
    }

    fn save<'a>(&'a self, _cache: &'a OfferingsIndex) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// A cache that always fails on save.
struct FailingSaveCache;

impl CatalogCache for FailingSaveCache {
    fn load(&self) -> BoxFut<'_, Result<Option<OfferingsIndex>>> {
        Box::pin(async { Ok(None) })
    }

    fn save<'a>(&'a self, _cache: &'a OfferingsIndex) -> BoxFut<'a, Result<()>> {
        Box::pin(async { Err(anyhow::anyhow!("disk write failed")) })
    }
}

// ─── Harness ──────────────────────────────────────────────────────────

fn empty_registry() -> Arc<ManifestRegistry> {
    Arc::new(ManifestRegistry {
        sw: garden_common::manifests::OfferingRegistry {
            entries: std::collections::HashMap::new(),
            categories: Vec::new(),
        },
        hw: garden_common::manifests::HwManifests {
            entries: std::collections::HashMap::new(),
            vendors: Vec::new(),
        },
    })
}

async fn fresh() -> Catalog {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));
    let cache: Arc<dyn CatalogCache> = Arc::new(MemoryCatalogCache::new());
    Catalog::new(empty_registry(), caps, cache, metrics).await
}

async fn fresh_with_metrics() -> (Catalog, Arc<Metrics>) {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));
    let cache: Arc<dyn CatalogCache> = Arc::new(MemoryCatalogCache::new());
    let catalog = Catalog::new(empty_registry(), caps, cache, metrics.clone()).await;
    (catalog, metrics)
}

// ─── load ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn load_populates_empty_catalog() {
    let catalog = fresh().await;
    assert!(!catalog.is_loaded().await);

    catalog.load().await.expect("load succeeds");

    assert!(catalog.is_loaded().await);
}

#[tokio::test]
async fn load_is_idempotent() {
    let catalog = fresh().await;
    catalog.load().await.unwrap();
    // Second load is a no-op — no error, no state change.
    catalog.load().await.unwrap();
    assert!(catalog.is_loaded().await);
}

#[tokio::test]
async fn load_emits_loaded_event_fresh_rebuild() {
    let catalog = fresh().await;
    let mut rx = catalog.changes();

    catalog.load().await.unwrap();

    match rx.recv().await.expect("event received") {
        CatalogChanged::Loaded {
            source,
            compiled_count,
            ..
        } => {
            assert_eq!(source, LoadSource::FreshRebuild);
            // Empty registry → 0 compiled offerings.
            assert_eq!(compiled_count, 0);
        }
        other => panic!("expected Loaded, got {:?}", other),
    }
}

#[tokio::test]
async fn load_from_disk_cache_when_fingerprint_matches() {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));

    // Pre-populate the cache with a matching fingerprint.
    // First, build a catalog to get the correct fingerprint.
    let bootstrap_cache: Arc<dyn CatalogCache> = Arc::new(MemoryCatalogCache::new());
    let bootstrap = Catalog::new(
        empty_registry(),
        caps.clone(),
        bootstrap_cache.clone(),
        metrics.clone(),
    )
    .await;
    bootstrap.load().await.unwrap();

    // Now create a new catalog pointing at the same cache.
    let catalog = Catalog::new(empty_registry(), caps, bootstrap_cache, metrics).await;
    let mut rx = catalog.changes();
    catalog.load().await.unwrap();

    match rx.recv().await.expect("event received") {
        CatalogChanged::Loaded { source, .. } => {
            assert_eq!(source, LoadSource::DiskCache);
        }
        other => panic!("expected Loaded from DiskCache, got {:?}", other),
    }
}

// ─── rebuild ──────────────────────────────────────────────────────────

#[tokio::test]
async fn rebuild_forces_recompile() {
    let catalog = fresh().await;
    // Load first so there's an existing state to compare.
    catalog.load().await.unwrap();

    let mut rx = catalog.changes();
    catalog.rebuild().await.unwrap();

    match rx.recv().await.expect("event received") {
        CatalogChanged::Rebuilt {
            compiled_count,
            fingerprint_changed,
            ..
        } => {
            assert_eq!(compiled_count, 0);
            // Same empty registry + same caps → fingerprint unchanged.
            assert!(!fingerprint_changed);
        }
        other => panic!("expected Rebuilt, got {:?}", other),
    }
}

#[tokio::test]
async fn rebuild_without_prior_load_sets_fingerprint_changed() {
    let catalog = fresh().await;
    let mut rx = catalog.changes();

    catalog.rebuild().await.unwrap();

    match rx.recv().await.expect("event received") {
        CatalogChanged::Rebuilt {
            fingerprint_changed,
            ..
        } => {
            // No prior state → fingerprint_changed = true.
            assert!(fingerprint_changed);
        }
        other => panic!("expected Rebuilt, got {:?}", other),
    }
}

// ─── queries ──────────────────────────────────────────────────────────

#[tokio::test]
async fn get_manifest_returns_none_for_empty_registry() {
    let catalog = fresh().await;
    assert!(catalog.get_manifest("nonexistent").is_none());
}

#[tokio::test]
async fn manifest_count_zero_for_empty_registry() {
    let catalog = fresh().await;
    assert_eq!(catalog.manifest_count(), 0);
}

#[tokio::test]
async fn get_compiled_returns_none_before_load() {
    let catalog = fresh().await;
    assert!(catalog.get_compiled("anything").await.is_none());
}

#[tokio::test]
async fn compiled_snapshot_returns_none_before_load() {
    let catalog = fresh().await;
    assert!(catalog.compiled_snapshot().await.is_none());
}

#[tokio::test]
async fn compiled_snapshot_returns_empty_vec_after_load_with_empty_registry() {
    let catalog = fresh().await;
    catalog.load().await.unwrap();

    let snap = catalog.compiled_snapshot().await.expect("loaded");
    assert!(snap.is_empty());
}

#[tokio::test]
async fn stats_before_and_after_load() {
    let catalog = fresh().await;

    let before = catalog.stats().await;
    assert_eq!(before.manifest_count, 0);
    assert_eq!(before.compiled_count, 0);
    assert!(before.fingerprint.is_none());

    catalog.load().await.unwrap();

    let after = catalog.stats().await;
    assert_eq!(after.manifest_count, 0);
    assert_eq!(after.compiled_count, 0);
    assert!(after.fingerprint.is_some());
}

#[tokio::test]
async fn find_hw_manifest_returns_none_for_empty_registry() {
    let catalog = fresh().await;
    assert!(
        catalog
            .find_hw_manifest(Some("Dell"), Some("Wyse 5070"))
            .is_none()
    );
}

// ─── error handling ───────────────────────────────────────────────────

#[tokio::test]
async fn load_returns_cache_read_error() {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));
    let cache: Arc<dyn CatalogCache> = Arc::new(FailingLoadCache);
    let catalog = Catalog::new(empty_registry(), caps, cache, metrics).await;

    let err = catalog.load().await.unwrap_err();
    assert!(matches!(err, CatalogError::CacheReadFailed(_)));
    assert!(!catalog.is_loaded().await);
}

#[tokio::test]
async fn load_returns_cache_write_error_on_rebuild_path() {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));
    let cache: Arc<dyn CatalogCache> = Arc::new(FailingSaveCache);
    let catalog = Catalog::new(empty_registry(), caps, cache, metrics).await;

    let err = catalog.load().await.unwrap_err();
    assert!(matches!(err, CatalogError::CacheWriteFailed(_)));
    // State should NOT be populated on save failure.
    assert!(!catalog.is_loaded().await);
}

#[tokio::test]
async fn rebuild_returns_cache_write_error() {
    let metrics = Arc::new(Metrics::new());
    let caps = Arc::new(RwLock::new(None));
    let cache: Arc<dyn CatalogCache> = Arc::new(FailingSaveCache);
    let catalog = Catalog::new(empty_registry(), caps, cache, metrics).await;

    let err = catalog.rebuild().await.unwrap_err();
    assert!(matches!(err, CatalogError::CacheWriteFailed(_)));
}

// ─── events + metrics integration ────────────────────────────────────

#[tokio::test]
async fn metrics_records_domain_event_on_load() {
    let (catalog, metrics) = fresh_with_metrics().await;
    catalog.load().await.unwrap();

    let snap = metrics
        .domain(Catalog::NAME)
        .await
        .expect("domain registered");
    assert_eq!(snap.events_total, 1);
    assert_eq!(snap.events_by_kind.get("loaded").copied(), Some(1));
}

#[tokio::test]
async fn metrics_records_domain_event_on_rebuild() {
    let (catalog, metrics) = fresh_with_metrics().await;
    catalog.rebuild().await.unwrap();

    let snap = metrics
        .domain(Catalog::NAME)
        .await
        .expect("domain registered");
    assert_eq!(snap.events_total, 1);
    assert_eq!(snap.events_by_kind.get("rebuilt").copied(), Some(1));
}

#[tokio::test]
async fn metrics_records_latency_on_mutation() {
    let (catalog, metrics) = fresh_with_metrics().await;
    catalog.load().await.unwrap();

    let snap = metrics
        .domain(Catalog::NAME)
        .await
        .expect("domain registered");
    assert!(snap.mutation_latency.count >= 1);
}

// ─── changes stream ──────────────────────────────────────────────────

#[tokio::test]
async fn changes_kind_loaded() {
    let catalog = fresh().await;
    let mut rx = catalog.changes();
    catalog.load().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.kind(), ChangeKind::Loaded);
}

#[tokio::test]
async fn changes_kind_rebuilt() {
    let catalog = fresh().await;
    let mut rx = catalog.changes();
    catalog.rebuild().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.kind(), ChangeKind::Rebuilt);
}
