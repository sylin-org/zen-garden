//! Core trait abstractions for Zen Garden components
//!
//! This module defines the fundamental interfaces that enable:
//! - Auth: Authentication/authorization (slots defined for future JWT)
//! - Persistence: File-based storage with atomic writes
//! - Job Execution: Background task processing
//!
//! Stone discovery (DiscoveryProvider trait) moved to the
//! `garden-discovery` crate per DISC-0001.

pub mod auth;
pub mod job_executor;
pub mod persistence;

pub use auth::AuthProvider;
pub use job_executor::JobExecutor;
pub use persistence::PersistenceProvider;
