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
    /// Where to persist the snapshot. M2 supports `local`
    /// (default — `<data_dir>/snapshots/<fqn-encoded>`).
    /// Bank-backed targets (`bank:<name>`) ship in S5.
    #[serde(default)]
    pub target: Option<String>,
    /// User identifier for the event log's actor field.
    /// Omitted means system-driven (periodic scheduler, etc).
    #[serde(default)]
    pub user: Option<String>,
}

/// Successful capture response. Surfaces the snapshot's id and
/// the event_id of the `BackupTaken` event so callers can
/// correlate (e.g. Pavilion's drag-canvas marks the seed
/// "forming" then "complete" using these).
#[derive(Debug, Serialize)]
pub struct CaptureSnapshotResponse {
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
    let fqn = OfferingFqn::parse(&offering_name).map_err(|e| {
        bad_request(
            "INVALID_OFFERING_NAME",
            format!("Invalid offering name '{}': {}", offering_name, e),
        )
    })?;

    let request = body.map(|Json(b)| b).unwrap_or_default();

    // Target selection. M2: local disk only. The picker accepts
    // an explicit `local` to make the choice visible at the
    // wire level even though it's the default; future strings
    // (`bank:<name>`) reject loud-and-clear here so a typo
    // doesn't silently fall back to local.
    match request.target.as_deref() {
        None | Some("local") => {}
        Some(other) => {
            return Err(bad_request(
                "UNSUPPORTED_SNAPSHOT_TARGET",
                format!(
                    "Snapshot target '{}' is not supported in M2; only 'local' is available. Bank-backed targets land in commit S5.",
                    other
                ),
            ));
        }
    }

    let snapshot_root = local_snapshot_root(&fqn);
    let store = LocalSnapshotStore::new(snapshot_root.clone());
    let log_path = offering_event_log_path(&fqn);
    let log = EventLog::open(log_path);

    let actor = match request.user {
        Some(user) => EventActor::user(state.current.stone.name.clone(), user),
        None => EventActor::system(state.current.stone.name.clone()),
    };

    let result = crate::infra::snapshot::capture_snapshot(&state, &fqn, &store, &log, actor)
        .await
        .map_err(|e| {
            tracing::error!(
                offering = %offering_name,
                error = %e,
                "Snapshot capture failed"
            );
            crate::infra::api_helpers::internal("SNAPSHOT_CAPTURE_FAILED", e.to_string())
        })?;

    crate::api::ok(CaptureSnapshotResponse {
        snapshot_id: result.manifest.id,
        event_id: result.event_id,
        source_fqn: result.manifest.source_fqn,
        source_stone: result.manifest.source_stone,
        size_total_bytes: result.manifest.size_total_bytes,
        volumes: result.manifest.volumes.len(),
        external_mounts: result.manifest.external_mounts.len(),
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
