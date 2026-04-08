//! Skill subsystem (ORCH-0029).
//!
//! A skill is a **narrowing of a primitive's vocabulary plus a binding
//! to a backend execution plan**. The orchestrator core holds the
//! schema (vocabulary), the static metadata (Directory registrations),
//! and the dynamic state (this module's `Skills` aggregate). Each
//! skill-aware adapter (currently ComfyUI; later Whisper, Docling,
//! and others) owns the lifecycle: load from disk, register, watch
//! the filesystem, provision dependencies, dispatch at execution
//! time.
//!
//! This module hosts:
//!
//! - [`types`] — the v3 disk schema and the in-memory shapes the
//!   loader produces (`SkillDefinition`, `Binding`, `ModelSelector`,
//!   `ModelRef`, `Variant`).
//! - [`loader`] — disk scanner with v1/v2 → v3 legacy translation.
//! - [`registry`] — the `Skills` aggregate: private mutable state
//!   behind a `tokio::sync::Mutex`, snapshot via `watch::channel`,
//!   event API per ORCH-0028 §13.
//! - `cache` (Phase 2) — content-addressed dependency cache + manifest.
//! - `provisioner` (Phase 2) — streaming download + push to instance.
//! - `queue` (Phase 2) — bounded provisioning worker.
//! - `import` (Phase 3) — CivitAI / PNG / JSON import pipeline.
//!
//! See [`docs/decisions/ORCH-0029-skill-subsystem.md`] for the design.

pub mod cache;
pub mod import;
pub mod loader;
pub mod moss_volume;
pub mod provisioner;
pub mod queue;
pub mod registry;
pub mod types;

pub use queue::{
    Priority, ProvisioningJob, ProvisioningQueue, ProvisioningSnapshot, ProvisioningTarget,
    QueueEvent,
};

pub use registry::{InstanceReadiness, Skills, SkillsSnapshot, SkillEntry, SkillEvent, SkillKey, SkillMeta};
pub use types::{
    AutoKind, Binding, BindingTarget, FieldConstraint, ModelRef, ModelSelector, ParamOption,
    SkillDefinition, Variant,
};
