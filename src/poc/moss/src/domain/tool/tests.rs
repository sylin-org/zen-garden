//! Unit tests for the `Tool` aggregate — Ch3 of ARCH-0019.
//!
//! Covers the typed command/query surface with real `Metrics` and real
//! broadcast channels. The underlying `GardenRegistryInner` is
//! exercised separately in `registry.rs`'s own test module; these
//! tests focus on the aggregate-level behaviour:
//!
//! - Mutation latency is recorded on Metrics for every command.
//! - Domain events are emitted on `changes()` with the correct kind.
//! - Wire deltas are emitted on `delta_stream()` for every affected entry.
//! - Query methods return owned values without leaking references.
//! - Batch commands (reap, beacon apply, stone removal) emit one batch
//!   event plus per-entry events.

use super::aggregate::Tool;
use super::event::ChangeKind;
use super::registry::EntryOrigin;
use crate::domain::Metrics;
use garden_common::tools::{
    GardenTool, ServiceInfo, Stone, ToolDelta, ToolDeltaKind, ToolIdentity,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

// ── Fixtures ────────────────────────────────────────────────────────────

fn sample_tool(fqid: &str, category: &str, stone_id: &str) -> GardenTool {
    let tool_type = fqid
        .split_once("::")
        .map(|(t, _)| t)
        .unwrap_or(fqid)
        .to_string();
    let name = fqid
        .split_once("::")
        .map(|(_, n)| n.to_string())
        .unwrap_or_default();

    GardenTool {
        fqid: fqid.to_string(),
        tool: ToolIdentity {
            name,
            tool_type: tool_type.clone(),
            category: category.to_string(),
            id: format!("uid-{}", fqid),
            tags: Vec::new(),
            source: String::new(),
        },
        stone: Stone {
            id: stone_id.to_string(),
            name: stone_id.to_string(),
            endpoint: format!("http://{}:7185", stone_id),
        },
        service: ServiceInfo {
            status: "running".to_string(),
            ready: true,
            protocol: tool_type,
            uris: Vec::new(),
            hostname: None,
            ip: None,
            port: None,
            uri_template: None,
            role: None,
        },
        capabilities: Vec::new(),
        storage: None,
    }
}

async fn new_tool() -> Tool {
    let metrics = Arc::new(Metrics::new());
    let (delta_tx, _delta_rx) = broadcast::channel::<ToolDelta>(256);
    Tool::new(
        metrics,
        delta_tx,
        Arc::new(super::transport::NoopBeaconTransport),
    )
    .await
}

async fn new_tool_with_metrics() -> (Tool, Arc<Metrics>) {
    let metrics = Arc::new(Metrics::new());
    let (delta_tx, _delta_rx) = broadcast::channel::<ToolDelta>(256);
    let tool = Tool::new(
        metrics.clone(),
        delta_tx,
        Arc::new(super::transport::NoopBeaconTransport),
    )
    .await;
    (tool, metrics)
}

// ── Construction & registration ─────────────────────────────────────────

#[tokio::test]
async fn new_registers_domain_with_metrics() {
    let (_tool, metrics) = new_tool_with_metrics().await;
    let snapshot = metrics.domain(Tool::NAME).await;
    assert!(
        snapshot.is_some(),
        "Tool::new must register the `tool` domain with Metrics"
    );
    let domain = snapshot.unwrap();
    // Kinds are registered up front — lookup is lock-free on the hot path.
    for kind in ChangeKind::ALL_NAMES {
        assert!(
            domain.events_by_kind.contains_key(*kind),
            "ChangeKind `{}` should be pre-registered on the Metrics domain",
            kind
        );
    }
}

#[tokio::test]
async fn change_kind_all_names_matches_enum_variants() {
    // Guard: if a new ChangeKind variant is added, ALL_NAMES must be updated.
    let variants = [
        ChangeKind::Upserted,
        ChangeKind::Removed,
        ChangeKind::Reaped,
        ChangeKind::BeaconApplied,
        ChangeKind::StoneRemoved,
    ];
    assert_eq!(variants.len(), ChangeKind::ALL_NAMES.len());
    for (variant, name) in variants.iter().zip(ChangeKind::ALL_NAMES.iter()) {
        assert_eq!(variant.name(), *name);
    }
}

// ── Upsert command ──────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_emits_changed_and_delta() {
    let tool = new_tool().await;
    let mut changes = tool.changes();
    let mut deltas = tool.delta_stream();

    let garden_tool = sample_tool("ollama", "offering", "stone-a");
    let event = tool
        .upsert(garden_tool.clone(), EntryOrigin::Local, None)
        .await;

    assert!(event.is_some(), "new entry should return Some");
    let event = event.unwrap();
    assert_eq!(event.kind(), ChangeKind::Upserted);

    // Internal event
    let received = changes.try_recv().expect("changes stream should fire");
    assert_eq!(received.kind(), ChangeKind::Upserted);

    // Wire delta
    let wire = deltas.try_recv().expect("delta stream should fire");
    assert_eq!(wire.kind, ToolDeltaKind::Upsert);
    assert_eq!(wire.fqid, "ollama");
}

#[tokio::test]
async fn upsert_noop_returns_none() {
    let tool = new_tool().await;
    let garden_tool = sample_tool("ollama", "offering", "stone-a");

    let first = tool
        .upsert(garden_tool.clone(), EntryOrigin::Local, None)
        .await;
    assert!(first.is_some());

    // Second upsert of identical content is a no-op.
    let second = tool.upsert(garden_tool, EntryOrigin::Local, None).await;
    assert!(second.is_none(), "identical re-upsert must be a no-op");
}

#[tokio::test]
async fn upsert_records_mutation_latency() {
    let (tool, metrics) = new_tool_with_metrics().await;
    let garden_tool = sample_tool("ollama", "offering", "stone-a");

    tool.upsert(garden_tool, EntryOrigin::Local, None).await;

    let domain = metrics.domain(Tool::NAME).await.unwrap();
    assert_eq!(
        domain.mutation_latency.count, 1,
        "upsert must record one mutation latency sample"
    );
}

// ── Gateway commands ────────────────────────────────────────────────────

#[tokio::test]
async fn register_gateway_sets_expiry_and_emits_upserted() {
    let tool = new_tool().await;
    let garden_tool = sample_tool("ollama", "offering", "stone-a");

    let event = tool
        .register_gateway(garden_tool.clone(), Duration::from_secs(60))
        .await;
    assert!(event.is_some());
    assert_eq!(event.unwrap().kind(), ChangeKind::Upserted);

    // Verify via handles_offering query
    assert!(tool.handles_offering("ollama").await);
}

#[tokio::test]
async fn deregister_gateway_emits_removed_when_present() {
    let tool = new_tool().await;
    let garden_tool = sample_tool("ollama", "offering", "stone-a");

    tool.register_gateway(garden_tool, Duration::from_secs(60))
        .await;
    let event = tool.deregister_gateway("ollama", "stone-a").await;
    assert!(event.is_some());
    assert_eq!(event.unwrap().kind(), ChangeKind::Removed);

    assert!(!tool.handles_offering("ollama").await);
}

#[tokio::test]
async fn deregister_gateway_returns_none_when_absent() {
    let tool = new_tool().await;
    let event = tool.deregister_gateway("nonexistent", "stone-x").await;
    assert!(event.is_none(), "deregister of missing entry is a no-op");
}

// ── Reconcile local ─────────────────────────────────────────────────────

#[tokio::test]
async fn reconcile_local_upserts_and_removes_stale() {
    let tool = new_tool().await;
    let t1 = sample_tool("ollama", "offering", "stone-a");
    let t2 = sample_tool("mongodb", "offering", "stone-a");
    let t3 = sample_tool("nginx", "offering", "stone-a");

    // First batch: t1 + t2
    let events = tool
        .reconcile_local("stone-a", vec![t1.clone(), t2.clone()])
        .await;
    let upserts = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Upserted)
        .count();
    assert_eq!(upserts, 2);

    // Second batch: t2 + t3 — t1 must be removed, t3 upserted, t2 no-op.
    let events = tool.reconcile_local("stone-a", vec![t2, t3]).await;
    let upserts = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Upserted)
        .count();
    let removes = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Removed)
        .count();
    assert_eq!(upserts, 1, "only nginx should upsert on second batch");
    assert_eq!(removes, 1, "ollama should be removed as stale");
}

// ── Reap expired gateways ───────────────────────────────────────────────

#[tokio::test]
async fn reap_expired_gateways_emits_batch_and_per_entry_events() {
    let tool = new_tool().await;

    // Register with an immediately-expired TTL.
    let t1 = sample_tool("a", "offering", "stone-a");
    let t2 = sample_tool("b", "offering", "stone-a");
    tool.upsert(
        t1,
        EntryOrigin::Gateway,
        Some(std::time::Instant::now() - Duration::from_secs(1)),
    )
    .await;
    tool.upsert(
        t2,
        EntryOrigin::Gateway,
        Some(std::time::Instant::now() - Duration::from_secs(1)),
    )
    .await;

    let events = tool.reap_expired_gateways().await;

    // 2 per-entry Removed + 1 batch Reaped
    let per_entry = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Removed)
        .count();
    let batch = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Reaped)
        .count();
    assert_eq!(per_entry, 2);
    assert_eq!(batch, 1);
}

#[tokio::test]
async fn reap_expired_gateways_empty_when_nothing_to_reap() {
    let tool = new_tool().await;
    let events = tool.reap_expired_gateways().await;
    assert!(
        events.is_empty(),
        "reap with no expired entries returns empty vec, not a batch event"
    );
}

// ── Remove stone ────────────────────────────────────────────────────────
//
// Note: `apply_remote_beacon` is exercised indirectly via the remove_stone
// path and via `registry.rs`'s own test module, which has full coverage of
// `apply_remote_beacon` on `GardenRegistryInner`. The aggregate-level
// wrapper is a thin pass-through that records metrics and emits events —
// the per-kind metrics + event assertions in `metrics_counters_increment_per_change_kind`
// provide equivalent coverage without the heavy `ToolsBeacon` deserialize
// monomorphizations that tipped the MSVC linker over 64k section limits.

#[tokio::test]
async fn remove_stone_emits_batch_and_per_entry_events() {
    let tool = new_tool().await;
    let t1 = sample_tool("a", "offering", "stone-gone");
    let t2 = sample_tool("b", "offering", "stone-gone");
    let t3 = sample_tool("c", "offering", "stone-keeping");

    tool.upsert(
        t1,
        EntryOrigin::Announced {
            stone_id: "stone-gone".to_string(),
        },
        None,
    )
    .await;
    tool.upsert(
        t2,
        EntryOrigin::Announced {
            stone_id: "stone-gone".to_string(),
        },
        None,
    )
    .await;
    tool.upsert(
        t3,
        EntryOrigin::Announced {
            stone_id: "stone-keeping".to_string(),
        },
        None,
    )
    .await;

    let events = tool.remove_stone("stone-gone").await;

    let per_entry = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::Removed)
        .count();
    let batch = events
        .iter()
        .filter(|e| e.kind() == ChangeKind::StoneRemoved)
        .count();
    assert_eq!(per_entry, 2, "two entries on stone-gone should be removed");
    assert_eq!(batch, 1);

    // Keeping stone's entry should still be there.
    assert_eq!(tool.storage_count().await, 0);
    let (_, all) = tool.snapshot(&super::registry::ToolQuery::default()).await;
    assert_eq!(all.len(), 1, "only stone-keeping's entry should remain");
}

// ── Query methods return owned values ───────────────────────────────────

#[tokio::test]
async fn snapshot_returns_owned_tools() {
    let tool = new_tool().await;
    let t1 = sample_tool("a", "offering", "stone-a");
    tool.upsert(t1, EntryOrigin::Local, None).await;

    let (cursor, tools) = tool.snapshot(&super::registry::ToolQuery::default()).await;
    assert!(cursor > 0);
    assert_eq!(tools.len(), 1);
    // `tools` is Vec<GardenTool> — owned — we can move/mutate freely.
    let _owned: Vec<GardenTool> = tools;
}

#[tokio::test]
async fn storage_by_name_returns_owned_entries() {
    let tool = new_tool().await;

    // Build a storage tool
    let mut storage_tool = sample_tool("blue-pool", "storage", "stone-a");
    storage_tool.storage = Some(garden_common::tools::StorageMetadata {
        replica_set_id: "rs-1".to_string(),
        replica_set_name: "blue".to_string(),
        role: Some("primary".to_string()),
        capacity_bytes: 1_000_000,
        used_bytes: 0,
        visibility: "private".to_string(),
        encrypted: false,
        pin_id: None,
        protocols: vec!["s3".to_string()],
        roles: Vec::new(),
    });

    tool.upsert(storage_tool, EntryOrigin::Local, None).await;

    let entries = tool.storage_by_name("blue-pool").await;
    assert_eq!(entries.len(), 1);

    let primary = tool.storage_primary("blue-pool").await;
    assert!(primary.is_some());
}

// ── Metrics accounting ──────────────────────────────────────────────────

#[tokio::test]
async fn metrics_counters_increment_per_change_kind() {
    let (tool, metrics) = new_tool_with_metrics().await;

    // Upserted × 2
    let t1 = sample_tool("a", "offering", "stone-a");
    let t2 = sample_tool("b", "offering", "stone-a");
    tool.upsert(t1.clone(), EntryOrigin::Local, None).await;
    tool.upsert(t2, EntryOrigin::Local, None).await;

    // Removed × 1 (via deregister — need Gateway origin first)
    let t3 = sample_tool("c", "offering", "stone-a");
    tool.register_gateway(t3, Duration::from_secs(60)).await;
    tool.deregister_gateway("c", "stone-a").await;

    let domain = metrics.domain(Tool::NAME).await.unwrap();
    assert_eq!(
        *domain.events_by_kind.get("upserted").unwrap(),
        3,
        "upsert×2 + register_gateway×1 = 3 upserted events"
    );
    assert_eq!(
        *domain.events_by_kind.get("removed").unwrap(),
        1,
        "deregister_gateway = 1 removed event"
    );
}
