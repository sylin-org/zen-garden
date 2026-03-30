//! Provider implementations — one file per provider kind.
//!
//! Each provider implements the `Provider` trait from `catalog::traits`,
//! covering lifecycle (probe, enumerate) and inference (infer, stream,
//! embed, speak, transcribe) in a single struct.
//!
//! Protocol-specific HTTP clients and types live in `offerings/`.
//! Provider files import from there for protocol knowledge.

pub mod anthropic;
pub mod google;
pub mod infinity;
pub mod libretranslate;
pub mod ollama;
pub mod openai;
pub mod openedai_speech;
pub mod speaches;
