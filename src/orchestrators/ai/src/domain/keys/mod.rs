//! Canonical field-path constants.
//!
//! Every canonical key used at runtime is declared here. Providers,
//! vocabulary builders, and handlers import these constants; string
//! literals matching canonical key patterns outside this module are a
//! CI-enforced error (see `scripts/check-canonical-keys.sh`).
//!
//! Organization mirrors the vocabulary namespaces:
//!
//! - [`text`] — every field in the text modality (chat, translate,
//!   embed, rerank).
//! - [`image`] — every field in the image modality (generate, edit,
//!   upscale, analyze).
//! - [`audio`] — every field in the audio modality (generate,
//!   transcribe).
//! - [`usage`] — token/byte/cost accounting shared by every primitive.
//! - [`timing`] — routing/queue/inference timing shared by every
//!   primitive.
//! - [`meta`] — response metadata present on every envelope.
//! - [`job`] — async-job reference fields used in Async and Streaming
//!   outcomes.
//! - [`stream`] — streaming-delta envelope fields.
//! - [`providers`] — typed [`crate::domain::ids::ProviderName`]
//!   constants, one per adapter.

pub mod audio;
pub mod image;
pub mod job;
pub mod meta;
pub mod providers;
pub mod stream;
pub mod text;
pub mod timing;
pub mod usage;
