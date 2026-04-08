//! Stateless domain services + infra stores.
//!
//! - [`contextualizer`] — multi-pass request normalization and
//!   validation against the vocabulary and the Directory snapshot.
//! - [`media_resolver`] — applies the provider's declared
//!   `MediaDelivery` mode to each media reference.
//! - [`dispatcher`] — the ten-line coordinator: contextualize, check
//!   idempotency, resolve media, look up the provider, call `onboard`.
//! - [`recommendation`] — point-in-time model recommendation with
//!   layered scoring, pin support, and a passive demand ledger.
//! - [`media_store`] — in-memory implementation of
//!   [`crate::domain::media::MediaStore`].
//! - [`job_store`] — in-memory implementation of
//!   [`crate::domain::jobs::JobStore`].
//! - [`idempotency_store`] — in-memory implementation of
//!   [`crate::domain::idempotency::IdempotencyStore`].
//! - [`directory_maintenance`] — background task that rebuilds the
//!   Directory snapshot when providers publish new state.
//! - [`catalog_builder`] — background task that pre-renders
//!   `/v1/catalog` and `/v1/do` JSON documents whenever the Directory
//!   version bumps.

pub mod catalog_builder;
pub mod cloud_secrets;
pub mod contextualizer;
pub mod directory_maintenance;
pub mod directory_subscriber;
pub mod dispatcher;
pub mod garden_discovery;
pub mod idempotency_store;
pub mod instance_manager;
pub mod job_store;
pub mod media_resolver;
pub mod media_store;
pub mod recommendation;
pub mod skills;
pub mod vocab_drift;
