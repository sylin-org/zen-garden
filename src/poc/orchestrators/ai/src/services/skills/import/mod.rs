//! Skill import pipeline (ORCH-0029 Phase 3).
//!
//! Turns raw caller input — a CivitAI URL, a PNG file with embedded
//! ComfyUI workflow metadata, an A1111-format generation text dump,
//! or a pasted ComfyUI workflow JSON — into a draft v3 `skill.json`
//! on disk that the operator can review and publish.
//!
//! ## Architecture (SoC)
//!
//! Each sub-module does one thing:
//!
//! - [`input_detect`] — classify raw input into a typed enum (no I/O)
//! - [`png_extract`] — extract ComfyUI workflow from PNG `tEXt`/`zTXt`/`iTXt` chunks (pure parser)
//! - [`gen_data_parse`] — parse A1111-format generation text (pure parser)
//! - [`ui_to_api`] — convert ComfyUI UI-format workflow to API format (pure, 60+ widget table)
//! - [`workflow_parser`] — analyze an API-format workflow (node graph, models, inputs)
//! - [`param_extract`] — walk a workflow, plant placeholders, emit typed bindings
//! - [`civitai`] — CivitAI API client (image/model fetch, hash lookup, download)
//! - [`known_models`] — static registry of curated HuggingFace ecosystem models
//! - [`model_resolve`] — 5-level cascade to resolve a model filename to a download URL
//! - [`workflow_synth`] — synthesize txt2img workflows from generation parameters
//! - [`analyze`] — orchestrate the full pipeline, produce an [`analyze::AnalyzeResult`]
//! - [`draft_builder`] — write a draft v3 `skill.json` + workflow to disk
//! - [`namer`] — async AI naming via the orchestrator's own chat dispatcher
//!
//! ## Error handling
//!
//! Every module returns `Result` with descriptive errors. Failures in
//! optional steps (preview download, CivitAI hash lookup) become
//! warnings on the `AnalyzeResult`, not errors — the pipeline
//! continues. Unresolvable checkpoints are a hard failure because a
//! skill without a valid checkpoint can never execute.

pub mod analyze;
pub mod civitai;
pub mod draft_builder;
pub mod gen_data_parse;
pub mod input_detect;
pub mod known_models;
pub mod model_resolve;
pub mod namer;
pub mod param_extract;
pub mod png_extract;
pub mod ui_to_api;
pub mod workflow_parser;
pub mod workflow_synth;
