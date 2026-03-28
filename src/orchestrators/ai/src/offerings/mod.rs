//! Per-offering adapter implementations.
//!
//! Each submodule implements the [`Offering`](crate::catalog::Offering) trait
//! for a specific AI service type.

pub mod cloud;
pub mod comfyui;
pub mod infinity;
pub mod libretranslate;
pub mod ollama;
pub mod openedai_speech;
pub mod speaches;
pub mod whispercpp;
