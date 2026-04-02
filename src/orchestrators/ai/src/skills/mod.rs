//! Skill system — disk-based repository, workflow parsing, provisioning.
//!
//! - `loader`: Scan disk, parse skill.json, seed embedded skills, resolve workflows
//! - `parser`: Analyze ComfyUI workflow JSON to extract inputs, models, structure
//! - `builtin`: Legacy built-in skill definitions (being replaced by disk-based loader)
//! - `prep`: Stream model downloads and push to ComfyUI instances

pub mod cache;
pub mod import;
pub mod loader;
pub mod parser;
pub mod provisioner;
pub mod builtin;
pub mod prep;

#[cfg(test)]
mod tests;
