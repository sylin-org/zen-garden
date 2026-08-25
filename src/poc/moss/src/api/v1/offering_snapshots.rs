//! Snapshot capture + read endpoints for an offering instance.
//!
//! M2 ships local-disk snapshots (per ORCH-0039 §M2 cut). The
//! bank-backed target lands in commit S5; the route shape stays
//! the same — only the [`SnapshotStore`] adapter swaps.
//!
//! Endpoints:
//!
//! | Method | Path | Purpose |
//! |---|---|---|
//! | POST | `/offerings/{name}/snapshots` | Capture a new snapshot |
//! | GET | `/offerings/{name}/snapshots` | List snapshot ids for `name` |
//! | GET | `/offerings/{name}/snapshots/{id}` | Full manifest |
//! | GET | `/offerings/{name}/snapshots/{id}/files/{kind}/{name}` | Stream one artifact |
//! | DELETE | `/offerings/{name}/snapshots/{id}` | Remove a snapshot |
//!
//! The artifact-fetch path uses two segments (`kind`, `name`)
//! rather than a free-form `*path` so traversal is impossible
//! by construction: `kind ∈ {image, volume, external_mount}`
//! determines which store helper computes the actual filesystem
//! path. The `name` segment is sanitised to a single
//! filesystem path component before it ever reaches a join.

use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use futures_util::TryStreamExt;
use garden_common::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use crate::Moss;
use crate::api::ApiResult;
use crate::bad_request;
use crate::domain::offering_events::{EventActor, EventLog};
use crate::domain::snapshot::{LocalSnapshotStore, SnapshotManifest, SnapshotStore};
use crate::infra::api_helpers::{internal, not_found};

/// Optional request body for snapshot capture. All fields are
/// optional so a bare `POST` with no body produces a default
/// snapshot (local disk, system actor). Frontend / Rake send
/// the body when the user picks a non-default target or
/// attribution.
#[derive(Debug, Deserialize, Default)]
pub struct CaptureSnapshotRequest {
    /// Where to persist the snapshot. Two forms:
    ///
    /// - `"local"` (or absent) — `<data_dir>/snapshots/<fqn-encoded>`
    /// - `"bank:<bank_name>"` — under the named bank's mount,
    ///   at `<mount>/snapshots/<fqn-encoded>`
    ///
    /// See [`SnapshotTarget::parse`] for the canonical parser.
    #[serde(default)]
    pub target: Option<String>,
    /// User identifier for the event log's actor field.
    /// Omitted means system-driven (periodic scheduler, etc).
    #[serde(default)]
    pub user: Option<String>,
}

/// Resolved target the capture handler writes to. Pulled out as
/// an enum so the parser is testable in isolation and adding
/// future targets (registry-backed image stores, off-LAN
/// archives) is a single match arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotTarget {
    /// Default — `<data_dir>/snapshots/<encoded_fqn>/`.
    Local,
    /// Under a named storage bank's local mount.
    Bank(String),
}

impl SnapshotTarget {
    /// Parse the wire form (`Option<&str>` from the request body)
    /// into a typed target. `None` and `"local"` both produce
    /// `Local`. `"bank:<name>"` produces `Bank(name)` after
    /// stripping the prefix and rejecting empty names. Anything
    /// else is an error so a typo can't silently fall back to
    /// local disk.
    pub fn parse(s: Option<&str>) -> Result<Self, String> {
        match s {
            None => Ok(SnapshotTarget::Local),
            Some(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("local") {
                    Ok(SnapshotTarget::Local)
                } else if let Some(name) = trimmed.strip_prefix("bank:") {
                    let name = name.trim();
                    if name.is_empty() {
                        Err("Snapshot target 'bank:' must include a bank name".to_string())
                    } else {
                        Ok(SnapshotTarget::Bank(name.to_string()))
                    }
                } else {
                    Err(format!(
                        "Snapshot target '{raw}' is not recognised; expected 'local' or 'bank:<name>'"
                    ))
                }
            }
        }
    }
}

/// Initial capture response — returned synchronously when the POST
/// is accepted. The caller subscribes to
/// `GET /api/v1/jobs/{job_id}/stream` (or polls
/// `GET /api/v1/jobs/{job_id}`) to track progress and read the final
/// `CapturedSnapshotResult` once the job reaches `Completed`.
///
/// Per ORCH-0039 Item 2 §"async API surface": capture is now an
/// async submission rather than a synchronous call. The handler
/// returns this shape immediately and a tokio task runs the actual
/// capture, emitting per-step JobEvents and storing the final
/// `CapturedSnapshotResult` on the job's `result` field on
/// completion.
#[derive(Debug, Serialize)]
pub struct CaptureSnapshotResponse {
    /// Job id to subscribe to for progress.
    pub job_id: String,
    /// Operation name — `"capture_snapshot"`. Lets clients dispatch on
    /// shape when they receive job events without keeping a separate
    /// id→operation map.
    pub operation: &'static str,
}

/// Final capture result — stored as the Job's `result` field on
/// completion. SSE consumers see this in the `job.snapshot` frame
/// once the job is `Completed`; HTTP GET on `/jobs/{id}` returns it
/// as part of the Job shape.
#[derive(Debug, Serialize)]
pub struct CapturedSnapshotResult {
    pub snapshot_id: String,
    pub event_id: String,
    pub source_fqn: String,
    pub source_stone: String,
    pub size_total_bytes: u64,
    pub volumes: usize,
    pub external_mounts: usize,
}

/// `POST /api/v1/stone/offerings/{name}/snapshots`
///
/// Capture a snapshot of `name`'s current state into the chosen
/// target. The flow is: commit container image → save image tar
/// → archive volumes + external mounts → record `BackupTaken`
/// event → save manifest → truncate event log. See
/// [`crate::infra::snapshot::capture_snapshot`] for the full
/// orchestration.
pub async fn capture_offering_snapshot_v1(
    State(state): State<Moss>,
    Path(offering_name): Path<String>,
    body: Option<Json<CaptureSnapshotRequest>>,
) -> crate::api::ApiResult<CaptureSnapshotResponse> {
    // ── Synchronous validation ───────────────────────────────────
    // Anything that should produce a 4xx (invalid input) or 404
    // (missing bank) runs before we submit the job, so the API
    // returns a sync error rather than an async-failed job for bad
    // requests.
    let fqn = OfferingFqn::parse(&offering_name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering_name, e),
        )
    })?;

    let request = body.map(|Json(b)| b).unwrap_or_default();
    let target = SnapshotTarget::parse(request.target.as_deref()).map_err(|e| {
        bad_request("UNSUPPORTED_SNAPSHOT_TARGET", e.to_string())
    })?;

    let snapshot_root = match &target {
        SnapshotTarget::Local => local_snapshot_root(&fqn),
        SnapshotTarget::Bank(bank_name) => {
            let bank = crate::domain::storage::bank_aggregate::by_name(
                bank_name,
                &state.current.storage.volumes,
            )
            .await
            .ok_or_else(|| {
                not_found(
                    "BANK_NOT_FOUND",
                    format!("Bank '{bank_name}' has no managed online volume on this stone"),
                )
            })?;
            let mount = bank.mount_path.clone().ok_or_else(|| {
                internal(
                    "BANK_NOT_MOUNTED",
                    format!("Bank '{bank_name}' has no mount path"),
                )
            })?;
            mount.join("snapshots").join(fqn.encoded_for_container())
        }
    };

    let actor = match request.user.clone() {
        Some(user) => EventActor::user(state.current.stone.name.clone(), user),
        None => EventActor::system(state.current.stone.name.clone()),
    };

    // ── Submit job + spawn worker ────────────────────────────────
    let job_id = garden_common::utils::ids::generate_guidv7();
    let fqn_string = fqn.fqn();

    state
        .jobs
        .submit(job_id.clone(), "capture_snapshot", vec![fqn_string.clone()])
        .await;
    state
        .jobs
        .start(&job_id, "capture_snapshot", &fqn_string)
        .await;

    // Clone everything the spawned task needs to outlive this handler.
    let state_for_task = state.clone();
    let job_id_for_task = job_id.clone();
    let fqn_for_task = fqn.clone();
    let log_path = offering_event_log_path(&fqn);

    tokio::spawn(async move {
        let store = LocalSnapshotStore::new(snapshot_root);
        let log = EventLog::open(log_path);
        let fqn_string = fqn_for_task.fqn();

        match crate::infra::snapshot::capture_snapshot(
            &state_for_task,
            &fqn_for_task,
            &store,
            &log,
            actor,
            Some(&job_id_for_task),
        )
        .await
        {
            Ok(captured) => {
                let result = CapturedSnapshotResult {
                    snapshot_id: captured.manifest.id.clone(),
                    event_id: captured.event_id.clone(),
                    source_fqn: captured.manifest.source_fqn.clone(),
                    source_stone: captured.manifest.source_stone.clone(),
                    size_total_bytes: captured.manifest.size_total_bytes,
                    volumes: captured.manifest.volumes.len(),
                    external_mounts: captured.manifest.external_mounts.len(),
                };
                let result_json = serde_json::to_value(&result).unwrap_or_else(|e| {
                    tracing::error!(error = %e, "serialise capture result");
                    serde_json::json!({})
                });
                state_for_task
                    .jobs
                    .complete_with_result(&job_id_for_task, &fqn_string, result_json)
                    .await;
                tracing::info!(
                    job_id = %job_id_for_task,
                    snapshot_id = %captured.manifest.id,
                    "capture_snapshot job completed"
                );
            }
            Err(e) => {
                tracing::error!(
                    job_id = %job_id_for_task,
                    offering = %fqn_string,
                    error = %e,
                    "capture_snapshot job failed"
                );
                state_for_task
                    .jobs
                    .fail(
                        &job_id_for_task,
                        &fqn_string,
                        Some((fqn_string.clone(), format!("{e:#}"))),
                    )
                    .await;
            }
        }
    });

    crate::api::ok(CaptureSnapshotResponse {
        job_id,
        operation: "capture_snapshot",
    })
}

/// `GET /api/v1/stone/offerings/{name}/snapshots`
///
/// Catalog of snapshot ids for `name`, in chronological order
/// (lexicographic GUIDV7 sort). Empty when no snapshots exist.
pub async fn list_offering_snapshots_v1(
    State(_state): State<Moss>,
    Path(offering_name): Path<String>,
) -> ApiResult<ListSnapshotsResponse> {
    let fqn = parse_fqn(&offering_name)?;
    let store = LocalSnapshotStore::new(local_snapshot_root(&fqn));
    let ids = store
        .list_ids()
        .await
        .map_err(|e| internal("SNAPSHOT_LIST_FAILED", e.to_string()))?;
    crate::api::ok(ListSnapshotsResponse {
        offering: fqn.fqn(),
        snapshots: ids,
    })
}

/// `GET /api/v1/stone/offerings/{name}/snapshots/{id}`
///
/// Returns the full [`SnapshotManifest`]. This is also the
/// "preview" endpoint for the seed-as-noun UX: clients can
/// fetch the manifest alone (KB-sized) before deciding whether
/// to plant or download the full archive (potentially GB-sized).
pub async fn get_offering_snapshot_manifest_v1(
    State(_state): State<Moss>,
    Path((offering_name, snapshot_id)): Path<(String, String)>,
) -> ApiResult<SnapshotManifest> {
    let fqn = parse_fqn(&offering_name)?;
    let store = LocalSnapshotStore::new(local_snapshot_root(&fqn));
    match store.load_manifest(&snapshot_id).await {
        Ok(m) => crate::api::ok(m),
        Err(e) => Err(load_manifest_error(&snapshot_id, e)),
    }
}

/// `GET /api/v1/stone/offerings/{name}/snapshots/{id}/files/{kind}/{artifact}`
///
/// Stream a single artifact from a snapshot. `kind` selects
/// which store helper computes the path:
///
/// - `image` — the offering's docker-save tarball. `artifact`
///   must be exactly `image.tar`.
/// - `volume` — a managed-volume archive. `artifact` is the
///   volume's display name (e.g. `data`); the store appends
///   the `.tar.gz` extension.
/// - `external_mount` — an external-mount archive. `artifact`
///   is the *encoded* host path (the same encoding the store
///   uses on write).
///
/// `artifact` is sanitised to disallow traversal: any
/// `..`, `/`, or `\\` rejects with 400. The actual filesystem
/// path is derived by the store, so the only way to reach a
/// file outside the snapshot is to subvert that derivation —
/// which the validation above prevents.
pub async fn get_offering_snapshot_artifact_v1(
    State(_state): State<Moss>,
    Path((offering_name, snapshot_id, kind, artifact)): Path<(String, String, String, String)>,
) -> Result<Response, (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    let fqn = parse_fqn(&offering_name)?;
    if !is_safe_artifact_name(&artifact) {
        return Err(bad_request(
            "INVALID_ARTIFACT_NAME",
            "artifact name must not contain path separators or '..' segments",
        ));
    }
    let store = LocalSnapshotStore::new(local_snapshot_root(&fqn));

    // Verify the manifest exists before serving any bytes —
    // gives 404 instead of "file not found" leakage.
    store
        .load_manifest(&snapshot_id)
        .await
        .map_err(|e| load_manifest_error(&snapshot_id, e))?;

    let path: PathBuf = match kind.as_str() {
        "image" => {
            if artifact != "image.tar" {
                return Err(bad_request(
                    "UNKNOWN_IMAGE_ARTIFACT",
                    format!("expected 'image.tar', got '{artifact}'"),
                ));
            }
            store.image_path(&snapshot_id)
        }
        "volume" => store.volume_path(&snapshot_id, &artifact),
        "external_mount" => {
            // For external mounts the on-disk filename already
            // encodes the host path, so the store's helper for
            // looking up by host path produces the wrong filename
            // (it re-encodes). Bypass the helper and resolve the
            // already-encoded artifact directly.
            store
                .root()
                .join(&snapshot_id)
                .join("external_mounts")
                .join(format!("{artifact}.tar.gz"))
        }
        other => {
            return Err(bad_request(
                "UNKNOWN_ARTIFACT_KIND",
                format!(
                    "artifact kind '{other}' is not supported; expected 'image', 'volume', or 'external_mount'"
                ),
            ));
        }
    };

    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(not_found(
                "SNAPSHOT_ARTIFACT_NOT_FOUND",
                format!("artifact '{kind}/{artifact}' not present in snapshot {snapshot_id}"),
            ));
        }
        Err(e) => {
            return Err(internal(
                "SNAPSHOT_ARTIFACT_OPEN_FAILED",
                format!("open {}: {}", path.display(), e),
            ));
        }
    };

    let size = file
        .metadata()
        .await
        .map(|m| m.len())
        .ok();
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream.map_err(std::io::Error::other));

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream");
    if let Some(size) = size {
        builder = builder.header(header::CONTENT_LENGTH, size);
    }
    builder
        .body(body)
        .map_err(|e| internal("RESPONSE_BUILD_FAILED", e.to_string()))
}

/// `DELETE /api/v1/stone/offerings/{name}/snapshots/{id}`
///
/// Remove a snapshot's directory entirely. Idempotent — a
/// delete for an unknown id is `Ok(())`. Used for explicit user
/// cleanup; periodic retention will land separately as a
/// background sweeper in commit S6.
pub async fn delete_offering_snapshot_v1(
    State(_state): State<Moss>,
    Path((offering_name, snapshot_id)): Path<(String, String)>,
) -> ApiResult<DeleteSnapshotResponse> {
    let fqn = parse_fqn(&offering_name)?;
    let store = LocalSnapshotStore::new(local_snapshot_root(&fqn));
    store
        .delete(&snapshot_id)
        .await
        .map_err(|e| internal("SNAPSHOT_DELETE_FAILED", e.to_string()))?;
    crate::api::ok(DeleteSnapshotResponse {
        offering: fqn.fqn(),
        snapshot_id,
    })
}

/// Plant request body.
///
/// - `from_snapshot` (required): the snapshot id to plant from.
/// - `from_stone` (optional): if set, fetch the snapshot from
///   the named peer stone first, materialise it locally, then
///   plant. Omitted = local snapshot store only.
/// - `from_fqn` (optional): the source FQN — only meaningful
///   with `from_stone`. Defaults to the URL FQN.
/// - `as_fqn` (optional): override the planted offering's FQN.
///   Defaults to the URL FQN. Used for "fork" — derive a new
///   instance from existing seeded data.
/// - `user` (optional): attribution for the RestoreApplied
///   event.
#[derive(Debug, Deserialize)]
pub struct PlantSnapshotRequest {
    pub from_snapshot: String,
    #[serde(default)]
    pub from_stone: Option<String>,
    #[serde(default)]
    pub from_fqn: Option<String>,
    #[serde(default)]
    pub as_fqn: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
}

/// Initial plant response. Mirrors `CaptureSnapshotResponse`: the
/// handler returns this synchronously when the POST is accepted, the
/// final `PlantedSnapshotResult` is delivered via the job's `result`
/// once the spawned task reaches `Completed`.
#[derive(Debug, Serialize)]
pub struct PlantSnapshotResponse {
    pub job_id: String,
    pub operation: &'static str,
}

/// Final plant result — stored on the Job's `result` field. Same
/// shape as the previous synchronous response so consumers can
/// decode it identically off the SSE terminal frame or off
/// `GET /api/v1/jobs/{id}`.
#[derive(Debug, Serialize)]
pub struct PlantedSnapshotResult {
    pub snapshot_id: String,
    pub event_id: String,
    pub source_fqn: String,
    pub target_fqn: String,
    pub digest_drift: String,
}

/// `POST /api/v1/stone/offerings/{name}/plant`
///
/// Plant a local snapshot onto this stone as `name`. M2 reads
/// snapshots from the local catalog under
/// `<data_dir>/snapshots/<encoded_fqn>/`; cross-stone fetch
/// (commit P2) extends this by downloading the snapshot from a
/// remote stone first, then taking the same plant path.
///
/// `name` in the URL is the **target** FQN — the FQN this stone
/// runs after the plant completes. The snapshot's recorded
/// `source_fqn` may match (restore in place) or differ (fork or
/// cross-FQN seed). When the body's `as_fqn` is set, the URL
/// FQN is ignored in favour of `as_fqn` — useful when the
/// caller wants a single endpoint shape regardless of fork
/// intent.
pub async fn plant_offering_snapshot_v1(
    State(state): State<Moss>,
    Path(target_offering_name): Path<String>,
    Json(request): Json<PlantSnapshotRequest>,
) -> ApiResult<PlantSnapshotResponse> {
    // ── Synchronous validation ───────────────────────────────────
    if request.from_snapshot.trim().is_empty() {
        return Err(bad_request(
            "MISSING_SNAPSHOT_ID",
            "from_snapshot is required",
        ));
    }

    let target_fqn_string = request
        .as_fqn
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(target_offering_name.clone());
    let target_fqn = parse_fqn(&target_fqn_string)?;

    let actor = match request.user.clone() {
        Some(user) => EventActor::user(state.current.stone.name.clone(), user),
        None => EventActor::system(state.current.stone.name.clone()),
    };

    // Resolve `from_stone` / `from_fqn` early so a mistyped source
    // FQN returns a 4xx synchronously rather than failing the job
    // asynchronously.
    let from_stone_opt = request
        .from_stone
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(String::from);
    let from_fqn_opt = if from_stone_opt.is_some() {
        let source_fqn_string = request
            .from_fqn
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(target_offering_name);
        Some(parse_fqn(&source_fqn_string)?)
    } else {
        None
    };

    // ── Submit job + spawn worker ────────────────────────────────
    let job_id = garden_common::utils::ids::generate_guidv7();
    let target_fqn_str = target_fqn.fqn();

    state
        .jobs
        .submit(job_id.clone(), "plant_snapshot", vec![target_fqn_str.clone()])
        .await;
    state
        .jobs
        .start(&job_id, "plant_snapshot", &target_fqn_str)
        .await;

    let state_for_task = state.clone();
    let job_id_for_task = job_id.clone();
    let target_fqn_for_task = target_fqn.clone();
    let from_snapshot = request.from_snapshot.clone();
    let snapshot_root_for_task = local_snapshot_root(&target_fqn);
    let log_path_for_task = offering_event_log_path(&target_fqn);

    tokio::spawn(async move {
        let store = LocalSnapshotStore::new(snapshot_root_for_task);
        let log = EventLog::open(log_path_for_task);
        let target_fqn_str = target_fqn_for_task.fqn();

        // Cross-stone fetch (commit P2 path) — runs inside the job's
        // task so any fetch failure surfaces as a job failure rather
        // than a sync 5xx.
        if let (Some(source_stone), Some(source_fqn)) = (from_stone_opt, from_fqn_opt) {
            if let Err(e) = crate::infra::plant::fetch_snapshot_from_stone(
                &state_for_task,
                &source_stone,
                &source_fqn,
                &from_snapshot,
                &store,
            )
            .await
            {
                tracing::error!(
                    job_id = %job_id_for_task,
                    source_stone,
                    source_fqn = %source_fqn.fqn(),
                    snapshot_id = %from_snapshot,
                    error = %e,
                    "Cross-stone snapshot fetch failed"
                );
                state_for_task
                    .jobs
                    .fail(
                        &job_id_for_task,
                        &target_fqn_str,
                        Some((target_fqn_str.clone(), format!("fetch from {source_stone}: {e:#}"))),
                    )
                    .await;
                return;
            }
        }

        match crate::infra::plant::plant_from_local_snapshot(
            &state_for_task,
            &target_fqn_for_task,
            &store,
            &from_snapshot,
            &log,
            actor,
            Some(&job_id_for_task),
        )
        .await
        {
            Ok(planted) => {
                let result = PlantedSnapshotResult {
                    snapshot_id: planted.snapshot_id.clone(),
                    event_id: planted.event_id.clone(),
                    source_fqn: planted.source_fqn.clone(),
                    target_fqn: planted.target_fqn.clone(),
                    digest_drift: format!("{:?}", planted.digest_drift).to_lowercase(),
                };
                let result_json = serde_json::to_value(&result).unwrap_or_else(|e| {
                    tracing::error!(error = %e, "serialise plant result");
                    serde_json::json!({})
                });
                state_for_task
                    .jobs
                    .complete_with_result(&job_id_for_task, &target_fqn_str, result_json)
                    .await;
                tracing::info!(
                    job_id = %job_id_for_task,
                    snapshot_id = %planted.snapshot_id,
                    target_fqn = %planted.target_fqn,
                    "plant_snapshot job completed"
                );
            }
            Err(e) => {
                tracing::error!(
                    job_id = %job_id_for_task,
                    target_fqn = %target_fqn_str,
                    error = %e,
                    "plant_snapshot job failed"
                );
                state_for_task
                    .jobs
                    .fail(
                        &job_id_for_task,
                        &target_fqn_str,
                        Some((target_fqn_str.clone(), format!("{e:#}"))),
                    )
                    .await;
            }
        }
    });

    crate::api::ok(PlantSnapshotResponse {
        job_id,
        operation: "plant_snapshot",
    })
}

/// Catalog response for `GET /offerings/{name}/snapshots`.
#[derive(Debug, Serialize)]
pub struct ListSnapshotsResponse {
    pub offering: String,
    pub snapshots: Vec<String>,
}

/// Confirmation response for `DELETE /offerings/{name}/snapshots/{id}`.
#[derive(Debug, Serialize)]
pub struct DeleteSnapshotResponse {
    pub offering: String,
    pub snapshot_id: String,
}

fn parse_fqn(
    name: &str,
) -> Result<OfferingFqn, (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>)> {
    OfferingFqn::parse(name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", name, e),
        )
    })
}

/// Map a `load_manifest` failure to a 404 when the file is
/// absent and a 500 otherwise. Distinguishing "this snapshot
/// doesn't exist" (the user can recover) from "the store is
/// broken" (the operator should look) costs nothing at this
/// layer.
fn load_manifest_error(
    snapshot_id: &str,
    e: anyhow::Error,
) -> (StatusCode, Json<garden_common::api_utils::ApiErrorResponse>) {
    if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
        if io_err.kind() == std::io::ErrorKind::NotFound {
            return not_found(
                "SNAPSHOT_NOT_FOUND",
                format!("snapshot '{snapshot_id}' not found"),
            );
        }
    }
    if e.to_string().contains("os error 2") || e.to_string().contains("system cannot find") {
        // io::ErrorKind::NotFound surfaces through anyhow with
        // platform-specific text; do a textual fallback for the
        // cases where downcast_ref doesn't reach the io::Error.
        return not_found(
            "SNAPSHOT_NOT_FOUND",
            format!("snapshot '{snapshot_id}' not found"),
        );
    }
    internal("SNAPSHOT_MANIFEST_LOAD_FAILED", e.to_string())
}

/// Reject filenames that could escape the snapshot directory.
/// Public so the unit test below can exercise the same logic
/// the production handler relies on.
pub(crate) fn is_safe_artifact_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    if name == ".." || name == "." {
        return false;
    }
    // Defensive: reject NUL and other control characters that
    // some filesystems treat oddly.
    if name.chars().any(|c| c.is_control()) {
        return false;
    }
    true
}

/// Resolve the local-disk snapshot root for an FQN.
/// `<data_dir>/snapshots/<encoded_fqn>/`. Each FQN gets its own
/// catalog so listing is fast and deletion is bounded.
fn local_snapshot_root(fqn: &OfferingFqn) -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir())
        .join("snapshots")
        .join(fqn.encoded_for_container())
}

/// Resolve the per-offering event log path.
/// `<data_dir>/offerings/<encoded_fqn>/events.log` — the
/// sidecar location ORCH-0039 §"Event log" specifies.
fn offering_event_log_path(fqn: &OfferingFqn) -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir())
        .join("offerings")
        .join(fqn.encoded_for_container())
        .join("events.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_snapshot_root_uses_encoded_fqn() {
        let fqn = OfferingFqn::parse("mongodb::prd").unwrap();
        let root = local_snapshot_root(&fqn);
        assert!(
            root.ends_with("snapshots/mongodb--prd"),
            "expected snapshots/mongodb--prd suffix: {}",
            root.display()
        );
    }

    #[test]
    fn offering_event_log_path_lives_under_per_fqn_dir() {
        let fqn = OfferingFqn::parse("mongodb::prd").unwrap();
        let path = offering_event_log_path(&fqn);
        assert!(
            path.ends_with("offerings/mongodb--prd/events.log"),
            "expected offerings/mongodb--prd/events.log suffix: {}",
            path.display()
        );
    }

    #[test]
    fn capture_snapshot_request_default_is_no_target_no_user() {
        let req = CaptureSnapshotRequest::default();
        assert!(req.target.is_none());
        assert!(req.user.is_none());
    }

    #[test]
    fn capture_snapshot_request_deserializes_target_and_user() {
        let req: CaptureSnapshotRequest =
            serde_json::from_str(r#"{"target":"local","user":"leo"}"#).unwrap();
        assert_eq!(req.target.as_deref(), Some("local"));
        assert_eq!(req.user.as_deref(), Some("leo"));
    }

    #[test]
    fn capture_snapshot_request_accepts_empty_object() {
        // Empty body — both fields omitted, defaults apply.
        let req: CaptureSnapshotRequest = serde_json::from_str("{}").unwrap();
        assert!(req.target.is_none());
        assert!(req.user.is_none());
    }

    #[test]
    fn artifact_name_rejects_path_separators() {
        // Forward slash, backslash, and `..` segments must not
        // pass — these are the building blocks of path traversal
        // attacks against the per-file fetch endpoint.
        assert!(!is_safe_artifact_name(""));
        assert!(!is_safe_artifact_name(".."));
        assert!(!is_safe_artifact_name("."));
        assert!(!is_safe_artifact_name("../etc/passwd"));
        assert!(!is_safe_artifact_name("foo/bar"));
        assert!(!is_safe_artifact_name("foo\\bar"));
        assert!(!is_safe_artifact_name("foo\0bar"));
    }

    #[test]
    fn snapshot_target_default_and_local_are_equivalent() {
        assert_eq!(SnapshotTarget::parse(None).unwrap(), SnapshotTarget::Local);
        assert_eq!(
            SnapshotTarget::parse(Some("local")).unwrap(),
            SnapshotTarget::Local
        );
        assert_eq!(
            SnapshotTarget::parse(Some("LOCAL")).unwrap(),
            SnapshotTarget::Local,
            "case-insensitive"
        );
        assert_eq!(
            SnapshotTarget::parse(Some("  ")).unwrap(),
            SnapshotTarget::Local,
            "whitespace-only treated as default"
        );
    }

    #[test]
    fn snapshot_target_parses_bank_prefix() {
        assert_eq!(
            SnapshotTarget::parse(Some("bank:personal")).unwrap(),
            SnapshotTarget::Bank("personal".into())
        );
        // Whitespace around the bank name is trimmed.
        assert_eq!(
            SnapshotTarget::parse(Some("bank: media ")).unwrap(),
            SnapshotTarget::Bank("media".into())
        );
    }

    #[test]
    fn snapshot_target_rejects_empty_bank_name() {
        let err = SnapshotTarget::parse(Some("bank:")).unwrap_err();
        assert!(
            err.contains("must include a bank name"),
            "useful error message: {err}"
        );
        let err = SnapshotTarget::parse(Some("bank: ")).unwrap_err();
        assert!(err.contains("must include a bank name"));
    }

    #[test]
    fn snapshot_target_rejects_unknown_prefix() {
        // Typo prevention — silent fallback to local would
        // surprise users who explicitly tried to write to a
        // storage system.
        let err = SnapshotTarget::parse(Some("registry:some-where")).unwrap_err();
        assert!(err.contains("not recognised"), "{err}");
        let err = SnapshotTarget::parse(Some("garbage")).unwrap_err();
        assert!(err.contains("not recognised"), "{err}");
    }

    #[test]
    fn artifact_name_accepts_well_formed_artifact_names() {
        // Image (must equal "image.tar" but the predicate only
        // checks safety; the exact-match check is in the handler).
        assert!(is_safe_artifact_name("image.tar"));
        // Volume names (basename of container_path).
        assert!(is_safe_artifact_name("data"));
        assert!(is_safe_artifact_name("postgres"));
        // Encoded external mount filenames.
        assert!(is_safe_artifact_name("var--data--photos"));
        assert!(is_safe_artifact_name("C_--data--photos"));
    }
}
