//! Skill subsystem (ORCH-0029, post-ORCH-0030 R2 M3).
//!
//! A skill is a **narrowing of a primitive's vocabulary plus a binding
//! to a backend execution plan**. The orchestrator core holds the
//! schema (vocabulary). After M3, **adapters own the dynamic state
//! directly** — there is no central `Skills` aggregate any more.
//! ComfyUI loads its skills from disk, holds the `LoadedSkill` map
//! internally, and publishes its skill catalogue as part of its
//! `CapabilityAnnouncement`.
//!
//! This module hosts the still-shared infrastructure:
//!
//! - [`types`] — the v3 disk schema and the in-memory shapes the
//!   loader produces (`SkillDefinition`, `Binding`, `ModelSelector`,
//!   `ModelRef`, `Variant`, `FieldConstraint`, `ParamOption`,
//!   `AutoKind`).
//! - [`loader`] — disk scanner with v1/v2 → v3 legacy translation.
//! - [`cache`] — content-addressed dependency cache + manifest.
//! - [`provisioner`] — streaming download + push to instance.
//! - [`queue`] — bounded provisioning worker.
//! - [`moss_volume`] — Moss volume API helpers for pushing models.
//! - [`import`] — CivitAI / PNG / JSON import pipeline.
//!
//! **Removed in M3:** the `registry` submodule (the `Skills`
//! aggregate). ComfyUI now owns its skill state directly.
//!
//! See [`docs/decisions/ORCH-0029-skill-subsystem.md`] for the design.

pub mod cache;
pub mod import;
pub mod loader;
pub mod moss_volume;
pub mod provisioner;
pub mod queue;
pub mod types;

pub use queue::{
    Priority, ProvisioningJob, ProvisioningQueue, ProvisioningSnapshot, ProvisioningTarget,
    QueueEvent,
};

pub use types::{
    AutoKind, Binding, BindingTarget, FieldConstraint, ModelRef, ModelSelector, ParamOption,
    SkillDefinition, Variant,
};
