//! The will (ADR-0005): a declared disaster-recovery policy and its
//! execution. One context, four concerns:
//!
//! - [`policy`] — the DECLARED will, parsed into a plan no executor can
//!   disagree with (parse, don't validate).
//! - [`run`] — the Run aggregate: a saga's identity and legal
//!   transitions (never a string phase mutated in place).
//! - [`checkpoint`] — the Checkpoint entity: immutable once committed,
//!   manifest-as-commit-marker, verify, rotation.
//! - [`saga`] — the executor: quiesce/imprint/resume → pack → deliver
//!   to sinks (at-least-once) → commit; plus the due-date scheduler.

pub mod checkpoint;
pub mod policy;
pub mod run;
pub mod saga;

pub use policy::{readiness, CaptureMode, CapturePolicy, Readiness};
pub use run::{Phase, Run, RunInfo};
pub use crate::garden::runtime::{ExecLines, HookRunner, NullHooks};
pub use saga::{run_scheduler, workload_for, Runner, Workload, CAPTURE_CADENCE_SECS};
pub use checkpoint::VerifyReport;
pub use checkpoint::{checkpoints_root, Checkpoint};
