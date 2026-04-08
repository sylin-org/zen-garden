//! Vendor provider implementations.
//!
//! Each provider wraps a vendor's native HTTP API behind the
//! [`crate::domain::provider::Provider`] trait. Providers own their
//! instances, their health tracking, their wire-format translation,
//! and (post-ORCH-0030 R2 M3) their model resolution and capability
//! publication.
//!
//! M1 inventory: Ollama, ComfyUI, WhisperCpp, Speaches, Kokoro,
//! OpenedaiSpeech, Docling, LibreTranslate, Google/Gemini.
//!
//! Anthropic, OpenAI, and Infinity were removed in M3 — Anthropic
//! and OpenAI return in M2 (M7 of the M1 plan), Infinity is
//! permanently dropped (Ollama serves text.embed).

pub mod cloud_common;
pub mod comfyui;
pub mod common;
pub mod docling;
pub mod google;
pub mod kokoro;
pub mod libretranslate;
pub mod ollama;
pub mod ollama_matrix;
pub mod openedai_speech;
pub mod speaches;
pub mod whispercpp;
