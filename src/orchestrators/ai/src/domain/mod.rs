//! Pure domain layer — zero I/O, zero async.
//!
//! All algorithms operate on plain data structures. This layer never
//! imports `catalog/`, `offerings/`, `infra/`, or `tasks/`.

pub mod advisor;
pub mod demand;
pub mod fitness;
pub mod gpu_catalog;
pub mod lease;
pub mod metrics;
pub mod placement;
pub mod policy;
pub mod recommendation;
pub mod reconciliation;
pub mod registry;
pub mod directory_domain;
pub mod intelligence;
pub mod observability;
pub mod skills_domain;
pub mod provisioning;
pub mod provisioning_domain;
pub mod routing;
pub mod skill;
pub mod tiering;
pub mod types;
