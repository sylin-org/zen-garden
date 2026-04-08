//! HTTP layer — axum handlers and router.
//!
//! Every handler is a thin adapter between the wire and the services
//! layer. Nothing in this module owns mutable state; handlers read
//! from the pre-rendered `CatalogDocuments` watch channel, the
//! Directory snapshot, and the stores behind trait objects.

pub mod actions_index;
pub mod catalog;
pub mod envelopes;
pub mod errors;
pub mod flush;
pub mod health;
pub mod ingress;
pub mod jobs;
pub mod media;
pub mod metrics;
pub mod recommendations;
pub mod router;
pub mod sitemap;
pub mod skills;
