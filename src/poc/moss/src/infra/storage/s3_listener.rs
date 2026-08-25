//! S3 Listener Manager (STORAGE-0016)
//!
//! Arms a dedicated S3-compatible HTTP listener per managed storage volume.
//! Each listener serves standard S3 protocol at root `/` on a unique port,
//! so standard S3 clients (MinIO SDK, aws-cli, rclone) can connect directly.
//!
//! ## Port Allocation
//!
//! Ports are allocated from a configurable base (default 23400) with
//! deterministic offsets per replica set name. The mapping is persisted
//! in-memory and published via `GET /api/v1/stone/storage/s3/ports`.
//!
//! ## Lifecycle
//!
//! - **Arm**: When a managed volume is classified (primary role), a listener
//!   is spawned on the allocated port.
//! - **503 Degradation**: When a volume goes offline (USB unplug), the listener
//!   stays armed but returns 503 for all operations.
//! - **Disarm**: When a volume is permanently removed, the listener is
//!   cancelled and the port is released.
//! - **Re-arm**: When a volume comes back online, the 503 gate opens.
//!
//! ## Design
//!
//! Infrastructure layer — manages TCP listeners and port allocation.
//! Routes are a subset of the full S3 gateway (no proxy fallback needed
//! since each listener is bound to a specific local storage).

use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::get,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::Moss;

/// Default base port for S3 listeners (23400-23499 range)
pub const S3_LISTENER_BASE_PORT: u16 = 23400;

/// Maximum number of concurrent S3 listeners
const MAX_S3_LISTENERS: usize = 100;

/// Port assignment for a storage's S3 listener
#[derive(Debug, Clone)]
pub struct S3PortAssignment {
    pub replica_set_name: String,
    pub port: u16,
    pub storage_id: String,
    pub online: bool,
}

/// Manages per-storage S3 listeners
pub struct S3Listeners {
    /// Port assignments: replica_set_name → assignment
    assignments: Arc<RwLock<HashMap<String, S3PortAssignment>>>,
    /// Cancellation tokens per replica set (to disarm individual listeners)
    listener_tokens: Arc<RwLock<HashMap<String, CancellationToken>>>,
    /// Base port for allocation
    base_port: u16,
    /// Global shutdown token
    shutdown_token: CancellationToken,
}

impl S3Listeners {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        Self {
            assignments: Arc::new(RwLock::new(HashMap::new())),
            listener_tokens: Arc::new(RwLock::new(HashMap::new())),
            base_port: S3_LISTENER_BASE_PORT,
            shutdown_token,
        }
    }

    pub fn with_base_port(mut self, base_port: u16) -> Self {
        self.base_port = base_port;
        self
    }

    /// Get current port assignments (for the catalog endpoint)
    pub async fn port_catalog(&self) -> HashMap<String, u16> {
        let assignments = self.assignments.read().await;
        assignments
            .iter()
            .map(|(name, a)| (name.clone(), a.port))
            .collect()
    }

    /// Get detailed assignments including online status
    pub async fn assignments(&self) -> Vec<S3PortAssignment> {
        let assignments = self.assignments.read().await;
        assignments.values().cloned().collect()
    }

    /// Allocate a port for a replica set name.
    ///
    /// Uses deterministic hashing so the same replica set always gets the
    /// same port offset (within the range). Falls back to sequential
    /// allocation if there's a collision.
    async fn allocate_port(&self, replica_set_name: &str) -> Option<u16> {
        let assignments = self.assignments.read().await;
        if assignments.len() >= MAX_S3_LISTENERS {
            warn!("S3 listener limit reached ({})", MAX_S3_LISTENERS);
            return None;
        }

        // Check if already assigned
        if let Some(existing) = assignments.get(replica_set_name) {
            return Some(existing.port);
        }

        // Deterministic offset from name hash
        let hash = simple_hash(replica_set_name);
        let range = MAX_S3_LISTENERS as u16;
        let preferred_offset = (hash % range as u32) as u16;

        let used_ports: std::collections::HashSet<u16> =
            assignments.values().map(|a| a.port).collect();

        // Try preferred port first, then scan sequentially
        for i in 0..range {
            let offset = (preferred_offset + i) % range;
            let port = self.base_port + offset;
            if !used_ports.contains(&port) {
                return Some(port);
            }
        }

        None // All ports exhausted (shouldn't happen with MAX_S3_LISTENERS check)
    }

    /// Arm an S3 listener for a storage volume.
    ///
    /// Spawns a new HTTP listener on the allocated port with S3 routes at root `/`.
    /// Returns the assigned port, or None if allocation fails.
    ///
    /// # Thread Safety
    ///
    /// Port allocation (`allocate_port`) and assignment are not atomic — the port
    /// is read under a read lock, then the assignment is written under a separate
    /// write lock. This is safe because `arm` is only called from the storage
    /// orchestration task (single-threaded). If concurrent callers are needed in
    /// the future, allocation and assignment should use a single write lock.
    pub async fn arm(
        &self,
        replica_set_name: &str,
        storage_id: &str,
        state: Moss,
    ) -> Option<u16> {
        let port = self.allocate_port(replica_set_name).await?;

        // Check if already armed
        {
            let tokens = self.listener_tokens.read().await;
            if tokens.contains_key(replica_set_name) {
                debug!(replica_set = %replica_set_name, port, "S3 listener already armed");
                return Some(port);
            }
        }

        // Create child cancellation token
        let listener_token = self.shutdown_token.child_token();
        let listener_cancel = listener_token.clone();

        // Build S3 router (routes at root `/` for standard S3 compatibility)
        let app = build_s3_router(state, replica_set_name.to_string());

        // Bind and spawn
        let rs_name = replica_set_name.to_string();
        match bind_s3_port(port).await {
            Ok(listener) => {
                info!(
                    replica_set = %rs_name,
                    port,
                    storage_id,
                    "S3 listener armed"
                );

                let handle_token = listener_cancel.clone();
                tokio::spawn(async move {
                    let server = axum::serve(listener, app)
                        .with_graceful_shutdown(async move { handle_token.cancelled().await });

                    tokio::select! {
                        result = server => {
                            if let Err(e) = result {
                                error!(error = ?e, replica_set = %rs_name, "S3 listener error");
                            }
                            info!(replica_set = %rs_name, "S3 listener drained");
                        }
                        _ = async {
                            listener_cancel.cancelled().await;
                            tokio::time::sleep(tokio::time::Duration::from_secs(
                                garden_common::constants::server::DRAIN_DEADLINE_SECS,
                            )).await;
                        } => {
                            warn!(replica_set = %rs_name, "S3 listener drain deadline exceeded");
                        }
                    }
                });

                // Store assignment and token
                {
                    let mut assignments = self.assignments.write().await;
                    assignments.insert(
                        replica_set_name.to_string(),
                        S3PortAssignment {
                            replica_set_name: replica_set_name.to_string(),
                            port,
                            storage_id: storage_id.to_string(),
                            online: true,
                        },
                    );
                }
                {
                    let mut tokens = self.listener_tokens.write().await;
                    tokens.insert(replica_set_name.to_string(), listener_token);
                }

                Some(port)
            }
            Err(e) => {
                error!(
                    error = ?e,
                    port,
                    replica_set = %replica_set_name,
                    "Failed to bind S3 listener"
                );
                None
            }
        }
    }

    /// Mark a storage as offline (503 degradation).
    ///
    /// The listener stays armed but returns 503 for all operations.
    pub async fn set_offline(&self, replica_set_name: &str) {
        let mut assignments = self.assignments.write().await;
        if let Some(assignment) = assignments.get_mut(replica_set_name) {
            assignment.online = false;
            warn!(
                replica_set = %replica_set_name,
                port = assignment.port,
                "S3 listener degraded (503)"
            );
        }
    }

    /// Mark a storage as online (resume normal operation).
    pub async fn set_online(&self, replica_set_name: &str) {
        let mut assignments = self.assignments.write().await;
        if let Some(assignment) = assignments.get_mut(replica_set_name) {
            assignment.online = true;
            info!(
                replica_set = %replica_set_name,
                port = assignment.port,
                "S3 listener resumed"
            );
        }
    }

    /// Disarm an S3 listener (permanently remove).
    pub async fn disarm(&self, replica_set_name: &str) {
        // Cancel the listener
        {
            let mut tokens = self.listener_tokens.write().await;
            if let Some(token) = tokens.remove(replica_set_name) {
                token.cancel();
            }
        }
        // Remove assignment
        {
            let mut assignments = self.assignments.write().await;
            if let Some(removed) = assignments.remove(replica_set_name) {
                info!(
                    replica_set = %replica_set_name,
                    port = removed.port,
                    "S3 listener disarmed"
                );
            }
        }
    }
}

/// Build the S3 router for a specific storage.
///
/// Routes are at root `/` for standard S3 compatibility:
/// - `GET /` → ListBuckets
/// - `PUT /{bucket}` → CreateBucket
/// - `GET /{bucket}` → ListObjects
/// - `GET /{bucket}/{*key}` → GetObject
/// - `PUT /{bucket}/{*key}` → PutObject (+ CopyObject via x-amz-copy-source)
/// - `HEAD /{bucket}/{*key}` → HeadObject
/// - `DELETE /{bucket}/{*key}` → DeleteObject
fn build_s3_router(state: Moss, replica_set_name: String) -> Router {
    let s3_state = S3ListenerState {
        app_state: state,
        replica_set_name,
    };

    Router::new()
        .route("/", get(s3_list_buckets))
        .route("/{bucket}", get(s3_list_objects).put(s3_create_bucket))
        .route(
            "/{bucket}/{*key}",
            get(s3_get_object)
                .put(s3_put_object)
                .post(s3_complete_or_initiate_multipart)
                .head(s3_head_object)
                .delete(s3_delete_object),
        )
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024))
        .with_state(s3_state)
}

/// State for per-storage S3 listeners
#[derive(Clone)]
struct S3ListenerState {
    app_state: Moss,
    replica_set_name: String,
}

// ============================================================================
// S3 Handler Wrappers (delegate to s3_gateway with fixed replica set)
// ============================================================================

// These thin handlers extract the fixed replica_set_name and delegate
// to the existing s3_gateway functions with the storage pre-selected.
// This avoids code duplication — the core logic lives in s3_gateway.

async fn s3_list_buckets(State(s3): State<S3ListenerState>, headers: HeaderMap) -> Response {
    use crate::api::v1::s3_gateway;
    let mut headers = headers;
    // replica_set_name is already passed via SeedBankSelector query param below;
    // only set the header if the value is valid ASCII.
    if let Ok(val) = s3.replica_set_name.parse::<axum::http::HeaderValue>() {
        headers.insert(garden_common::constants::headers::HEADER_SEED_BANK, val);
    }
    s3_gateway::list_buckets(
        axum::extract::State(s3.app_state),
        Query(s3_gateway::SeedBankSelector {
            seed_bank: Some(s3.replica_set_name),
        }),
        headers,
    )
    .await
}

async fn s3_list_objects(
    State(s3): State<S3ListenerState>,
    Path(bucket): Path<String>,
    Query(mut query): Query<crate::api::v1::s3_gateway::ListObjectsQuery>,
    headers: HeaderMap,
) -> Response {
    query.storage = Some(s3.replica_set_name.clone());
    crate::api::v1::s3_gateway::list_objects(
        axum::extract::State(s3.app_state),
        Path(bucket),
        Query(query),
        headers,
    )
    .await
}

async fn s3_create_bucket(
    State(s3): State<S3ListenerState>,
    Path(bucket): Path<String>,
    headers: HeaderMap,
) -> Response {
    crate::api::v1::s3_gateway::create_bucket(
        axum::extract::State(s3.app_state),
        Path(bucket),
        Query(crate::api::v1::s3_gateway::SeedBankSelector {
            seed_bank: Some(s3.replica_set_name),
        }),
        headers,
    )
    .await
}

async fn s3_get_object(
    State(s3): State<S3ListenerState>,
    Path((bucket, key)): Path<(String, String)>,
    raw_query: axum::extract::RawQuery,
    headers: HeaderMap,
) -> Response {
    crate::api::v1::s3_gateway::get_object(
        axum::extract::State(s3.app_state),
        Path((bucket, key)),
        Query(crate::api::v1::s3_gateway::SeedBankSelector {
            seed_bank: Some(s3.replica_set_name),
        }),
        raw_query,
        headers,
    )
    .await
}

async fn s3_complete_or_initiate_multipart(
    State(s3): State<S3ListenerState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(mut query): Query<crate::api::v1::s3_gateway::CompleteMultipartQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    query.seed_bank = Some(s3.replica_set_name.clone());
    crate::api::v1::s3_gateway::complete_or_initiate_multipart(
        axum::extract::State(s3.app_state),
        Path((bucket, key)),
        Query(query),
        headers,
        body,
    )
    .await
}

async fn s3_put_object(
    State(s3): State<S3ListenerState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(mut query): Query<crate::api::v1::s3_gateway::UploadPartQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    query.seed_bank = Some(s3.replica_set_name.clone());
    crate::api::v1::s3_gateway::put_object(
        axum::extract::State(s3.app_state),
        Path((bucket, key)),
        Query(query),
        headers,
        body,
    )
    .await
}

async fn s3_head_object(
    State(s3): State<S3ListenerState>,
    Path((bucket, key)): Path<(String, String)>,
    raw_query: axum::extract::RawQuery,
    headers: HeaderMap,
) -> Response {
    crate::api::v1::s3_gateway::head_object(
        axum::extract::State(s3.app_state),
        Path((bucket, key)),
        Query(crate::api::v1::s3_gateway::SeedBankSelector {
            seed_bank: Some(s3.replica_set_name),
        }),
        raw_query,
        headers,
    )
    .await
}

async fn s3_delete_object(
    State(s3): State<S3ListenerState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(mut query): Query<crate::api::v1::s3_gateway::DeleteObjectQuery>,
    headers: HeaderMap,
) -> Response {
    query.seed_bank = Some(s3.replica_set_name.clone());
    crate::api::v1::s3_gateway::delete_object(
        axum::extract::State(s3.app_state),
        Path((bucket, key)),
        Query(query),
        headers,
    )
    .await
}

/// Bind a TCP listener for an S3 port with SO_REUSEADDR.
///
/// Lightweight version of `bootstrap::server::bind` that uses tracing
/// instead of console printer (S3 listeners are background infrastructure).
async fn bind_s3_port(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    use std::net::SocketAddr;

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| anyhow::anyhow!("S3 socket create: {}", e))?;

    socket
        .set_reuse_address(true)
        .map_err(|e| anyhow::anyhow!("S3 socket SO_REUSEADDR: {}", e))?;

    socket
        .set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("S3 socket non-blocking: {}", e))?;

    socket
        .bind(&addr.into())
        .map_err(|e| anyhow::anyhow!("S3 bind :{}: {}", port, e))?;

    socket
        .listen(garden_common::constants::server::TCP_BACKLOG)
        .map_err(|e| anyhow::anyhow!("S3 listen :{}: {}", port, e))?;

    let std_listener: std::net::TcpListener = socket.into();
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| anyhow::anyhow!("S3 tokio convert :{}: {}", port, e))?;

    debug!(port, "S3 port bound");
    Ok(listener)
}

/// Simple deterministic hash for port allocation
fn simple_hash(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_hash_is_deterministic() {
        assert_eq!(simple_hash("storage"), simple_hash("storage"));
        assert_eq!(simple_hash("prod"), simple_hash("prod"));
    }

    #[test]
    fn simple_hash_varies_by_name() {
        assert_ne!(simple_hash("storage"), simple_hash("prod"));
        assert_ne!(simple_hash("a"), simple_hash("b"));
    }

    #[test]
    fn port_allocation_is_within_range() {
        let hash = simple_hash("storage");
        let offset = (hash % 100) as u16;
        let port = S3_LISTENER_BASE_PORT + offset;
        assert!(port >= S3_LISTENER_BASE_PORT);
        assert!(port < S3_LISTENER_BASE_PORT + 100);
    }

    #[tokio::test]
    async fn manager_port_catalog_empty_initially() {
        let token = CancellationToken::new();
        let mgr = S3Listeners::new(token);
        assert!(mgr.port_catalog().await.is_empty());
    }

    #[tokio::test]
    async fn allocate_port_deterministic() {
        let token = CancellationToken::new();
        let mgr = S3Listeners::new(token);

        let p1 = mgr.allocate_port("storage").await.unwrap();
        let p2 = mgr.allocate_port("storage").await.unwrap();
        assert_eq!(p1, p2, "Same name should get same port");
    }
}
