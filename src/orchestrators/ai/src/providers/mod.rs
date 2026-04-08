//! Vendor provider implementations.
//!
//! Each provider wraps a vendor's native HTTP API behind the
//! [`crate::domain::provider::Provider`] trait. Providers own their
//! instances, their health tracking, and their wire-format translation.
//!
//! The v1 inventory registers providers for every primitive that has
//! a corresponding adapter module below. Primitives without a
//! registered provider are absent from the catalog, per the ADR.

pub mod anthropic;
pub mod comfyui;
pub mod common;
pub mod docling;
pub mod google;
pub mod infinity;
pub mod kokoro;
pub mod libretranslate;
pub mod ollama;
pub mod ollama_matrix;
pub mod openai;
pub mod openai_compat_stt;
pub mod openai_compat_tts;
pub mod openedai_speech;
pub mod speaches;
pub mod whispercpp;
