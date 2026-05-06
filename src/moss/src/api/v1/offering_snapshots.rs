//! Snapshot capture + read endpoints for an offering instance.
//!
//! M2 ships local-disk snapshots (per ORCH-0039 §M2 cut). The
//! bank-backed target lands in commit S5; the route shape stays
//! the same — only the [`SnapshotStore`] adapter swaps. Read
//! endpoints (catalog, manifest, per-file fetch) are added in
//! commit S4.

use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};
use garden_common::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};

use crate::Moss;
use crate::bad_request;
use crate::domain::offering_events::{EventActor, EventLog};
use crate::domain::snapshot::LocalSnapshotStore;

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
}
