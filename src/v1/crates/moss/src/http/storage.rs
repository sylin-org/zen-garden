//! The storage faces: banks, roles, and the files that make a sink a destination.

use super::*;

pub async fn storage_list(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let volumes = crate::garden::storage::scan_volumes();
    let adoptable: Vec<serde_json::Value> =
        crate::garden::storage::Storage::adoptable(&volumes)
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
pub struct AdoptRequest {
    /// The volume's mount point (a scan's `device` value).
    device: String,
    /// The bank's logical name - FQN or bare stem (canonicalized).
    name: String,
}

/// The adopt ceremony's API face (1:1 with `rake storage adopt`): write
/// the manifest onto the drive, remember the bank, sing the news.

pub async fn storage_adopt(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AdoptRequest>,
) -> ApiResult {
    use crate::garden::storage::AdoptError;
    let wanted = std::path::PathBuf::from(&req.device);
    let volumes = crate::garden::storage::scan_volumes();
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
        AdoptError::Io(_) => CommandError::Runtime(crate::garden::runtime::RuntimeError::Failed(
            e.to_string(),
        )),
    })?;
    Ok(Json(serde_json::json!({ "data": { "bank": bank } })))
}

/// Mount-point comparison tolerant of trailing separators (`E:` == `E:`+slash).

pub fn dirs_equal(a: &std::path::Path, b: &std::path::Path) -> bool {
    let clean = |p: &std::path::Path| -> std::path::PathBuf {
        p.components().collect::<std::path::PathBuf>()
    };
    clean(a) == clean(b)
}

/// The eject verb's API face (1:1 with `rake storage eject`): mark the
/// bank ejected, sing the authoritative absence.

pub async fn storage_eject(
    State(state): State<Arc<AppState>>,
    Path(fqn): Path<String>,
) -> ApiResult {
    use crate::garden::storage::EjectError;
    let bank = state.storage.eject(&fqn).map_err(|e| match e {
        EjectError::UnknownBank(_) => CommandError::NotFound(e.to_string()),
        EjectError::AlreadyEjected(_) => CommandError::Conflict(e.to_string()),
    })?;
    Ok(Json(serde_json::json!({ "data": { "bank": bank } })))
}

/// The room's banks (ADR-0004 §4 grid): self spliced first, then every
/// peer's banks from the one topology cache. Rows name the holding stone.

#[derive(Debug, Deserialize)]
pub struct RolesRequest {
    /// The complete role set for this bank (sink today).
    roles: Vec<String>,
}

/// Declare a bank's roles (1:1 with `rake storage roles`): a sink receives
/// checkpoints; role news is state news and sings.

pub async fn storage_roles(
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

pub fn files_err(e: crate::garden::storage::FilesError) -> CommandError {
    use crate::garden::storage::FilesError;
    match &e {
        FilesError::UnknownBank(_) | FilesError::Missing(_) => {
            CommandError::NotFound(e.to_string())
        }
        FilesError::NotMounted(_) | FilesError::NotThatKind(_) | FilesError::Exists(_) => {
            CommandError::Conflict(e.to_string())
        }
        FilesError::BadPath(_) => CommandError::BadRequest(e.to_string()),
        FilesError::Io(_) => CommandError::Runtime(
            crate::garden::runtime::RuntimeError::Failed(e.to_string()),
        ),
    }
}

/// The files faces' shared gate: resolve a bank FQN to its volume root
/// HERE, or hand back the answer the request deserves instead. Local
/// presence wins — the authority is the volume in the slot, and an
/// ejected bank is refused HERE even if the cache remembers it elsewhere
/// (the adoption record is local truth). Only a bank this stone never
/// adopted consults the room.

pub fn gate_bank(
    state: &AppState,
    fqn: &str,
) -> Result<(crate::garden::storage::Bank, std::path::PathBuf), Box<axum::response::Response>>
{
    use crate::garden::storage::FilesError;
    match state.storage.bank_root(fqn) {
        Ok(pair) => Ok(pair),
        Err(FilesError::UnknownBank(_)) => Err(Box::new(bank_not_here(state, fqn))),
        Err(e) => Err(Box::new(ApiError::from(files_err(e)).into_response())),
    }
}

/// Who holds this bank, as the room's cache hears it — the addressee of
/// a not-here answer. Self never appears: the caller asked the local
/// vault first.

pub fn bank_holder(state: &AppState, fqn: &str) -> Option<String> {
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

pub fn bank_not_here(state: &AppState, fqn: &str) -> axum::response::Response {
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

/// List a bank directory tree (`?path=` names a subdirectory; absent =
/// the bank's root; `?depth=` bounds the walk — 1 default, N levels,
/// `all` for the whole tree. Below level one, entry names are paths
/// relative to the listed root).

pub async fn storage_files_list(
    State(state): State<Arc<AppState>>,
    Path(fqn): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<axum::response::Response, ApiError> {
    use crate::garden::storage::{list_tree, safe_join};
    let rel = params.get("path").map(String::as_str).unwrap_or("");
    let depth = params
        .get("depth")
        .map(|d| match d.trim() {
            "all" | "max" => usize::MAX,
            n => n.parse().unwrap_or(1),
        })
        .unwrap_or(1);
    let (bank, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let dir = if rel.is_empty() {
        root.clone()
    } else {
        safe_join(&root, rel).map_err(files_err)?
    };
    let files = list_tree(&root, &dir, depth).map_err(files_err)?;
    Ok(Json(
        serde_json::json!({ "data": { "bank": bank.fqn, "path": rel, "files": files } }),
    )
        .into_response())
}

/// The outcome of one RFC 7233 Range header against a file of `size`.
/// Malformed specs are IGNORED (serve full — the RFC's MUST); the PoC
/// served 416 for `end < start`, but the RFC calls that an invalid spec,
/// and the objective is standard clients.

enum RangeOutcome {
    Full,
    Partial { start: u64, length: u64 },
    Unsatisfiable,
}


pub fn parse_range(header: Option<&str>, size: u64) -> RangeOutcome {
    let Some(raw) = header else {
        return RangeOutcome::Full;
    };
    let Some(spec) = raw.strip_prefix("bytes=") else {
        return RangeOutcome::Full;
    };
    if spec.contains(',') {
        return RangeOutcome::Full; // multi-range: unsupported, serve full
    }
    let Some((start_s, end_s)) = spec.split_once('-') else {
        return RangeOutcome::Full;
    };
    let (start_s, end_s) = (start_s.trim(), end_s.trim());
    if start_s.is_empty() {
        // Suffix form `bytes=-N`: the last N bytes.
        let Ok(n) = end_s.parse::<u64>() else {
            return RangeOutcome::Full;
        };
        if n == 0 || size == 0 {
            return RangeOutcome::Unsatisfiable;
        }
        let start = size.saturating_sub(n);
        return RangeOutcome::Partial {
            start,
            length: size - start,
        };
    }
    let Ok(start) = start_s.parse::<u64>() else {
        return RangeOutcome::Full;
    };
    if start >= size {
        return RangeOutcome::Unsatisfiable;
    }
    match end_s.parse::<u64>() {
        Ok(end) if end < start => RangeOutcome::Full, // invalid spec: ignore
        Ok(end) => RangeOutcome::Partial {
            start,
            length: end.min(size - 1) - start + 1,
        },
        Err(_) => RangeOutcome::Partial {
            start,
            length: size - start, // `bytes=N-` runs to EOF
        },
    }
}

/// Read one file from a bank: the raw bytes ride alone, content-type
/// guessed from the extension. `Range` is honored (RFC 7233 single
/// range) — a standard media client can stream straight off a bank.

pub async fn storage_file_get(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    use crate::garden::storage::{file_size, read_file, read_file_range, safe_join};
    let (_, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let path = safe_join(&root, &rel).map_err(files_err)?;
    let size = file_size(&root, &path).map_err(files_err)?;
    let content_type = content_type_for(&rel);
    match parse_range(headers.get(axum::http::header::RANGE).and_then(|v| v.to_str().ok()), size)
    {
        RangeOutcome::Unsatisfiable => Ok((
            axum::http::StatusCode::RANGE_NOT_SATISFIABLE,
            [(
                axum::http::header::CONTENT_RANGE,
                format!("bytes */{size}"),
            )],
        )
            .into_response()),
        RangeOutcome::Partial { start, length } => {
            let bytes = read_file_range(&root, &path, start, length).map_err(files_err)?;
            Ok((
                axum::http::StatusCode::PARTIAL_CONTENT,
                [
                    (axum::http::header::CONTENT_TYPE, content_type.to_string()),
                    (axum::http::header::ACCEPT_RANGES, "bytes".to_string()),
                    (
                        axum::http::header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", start, start + length - 1, size),
                    ),
                ],
                bytes,
            )
                .into_response())
        }
        RangeOutcome::Full => {
            let bytes = read_file(&root, &path).map_err(files_err)?;
            Ok((
                [
                    (axum::http::header::CONTENT_TYPE, content_type.to_string()),
                    (axum::http::header::ACCEPT_RANGES, "bytes".to_string()),
                ],
                bytes,
            )
                .into_response())
        }
    }
}

/// Write one file onto a bank: the raw body, parents created.

pub async fn storage_file_put(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<axum::response::Response, ApiError> {
    use crate::garden::storage::{safe_join, write_file};
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

pub async fn storage_file_delete(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
) -> Result<axum::response::Response, ApiError> {
    use crate::garden::storage::{delete_file, safe_join};
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

#[derive(Debug, Deserialize)]

pub struct MoveRequest {
    /// The file's new path, relative to the bank's root.
    move_to: String,
}

/// Move (rename) one file within a bank — no re-upload over slow media.
/// Both endpoints pass the escape gate; the move never leaves the bank
/// and never overwrites.

pub async fn storage_file_move(
    State(state): State<Arc<AppState>>,
    Path((fqn, rel)): Path<(String, String)>,
    Json(req): Json<MoveRequest>,
) -> Result<axum::response::Response, ApiError> {
    use crate::garden::storage::{move_file, safe_join};
    let (bank, root) = match gate_bank(&state, &fqn) {
        Ok(pair) => pair,
        Err(answer) => return Ok(*answer),
    };
    let from = safe_join(&root, &rel).map_err(files_err)?;
    let to = safe_join(&root, &req.move_to).map_err(files_err)?;
    move_file(&root, &from, &to).map_err(files_err)?;
    tracing::info!(bank = %bank.fqn, from = %rel, to = %req.move_to, "file moved on a bank");
    Ok(Json(
        serde_json::json!({ "data": { "bank": bank.fqn, "from": rel, "to": req.move_to, "moved": true } }),
    )
        .into_response())
}

/// A small honest content-type table — extension guessed, everything else
/// rides as octet-stream. No mime crate: the surface stays lean (P5).

pub fn content_type_for(path: &str) -> &'static str {
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

