//! Core trait abstractions for Zen Garden components
//!
//! This module defines the fundamental interfaces that enable:
//! - Discovery: Stone location via UDP broadcast, mDNS, explicit targeting
//! - Auth: Authentication/authorization (slots defined for future JWT)
//! - Persistence: File-based storage with atomic writes
//! - Job Execution: Background task processing

pub mod auth;
pub mod discovery;
pub mod job_executor;
pub mod persistence;

pub use auth::AuthProvider;
pub use discovery::DiscoveryProvider;
pub use job_executor::JobExecutor;
pub use persistence::PersistenceProvider;
