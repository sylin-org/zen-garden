//! Cluster adapter trait — the database-specific contract.
//!
//! Each orchestrator implements `ClusterAdapter` to provide the wire-protocol
//! operations that the generic cluster primitives drive.

use std::future::Future;

use serde::{Deserialize, Serialize};

// ── Instance Contract ─────────────────────────────────────────────────

/// A single instance of a clustered service.
///
/// Implemented by each adapter's instance type (e.g. `MongoInstance`,
/// `SqlServerInstance`). The generic cluster primitives operate on this
/// trait without knowing the database-specific fields.
pub trait ClusterInstance: Clone + Send + Sync + 'static {
    /// Service-specific endpoint (e.g. `"192.168.1.5:27017"`).
    fn endpoint(&self) -> &str;

    /// Stone identity — immutable GUID v7.
    fn stone_id(&self) -> &str;

    /// Human-readable stone name.
    fn stone_name(&self) -> &str;

    /// Current health status.
    fn health(&self) -> &InstanceHealth;
}

// ── Health ────────────────────────────────────────────────────────────

/// Health status of a clustered instance.
///
/// Intentionally database-agnostic. Adapter-specific states (e.g. MongoDB's
/// `Incompatible`) are mapped into these categories by the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceHealth {
    /// Responding normally.
    Healthy,
    /// Discovered but not yet probed.
    Unknown,
    /// Stone is unreachable (network/power).
    Offline,
    /// Stone is online but the service is not responding.
    Down,
    /// Service responds but is in a degraded state.
    Degraded,
    /// Service is intentionally stopped.
    Stopped,
}

// ── Probe Result ──────────────────────────────────────────────────────

/// Result of probing a single instance's cluster membership status.
///
/// The adapter's `probe()` returns this to tell the generic lifecycle
/// state machine what phase the instance is in.
#[derive(Debug)]
pub enum ProbeResult {
    /// Instance is an active member of its cluster (healthy participant).
    Active,
    /// Service is running but cluster is not yet initialized.
    NotInitialized,
    /// Cluster exists but this instance's endpoint doesn't match the
    /// current configuration (typically DHCP drift).
    StaleConfig,
    /// Service needs configuration before it can join (e.g. missing
    /// `--replSet` flag, replication not enabled).
    ConfigPending,
    /// Instance is unreachable or returned an unrecognized error.
    Unreachable,
}

// ── Member Health ─────────────────────────────────────────────────────

/// Health snapshot of a single member within a logical set.
///
/// Returned by `ClusterAdapter::health_check()` for each member.
#[derive(Debug, Clone)]
pub struct MemberHealth {
    /// Service-specific endpoint.
    pub endpoint: String,
    /// Stone hosting this member.
    pub stone_name: String,
    /// Whether the member is healthy.
    pub healthy: bool,
    /// Replication lag (seconds). `None` for primary or unknown.
    pub lag_seconds: Option<f64>,
}

// ── Adapter Trait ─────────────────────────────────────────────────────

/// Database-specific operations for a clustered service.
///
/// The generic cluster primitives (`LogicalSet`, `InstanceRegistry`,
/// `ActionQueue`) call these methods to execute database-specific work.
/// Each orchestrator implements this for its database engine.
///
/// All methods are async and fallible. The generic layer handles retry,
/// queuing, and persistence — the adapter only needs to execute the
/// wire-protocol operation and report success or failure.
pub trait ClusterAdapter: Send + Sync + 'static {
    /// The adapter's instance type (e.g. `MongoInstance`).
    type Instance: ClusterInstance;

    /// Probe an instance to classify its cluster membership status.
    fn probe(
        &self,
        instance: &Self::Instance,
    ) -> impl Future<Output = ProbeResult> + Send;

    /// Bootstrap a new cluster from its first instance.
    ///
    /// Called when `ProbeResult::NotInitialized` is returned and the
    /// logical set has enough members to form a cluster.
    fn bootstrap(
        &self,
        set_name: &str,
        instance: &Self::Instance,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Add an instance to an existing cluster.
    fn add_member(
        &self,
        set_name: &str,
        instance: &Self::Instance,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Remove an instance (by endpoint) from a cluster.
    fn remove_member(
        &self,
        set_name: &str,
        endpoint: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Check health of all members in a cluster.
    fn health_check(
        &self,
        set_name: &str,
    ) -> impl Future<Output = Vec<MemberHealth>> + Send;
}
