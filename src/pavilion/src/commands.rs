//! Tauri commands exposed to the React frontend.
//!
//! Discovery is push-driven (chirp subscription emits `topology-changed`
//! events). Tile fetches against the tended stone (`get_services`,
//! `get_pond_status`) are pull-on-demand — the frontend invokes them
//! on mount and again whenever it sees a `tending-changed` event.

use std::sync::Arc;

use garden_common::storage::GardenStorageSummary;
use serde::Serialize;
use tauri::State;

use crate::announce::{ActivityEntry, ActivityStore};
use crate::awareness::{AwareStone, Awareness};
use crate::connection;
use crate::tending::{TendedStone, Tending};

// ── Awareness / tending ─────────────────────────────────────────────

/// Snapshot of currently-aware stones (those that chirped within the
/// last 90s). Frontend calls this on mount to render initial state;
/// thereafter it listens for `topology-changed` events for updates.
#[tauri::command]
pub async fn get_topology(
    awareness: State<'_, Arc<Awareness>>,
) -> Result<Vec<AwareStone>, String> {
    Ok(awareness.snapshot().await)
}

/// Currently-tended stone, if any. Frontend calls on mount; listens
/// for `tending-changed` thereafter.
#[tauri::command]
pub async fn get_tended(
    tending: State<'_, Arc<Tending>>,
) -> Result<Option<TendedStone>, String> {
    Ok(tending.current().await)
}

/// Explicitly tend a stone (user action — for instance, picking from a
/// stone selector in the UI). The stone must be in current awareness.
#[tauri::command]
pub async fn set_tended(
    stone_id: String,
    awareness: State<'_, Arc<Awareness>>,
    tending: State<'_, Arc<Tending>>,
) -> Result<(), String> {
    let snap = awareness.snapshot().await;
    let stone = snap
        .iter()
        .find(|s| s.stone_id == stone_id)
        .ok_or_else(|| format!("stone '{stone_id}' is not currently in awareness"))?;
    tending.set(stone).await;
    Ok(())
}

// ── Tended-stone API fetches ────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ServiceLite {
    pub name: String,
    pub offering: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServicesPayload {
    pub count: usize,
    pub services: Vec<ServiceLite>,
}

/// Fetch the services list from the currently-tended stone.
/// Returns `Ok(None)` when no stone is tended.
#[tauri::command]
pub async fn get_services(
    tending: State<'_, Arc<Tending>>,
) -> Result<Option<ServicesPayload>, String> {
    let Some(tended) = tending.current().await else {
        return Ok(None);
    };
    let api = connection::api_for(&tended);
    let services = api
        .services()
        .list()
        .await
        .map_err(|e| format!("services fetch from {}: {e}", tended.endpoint))?;
    Ok(Some(ServicesPayload {
        count: services.len(),
        services: services
            .into_iter()
            .map(|s| ServiceLite {
                name: s.name,
                offering: s.offering,
                // FoundService.status is already a lowercase string
                // ("running", "stopped", "degraded") so no Debug
                // formatting is needed.
                status: s.status,
            })
            .collect(),
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct PondPayload {
    /// Whether the pond exists and is reachable on the tended stone.
    pub initialised: bool,
    /// `active`, `locked`, `inactive`, etc. — passed through from Moss.
    pub status: String,
    /// Pond display name (`pond-foo-bar`), if set.
    pub name: Option<String>,
    /// Number of enrolled members, if reported.
    pub member_count: Option<usize>,
    /// Cornerstone stone-name, if reported.
    pub cornerstone: Option<String>,
}

/// Fetch pond status from the currently-tended stone. Pond endpoints
/// return a free-form JSON shape (not yet typed in `garden-common`)
/// so we extract a minimum viable subset for the tile.
#[tauri::command]
pub async fn get_pond_status(
    tending: State<'_, Arc<Tending>>,
) -> Result<Option<PondPayload>, String> {
    let Some(tended) = tending.current().await else {
        return Ok(None);
    };
    let api = connection::api_for(&tended);
    let raw = match api.pond().status().await {
        Ok(v) => v,
        Err(e) => {
            // 404 is "no pond on this stone" — don't surface as an error.
            if e.is_not_found() {
                return Ok(Some(PondPayload {
                    initialised: false,
                    status: "uninitialised".into(),
                    name: None,
                    member_count: None,
                    cornerstone: None,
                }));
            }
            return Err(format!("pond status fetch from {}: {e}", tended.endpoint));
        }
    };

    let status = raw
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let initialised = raw
        .get("initialised")
        .or_else(|| raw.get("initialized"))
        .or_else(|| raw.get("active"))
        .and_then(|v| v.as_bool())
        .unwrap_or(status != "uninitialised" && status != "unknown");
    let name = raw
        .get("name")
        .or_else(|| raw.get("pond_name"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let member_count = raw
        .get("members")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .or_else(|| {
            raw.get("member_count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
        });
    let cornerstone = raw
        .get("cornerstone")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(Some(PondPayload {
        initialised,
        status,
        name,
        member_count,
        cornerstone,
    }))
}

#[derive(Debug, Clone, Serialize)]
pub struct StoragePayload {
    pub count: usize,
    pub banks: Vec<GardenStorageSummary>,
}

/// Snapshot of the in-memory Activity ring buffer. Newest first.
/// Frontend calls on mount and listens for an `activity-changed` event
/// thereafter (event wiring is a follow-up — for v1 the frontend may
/// poll on demand).
#[tauri::command]
pub async fn get_activity(
    store: State<'_, ActivityStore>,
) -> Result<Vec<ActivityEntry>, String> {
    Ok(store.snapshot().await)
}

/// Fetch the garden-wide storage summary from the currently-tended
/// stone. Tended Moss aggregates local volumes with registry beacons,
/// so a single call surfaces every bank visible to this garden.
/// Returns `Ok(None)` when no stone is tended.
#[tauri::command]
pub async fn get_storage(
    tending: State<'_, Arc<Tending>>,
) -> Result<Option<StoragePayload>, String> {
    let Some(tended) = tending.current().await else {
        return Ok(None);
    };
    let api = connection::api_for(&tended);
    let banks = api
        .garden()
        .storage()
        .list()
        .await
        .map_err(|e| format!("storage fetch from {}: {e}", tended.endpoint))?;
    Ok(Some(StoragePayload {
        count: banks.len(),
        banks,
    }))
}
