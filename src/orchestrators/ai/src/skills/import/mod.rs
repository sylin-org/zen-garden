//! Skill import pipeline — detect, fetch, extract, resolve, draft (ORCH-0023).
//!
//! ## Architecture (SoC)
//!
//! Each module does one thing:
//! - `input_detect`: classify raw input into a typed enum
//! - `civitai`: CivitAI API client — fetch metadata, resolve model versions
//! - `png_extract`: extract ComfyUI workflow from PNG tEXt/zTXt chunks
//! - `gen_data_parse`: parse A1111-format generation data text
//! - `workflow_synth`: synthesize a ComfyUI API workflow from generation parameters
//! - `model_resolve`: resolve model filenames → download URLs (cascade)
//! - `analyze`: orchestrate the full pipeline, produce an AnalyzeResult
//! - `draft_builder`: write a draft skill.json + workflow to disk
//!
//! ## Error handling
//!
//! Every module returns `Result` with descriptive errors.
//! No panics, no unwrap on external data.
//! Failures in optional steps (model resolution, preview download) are
//! captured as warnings, not errors — the pipeline continues.

pub mod input_detect;
pub mod civitai;
pub mod png_extract;
pub mod gen_data_parse;
pub mod workflow_synth;
pub mod model_resolve;
pub mod analyze;
pub mod draft_builder;
