//! Bank-scoped snapshot catalog.
//!
//! Per ORCH-0039, snapshots can be written either to local disk
//! or under a storage bank's mount via the `target=bank:<name>`
//! capture option. The per-FQN listing endpoint
//! (`GET /offerings/{fqn}/snapshots`) only sees the local
//! catalog. Pavilion's drag-canvas needs the inverse: given a
//! bank, list every seed living in it across all FQNs so the
//! bank card can render them as draggable chips.
//!
//! This endpoint walks `<bank_mount>/snapshots/<encoded_fqn>/`
//! for every encoded-FQN subdirectory the bank holds, loads each
//! snapshot's `manifest.json`, and returns a flat list keyed by
//! snapshot id with the `source_fqn` carried through. The walk
//! is bounded by the directory contents — banks rarely hold
//! thousands of seeds, and the manifest read is one small JSON
//! per seed.

use axum::extract::{Path, State};
use serde::Serialize;

use crate::Moss;
use crate::api::ApiResult;
use crate::domain::snapshot::{LocalSnapshotStore, SnapshotStore};
use crate::infra::api_helpers::{internal, not_found};

/// One seed in the catalog response — the minimum a UI needs to
/// render a draggable chip and dispatch a plant.
#[derive(Debug, Serialize)]
pub struct BankSeedEntry {
    pub snapshot_id: String,
    pub source_fqn: String,
    pub source_stone: String,
    pub source_event_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub size_total_bytes: u64,
}

/// `GET /api/v1/stone/banks/{bank_name}/seeds` response.
#[derive(Debug, Serialize)]
pub struct BankSeedsResponse {
    pub bank: String,
    pub count: usize,
    pub seeds: Vec<BankSeedEntry>,
}

/// `GET /api/v1/stone/banks/{bank_name}/seeds`
///
/// Walks the bank's `<mount>/snapshots/` directory, enumerating
/// every `<encoded_fqn>` subdirectory and the snapshots within.
/// Returns a flat list ordered by `created_at` descending —
/// newest seed first matches the canvas's most-likely-clicked
/// chip ordering.
///
/// 404 when the bank is unknown to this stone (no managed online
/// volume); empty list (200) when the bank exists but holds no
/// seeds yet.
pub async fn list_bank_seeds_v1(
    State(state): State<Moss>,
    Path(bank_name): Path<String>,
) -> ApiResult<BankSeedsResponse> {
    let bank = crate::domain::storage::bank_aggregate::by_name(
        &bank_name,
        &state.current.storage.volumes,
    )
    .await
    .ok_or_else(|| {
        not_found(
            "BANK_NOT_FOUND",
            format!("Bank '{bank_name}' has no managed online volume on this stone"),
        )
    })?;

    let mount = bank
        .mount_path
        .as_ref()
        .ok_or_else(|| {
            internal(
                "BANK_NOT_MOUNTED",
                format!("Bank '{bank_name}' has no mount path"),
            )
        })?
        .clone();
    let snapshots_root = mount.join("snapshots");

    let mut seeds: Vec<BankSeedEntry> = Vec::new();
    let mut fqn_dirs = match tokio::fs::read_dir(&snapshots_root).await {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Fresh bank that hasn't received any snapshot yet —
            // empty catalog is the correct answer, not an error.
            return crate::api::ok(BankSeedsResponse {
                bank: bank_name,
                count: 0,
                seeds: Vec::new(),
            });
        }
        Err(e) => {
            return Err(internal(
                "BANK_SCAN_FAILED",
                format!("read {}: {e}", snapshots_root.display()),
            ));
        }
    };

    while let Some(entry) = fqn_dirs.next_entry().await.map_err(|e| {
        internal(
            "BANK_SCAN_FAILED",
            format!("read_dir entry under {}: {e}", snapshots_root.display()),
        )
    })? {
        let metadata = match entry.file_type().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !metadata.is_dir() {
            continue;
        }
        let fqn_root = entry.path();
        let store = LocalSnapshotStore::new(fqn_root.clone());
        let ids = match store.list_ids().await {
            Ok(ids) => ids,
            Err(_) => continue, // Skip unreadable per-FQN catalogs;
            // surfacing one as a 500 would block the
            // rest of the bank's seeds.
        };
        for id in ids {
            match store.load_manifest(&id).await {
                Ok(m) => seeds.push(BankSeedEntry {
                    snapshot_id: m.id,
                    source_fqn: m.source_fqn,
                    source_stone: m.source_stone,
                    source_event_id: m.source_event_id,
                    created_at: m.created_at,
                    size_total_bytes: m.size_total_bytes,
                }),
                Err(_) => continue, // Manifest unreadable / missing — skip
            }
        }
    }

    // Newest first — matches how a user thinks about seeds
    // ("the most recent backup is the one I want").
    seeds.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let count = seeds.len();
    crate::api::ok(BankSeedsResponse {
        bank: bank_name,
        count,
        seeds,
    })
}
