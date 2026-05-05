//! Garden-scoped storage API (STORAGE-0009)
//!
//! Name-based storage operations that route to the Primary replica.
//! Any Moss can be the entry point — if local is Primary, execute
//! locally; otherwise proxy to the stone hosting the Primary.
//!
//! Three content namespaces:
//! - `/fs/`       — user content at storage root
//! - `/objects/`  — S3 objects under `.zen-garden/storage/`
//! - `/memories/` — harvest artifacts under `.zen-garden/memories/`
//!
//! ## Routes
//!
//! ```text
//! GET    /api/v1/garden/storage                                      → list all storages
//! GET    /api/v1/garden/storage/{name}                               → discovery (all replicas)
//! GET    /api/v1/garden/storage/{name}/fs                            → directory listing (?path=&depth=N)
//! GET    /api/v1/garden/storage/{name}/fs/{*path}                    → read user file
//! PUT    /api/v1/garden/storage/{name}/fs/{*path}                    → write user file
//! DELETE /api/v1/garden/storage/{name}/fs/{*path}                    → delete user file
//! HEAD   /api/v1/garden/storage/{name}/fs/{*path}                    → file metadata
//! GET    /api/v1/garden/storage/{name}/objects/{*path}               → read S3 object
//! PUT    /api/v1/garden/storage/{name}/objects/{*path}               → write S3 object
//! DELETE /api/v1/garden/storage/{name}/objects/{*path}               → delete S3 object
//! HEAD   /api/v1/garden/storage/{name}/objects/{*path}               → object metadata
//! GET    /api/v1/garden/storage/{name}/memories                      → list offerings
//! GET    /api/v1/garden/storage/{name}/memories/{offering}           → list snapshots
//! GET    /api/v1/garden/storage/{name}/memories/{offering}/manifest  → offering manifest
//! GET    /api/v1/garden/storage/{name}/memories/{offering}/{harvest} → download snapshot
//! ```
//!
//! Directory listing uses query parameters: `?path=subdir&depth=N` (default depth 1,
//! or `all` for recursive).  Listing is a metadata query on `/fs`, while content
//! operations use the `/fs/{*path}` wildcard.

pub mod audit;
pub mod files;
pub mod objects;
pub mod snapshots;

pub use files::{
    delete_file_v1, get_file_v1, head_file_v1, list_fs_v1, move_file_v1, put_file_v1,
};
pub use objects::{delete_object_v1, get_object_v1, head_object_v1, put_object_v1};
pub use snapshots::{
    download_snapshot_v1, get_offering_manifest_v1, list_memories_v1, list_offering_snapshots_v1,
};

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::storage::StorageRole;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::domain::storage_service::{ProxyTarget, StorageRoute};
use crate::{Moss, error_response};

// ============================================================================
// Constants
// ============================================================================

/// Header set on proxied requests to break loops.
pub(crate) const HEADER_ZEN_PROXIED: &str = "X-Zen-Proxied";

// ============================================================================
// Shared types
// ============================================================================

/// Query parameters for directory / object listing.
#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    #[serde(default)]
    pub depth: Option<String>,
}

impl ListQueryParams {
    pub fn parse_depth(&self) -> Option<usize> {
        match self.depth.as_deref() {
            None | Some("1") => Some(1),
            Some("all") | Some("-1") => None,
            Some(s) => s.parse().ok(),
        }
    }
}

/// A single storage replica instance.
#[derive(Debug, Serialize)]
pub struct StorageInstance {
    pub stone_id: String,
    pub stone_name: String,
    pub storage_id: String,
    pub role: StorageRole,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_id: Option<String>,
    pub endpoint: String,
    pub visibility: String,
    pub health: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// S3 connection details for a storage replica set.
///
/// Included in the discovery response so clients get everything needed
/// to connect via standard S3 in a single API call.
#[derive(Debug, Serialize)]
pub struct S3Connection {
    /// S3 endpoint (host:port). Standard S3 at root /.
    pub endpoint: String,
    /// Access key for S3 authentication.
    pub access_key: String,
    /// Secret key for S3 authentication.
    pub secret_key: String,
    /// Region (always "zen-garden" for Moss-hosted storage).
    pub region: String,
}

/// Response for the discovery endpoint.
#[derive(Debug, Serialize)]
pub struct StorageDiscovery {
    pub name: String,
    pub instances: Vec<StorageInstance>,
    /// S3 connection details for the primary instance.
    /// Present when at least one instance has an armed S3 listener.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3Connection>,
}

// `GardenStorageSummary`, `DirectoryListResponse`, and `DirectoryEntry`
// are wire types shared with clients (Pavilion's Cloud Filter provider,
// Rake) — they live in `garden_common::storage` so both ends of the
// API boundary use the same definition.
pub use garden_common::storage::{DirectoryEntry, DirectoryListResponse, GardenStorageSummary};

/// Object metadata response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
}

// ============================================================================
// Shared helpers
// ============================================================================

pub(crate) fn err(
    status: StatusCode,
    code: &str,
    msg: &str,
) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

pub(crate) fn error_response_raw(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message
        }
    });
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string().into())
        .unwrap()
}

pub(crate) fn has_path_traversal(value: &str) -> bool {
    garden_common::constants::storage::share::has_path_traversal(value)
}

// ============================================================================
// HTTP Range header parsing
// ============================================================================

/// Outcome of parsing the `Range` request header against a known body size.
///
/// Only the single-range form (`bytes=start-end`, `bytes=start-`,
/// `bytes=-N`) is supported. RFC 7233 also allows multi-range
/// responses — current clients (Pavilion's Cloud Filter, S3 SDKs)
/// never ask for them, so we reject.
pub(crate) enum RangeOutcome {
    /// No Range header — serve the full body at 200 OK.
    Full,
    /// Valid single range; serve as 206 Partial Content.
    Partial { start: u64, length: u64 },
    /// Range header present but cannot be satisfied (start beyond EOF,
    /// or end < start). RFC 7233 §4.4 — return 416.
    Unsatisfiable,
    /// Range header present but syntactically invalid.
    Malformed,
}

pub(crate) fn parse_range_header(
    headers: &axum::http::HeaderMap,
    body_size: u64,
) -> RangeOutcome {
    let raw = match headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s.trim(),
        None => return RangeOutcome::Full,
    };

    let Some(spec) = raw.strip_prefix("bytes=") else {
        return RangeOutcome::Malformed;
    };

    if spec.contains(',') {
        // Multi-range — not supported.
        return RangeOutcome::Malformed;
    }

    let Some((start_s, end_s)) = spec.split_once('-') else {
        return RangeOutcome::Malformed;
    };
    let start_s = start_s.trim();
    let end_s = end_s.trim();

    // Suffix range `bytes=-N` (last N bytes) — supported.
    if start_s.is_empty() {
        if end_s.is_empty() {
            return RangeOutcome::Malformed;
        }
        let n: u64 = match end_s.parse() {
            Ok(n) if n > 0 => n,
            _ => return RangeOutcome::Malformed,
        };
        if body_size == 0 {
            return RangeOutcome::Unsatisfiable;
        }
        let length = n.min(body_size);
        let start = body_size - length;
        return RangeOutcome::Partial { start, length };
    }

    let start: u64 = match start_s.parse() {
        Ok(n) => n,
        Err(_) => return RangeOutcome::Malformed,
    };

    if start >= body_size {
        return RangeOutcome::Unsatisfiable;
    }

    let end_inclusive: u64 = if end_s.is_empty() {
        // Open-ended: bytes=start-  → start through EOF.
        body_size - 1
    } else {
        match end_s.parse::<u64>() {
            Ok(n) if n >= start => n.min(body_size - 1),
            Ok(_) => return RangeOutcome::Unsatisfiable,
            Err(_) => return RangeOutcome::Malformed,
        }
    };

    let length = end_inclusive - start + 1;
    RangeOutcome::Partial { start, length }
}

#[cfg(test)]
mod range_tests {
    use super::*;
    use axum::http::{HeaderMap, header};

    fn header_with(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::RANGE, value.parse().unwrap());
        h
    }

    fn assert_partial(out: RangeOutcome, expected_start: u64, expected_length: u64) {
        match out {
            RangeOutcome::Partial { start, length } => {
                assert_eq!(start, expected_start);
                assert_eq!(length, expected_length);
            }
            _ => panic!("expected Partial"),
        }
    }

    #[test]
    fn missing_header_serves_full_body() {
        assert!(matches!(
            parse_range_header(&HeaderMap::new(), 1000),
            RangeOutcome::Full
        ));
    }

    #[test]
    fn closed_range_resolves() {
        assert_partial(parse_range_header(&header_with("bytes=0-15"), 1000), 0, 16);
        assert_partial(
            parse_range_header(&header_with("bytes=100-199"), 1000),
            100,
            100,
        );
    }

    #[test]
    fn open_ended_range_runs_to_eof() {
        assert_partial(parse_range_header(&header_with("bytes=512-"), 1024), 512, 512);
    }

    #[test]
    fn suffix_range_returns_last_n_bytes() {
        assert_partial(parse_range_header(&header_with("bytes=-128"), 1024), 896, 128);
    }

    #[test]
    fn suffix_range_clamps_to_body_size() {
        assert_partial(parse_range_header(&header_with("bytes=-9999"), 1000), 0, 1000);
    }

    #[test]
    fn end_past_eof_clamps_to_eof() {
        assert_partial(
            parse_range_header(&header_with("bytes=900-9999"), 1000),
            900,
            100,
        );
    }

    #[test]
    fn start_at_or_past_eof_is_unsatisfiable() {
        assert!(matches!(
            parse_range_header(&header_with("bytes=1000-1100"), 1000),
            RangeOutcome::Unsatisfiable
        ));
        assert!(matches!(
            parse_range_header(&header_with("bytes=2000-"), 1000),
            RangeOutcome::Unsatisfiable
        ));
    }

    #[test]
    fn end_before_start_is_unsatisfiable() {
        assert!(matches!(
            parse_range_header(&header_with("bytes=500-100"), 1000),
            RangeOutcome::Unsatisfiable
        ));
    }

    #[test]
    fn suffix_range_on_empty_body_is_unsatisfiable() {
        assert!(matches!(
            parse_range_header(&header_with("bytes=-10"), 0),
            RangeOutcome::Unsatisfiable
        ));
    }

    #[test]
    fn malformed_headers_are_rejected() {
        assert!(matches!(
            parse_range_header(&header_with("kilometers=0-100"), 1000),
            RangeOutcome::Malformed
        ));
        assert!(matches!(
            parse_range_header(&header_with("bytes=100"), 1000),
            RangeOutcome::Malformed
        ));
        assert!(matches!(
            parse_range_header(&header_with("bytes=0-99,200-299"), 1000),
            RangeOutcome::Malformed
        ));
        assert!(matches!(
            parse_range_header(&header_with("bytes=-"), 1000),
            RangeOutcome::Malformed
        ));
        assert!(matches!(
            parse_range_header(&header_with("bytes=abc-def"), 1000),
            RangeOutcome::Malformed
        ));
    }
}

/// Check if the incoming request was already proxied (loop guard).
pub(crate) fn is_proxied(headers: &HeaderMap) -> bool {
    headers
        .get(HEADER_ZEN_PROXIED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Proxy a request to a remote stone.
pub(crate) async fn proxy_request(
    method: reqwest::Method,
    target: &ProxyTarget,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: Option<Bytes>,
) -> Response {
    let url = format!(
        "{}/{}{}",
        target.endpoint.trim_end_matches('/'),
        path.trim_start_matches('/'),
        if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query)
        }
    );

    let mut request = crate::http::INSECURE_PROXY.request(method, &url);
    request = request.header(HEADER_ZEN_PROXIED, "true");

    if let Some(content_type) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(reqwest::header::CONTENT_TYPE, content_type);
    }

    if let Some(body) = body {
        request = request.body(body);
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return error_response_raw(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string());
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();

    let mut builder = Response::builder().status(status);

    for header_name in &[
        reqwest::header::CONTENT_TYPE,
        reqwest::header::CONTENT_LENGTH,
        reqwest::header::ETAG,
        reqwest::header::LAST_MODIFIED,
    ] {
        if let Some(value) = resp_headers.get(header_name).and_then(|v| v.to_str().ok()) {
            builder = builder.header(header_name.as_str(), value);
        }
    }

    builder.body(body.into()).unwrap()
}

// ============================================================================
// GET /api/v1/garden/storage — List all storages
// ============================================================================

/// List all storages visible across the garden.
///
/// Aggregates local managed storages with remote registry beacons.
/// Groups by **display name** — the user-facing identity used by every
/// other garden-tier endpoint (`/garden/storage/{name}/fs`, etc.).
/// Banks of the same replica set collapse into one entry; an unnamed
/// replica set surfaces as `DEFAULT_REPLICA_SET_DISPLAY` (`"storage"`).
/// Keying on the bank's individual `name` instead would expose two
/// disagreeing identities — list says `seed-soft-cliff`, read says
/// `storage` — and 404 every read.
pub async fn list_storages_v1(
    State(state): State<Moss>,
) -> crate::api::ApiResult<Vec<GardenStorageSummary>> {
    use garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY;

    let display_name_for = |replica_set_name: &str| -> String {
        if replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY.to_string()
        } else {
            replica_set_name.to_string()
        }
    };

    let mut by_name: std::collections::HashMap<String, GardenStorageSummary> =
        std::collections::HashMap::new();

    // Local storages
    for local in StorageRoute::list_local(&state.current.storage.volumes).await {
        let key = display_name_for(&local.replica_set_name);
        let entry = by_name
            .entry(key.clone())
            .or_insert_with(|| GardenStorageSummary {
                name: key.clone(),
                replica_count: 0,
                primary_stone: None,
                roles: local.roles.clone(),
            });
        entry.replica_count += 1;
        if local.role == StorageRole::Primary {
            entry.primary_stone = Some(state.current.stone.name.clone());
        }
    }

    // Remote storages from the Tool aggregate's registry.
    for storage_entry in state.tool.storage_entries().await {
        if storage_entry.tool.stone.id == state.current.stone.id {
            continue; // Already counted above
        }
        let sm = storage_entry.tool.storage.as_ref();
        let name = &storage_entry.tool.tool.name;
        let entry = by_name
            .entry(name.clone())
            .or_insert_with(|| GardenStorageSummary {
                name: name.clone(),
                replica_count: 0,
                primary_stone: None,
                roles: sm.map(|s| s.roles.clone()).unwrap_or_default(),
            });
        entry.replica_count += 1;
        if sm.and_then(|s| s.role.as_deref()) == Some(garden_common::constants::ROLE_PRIMARY)
            && entry.primary_stone.is_none()
        {
            entry.primary_stone = Some(storage_entry.tool.stone.name.clone());
        }
    }

    let mut storages: Vec<GardenStorageSummary> = by_name.into_values().collect();
    storages.sort_by(|a, b| a.name.cmp(&b.name));

    info!(count = storages.len(), "Listed garden storages");
    crate::api::ok(storages)
}

// ============================================================================
// GET /api/v1/garden/storage/{name} — Discovery
// ============================================================================

/// Returns all known replicas for a storage name.
///
/// Combines local managed storages with remote registry beacons.
pub async fn discover_v1(
    State(state): State<Moss>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> crate::api::ApiResult<StorageDiscovery> {
    let mut instances = Vec::new();

    // Check local storages
    if let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await {
        let (pin_id, roles) = {
            let map = state.current.storage.volumes.read().await;
            map.values()
                .find_map(|v| {
                    let m = v.management()?;
                    if m.name == name {
                        Some((v.pin_id().map(|s| s.to_string()), m.roles.clone()))
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        };

        let local_endpoint = state.current.address.read().await.http_base();

        instances.push(StorageInstance {
            stone_id: state.current.stone.id.clone(),
            stone_name: state.current.stone.name.clone(),
            storage_id: local.id.clone(),
            role: local.role,
            pinned: pin_id.is_some(),
            pin_id,
            endpoint: local_endpoint,
            visibility: garden_common::constants::VISIBILITY_OPEN.to_string(),
            health: garden_common::constants::HEALTH_HEALTHY.to_string(),
            roles,
        });
    }

    // Add remote instances from the Tool aggregate's registry.
    for entry in state.tool.storage_by_name(&name).await {
        if entry.tool.stone.id == state.current.stone.id {
            continue;
        }
        let sm = entry.tool.storage.as_ref();
        instances.push(StorageInstance {
            stone_id: entry.tool.stone.id.clone(),
            stone_name: entry.tool.stone.name.clone(),
            storage_id: entry.tool.tool.id.clone(),
            role: match sm.and_then(|s| s.role.as_deref()) {
                Some(garden_common::constants::ROLE_PRIMARY) => StorageRole::Primary,
                _ => StorageRole::Dormant,
            },
            pinned: sm.and_then(|s| s.pin_id.as_ref()).is_some(),
            pin_id: sm.and_then(|s| s.pin_id.clone()),
            endpoint: entry.tool.stone.endpoint.clone(),
            visibility: sm
                .map(|s| s.visibility.clone())
                .unwrap_or_else(|| garden_common::constants::VISIBILITY_OPEN.to_string()),
            health: entry.tool.service.status.clone(),
            roles: sm.map(|s| s.roles.clone()).unwrap_or_default(),
        });
    }

    if instances.is_empty() {
        return Err(err(
            StatusCode::NOT_FOUND,
            "STORAGE_NOT_FOUND",
            &format!("No storage named '{}' found in the garden", name),
        ));
    }

    // Build S3 connection block for the primary instance
    let s3 = build_s3_connection(&name, &instances, &state).await;

    crate::api::ok(StorageDiscovery {
        name,
        instances,
        s3,
    })
}

/// Build S3 connection details for a replica set's primary instance.
///
/// Resolves the S3 port from the port catalog and generates credentials.
/// Credentials are deterministic per replica set:
/// - **Unsigned mode**: derived from stone_id + replica set name (stable, not validated)
/// - **Pond active**: derived from pond CA fingerprint + replica set name (garden-scoped)
async fn build_s3_connection(
    replica_set: &str,
    instances: &[StorageInstance],
    state: &Moss,
) -> Option<S3Connection> {
    // Find primary instance (or first available)
    let primary = instances
        .iter()
        .find(|i| matches!(i.role, StorageRole::Primary))
        .or_else(|| instances.first())?;

    // S3 block is only populated when this stone hosts the storage.
    // Remote primaries: client follows the instance endpoint and asks that stone directly.
    if primary.stone_id != state.current.stone.id {
        return None;
    }

    let catalog = state
        .current
        .storage
        .coordination
        .s3_listeners
        .port_catalog()
        .await;
    let port = catalog
        .get(replica_set)
        .copied()
        .or_else(|| catalog.values().next().copied())?;

    // Build endpoint from the primary's stone endpoint host + S3 port
    let host = extract_host(&primary.endpoint).unwrap_or_else(|| "localhost".to_string());

    let endpoint = format!("{}:{}", host, port);

    // Generate credentials
    let (access_key, secret_key) = generate_s3_credentials(replica_set, state).await;

    Some(S3Connection {
        endpoint,
        access_key,
        secret_key,
        region: "zen-garden".to_string(),
    })
}

/// Extract host from an endpoint string like "http://192.168.1.174:7185".
fn extract_host(endpoint: &str) -> Option<String> {
    let stripped = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    let host = stripped.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Generate deterministic S3 credentials for a replica set.
///
/// Uses `resolve_key_material` for the two-tier key derivation
/// (pond CA fingerprint → stone_id fallback), then HMAC-derives
/// access and secret keys scoped to the replica set name.
async fn generate_s3_credentials(replica_set: &str, state: &Moss) -> (String, String) {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let key_material = crate::api::v1::s3_presign::resolve_key_material(state).await;

    // Access key: first 20 chars of HMAC(material, "s3-access:{name}")
    let access_msg = format!("s3-access:{}", replica_set);
    let mut mac = HmacSha256::new_from_slice(key_material.as_bytes()).expect("HMAC key length");
    mac.update(access_msg.as_bytes());
    let access_key = hex::encode(mac.finalize().into_bytes());
    let access_key = access_key[..20].to_uppercase();

    // Secret key: first 40 chars of HMAC(material, "s3-secret:{name}")
    let secret_msg = format!("s3-secret:{}", replica_set);
    let mut mac = HmacSha256::new_from_slice(key_material.as_bytes()).expect("HMAC key length");
    mac.update(secret_msg.as_bytes());
    let secret_key = hex::encode(mac.finalize().into_bytes());
    let secret_key = secret_key[..40].to_string();

    (access_key, secret_key)
}
