//! Generic cluster management primitives for stateful orchestrators (ORCH-0012).
//!
//! Provides the database-agnostic building blocks that every clustered-service
//! orchestrator needs: instance tracking, FQN-keyed logical sets, lifecycle
//! state machines, membership event detection, IP drift recovery, and action
//! queuing with persistence.
//!
//! Database-specific behavior is injected via the [`ClusterAdapter`] trait.
//! Each orchestrator (MongoDB, PostgreSQL, SQL Server, Redis) implements this
//! trait to provide its probe, bootstrap, add/remove, and health-check logic.

mod instance_registry;
mod logical_set;
mod action_queue;
mod adapter;
pub mod connection;
pub mod health_poller;

pub use instance_registry::InstanceRegistry;
pub use logical_set::{LogicalSet, SetPhase, SetAction, MembershipEvent, KnownMember};
pub use logical_set::{classify_probes, load_sets, save_sets};
pub use action_queue::{ActionQueue, PendingAction};
pub use adapter::{ClusterAdapter, ClusterInstance, InstanceHealth, ProbeResult, MemberHealth};
pub use connection::{ConnectionInfo, ConnectionModel, ConnectionPublisher};
pub use health_poller::{HealthPollerConfig, run as run_health_poller};
