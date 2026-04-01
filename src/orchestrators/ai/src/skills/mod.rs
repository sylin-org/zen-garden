//! Skill system — workflow parsing, built-in skills, and ComfyUI preparation.
//!
//! This module bridges raw ComfyUI workflows and the skill abstraction:
//! - `parser`: Analyzes a ComfyUI workflow JSON to extract inputs, models, and structure
//! - `builtin`: Pre-authored skill definitions with embedded workflow templates
//! - `prep`: Ensures a ComfyUI instance has the required models installed

pub mod parser;
pub mod builtin;
pub mod prep;

#[cfg(test)]
mod tests;
