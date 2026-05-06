//! Tauri commands exposed to the React frontend.
//!
//! Discovery is push-driven (chirp subscription emits `topology-changed`
//! events). Tile fetches against the tended stone (`get_services`,
//! `get_pond_status`) are pull-on-demand — the frontend invokes them
//! on mount and again whenever it sees a `tending-changed` event.

use std::sync::Arc;

use garden_common::storage::GardenStorageSummary;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::announce::{ActivityEntry, ActivityStore};
use crate::awareness::{AwareStone, Awareness};
use crate::connection;
use crate::facilitators::{FacilitatorEngine, Suggestion};
use crate::settings::{Settings, SettingsPatch, SettingsStore};
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

// ── Settings ────────────────────────────────────────────────────────

/// Current settings snapshot. The frontend calls this on mount and
/// listens for `settings-changed` events thereafter.
#[tauri::command]
pub async fn get_settings(
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<Settings, String> {
    Ok(settings.snapshot().await)
}

/// Apply a partial update and return the new snapshot. The store
/// persists to disk and emits `settings-changed` for any other
/// frontend listener and for Rust-side subscribers.
#[tauri::command]
pub async fn set_settings(
    patch: SettingsPatch,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<Settings, String> {
    Ok(settings.apply_patch(patch).await)
}

// ── Service lifecycle actions ───────────────────────────────────────
//
// These thin wrappers turn the typed StoneApi service-control
// methods into Tauri commands. Each maps to a POST on the tended
// stone (`/api/v1/stone/services/{name}/{wake,rest,restart}`) and
// returns `Ok(())` on a 2xx, surfacing the body as `Err(String)`
// otherwise so the frontend can show a meaningful failure.

async fn run_service_action(
    tending: &Tending,
    name: &str,
    op: ServiceOp,
) -> Result<(), String> {
    let Some(tended) = tending.current().await else {
        return Err("no stone tended".to_string());
    };
    let api = connection::api_for(&tended);
    let services = api.services();
    let resp = match op {
        ServiceOp::Wake => services.wake(name).await,
        ServiceOp::Rest => services.rest(name).await,
        ServiceOp::Restart => services.restart(name).await,
    };
    let resp = resp.map_err(|e| format!("{op:?} {name} on {}: {e}", tended.endpoint))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("{op:?} {name}: HTTP {status} {body}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ServiceOp {
    Wake,
    Rest,
    Restart,
}

#[tauri::command]
pub async fn restart_service(
    name: String,
    tending: State<'_, Arc<Tending>>,
) -> Result<(), String> {
    run_service_action(&tending, &name, ServiceOp::Restart).await
}

#[tauri::command]
pub async fn rest_service(
    name: String,
    tending: State<'_, Arc<Tending>>,
) -> Result<(), String> {
    run_service_action(&tending, &name, ServiceOp::Rest).await
}

#[tauri::command]
pub async fn wake_service(
    name: String,
    tending: State<'_, Arc<Tending>>,
) -> Result<(), String> {
    run_service_action(&tending, &name, ServiceOp::Wake).await
}

// ── Window control ─────────────────────────────────────────────────

/// Show the main Pavilion window (focused, restored from minimized).
/// Used by the tray popover's "Open Pavilion" CTA: the popover hides
/// itself on focus loss after this call brings the main window
/// forward, so there's no overlap window.
#[tauri::command]
pub async fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager as _;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

// ── Facilitators ───────────────────────────────────────────────────

/// Current active facilitator suggestion, if any. Frontend calls
/// on mount and listens for `suggestion-changed` events thereafter.
#[tauri::command]
pub async fn get_suggestion(
    engine: State<'_, FacilitatorEngine>,
) -> Result<Option<Suggestion>, String> {
    Ok(engine.current().await)
}

/// Dismiss the given suggestion id for the current session. The
/// engine recomputes immediately so the banner either disappears
/// or the next-priority suggestion takes its place.
#[tauri::command]
pub async fn dismiss_suggestion(
    id: String,
    engine: State<'_, FacilitatorEngine>,
) -> Result<(), String> {
    engine.dismiss_for_session(&id).await;
    engine.recompute().await;
    Ok(())
}

/// Dismiss a whole suggestion kind permanently — adds the kind to
/// `Settings::suppressed_kinds`. The settings change triggers an
/// engine recompute via the supervisor's settings watch.
#[tauri::command]
pub async fn hide_suggestion_kind(
    kind: String,
    settings: State<'_, Arc<SettingsStore>>,
) -> Result<Settings, String> {
    let current = settings.snapshot().await;
    if current.suppressed_kinds.iter().any(|k| k == &kind) {
        return Ok(current);
    }
    let mut next = current.suppressed_kinds.clone();
    next.push(kind);
    let patch = SettingsPatch {
        suppressed_kinds: Some(next),
        ..Default::default()
    };
    Ok(settings.apply_patch(patch).await)
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

// ── Snapshots / Plant (ORCH-0039) ──────────────────────────────

/// Result of `capture_snapshot` — the snapshot id the user can
/// reference later, the event_id that recorded the BackupTaken,
/// and totals for toast feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSnapshotResult {
    pub snapshot_id: String,
    pub event_id: String,
    pub source_fqn: String,
    pub source_stone: String,
    pub size_total_bytes: u64,
    pub volumes: usize,
    pub external_mounts: usize,
}

/// Trigger a snapshot capture for `fqn` on the named `stone`.
/// `target` follows the wire form: `local` (default) or
/// `bank:<bank_name>`. Resolves the stone's endpoint via the
/// awareness cache so the canvas can drag-from-stone-to-bank
/// without first tending the source.
#[tauri::command]
pub async fn capture_snapshot(
    stone: String,
    fqn: String,
    target: Option<String>,
    awareness: State<'_, Arc<Awareness>>,
    tending: State<'_, Arc<Tending>>,
) -> Result<CaptureSnapshotResult, String> {
    let endpoint = resolve_endpoint(&stone, &awareness, &tending)
        .await
        .ok_or_else(|| format!("stone '{stone}' not in awareness or tending"))?;

    let client = connection::raw_client_for_capture();
    let url = format!(
        "{}/api/v1/stone/offerings/{}/snapshots",
        endpoint.trim_end_matches('/'),
        encode_uri_segment(&fqn)
    );
    let body = serde_json::json!({
        "target": target.unwrap_or_else(|| "local".to_string()),
    });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("capture POST {url}: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("capture {status}: {text}"));
    }
    let parsed: ApiEnvelope<CaptureSnapshotResult> =
        resp.json().await.map_err(|e| format!("capture parse: {e}"))?;
    Ok(parsed.data)
}

/// Trigger a plant from a snapshot. `from_stone` is required —
/// the canvas drags a seed from a bank node onto a stone, and
/// the drop target stone tells us where to plant; we need the
/// snapshot's source stone to fetch from (which is `from_stone`).
/// `target_stone` is where the plant lands.
#[tauri::command]
pub async fn plant_snapshot(
    target_stone: String,
    target_fqn: String,
    from_snapshot: String,
    from_stone: Option<String>,
    from_fqn: Option<String>,
    as_fqn: Option<String>,
    awareness: State<'_, Arc<Awareness>>,
    tending: State<'_, Arc<Tending>>,
) -> Result<PlantSnapshotResult, String> {
    let endpoint = resolve_endpoint(&target_stone, &awareness, &tending)
        .await
        .ok_or_else(|| format!("target stone '{target_stone}' not in awareness or tending"))?;

    let client = connection::raw_client_for_capture();
    let url = format!(
        "{}/api/v1/stone/offerings/{}/plant",
        endpoint.trim_end_matches('/'),
        encode_uri_segment(&target_fqn)
    );
    let mut body = serde_json::json!({
        "from_snapshot": from_snapshot,
    });
    if let Some(s) = from_stone {
        body["from_stone"] = serde_json::Value::String(s);
    }
    if let Some(f) = from_fqn {
        body["from_fqn"] = serde_json::Value::String(f);
    }
    if let Some(a) = as_fqn {
        body["as_fqn"] = serde_json::Value::String(a);
    }
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("plant POST {url}: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("plant {status}: {text}"));
    }
    let parsed: ApiEnvelope<PlantSnapshotResult> =
        resp.json().await.map_err(|e| format!("plant parse: {e}"))?;
    Ok(parsed.data)
}

/// One seed entry returned by `list_seeds_in_bank`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankSeedEntry {
    pub snapshot_id: String,
    pub source_fqn: String,
    pub source_stone: String,
    pub source_event_id: String,
    pub created_at: String,
    pub size_total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankSeedsResult {
    pub bank: String,
    pub count: usize,
    pub seeds: Vec<BankSeedEntry>,
}

/// List every seed living in `bank_name` across all FQNs that
/// have ever captured into it. The frontend uses this to render
/// draggable seed chips on the bank's detail card.
#[tauri::command]
pub async fn list_seeds_in_bank(
    bank_name: String,
    tending: State<'_, Arc<Tending>>,
) -> Result<BankSeedsResult, String> {
    // The bank-snapshots endpoint lives on each stone — we query
    // the tended stone since that's the one the user is operating
    // through. A stone that doesn't hold the bank's volume locally
    // returns 404; we surface that as an empty list so the canvas
    // can still render something useful.
    let Some(tended) = tending.current().await else {
        return Err("no stone tended".to_string());
    };
    let client = connection::raw_client_for_capture();
    let url = format!(
        "{}/api/v1/stone/banks/{}/seeds",
        tended.endpoint.trim_end_matches('/'),
        encode_uri_segment(&bank_name)
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("seeds GET {url}: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(BankSeedsResult {
            bank: bank_name,
            count: 0,
            seeds: Vec::new(),
        });
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("seeds {status}: {text}"));
    }
    let parsed: ApiEnvelope<BankSeedsResult> =
        resp.json().await.map_err(|e| format!("seeds parse: {e}"))?;
    Ok(parsed.data)
}

/// Plant response. Mirrors the server-side
/// `PlantSnapshotResponse` shape so the typed Tauri call returns
/// usable data without schema drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlantSnapshotResult {
    pub snapshot_id: String,
    pub event_id: String,
    pub source_fqn: String,
    pub target_fqn: String,
    pub digest_drift: String,
}

/// Wire envelope used by every Moss API response: `{ data: T }`.
/// We unwrap `.data` so callers see the inner payload directly.
#[derive(Debug, Clone, Deserialize)]
struct ApiEnvelope<T>
where
    T: serde::de::DeserializeOwned,
{
    #[serde(bound(deserialize = "T: serde::de::DeserializeOwned"))]
    data: T,
}

// ── Logical sets (ARCH-0038) ─────────────────────────────────

/// One member of an offering set, in the shape Pavilion's canvas
/// consumes. Mirrors `OfferingSetMember` from Moss but the field
/// list is what the frontend actually renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingSetMember {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String,
    /// `"primary" | "replica" | "joining" | "degraded"`, or `None`
    /// before the first orchestration tick reports a role.
    pub role: Option<String>,
    pub status: String,
    pub ready: bool,
}

/// One offering set with its full member list. The Pavilion canvas
/// uses these to populate per-stone `offerings: []` arrays for
/// edge rendering and per-stone role badges. The shape is the
/// detail response from `/api/v1/sets/offerings/{fqn}` minus a few
/// fields the canvas doesn't render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingSet {
    pub name: String,
    pub primary_stone: Option<String>,
    pub members: Vec<OfferingSetMember>,
}

#[derive(Debug, Clone, Deserialize)]
struct OfferingSetSummary {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    coordination: String,
    #[serde(default)]
    #[allow(dead_code)]
    member_count: usize,
    #[serde(default)]
    #[allow(dead_code)]
    primary_stone: Option<String>,
}

/// Fetch every elected-offering set from the tended stone, including
/// per-stone members. Issues one list call followed by N parallel
/// detail calls — the canvas uses the per-member role to drive both
/// inter-stone edges and the role badges on stone cards.
///
/// Returns `Ok(Vec::new())` when no stone is tended. A list call that
/// succeeds but returns zero sets is normal in a garden with no
/// elected offerings running; the canvas just doesn't render edges.
#[tauri::command]
pub async fn get_offering_sets(
    tending: State<'_, Arc<Tending>>,
) -> Result<Vec<OfferingSet>, String> {
    let Some(tended) = tending.current().await else {
        return Ok(Vec::new());
    };
    let client = connection::raw_client_for_capture();
    let base = tended.endpoint.trim_end_matches('/').to_string();

    // 1. List set summaries (just FQNs we need to drill into).
    let list_url = format!("{}/api/v1/sets/offerings", base);
    let list_resp = client
        .get(&list_url)
        .send()
        .await
        .map_err(|e| format!("offering-sets list GET {list_url}: {e}"))?;
    if !list_resp.status().is_success() {
        let status = list_resp.status();
        let text = list_resp.text().await.unwrap_or_default();
        return Err(format!("offering-sets list {status}: {text}"));
    }
    let summaries: ApiEnvelope<Vec<OfferingSetSummary>> = list_resp
        .json()
        .await
        .map_err(|e| format!("offering-sets list parse: {e}"))?;

    // 2. Fan out one detail call per set in parallel.
    let mut futures = Vec::with_capacity(summaries.data.len());
    for summary in summaries.data {
        let client = client.clone();
        let url = format!(
            "{}/api/v1/sets/offerings/{}",
            base,
            encode_uri_segment(&summary.name),
        );
        futures.push(async move {
            let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(format!("{status}: {text}"));
            }
            let parsed: ApiEnvelope<OfferingSetDetailWire> =
                resp.json().await.map_err(|e| e.to_string())?;
            Ok::<_, String>(parsed.data)
        });
    }
    let details: Vec<OfferingSetDetailWire> = futures_util::future::try_join_all(futures)
        .await
        .map_err(|e| format!("offering-set detail fetch: {e}"))?;

    Ok(details
        .into_iter()
        .map(|d| OfferingSet {
            name: d.name,
            primary_stone: d.primary_stone,
            members: d
                .members
                .into_iter()
                .map(|m| OfferingSetMember {
                    stone_id: m.stone_id,
                    stone_name: m.stone_name,
                    endpoint: m.endpoint,
                    role: m.role,
                    status: m.status,
                    ready: m.ready,
                })
                .collect(),
        })
        .collect())
}

/// Wire shape of `GET /api/v1/sets/offerings/{fqn}`. We don't expose
/// every field to the frontend — `OfferingSet` is the trimmed shape
/// the canvas actually renders.
#[derive(Debug, Deserialize)]
struct OfferingSetDetailWire {
    name: String,
    primary_stone: Option<String>,
    members: Vec<OfferingSetMemberWire>,
}

#[derive(Debug, Deserialize)]
struct OfferingSetMemberWire {
    stone_id: String,
    stone_name: String,
    endpoint: String,
    #[serde(default)]
    role: Option<String>,
    status: String,
    ready: bool,
}

/// Percent-encode the characters that matter inside a URL path
/// segment for the FQN — `:`, `/`, space, etc. The full RFC 3986
/// encoder lives in `urlencoding` but pavilion doesn't pull that
/// in; the FQN alphabet is constrained enough that this small
/// helper covers every value we'd produce.
fn encode_uri_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            other => {
                let mut buf = [0u8; 4];
                let bytes = other.encode_utf8(&mut buf).as_bytes();
                for b in bytes {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

/// Resolve a stone name to its HTTP endpoint. Checks awareness
/// first (covers any peer chirping on the LAN); falls back to
/// the tended stone for the local-self case where the user is
/// dragging on the stone they're currently tending.
async fn resolve_endpoint(
    stone: &str,
    awareness: &Arc<Awareness>,
    tending: &Arc<Tending>,
) -> Option<String> {
    let snap = awareness.snapshot().await;
    if let Some(s) = snap.iter().find(|s| s.stone_name == stone) {
        return Some(s.endpoint.clone());
    }
    if let Some(t) = tending.current().await
        && t.stone_name == stone
    {
        return Some(t.endpoint);
    }
    None
}
