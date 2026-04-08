//! Orchestrator domain — value objects, aggregates, and canonical vocabulary.
//!
//! Every type that crosses the pipeline stages lives here. External
//! integrations (HTTP ingress, provider adapters, background tasks)
//! import domain types; the domain never imports them back.
//!
//! Organization follows ORCH-0028:
//!
//! - [`keys`] — canonical field-path constants. No magic strings in the
//!   orchestrator; every key name used at runtime is declared here.
//! - [`ids`] — identity types (request, response, media, job, registration,
//!   correlation). Mutable identities are GUIDv7; human-readable identities
//!   are plain strings wrapped in newtypes.
//! - [`primitive`] — the 10 locked primitives.
//! - [`moniker`] — skill monikers (lowercase kebab-case, reserved words
//!   rejected, length-bounded).
//! - [`field_path`] — `FieldPath` value object used by every vocabulary.
//! - [`vocabulary`] — per-primitive input/output schemas, aliases, shared
//!   namespaces, and the `VocabularyRegistry`.
//! - [`output`] — flat dotted-key map with nested-JSON serialization.
//! - [`provider`] — the `Provider` trait, `ProviderState`, `ProviderOutcome`,
//!   and all supporting value objects (registration, model, performance
//!   hints, errors).
//! - [`directory`] — the `Directory` aggregate with its maintenance task
//!   and published snapshot.
//! - [`request`] — `OrchestratorRequest` and `ExecutionContext`.
//! - [`media`] — media store trait, sink, entry, delivery modes, transfer
//!   targets, lifecycle.
//! - [`jobs`] — job model, job sink, job store.
//! - [`idempotency`] — idempotency key computation and cache.
//! - [`errors`] — error taxonomy, error response envelope, actionable error
//!   messages.
//! - [`selectors`] — request selectors and constraints.
//! - [`recommendation_types`] — value objects used by the
//!   `RecommendationEngine` (lives in [`crate::services::recommendation`]).

pub mod directory;
pub mod errors;
pub mod events;
pub mod field_path;
pub mod ids;
pub mod idempotency;
pub mod jobs;
pub mod keys;
pub mod media;
pub mod moniker;
pub mod output;
pub mod primitive;
pub mod provider;
pub mod recommendation_types;
pub mod request;
pub mod selectors;
pub mod vocabulary;
