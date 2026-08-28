//! The HTTP surface: observe the garden, describe thyself, tend offerings
//! (L1, L7, L22). Handlers are thin: parse → delegate to the application
//! service or the kernel aggregates → envelope. No domain logic lives here.
//!
//! Grammar (ADR-0004 §4): bare nouns name THIS stone's domain resources ·
//! `/garden/*` projects the room read-only · deeper paths hang off nouns.
//! The surface is declared ONCE — [`Face`] is the manifest, and the router
//! is built from it, so an unadvertised emission is structurally
//! impossible and an unrouted claim fails the manifest gates (L9, R4.7).

use crate::offerings::service::{CommandError, OfferingService};
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use garden_contract::consts::PROTO_V1;
use garden_kernel::announce::ChirpSource;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state behind the routes.
pub struct AppState {
    /// The offering application service (domain + worlds coordinated).
    pub garden: Arc<OfferingService>,
    /// This stone's banks (ADR-0005 §8) — the storage MVP's state.
    pub storage: Arc<crate::offerings::storage::Storage>,
    /// The living will's runner (ADR-0005 §2).
    pub capture: Arc<crate::offerings::capture_run::Runner>,
    /// The async operation tracker (the data plane's async contract).
    pub jobs: crate::jobs::JobTracker,
    pub topology: Arc<garden_kernel::topology::Topology>,
    pub dispatcher: Dispatcher,
    pub ingest_counters: Arc<IngestCounters>,
    /// This stone's voice — the SelfView's composer (self is a projection,
    /// never a stored peer; ADR-0004 §3).
    pub chirp_source: Arc<dyn ChirpSource>,
    pub stone_name: String,
    pub boot_id: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

use garden_kernel::dispatch::Dispatcher;
use garden_kernel::ingress::IngestCounters;

/// The surface, declared once (L9, R4.7): routes exist ONLY as rows of
/// [`Face::ALL`]. Adding a face means adding a variant — the compiler then
/// demands its method, path, summary, and wiring; removing one leaves
/// nowhere for a stale row to hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    Health,
    /// The front door: this route table (ADR-0004 §4 — kills the
    /// `/manifest` offering-name collision).
    FrontDoor,
    /// Me: the SelfView.
    StoneSelf,
    /// Me, spelled explicitly.
    StoneThis,
    /// Any stone by name or id: mine answered, others redirected home.
    StoneRef,
    StonePosture,
    GardenStones,
    Catalog,
    /// Local banks + adoptable volumes (L22: stone data).
    StorageList,
    /// The adopt ceremony: claim a removable volume for the garden.
    StorageAdopt,
    /// Authoritative absence: the eject verb (ADR-0005 §8.3).
    StorageEject,
    /// Declare a bank's roles (sink today; ADR-0005 §4).
    StorageRoles,
    /// List a bank directory: the files riding the volume.
    StorageFileList,
    /// Read one file from a bank.
    StorageFileGet,
    /// Write one file onto a bank.
    StorageFilePut,
    /// Delete one file from a bank.
    StorageFileDelete,
    /// Run a will: the capture pipeline (ADR-0005 §2).
    OfferingCapture,
    /// The last capture run of an offering.
    OfferingCaptureLast,
    /// Replant: restore an incarnation from its checkpoint (ADR-0005 §6).
    OfferingReplant,
    /// Every async operation on this stone.
    JobList,
    /// One job by id.
    JobDetail,
    /// The stone's living landing page (PORTRAIT idea: identity + presence).
    Portrait,
    /// The root lands on the portrait.
    Root,
    /// The live page: the room, as events happen.
    PulsePage,
    /// The SSE firehose: topology + offering events, one stream.
    PulseStream,
    /// The room's banks, projected from the cache (ADR-0004 §4 grid).
    GardenStorage,
    OfferingList,
    OfferingPlant,
    OfferingShow,
    OfferingRest,
    OfferingWake,
    OfferingUproot,
}

impl Face {
    const ALL: [Face; 32] = [
        Face::Health,
        Face::FrontDoor,
        Face::StoneSelf,
        Face::StoneThis,
        Face::StoneRef,
        Face::StonePosture,
        Face::GardenStones,
        Face::Catalog,
        Face::StorageList,
        Face::StorageAdopt,
        Face::StorageEject,
        Face::StorageRoles,
        Face::StorageFileList,
        Face::StorageFileGet,
        Face::StorageFilePut,
        Face::StorageFileDelete,
        Face::GardenStorage,
        Face::OfferingCapture,
        Face::OfferingCaptureLast,
        Face::OfferingReplant,
        Face::JobList,
        Face::JobDetail,
        Face::Portrait,
        Face::Root,
        Face::PulsePage,
        Face::PulseStream,
        Face::OfferingList,
        Face::OfferingPlant,
        Face::OfferingShow,
        Face::OfferingRest,
        Face::OfferingWake,
        Face::OfferingUproot,
    ];

    fn method(self) -> &'static str {
        match self {
            Face::Health
            | Face::FrontDoor
            | Face::StoneSelf
            | Face::StoneThis
            | Face::StoneRef
            | Face::StonePosture
            | Face::GardenStones
            | Face::Catalog
            | Face::StorageList
            | Face::StorageFileList
            | Face::StorageFileGet
            | Face::GardenStorage
            | Face::OfferingCaptureLast | Face::OfferingList | Face::OfferingShow
            | Face::JobList | Face::JobDetail
            | Face::Portrait | Face::Root | Face::PulsePage | Face::PulseStream => "GET",
            | Face::StorageAdopt | Face::StorageEject | Face::StorageRoles
            | Face::OfferingCapture | Face::OfferingReplant => "POST",
            Face::OfferingPlant | Face::OfferingRest | Face::OfferingWake => "POST",
            Face::StorageFilePut => "PUT",
            Face::StorageFileDelete | Face::OfferingUproot => "DELETE",
        }
    }

    fn path(self) -> &'static str {
        match self {
            Face::Health => "/health",
            Face::FrontDoor => "/api/v1",
            Face::StoneSelf => "/api/v1/stone",
            Face::StoneThis => "/api/v1/stone/this",
            Face::StoneRef => "/api/v1/stone/{ref}",
            Face::StonePosture => "/api/v1/stone/posture",
            Face::GardenStones => "/api/v1/garden/stones",
            Face::Catalog => "/api/v1/catalog",
            Face::StorageList => "/api/v1/storage",
            Face::StorageAdopt => "/api/v1/storage/adopt",
            Face::StorageRoles => "/api/v1/storage/{fqn}/roles",
            Face::StorageEject => "/api/v1/storage/{fqn}/eject",
            Face::StorageFileList => "/api/v1/storage/{fqn}/files",
            Face::StorageFileGet | Face::StorageFilePut | Face::StorageFileDelete => {
                "/api/v1/storage/{fqn}/files/{*path}"
            }
            Face::GardenStorage => "/api/v1/garden/storage",
            Face::OfferingList => "/api/v1/offerings",
            Face::OfferingPlant | Face::OfferingShow | Face::OfferingUproot => {
                "/api/v1/offerings/{fqn}"
            }
            Face::OfferingCapture | Face::OfferingCaptureLast => {
                "/api/v1/offerings/{fqn}/capture"
            }
            Face::OfferingReplant => "/api/v1/offerings/{fqn}/replant",
            Face::Portrait => "/portrait",
            Face::Root => "/",
            Face::JobList => "/api/v1/jobs",
            Face::JobDetail => "/api/v1/jobs/{id}",
            Face::PulsePage => "/pulse",
            Face::PulseStream => "/pulse/stream",
            Face::OfferingRest => "/api/v1/offerings/{fqn}/rest",
            Face::OfferingWake => "/api/v1/offerings/{fqn}/wake",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Face::Health => "Liveness probe of this stone and its wire protocol marker.",
            Face::FrontDoor => "This route table - every surface, described in place.",
            Face::StoneSelf => "Me: my frame, sung full-voice (the SelfView projection).",
            Face::StoneThis => "Me, spelled explicitly (same SelfView).",
            Face::StoneRef => {
                "A stone by name or id: mine answered here; others answer 404 with a \
                 Location to their home stone (the garden's only true redirect)."
            }
            Face::StonePosture => {
                "Local data (L22): this moss's live counters - ingest, dispatch, \
                 topology, offerings."
            }
            Face::GardenStones => {
                "Garden data (L22): the room as this moss sees it - self spliced \
                 among the peers, every row a canonical frame."
            }
            Face::Catalog => "The catalog this stone can place from (derived).",
            Face::StorageList => {
                "This stone's banks, plus the removable volumes ready for adoption."
            }
            Face::StorageAdopt => {
                "The adopt ceremony: {device: mount point, name: bank FQN} - writes the manifest onto the drive and sings the news (ADR-0005 sec 8)."
            }
            Face::StorageEject => {
                "Eject a bank by name: authoritative absence, sung to the room (ADR-0005 sec 8.3)."
            }
            Face::StorageRoles => {
                "Declare a bank's roles: {roles: [sink]} - a sink receives checkpoints (ADR-0005 sec 4)."
            }
            Face::StorageFileList => {
                "List a bank directory (optional ?path= subdirectory): the files riding \
                 the volume, minus the adoption record. A bank held by a peer answers \
                 the garden's redirect (knows_at)."
            }
            Face::StorageFileGet => {
                "Read one file from a bank: the raw bytes, content-type guessed from the \
                 extension; the path is relative to the bank's root. A peer's bank \
                 answers the garden's redirect (knows_at)."
            }
            Face::StorageFilePut => {
                "Write one file onto a bank: the raw body, parent directories created - \
                 makes a sink a real storage destination. A peer's bank answers the \
                 garden's redirect (knows_at); writes bind at their authority."
            }
            Face::StorageFileDelete => {
                "Delete one file from a bank. Directories refuse - wholesale removal is \
                 the operator's hand. A peer's bank answers the garden's redirect."
            }
            Face::GardenStorage => {
                "Garden data (L22): every bank in the room, self included, from the one cache."
            }
            Face::OfferingPlant => {
                "Plant a managed offering {image?, ports:{name:container}, runtime?, \
                 inputs?}; catalog name wins when one exists."
            }
            Face::OfferingList => "Every offering placed on this stone (the collection).",
            Face::OfferingShow => "The placed record - plan, decisions, ports (OFFERINGS.md §5.3).",
            Face::OfferingCapture => {
                "Run this offering's declared will: Phase A imprint (quiesce -> copy -> resume), then pack, ferry, commit."
            }
            Face::OfferingCaptureLast => "The last capture run of this offering: phase, checkpoint, ferried sinks.",
            Face::OfferingReplant => {
                "Replant from a checkpoint {run?}: verify, restore the directory, place from the stored spec - same FQN, same connection strings (ADR-0005 §6)."
            }
            Face::Portrait => {
                "This stone's living landing page: identity, offerings, banks, the room."
            }
            Face::Root => "Lands on the portrait.",
            Face::JobList => "Every async operation on this stone, newest first.",
            Face::JobDetail => "One job by id: kind, subject, status, error, result.",
            Face::PulsePage => {
                "The live page: stones, offerings, and the event ring as they happen."
            }
            Face::PulseStream => {
                "SSE firehose: topology events (seen/goodbye/expired) and offering changes."
            }
            Face::OfferingRest => {
                "Rest a managed offering - stopped, and reconcile will keep it so."
            }
            Face::OfferingWake => {
                "Wake a rested offering; resurrects from its stored spec if reality lost it."
            }
            Face::OfferingUproot => "Uproot - remove the workload and forget the offering.",
        }
    }

    fn method_router(self) -> axum::routing::MethodRouter<Arc<AppState>> {
        match self {
            Face::Health => get(health),
            Face::FrontDoor => get(front_door),
            Face::StoneSelf | Face::StoneThis => get(stone_self),
            Face::StoneRef => get(stone_ref),
            Face::StonePosture => get(posture),
            Face::GardenStones => get(garden_stones),
            Face::Catalog => get(catalog),
            Face::StorageList => get(storage_list),
            Face::StorageAdopt => post(storage_adopt),
            Face::StorageRoles => post(storage_roles),
            Face::StorageEject => post(storage_eject),
            Face::StorageFileList => get(storage_files_list),
            Face::StorageFileGet => get(storage_file_get),
            Face::StorageFilePut => axum::routing::put(storage_file_put),
            Face::StorageFileDelete => axum::routing::delete(storage_file_delete),
            Face::GardenStorage => get(garden_storage),
            Face::OfferingList => get(offerings_list),
            Face::OfferingPlant => post(plant_offering),
            Face::OfferingCapture => post(capture_offer),
            Face::OfferingCaptureLast => get(capture_last),
            Face::OfferingReplant => post(replant_offer),
            Face::Portrait => get(portrait),
            Face::Root => get(root),
            Face::JobList => get(job_list),
            Face::JobDetail => get(job_detail),
            Face::PulsePage => get(pulse_page),
            Face::PulseStream => get(pulse_stream),
            Face::OfferingShow => get(show_offering),
            Face::OfferingRest => post(rest_offering),
            Face::OfferingWake => post(wake_offering),
            Face::OfferingUproot => axum::routing::delete(uproot_offering),
        }
    }
}

async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.started_at;
    Json(serde_json::json!({
        "data": {
            "ok": true,
            "asset": "moss",
            "proto": PROTO_V1,
            "stone_name": state.stone_name,
            "boot_id": state.boot_id,
            "uptime_secs": uptime.num_seconds(),
        }
    }))
}

async fn posture(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime = chrono::Utc::now() - state.started_at;
    let dispatch = state.dispatcher.stats();
    let (capture_tracked, capture_failed) = state.capture.run_stats();
    Json(serde_json::json!({
        "data": {
            "asset": "moss",
            "stone_name": state.stone_name,
            "boot_id": state.boot_id,
            "uptime_secs": uptime.num_seconds(),
            "ingest": {
                "parsed": state.ingest_counters.parsed(),
                "bad_json": state.ingest_counters.bad_json(),
                "deduped": state.ingest_counters.deduped(),
            },
            "dispatch": {
                "delivered": dispatch.delivered,
                "dropped": dispatch.dropped,
                "unclaimed": dispatch.unclaimed,
            },
            "topology": {
                "stones": state.topology.snapshot().len(),
                "candidates": state.topology.candidates().len(),
                "chirps_total": state.topology.chirps_total(),
            },
            "offerings": {
                "active": state.garden.counts().active,
                "candidates": state.garden.counts().candidates,
                "catalog": state.garden.catalog_size(),
            },
            "runtimes": state.garden.available_worlds(),
            "capture": {
                "tracked": capture_tracked,
                "failed": capture_failed
            },
        }
    }))
}

/// The SelfView (ADR-0004 §3): self is rebuilt, never stored. The stone's
/// own frame, re-voiced with its full inventory — one composer, many
/// mouths (B1).
fn self_view(state: &AppState) -> serde_json::Value {
    let mut body = state.chirp_source.body();
    body.inventory =
        garden_contract::chirp::InventoryMap::from_pairs(state.chirp_source.song_blocks());
    serde_json::to_value(&body).unwrap_or_default()
}

async fn stone_self(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "data": self_view(&state) }))
}

/// `/stone/{ref}` — the garden's only true redirect (ADR-0004 §4): mine
/// answered here; a peer's is a not-here answer carrying its home address
/// (Location header + `knows_at`), because reads delegate and writes bind
/// at their authority. Unknown names are a plain 404.
async fn stone_ref(State(state): State<Arc<AppState>>, Path(reference): Path<String>) -> Response {
    let my_frame = state.chirp_source.body();
    if reference == state.stone_name || reference == my_frame.stone.id {
        return Json(serde_json::json!({ "data": self_view(&state) })).into_response();
    }
    if let Some(peer) = state.topology.find(&reference) {
        let name = peer.body.stone.name.clone();
        let address = peer.body.stone.network.address.clone();
        let knows_at = format!("http://{}:{}/api/v1/stone", address.ip, address.port);
        return (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::LOCATION, knows_at.clone())],
            Json(serde_json::json!({
                "error": {
                    "not_here": true,
                    "stone": name,
                    "knows_at": knows_at,
                    "message": "That stone does not grow here. Its home answers at \
                                `knows_at` - this stone only knows the way."
                }
            })),
        )
            .into_response();
    }
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": { "message": format!("No stone '{reference}' in this garden's ken.") }
        })),
    )
        .into_response()
}

async fn garden_stones(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    // Self spliced first — the room's projection obviously includes the
    // current stone (ADR-0004 §3).
    let mut self_row = self_view(&state);
    if let Some(obj) = self_row.as_object_mut() {
        obj.insert("self".into(), serde_json::json!(true));
    }
    let mut stones = vec![self_row];
    for peer in state.topology.snapshot() {
        let mut v = serde_json::to_value(&peer.body).unwrap_or_default();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("chirps".into(), serde_json::json!(peer.chirps));
        }
        stones.push(v);
    }
    Json(serde_json::json!({ "data": { "stones": stones } }))
}

/// The derived catalog face: what this stone can place from.
async fn catalog(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let entries: Vec<serde_json::Value> = state
        .garden
        .catalog
        .names()
        .into_iter()
        .filter_map(|stem| {
            state.garden.catalog.get(&stem).map(|m| {
                serde_json::json!({
                    "stem": stem,
                    "category": m.category,
                    "description": m.description,
                })
            })
        })
        .collect();
    Json(serde_json::json!({ "data": { "catalog": entries } }))
}

/// Local storage (L22): this stone's banks and the volumes ready for
/// adoption.
async fn storage_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let volumes = crate::offerings::storage::scan_volumes();
    let adoptable: Vec<serde_json::Value> =
        crate::offerings::storage::Storage::adoptable(&volumes)
            .into_iter()
            .map(|v| {
                serde_json::json!({
                    "device": v.mount_point.display().to_string(),
                    "capacity_bytes": v.capacity_bytes,
                })
            })
            .collect();
    Json(serde_json::json!({
        "data": {
            "banks": state.storage.banks(),
            "adoptable": adoptable,
        }
    }))
}

#[derive(Debug, Deserialize)]
struct AdoptRequest {
    /// The volume's mount point (a scan's `device` value).
    device: String,
    /// The bank's logical name - FQN or bare stem (canonicalized).
    name: String,
}

/// The adopt ceremony's API face (1:1 with `rake storage adopt`): write
/// the manifest onto the drive, remember the bank, sing the news.
async fn storage_adopt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptRequest>,
) -> ApiResult {
    use crate::offerings::storage::AdoptError;
    let wanted = std::path::PathBuf::from(&req.device);
    let volumes = crate::offerings::storage::scan_volumes();
    let vol = volumes
        .iter()
        .find(|v| dirs_equal(&v.mount_point, &wanted))
        .ok_or(CommandError::Conflict(format!(
            "no removable volume answers at '{}' - plug it in, or name its mount point",
            req.device
        )))?;
    let stone_id = state.chirp_source.body().stone.id;
    let bank = state.storage.adopt(vol, &req.name, &stone_id).map_err(|e| match e {
        AdoptError::AlreadyAdopted(_) => CommandError::Conflict(e.to_string()),
        AdoptError::BadName(_) => CommandError::Conflict(e.to_string()),
        AdoptError::Io(_) => CommandError::Runtime(crate::offerings::runtime::RuntimeError::Failed(
            e.to_string(),
        )),
    })?;
    Ok(Json(serde_json::json!({ "data": { "bank": bank } })))
}

/// Mount-point comparison tolerant of trailing separators (`E:` == `E:`+slash).
fn dirs_equal(a: &std::path::Path, b: &std::path::Path) -> bool {
    let clean = |p: &std::path::Path| -> std::path::PathBuf {
        p.components().collect::<std::path::PathBuf>()
    };
    clean(a) == clean(b)
}

/// The eject verb's API face (1:1 with `rake storage eject`): mark the
/// bank ejected, sing the authoritative absence.
async fn storage_eject(
    State(state): State<Arc<AppState>>,
    Path(fqn): Path<String>,
) -> ApiResult {
    use crate::offerings::storage::EjectError;
    let bank = state.storage.eject(&fqn).map_err(|e| match e {
        EjectError::UnknownBank(_) => CommandError::NotFound(e.to_string()),
        EjectError::AlreadyEjected(_) => CommandError::Conflict(e.to_string()),
    })?;
    Ok(Json(serde_json::json!({ "data": { "bank": bank } })))
}

/// The room's banks (ADR-0004 §4 grid): self spliced first, then every
/// peer's banks from the one topology cache. Rows name the holding stone.
async fn garden_storage(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for bank in state.storage.banks() {
        rows.push(serde_json::json!({
            "self": true,
            "stone": state.stone_name,
            "bank": bank,
        }));
    }
    for peer in state.topology.snapshot() {
        let Some(banks) = &peer.body.inventory.banks else {
            continue; // absent key: the stone says nothing about banks
        };
        for bank in &banks.items {
            rows.push(serde_json::json!({
                "stone": peer.body.stone.name,
                "stone_id": peer.body.stone.id,
                "bank": bank,
            }));
        }
    }
    Json(serde_json::json!({ "data": { "banks": rows } }))
}

/// Run a will (1:1 with `rake capture {name}`): Phase A imprint, then
/// pack/ferry/commit in the background. The response carries the run; ask
/// again on the GET face for its progress.
async fn capture_offer(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    use crate::offerings::capture::{readiness, Readiness};
    use crate::offerings::capture_run::RunInfo;

    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let offering = state
        .garden
        .placed(&fqn)
        .ok_or(CommandError::NotFound(format!("'{}' is not planted here", fqn)))?;

    // The will lives in the catalog manifest (one machine-truth parse).
    let manifest = state.garden.catalog.get(&offering.offering).ok_or_else(|| {
        CommandError::Conflict(format!(
            "'{fqn}' grows from stem '{}' with no catalog manifest - its will cannot be read",
            offering.offering
        ))
    })?;
    match readiness(manifest) {
        Readiness::NothingToPreserve => {}
        Readiness::Trusted(_) => {}
        Readiness::Untrusted => {
            return Err(CommandError::Conflict(format!(
                "'{}' declares volumes but no capture policy - raw copy would be a lie;                  declare a `capture:` section in the manifest first",
                fqn
            ))
            .into())
        }
    }
    let Some(policy) = &manifest.capture else {
        return Err(CommandError::Conflict(format!(
            "'{}' declares no capture policy and no volumes - nothing to preserve",
            fqn
        ))
        .into())
    };

    let workload =
        crate::offerings::capture_run::workload_for(&offering, &state.garden.dirs_root);

    let runner = Arc::clone(&state.capture);
    let policy = policy.clone();
    let fqn_str = fqn.clone();
    let run_id = uuid::Uuid::now_v7().to_string();
    let mut announced = RunInfo {
        fqn: fqn_str.clone(),
        run_id: run_id.clone(),
        started_at: chrono::Utc::now(),
        phase: "imprint".into(),
        error: None,
        checkpoint: None,
        ferried_to: None,
    };
    state.capture.announce(announced.clone());

    // Track the capture as a job (the data plane's async contract).
    let job_id = state.jobs.start("capture", &fqn_str);
    let job_id_resp = job_id.clone();

    let task_fqn = fqn_str.clone();
    let task_run = run_id.clone();
    let jobs = state.jobs.clone();
    tokio::spawn(async move {
        match runner
            .execute_named(&task_fqn, &policy, &workload, &task_run)
            .await
        {
            Ok(checkpoint) => {
                jobs.complete(
                    &job_id,
                    serde_json::json!({
                        "checkpoint": checkpoint.display().to_string(),
                    }),
                );
            }
            Err(e) => {
                jobs.fail(&job_id, &e);
                tracing::warn!(offering = %task_fqn, error = %e, "capture run failed");
            }
        }
    });
    announced.phase = "accepted".into();
    Ok(Json(
        serde_json::json!({ "data": { "run": announced }, "job_id": job_id_resp }),
    ))
}

/// The last capture run of an offering.
async fn capture_last(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    match state.capture.last_run(&fqn) {
        Some(run) => Ok(Json(serde_json::json!({ "data": { "run": run } }))),
        None => Err(CommandError::NotFound(format!(
            "'{}' has run no capture on this stone",
            fqn
        ))
        .into()),
    }
}

#[derive(Debug, Deserialize)]
struct RolesRequest {
    /// The complete role set for this bank (sink today).
    roles: Vec<String>,
}

/// Declare a bank's roles (1:1 with `rake storage roles`): a sink receives
/// checkpoints; role news is state news and sings.
async fn storage_roles(
    State(state): State<Arc<AppState>>,
    Path(fqn): Path<String>,
    Json(req): Json<RolesRequest>,
) -> ApiResult {
    let bank = state
        .storage
        .set_roles(&fqn, req.roles)
        .map_err(CommandError::Conflict)?
        .ok_or(CommandError::NotFound(format!(
            "no bank '{fqn}' is adopted here - rake storage lists what this stone holds"
        )))?;
    Ok(Json(serde_json::json!({ "data": { "bank": bank } })))
}

// ---- bank files: CRUD on a mounted bank's volume ---------------------------
// The gate is one: `Storage::bank_root` resolves the FQN to a mounted
// volume; `safe_join` keeps every path under it. The adoption record
// (`.zen-garden`) is ceremony-owned and never crosses this surface.
// And a bank grows on ONE stone: a file request landing where the volume
// is not is answered with the garden's only true redirect (ADR-0004 §4
// — reads delegate and writes bind at their authority).

/// The one mapping from the storage domain's file refusals onto the
/// command taxonomy (each refuses as it truly is — R3.3).
fn files_err(e: crate::offerings::storage::FilesError) -> CommandError {
    use crate::offerings::storage::FilesError;
    match &e {
        FilesError::UnknownBank(_) | FilesError::Missing(_) => {
            CommandError::NotFound(e.to_string())
        }
        FilesError::NotMounted(_) | FilesError::NotThatKind(_) => {
            CommandError::Conflict(e.to_string())
        }
        FilesError::BadPath(_) => CommandError::BadRequest(e.to_string()),
        FilesError::Io(_) => CommandError::Runtime(
            crate::offerings::runtime::RuntimeError::Failed(e.to_string()),
        ),
    }
}

/// The files faces' shared gate: resolve a bank FQN to its volume root
/// HERE, or hand back the answer the request deserves instead. Local
/// presence wins — the authority is the volume in the slot, and an
/// ejected bank is refused HERE even if the cache remembers it elsewhere
/// (the adoption record is local truth). Only a bank this stone never
/// adopted consults the room.
fn gate_bank(
    state: &AppState,
    fqn: &str,
) -> Result<(crate::offerings::storage::Bank, std::path::PathBuf), Box<axum::response::Response>>
{
    use crate::offerings::storage::FilesError;
    match state.storage.bank_root(fqn) {
        Ok(pair) => Ok(pair),
        Err(FilesError::UnknownBank(_)) => Err(Box::new(bank_not_here(state, fqn))),
        Err(e) => Err(Box::new(ApiError::from(files_err(e)).into_response())),
    }
}

/// Who holds this bank, as the room's cache hears it — the addressee of
/// a not-here answer. Self never appears: the caller asked the local
/// vault first.
fn bank_holder(state: &AppState, fqn: &str) -> Option<String> {
    for peer in state.topology.snapshot() {
        let Some(banks) = &peer.body.inventory.banks else {
            continue; // the stone says nothing about banks
        };
        if banks.items.iter().any(|b| b.fqn == fqn) {
            let address = &peer.body.stone.network.address;
            return Some(format!(
                "http://{}:{}/api/v1/stone",
                address.ip, address.port
            ));
        }
    }
    None
}

/// The not-here answer for a bank that grows elsewhere (1:1 with the
/// stone face's): 404, a Location header, and `knows_at` naming the
/// holder. A bank NOBODY holds is a plain 404 — the room was consulted
/// and keeps its silence.
fn bank_not_here(state: &AppState, fqn: &str) -> axum::response::Response {
    let canonical = garden_glossary::fqn::canonicalize(fqn).unwrap_or_else(|_| fqn.to_string());
    match bank_holder(state, &canonical) {
        Some(knows_at) => (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::LOCATION, knows_at.clone())],
            Json(serde_json::json!({
                "error": {
                    "not_here": true,
                    "bank": canonical,
                    "knows_at": knows_at,
                    "message": "That bank does not grow here. Its home stone answers at \
                                `knows_at` - files bind at their authority."
                }
            })),
        )
            .into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": {
                    "message": format!(
                        "no bank '{canonical}' is adopted here, and the room's cache knows no \
                         holder - rake storage lists what this stone holds"
                    )
                }
            })),
        )
            .into_response(),
    }
}

/// List a bank directory (`?path=` names a subdirectory; absent = the
/// bank's root).
async fn storage_files_list(
    State(state): State<Arc<AppState>>,
    Path(fqn): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    use crate::offerings::storage::{list_dir, safe_join};
    let rel = params.get("path").map(String::as_str).unwrap_or("");
    let (bank, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let dir = if rel.is_empty() {
        root.clone()
    } else {
        safe_join(&root, rel).map_err(files_err)?
    };
    let files = list_dir(&root, &dir).map_err(files_err)?;
    Ok(Json(
        serde_json::json!({ "data": { "bank": bank.fqn, "path": rel, "files": files } }),
    )
        .into_response())
}

/// Read one file from a bank: the raw bytes ride alone, content-type
/// guessed from the extension (payload faces are not envelope faces —
/// the portrait and the pulse set the precedent).
async fn storage_file_get(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
    use crate::offerings::storage::{read_file, safe_join};
    let (_, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let path = safe_join(&root, &rel).map_err(files_err)?;
    let bytes = read_file(&root, &path).map_err(files_err)?;
    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type_for(&rel))],
        bytes,
    )
        .into_response())
}

/// Write one file onto a bank: the raw body, parents created.
async fn storage_file_put(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<axum::response::Response, ApiError> {
    use crate::offerings::storage::{safe_join, write_file};
    let (bank, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let path = safe_join(&root, &rel).map_err(files_err)?;
    let n = write_file(&root, &path, &body).map_err(files_err)?;
    tracing::info!(bank = %bank.fqn, path = %rel, bytes = n, "file written onto a bank");
    Ok(Json(
        serde_json::json!({ "data": { "bank": bank.fqn, "path": rel, "size_bytes": n } }),
    )
        .into_response())
}

/// Delete one file from a bank.
async fn storage_file_delete(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
    use crate::offerings::storage::{delete_file, safe_join};
    let (bank, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let path = safe_join(&root, &rel).map_err(files_err)?;
    delete_file(&root, &path).map_err(files_err)?;
    tracing::info!(bank = %bank.fqn, path = %rel, "file deleted from a bank");
    Ok(Json(
        serde_json::json!({ "data": { "bank": bank.fqn, "path": rel, "deleted": true } }),
    )
        .into_response())
}

/// A small honest content-type table — extension guessed, everything else
/// rides as octet-stream. No mime crate: the surface stays lean (P5).
fn content_type_for(path: &str) -> &'static str {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "log" | "conf" | "toml" | "yaml" | "yml" | "csv" => "text/plain; charset=utf-8",
        "json" => "application/json",
        "html" | "htm" => "text/html; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "zst" => "application/zstd",
        _ => "application/octet-stream",
    }
}

/// The collection: every offering placed on this stone.
async fn offerings_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let rows: Vec<serde_json::Value> =
        state.garden.snapshot().iter()
        .map(record_view).collect();
    Json(serde_json::json!({ "data": { "offerings": rows } }))
}

#[derive(Debug, Deserialize)]
struct ReplantRequest {
    /// The checkpoint run to restore; absent = the newest.
    #[serde(default)]
    run: Option<String>,
}

/// Replant (1:1 with `rake replant`): select -> verify -> restore the
/// directory -> place from the stored spec. The audit chain opens with
/// Replanted{predecessor_offering_id, final_hash}.
async fn replant_offer(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    body: Option<Json<ReplantRequest>>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    let run = body.as_ref().and_then(|Json(req)| req.run.clone());
    let checkpoint = state
        .capture
        .select_checkpoint(&fqn, run.as_deref())
        .map_err(CommandError::NotFound)?;

    let dir = state.garden.dirs_root.dir_for(&fqn);
    let (count, final_hash) = state
        .capture
        .restore_into(&checkpoint, &dir.root)
        .map_err(CommandError::Conflict)?;

    // The restored record IS the identity: same offering_id, same spec,
    // same connection strings as the predecessor.
    let bytes = std::fs::read(dir.record_json())
        .map_err(|e| CommandError::Conflict(format!("restored record unreadable: {e}")))?;
    let record: crate::offerings::record::OfferingRecord =
        serde_json::from_slice(&bytes).map_err(|e| {
            CommandError::Conflict(format!("restored record unparsable: {e}"))
        })?;
    let offering = state
        .garden
        .replant(record.into_domain(), &final_hash)
        .await?;
    tracing::info!(offering = %fqn, from = %checkpoint.display(), files = count, "replanted");
    Ok(Json(
        serde_json::json!({ "data": { "offering": {
            "name": offering.name,
            "status": offering.status.as_str(),
            "replanted_from": checkpoint.display().to_string(),
            "final_hash": final_hash,
        } } }),
    ))
}

/// The stone's portrait: the landing page, rendered from the SelfView and
/// the room - one composer, one more mouth (B1; the PoC's PORTRAIT idea).
async fn portrait(State(state): State<Arc<AppState>>) -> axum::response::Html<String> {
    let frame = state.chirp_source.body();
    let offerings = state.garden.snapshot();
    let peers = state.topology.snapshot();
    let uptime = chrono::Utc::now() - state.started_at;

    let mut offering_rows = String::new();
    if offerings.is_empty() {
        offering_rows.push_str("<div><em>nothing planted yet</em></div>");
    } else {
        offering_rows.push_str("<table><tr><th>offering</th><th>status</th><th>home</th></tr>");
        for o in &offerings {
            let home = o
                .managed()
                .and_then(|m| m.port_map.values().next())
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".into());
            offering_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                html_escape(&o.name),
                html_escape(o.status.as_str()),
                html_escape(&home)
            ));
        }
        offering_rows.push_str("</table>");
    }

    let banks = state.storage.banks();
    let bank_rows = if banks.is_empty() {
        "<div><em>no banks adopted</em></div>".to_string()
    } else {
        let mut s = String::from("<table><tr><th>bank</th><th>state</th></tr>");
        for b in &banks {
            s.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>",
                html_escape(&b.fqn),
                html_escape(&b.state)
            ));
        }
        s.push_str("</table>");
        s
    };

    let mut peer_rows = String::new();
    for p in &peers {
        peer_rows.push_str(&format!("<div>{}</div>", html_escape(&p.body.stone.name)));
    }
    if peer_rows.is_empty() {
        peer_rows.push_str("<div><em>the room is quiet</em></div>");
    }

    let page = include_str!("../assets/portrait.html")
        .replace("__STONE_NAME__", &html_escape(&state.stone_name))
        .replace("__MOSS_VERSION__", &html_escape(&frame.stone.moss.version))
        .replace("__HEALTH__", &html_escape(&frame.presence.health))
        .replace("__STONE_ID__", &html_escape(&frame.stone.id))
        .replace(
            "__ADDRESS__",
            &html_escape(&format!(
                "{}:{}",
                frame.stone.network.address.ip, frame.stone.network.address.port
            )),
        )
        .replace("__BOOT_ID__", &html_escape(&state.boot_id.to_string()))
        .replace("__UPTIME__", &format!("{}s", uptime.num_seconds()))
        .replace("__OFFERING_COUNT__", &offerings.len().to_string())
        .replace("__OFFERINGS__", &offering_rows)
        .replace("__BANK_COUNT__", &banks.len().to_string())
        .replace("__BANKS__", &bank_rows)
        .replace("__PEER_COUNT__", &peers.len().to_string())
        .replace("__PEERS__", &peer_rows);
    axum::response::Html(page)
}

/// The root lands on the portrait (the stone's face is its front door).
async fn root() -> axum::response::Redirect {
    axum::response::Redirect::temporary("/portrait")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The pulse page: the live view. Connects to /pulse/stream, seeds itself
/// from /garden/stones.
async fn pulse_page() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../assets/pulse.html"))
}

/// The SSE firehose (L18 at the edge): topology events and offering
/// changes, merged into one stream. Each connection holds its own
/// receivers; events are JSON, keep-alives keep proxies honest.
async fn pulse_stream(
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    let topology = state.topology.events();
    let offerings = state.garden.events();

    let stream = futures::stream::unfold(
        (topology, offerings),
        |(mut topology, mut offerings)| async move {
            loop {
                tokio::select! {
                    ev = topology.recv() => {
                        match ev {
                            Ok(ev) => {
                                let line = match &ev {
                                    garden_kernel::topology::TopologyEvent::Seen(v) => {
                                        serde_json::json!({
                                            "stream": "topology", "kind": "seen",
                                            "stone": v.body.stone.name,
                                            "health": v.body.presence.health,
                                        })
                                    }
                                    garden_kernel::topology::TopologyEvent::Goodbye { stone_name, .. } => {
                                        serde_json::json!({
                                            "stream": "topology", "kind": "goodbye",
                                            "stone": stone_name,
                                        })
                                    }
                                    garden_kernel::topology::TopologyEvent::Expired { stone_name, .. } => {
                                        serde_json::json!({
                                            "stream": "topology", "kind": "expired",
                                            "stone": stone_name,
                                        })
                                    }
                                };
                                let event = axum::response::sse::Event::default()
                                    .event("topology")
                                    .data(line.to_string());
                                return Some((Ok::<_, std::convert::Infallible>(event), (topology, offerings)));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                let event = axum::response::sse::Event::default()
                                    .event("lagged")
                                    .data(serde_json::json!({ "missed": n }).to_string());
                                return Some((Ok::<_, std::convert::Infallible>(event), (topology, offerings)));
                            }
                            Err(_) => continue,
                        }
                    }
                    ev = offerings.recv() => {
                        match ev {
                            Ok(ev) => {
                                let event = axum::response::sse::Event::default()
                                    .event("offerings")
                                    .data(serde_json::json!({ "name": ev.name }).to_string());
                                return Some((Ok::<_, std::convert::Infallible>(event), (topology, offerings)));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                let event = axum::response::sse::Event::default()
                                    .event("lagged")
                                    .data(serde_json::json!({ "missed": n }).to_string());
                                return Some((Ok::<_, std::convert::Infallible>(event), (topology, offerings)));
                            }
                            Err(_) => continue,
                        }
                    }
                }
            }
        },
    );

    axum::response::IntoResponse::into_response(axum::response::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::default()))
}

/// Every tracked async operation, newest first.
async fn job_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let jobs = state.jobs.list();
    Json(serde_json::json!({ "data": { "jobs": jobs } }))
}

/// One job by id.
async fn job_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .jobs
        .get(&id)
        .map(|j| Json(serde_json::json!({ "data": { "job": j } })))
        .ok_or_else(|| {
            ApiError(CommandError::NotFound(format!("no job '{id}' on this stone")))
        })
}

async fn front_door() -> Json<serde_json::Value> {
    let routes: Vec<serde_json::Value> = Face::ALL
        .iter()
        .map(|face| {
            serde_json::json!({
                "method": face.method(),
                "path": face.path(),
                "summary": face.summary(),
            })
        })
        .collect();
    Json(serde_json::json!({ "data": { "routes": routes } }))
}

// ---- offerings (L22) — thin delegation to the application service ---------

type ApiResult = Result<Json<serde_json::Value>, ApiError>;

struct ApiError(CommandError);

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use CommandError::*;
        let status = match self.0 {
            NotFound(_) => axum::http::StatusCode::NOT_FOUND,
            Conflict(_) => axum::http::StatusCode::CONFLICT,
            BadRequest(_) => axum::http::StatusCode::BAD_REQUEST,
            WorldUnavailable(_) => axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Runtime(_) => axum::http::StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(serde_json::json!({ "error": { "message": self.0.to_string() } })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct PlantRequest {
    /// Required for ad-hoc placement; absent when planting from catalog.
    image: Option<String>,
    /// Named ports: name → container port. Host mapping is the world's.
    #[serde(default)]
    ports: HashMap<String, u16>,
    #[serde(default = "default_category")]
    category: String,
    /// Which world to place into; absent = this host's default.
    #[serde(default)]
    runtime: Option<String>,
    /// Declared install form values (OFFERINGS.md §5.1 `inputs`).
    #[serde(default)]
    inputs: std::collections::BTreeMap<String, String>,
}

fn default_category() -> String {
    "misc".into()
}

async fn plant_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<PlantRequest>,
) -> ApiResult {
    let offering = state
        .garden
        .offer(&name, req.image, req.ports, Some(req.category), req.runtime.as_deref(), &req.inputs)
        .await?;
    Ok(Json(
        serde_json::json!({ "data": { "offering": record_view(&offering) } }),
    ))
}

/// Offerings render the sectioned record — disk and HTTP speak one shape
/// (R3.9, B1; S5.5).
fn record_view(offering: &crate::offerings::model::Offering) -> serde_json::Value {
    serde_json::to_value(crate::offerings::record::OfferingRecord::from_domain(offering))
        .unwrap_or_default()
}

/// §5.3: the placed record with its plan attached. Off-grammar names
/// refuse loudly here too — a tag-shaped read is an identity question,
/// not a quiet miss.
async fn show_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    let fqn = garden_glossary::fqn::canonicalize(&name)
        .map_err(|e| CommandError::Conflict(e.to_string()))?;
    match state.garden.placed(&fqn) {
        Some(o) => {
            let capture = capture_view(&state, &o);
            Ok(Json(
                serde_json::json!({ "data": { "offering": record_view(&o), "capture": capture } }),
            ))
        }
        None => Err(CommandError::NotFound(fqn).into()),
    }
}

/// The living will's surfacing for one offering (L3: never silent).
/// Readiness comes from the catalog manifest's declared policy; volumes
/// without a will are UNTRUSTED and say so.
fn capture_view(
    state: &AppState,
    offering: &crate::offerings::model::Offering,
) -> serde_json::Value {
    let manifest = state.garden.catalog.get(&offering.offering);
    let declared = manifest.and_then(|m| m.capture.as_ref());
    let readiness = match (declared, offering.managed()) {
        (Some(_), _) => "trusted",
        (None, Some(m)) if !m.spec.volumes.is_empty() => "untrusted",
        _ => "nothing-to-preserve",
    };
    let mut v = serde_json::json!({ "readiness": readiness });
    if let Some(policy) = declared {
        v["mode"] = serde_json::json!(policy.mode.as_str());
        if policy.mode == crate::offerings::capture::CaptureMode::LockAndCopy {
            v["max_locked_s"] = serde_json::json!(policy.max_locked_s);
        }
    }
    if let Some(run) = state.capture.last_run(&offering.name) {
        v["last_run"] = serde_json::json!(run);
    }
    v
}

async fn rest_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.rest(&name).await?;
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str() }
    })))
}

async fn wake_offering(State(state): State<Arc<AppState>>, Path(name): Path<String>) -> ApiResult {
    let offering = state.garden.wake(&name).await?;
    let port_map = offering.managed().map(|m| m.port_map.clone()).unwrap_or_default();
    Ok(Json(serde_json::json!({
        "data": { "name": offering.name, "status": offering.status.as_str(), "port_map": port_map }
    })))
}

async fn uproot_offering(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult {
    state.garden.uproot(&name).await?;
    Ok(Json(serde_json::json!({ "data": { "name": name, "uprooted": true } })))
}

/// The complete surface, built FROM the manifest (L9, R4.7): the router's
/// routes are exactly [`Face::ALL`] — nothing emits unadvertised, nothing
/// advertises unrouted.
pub fn router(state: Arc<AppState>) -> Router {
    let router = Face::ALL
        .iter()
        .fold(Router::new(), |r, face| r.route(face.path(), face.method_router()));
    router.with_state(state)
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::registry::{MemorySnapshotStore, Registry};
    use crate::offerings::runtime::{NullRuntime, RuntimeRegistry};
    use crate::source::{DynamicChirpSource, Voice};
    use axum::http::StatusCode;
    use garden_contract::chirp::ChirpFrame;
    use garden_kernel::topology::StoneView;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let worlds = Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)]));
        let factsheet = Arc::new(crate::offerings::facts::Factsheet::empty());
        let service = Arc::new(OfferingService::new(
            registry.clone(),
            worlds,
            "null".into(),
            Arc::new(crate::offerings::manifest::Catalog::default()),
            factsheet,
            crate::offerings::directory::OfferingsRoot::new(
                std::env::temp_dir().join(format!("moss-test-offer-{}", Uuid::now_v7())),
            ),
            crate::offerings::ports::Pool::default(),
        ));
        let chirp_source = DynamicChirpSource::new(
            Voice {
                stone_id: "0198e0c7-0000-7000-8000-000000000001".into(),
                stone_name: "stone-test".into(),
                http_port: 7285,
                moss_version: "1.0.0".into(),
            },
            "boot-test".into(),
            registry,
            Arc::new(crate::offerings::storage::Storage::new()),
        );
        Arc::new(AppState {
            garden: service,
            storage: Arc::new(crate::offerings::storage::Storage::new()),
            capture: Arc::new(crate::offerings::capture_run::Runner::new(
                Arc::new(crate::offerings::storage::Storage::new()),
                Arc::new(crate::offerings::capture_run::NullHooks),
            )),
            jobs: crate::jobs::JobTracker::new(),
            topology: Arc::new(garden_kernel::topology::Topology::new()),
            dispatcher: Dispatcher::new(16).0,
            ingest_counters: Arc::new(IngestCounters::default()),
            chirp_source,
            stone_name: "stone-test".into(),
            boot_id: Uuid::now_v7(),
            started_at: chrono::Utc::now(),
        })
    }

    async fn send(app: &Router, method: &str, path: &str) -> axum::http::Response<axum::body::Body> {
        let req = match method {
            "GET" => axum::http::Request::builder().uri(path).body(axum::body::Body::empty()),
            _ => axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}")),
        }
        .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    /// L7: self-description is generated truth — every manifest face
    /// answers through the real router. Static GET faces answer 200; the
    /// redirect face answers 404 by design ({ref} is nobody in a bare
    /// test state); every face must at least be ROUTED, never a method
    /// miss.
    #[tokio::test]
    async fn every_manifest_face_answers() {
        let app = router(test_state());

        for face in Face::ALL {
            let res = send(&app, face.method(), face.path()).await;
            assert_ne!(
                res.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{} {} must be routed",
                face.method(),
                face.path()
            );
            match face {
                Face::StoneRef => {
                    assert_eq!(res.status(), StatusCode::NOT_FOUND, "nobody by that ref here");
                }
                _ if face.method() == "GET" && !face.path().contains('{') => {
                    // Root redirects to the portrait; every other static GET answers.
                    let want = if face == Face::Root {
                        StatusCode::TEMPORARY_REDIRECT
                    } else {
                        StatusCode::OK
                    };
                    assert_eq!(
                        res.status(),
                        want,
                        "{} {} must answer",
                        face.method(),
                        face.path()
                    );
                }
                _ => {}
            }
        }
    }

    /// The grammar cut is CLEAN (ADR-0004 §4): no legacy aliases. The old
    /// spellings are dead — unrouted (404) or method-less (405), never a
    /// 200 wearing an old name.
    #[tokio::test]
    async fn legacy_spellings_are_dead() {
        let app = router(test_state());
        for path in [
            "/api/v1/manifest",
            "/api/v1/local/posture",
            "/api/v1/garden/observe",
            "/api/v1/stone/offerings/redis::default",
            "/api/v1/stone/offerings/redis::default/rest",
        ] {
            let res = send(&app, "GET", path).await;
            assert_ne!(
                res.status(),
                StatusCode::OK,
                "{path} must not answer under the old grammar"
            );
        }
    }

    /// The front door is the manifest, and the manifest is complete:
    /// the table lists every face, exactly once per (method, path).
    #[tokio::test]
    async fn front_door_lists_every_face_exactly_once() {
        let app = router(test_state());
        let res = send(&app, "GET", "/api/v1").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let routes = v["data"]["routes"].as_array().expect("routes array");
        assert_eq!(routes.len(), Face::ALL.len(), "every face advertised");

        let mut keys: Vec<(String, String)> = routes
            .iter()
            .map(|r| {
                (
                    r["method"].as_str().unwrap().into(),
                    r["path"].as_str().unwrap().into(),
                )
            })
            .collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "no duplicate (method, path) rows");
    }

    /// The SelfView: /stone speaks MY frame, full-voiced (B1 — the pull
    /// face renders the same canonical shape).
    #[tokio::test]
    async fn stone_self_is_my_frame() {
        let app = router(test_state());
        for path in ["/api/v1/stone", "/api/v1/stone/this", "/api/v1/stone/stone-test"] {
            let res = send(&app, "GET", path).await;
            assert_eq!(res.status(), StatusCode::OK, "{path} is me");
            let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["data"]["stone"]["name"], "stone-test", "{path}");
            assert_eq!(v["data"]["meta"]["boot_id"], "boot-test", "{path}");
        }
    }

    /// Land a peer frame in the topology through the real claim path —
    /// the same door the wire uses (R4.5: test the promise, not the guts).
    async fn wire_peer(topology: &Arc<garden_kernel::topology::Topology>, peer: StoneView) {
        let (dispatcher, handle) = Dispatcher::new(16);
        let token = tokio_util::sync::CancellationToken::new();
        topology.claim(&dispatcher, token.clone());
        tokio::spawn(handle.run(token.clone()));
        dispatcher
            .ingest(garden_kernel::ingress::Ingested {
                announcement: garden_contract::wire::Announcement::new(
                    garden_contract::consts::announcement::STONE_CHIRP,
                    serde_json::to_value(&peer.body).unwrap(),
                ),
                source: std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 51000)),
                received_at: chrono::Utc::now(),
            })
            .await;
        let mut version = topology.version();
        tokio::time::timeout(std::time::Duration::from_secs(2), version.changed())
            .await
            .expect("cache must settle")
            .expect("watch alive");
        token.cancel();
    }

    /// The delight face (ADR-0004 §4): asking for a peer by name is a
    /// not-here answer that teaches — 404, a Location header, and a
    /// knows_at field naming where the stone answers.
    #[tokio::test]
    async fn asking_for_a_peer_teaches_the_way() {
        let state = test_state();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/stone/stone-peer").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let location = res
            .headers()
            .get(axum::http::header::LOCATION)
            .expect("the way is named")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "http://192.168.1.50:7285/api/v1/stone");

        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["not_here"], true);
        assert_eq!(v["error"]["stone"], "stone-peer");
        assert_eq!(v["error"]["knows_at"], location);

        // An unknown name: a plain 404, no way to offer.
        let res = send(&app, "GET", "/api/v1/stone/nobody").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert!(res.headers().get(axum::http::header::LOCATION).is_none());
    }

    /// The splice (ADR-0004 §3): /garden/stones obviously includes the
    /// current stone, among the peers, every row a canonical frame.
    #[tokio::test]
    async fn garden_stones_splices_self_among_peers() {
        let state = test_state();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/garden/stones").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let stones = v["data"]["stones"].as_array().expect("stones array");
        assert_eq!(stones.len(), 2, "self + one peer");
        assert_eq!(stones[0]["self"], true, "self spliced first");
        assert_eq!(stones[0]["stone"]["name"], "stone-test");
        assert_eq!(stones[1]["stone"]["name"], "stone-peer");
        assert_eq!(stones[1]["chirps"], 1, "one accepted frame through the real door");
    }

    /// The adopt face routes and validates: a device no scan reports is a
    /// loud Conflict naming the problem (R3.3), never a silent empty.
    #[tokio::test]
    async fn adopt_refuses_unknown_devices_loudly() {
        let app = router(test_state());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/storage/adopt")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"device": "Q:", "name": "seed-vault"}"#,
            ))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(res.into_body(), 100_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no removable volume"),
            "the refusal teaches: {}",
            v["error"]["message"]
        );
    }

    /// The room's banks (ADR-0004 §4 grid): self spliced first, then the
    /// peer's banks as the cache heard them — end-to-end from song merge
    /// to surface.
    #[tokio::test]
    async fn garden_storage_projects_the_room() {
        let state = test_state();
        // Self holds a bank; the peer holds another (via its song frame).
        state
            .storage
            .adopt(
                &crate::offerings::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: std::path::PathBuf::from("E:\\tmp-adopt"),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 4000,
                    available_bytes: 3000,
                },
                "local-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);

        let res = send(&app, "GET", "/api/v1/garden/storage").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let rows = v["data"]["banks"].as_array().expect("rows");
        assert_eq!(rows.len(), 2, "self + peer");
        assert_eq!(rows[0]["self"], true);
        assert_eq!(rows[0]["bank"]["fqn"], "local-vault::default");
        assert_eq!(rows[1]["stone"], "stone-peer");
        assert_eq!(rows[1]["bank"]["fqn"], "seed-vault::default");
        assert_eq!(rows[1]["bank"]["state"], "mounted");
    }

    /// The eject verb's happy path: adopted banks eject, the state sings,
    /// and the refusal cases stay loud (R3.3).
    #[tokio::test]
    async fn eject_announces_authoritative_absence() {
        let state = test_state();
        state
            .storage
            .adopt(
                &crate::offerings::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: std::path::PathBuf::from("E:\\tmp-eject"),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1000,
                    available_bytes: 900,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        let app = router(state);

        let res = send(&app, "POST", "/api/v1/storage/seed-vault/eject").await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 100_000).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"]["bank"]["state"], "ejected");

        // Ejecting twice is a conflict; ejecting a ghost is a 404.
        let res = send(&app, "POST", "/api/v1/storage/seed-vault/eject").await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let res = send(&app, "POST", "/api/v1/storage/nobody/eject").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    // ---- bank files: the storage data plane over the real router -------

    /// A test state whose stone holds a bank on a real temp volume.
    async fn state_with_bank() -> (Arc<AppState>, std::path::PathBuf) {
        let state = test_state();
        let tmp = std::env::temp_dir().join(format!("zg-http-files-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        state
            .storage
            .adopt(
                &crate::offerings::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1_000_000,
                    available_bytes: 900_000,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        (state, tmp)
    }

    /// Like `send`, but carrying a raw body (the file verbs' payloads).
    async fn send_bytes(
        app: &Router,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> axum::http::Response<axum::body::Body> {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/octet-stream")
            .body(axum::body::Body::from(body.to_vec()))
            .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    async fn body_json(res: &mut axum::http::Response<axum::body::Body>) -> serde_json::Value {
        let bytes = axum::body::to_bytes(std::mem::take(res).into_body(), 1_000_000)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// The CRUD roundtrip: put creates parents and reports size, get rides
    /// the raw bytes with a guessed type, list shows files but never the
    /// adoption record, delete removes, and the gone file answers 404.
    #[tokio::test]
    async fn bank_files_crud_over_http() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        let mut res =
            send_bytes(&app, "PUT", &format!("{base}/dumps/notes.txt"), b"hello bank").await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        assert_eq!(v["data"]["size_bytes"], 10, "the write is sized honestly");
        assert_eq!(v["data"]["bank"], "seed-vault::default");

        let res = send(&app, "GET", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()[axum::http::header::CONTENT_TYPE],
            "text/plain; charset=utf-8",
            "the extension guesses the type"
        );
        let bytes = axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap();
        assert_eq!(&bytes[..], b"hello bank", "the raw bytes ride alone");

        let mut res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        let rows = v["data"]["files"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "only dumps shows — the adoption record is invisible");
        assert_eq!(rows[0]["name"], "dumps");
        assert_eq!(rows[0]["kind"], "dir");

        let mut res = send(&app, "GET", &format!("{base}?path=dumps")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        let rows = v["data"]["files"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "notes.txt");
        assert_eq!(rows[0]["size_bytes"], 10);

        let mut res = send(&app, "DELETE", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = body_json(&mut res).await;
        assert_eq!(v["data"]["deleted"], true);

        let res = send(&app, "GET", &format!("{base}/dumps/notes.txt")).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "gone is gone");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The escape laws hold at the wire: `..` (raw or percent-spelled)
    /// and the adoption record refuse with a 400 that teaches.
    #[tokio::test]
    async fn file_paths_refuse_escapes_and_the_manifest() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        for path in [
            format!("{base}/..%2Fsecret"),
            format!("{base}/ok/../../secret"),
            format!("{base}/.zen-garden/manifest.json"),
        ] {
            let mut res = send(&app, "GET", &path).await;
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{path}");
            let v = body_json(&mut res).await;
            assert!(
                !v["error"]["message"].as_str().unwrap_or("").is_empty(),
                "{path} must teach"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The gates before the filesystem: unknown banks 404, ejected banks
    /// 409 — even before a byte is asked for.
    #[tokio::test]
    async fn file_faces_refuse_unknown_and_ejected_banks() {
        let (state, tmp) = state_with_bank().await;
        let mut res = send(
            &router(state.clone()),
            "GET",
            "/api/v1/storage/ghost/files",
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let v = body_json(&mut res).await;
        assert!(
            v["error"]["message"].as_str().unwrap().contains("no bank"),
            "the refusal names the miss: {}",
            v["error"]["message"]
        );

        state.storage.eject("seed-vault").unwrap();
        let mut res = send(
            &router(state),
            "GET",
            "/api/v1/storage/seed-vault/files/dumps/notes.txt",
        )
        .await;
        assert_eq!(res.status(), StatusCode::CONFLICT, "ejected: no volume");
        let v = body_json(&mut res).await;
        assert!(
            v["error"]["message"].as_str().unwrap().contains("ejected"),
            "{}",
            v["error"]["message"]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Verbs and kinds agree at the wire: reading or deleting a directory
    /// conflicts — the path is real, the verb does not apply.
    #[tokio::test]
    async fn directories_refuse_the_file_verbs() {
        let (state, tmp) = state_with_bank().await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";
        send_bytes(&app, "PUT", &format!("{base}/dumps/a.txt"), b"x")
            .await;

        let res = send(&app, "GET", &format!("{base}/dumps")).await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let res = send(&app, "DELETE", &format!("{base}/dumps")).await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A bank grows on ONE stone: the files faces landing where the
    /// volume is not answer the garden's only true redirect — 404, a
    /// Location, and `knows_at` naming the holder (1:1 with the stone
    /// face). Writes bind the same way; a bank nobody holds is a plain
    /// 404; local presence beats the room's stale claim.
    #[tokio::test]
    async fn a_peers_bank_teaches_the_way() {
        let state = test_state();
        // The peer's song carries seed-vault::default at 192.168.1.50.
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);
        let base = "/api/v1/storage/seed-vault/files";

        let res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let location = res
            .headers()
            .get(axum::http::header::LOCATION)
            .expect("the way is named")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "http://192.168.1.50:7285/api/v1/stone");

        let mut res = send(&app, "GET", base).await;
        let v = body_json(&mut res).await;
        assert_eq!(v["error"]["not_here"], true);
        assert_eq!(v["error"]["bank"], "seed-vault::default");
        assert_eq!(v["error"]["knows_at"], location);

        let res = send_bytes(&app, "PUT", &format!("{base}/notes.txt"), b"x").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "writes redirect too");
        assert_eq!(
            res.headers().get(axum::http::header::LOCATION).unwrap(),
            "http://192.168.1.50:7285/api/v1/stone"
        );

        let res = send(&app, "GET", "/api/v1/storage/ghost/files").await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "nobody holds it");
        assert!(res.headers().get(axum::http::header::LOCATION).is_none());

        // Local presence wins: the same FQN adopted HERE answers HERE,
        // even though the peer's song claims the name too.
        let state = test_state();
        let tmp = std::env::temp_dir().join(format!("zg-local-wins-{}", Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        state
            .storage
            .adopt(
                &crate::offerings::storage::VolumeFact {
                    roles: Vec::new(),
                    mount_point: tmp.clone(),
                    device_id: None,
                    fqn: None,
                    capacity_bytes: 1,
                    available_bytes: 1,
                },
                "seed-vault",
                "0198e0c7-0000-7000-8000-000000000001",
            )
            .unwrap();
        wire_peer(&state.topology, sample_peer()).await;
        let app = router(state);
        let res = send(&app, "GET", base).await;
        assert_eq!(res.status(), StatusCode::OK, "the volume is in MY slot");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn sample_peer() -> StoneView {
        use garden_contract::chirp::{
            Inventory, Moss, Network, PeerAddress, Presence, Reception, ServiceEntry, ServiceState,
            Stone,
        };
        let now = chrono::Utc::now();
        StoneView {
            body: ChirpFrame {
                stone: Stone {
                    id: "0198e0c7-0000-7000-8000-0000000000ef".into(),
                    name: "stone-peer".into(),
                    moss: Moss { version: "0.1.0".into() },
                    network: Network {
                        address: PeerAddress {
                            ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 50)),
                            port: 7285,
                            tls_port: None,
                        },
                        mac: None,
                    },
                },
                presence: Presence {
                    health: garden_glossary::health::THRIVING.into(),
                    status: garden_glossary::presence::ONLINE.into(),
                },
                inventory: garden_contract::chirp::InventoryMap {
                    services: Some(Inventory {
                        rev: Some(1),
                        total: None,
                        items: vec![ServiceEntry {
                            offering_id: String::new(),
                            name: "mongodb::default".into(),
                            stem: "mongodb".into(),
                            category: "data".into(),
                            state: ServiceState { status: "running".into(), role: None },
                            ports: Default::default(),
                        }],
                    }),
                    banks: Some(Inventory {
                        rev: Some(2),
                        total: None,
                        items: vec![garden_contract::chirp::BankEntry {
                            fqn: "seed-vault::default".into(),
                            device_id: "dev-peer".into(),
                            state: "mounted".into(),
                            roles: vec![garden_glossary::bank::role::SINK.into()],
                            capacity_bytes: Some(1_000_000),
                            used_bytes: Some(10),
                        }],
                    }),
                    ..Default::default()
                },
                meta: garden_contract::chirp::FrameMeta {
                    proto: Some(PROTO_V1.into()),
                    boot_id: None,
                    seq: Some(7),
                    part: None,
                },
                received: Reception { discovered_at: now, last_seen: now },
            },
            last_seen: now,
            chirps: 3,
        }
    }

    /// B1: the cache, HTTP, and the wire render ONE canonical shape — the
    /// sectioned frame — with reception facts filled by the listener.
    #[test]
    fn observe_stone_renders_the_canonical_shape() {
        let peer = sample_peer();
        let mut v = serde_json::to_value(&peer.body).unwrap();
        v.as_object_mut().unwrap().insert("chirps".into(), serde_json::json!(3));
        assert_eq!(v["stone"]["name"], "stone-peer");
        assert_eq!(v["stone"]["network"]["address"]["port"], 7285);
        assert_eq!(v["meta"]["proto"], PROTO_V1);
        assert_eq!(v["chirps"], 3);
    }
}
