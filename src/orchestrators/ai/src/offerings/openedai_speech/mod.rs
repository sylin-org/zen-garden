//! OpenedAI Speech protocol layer — HTTP client and types.
//!
//! The Offering trait impl has moved to `providers/openedai_speech.rs`.
//! This module exposes only the client and types needed by other modules.

pub mod client;
pub mod types;

pub use client::OpenedaiSpeechClient;
