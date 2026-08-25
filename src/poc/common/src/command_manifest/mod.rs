//! Command Manifest Contracts for Zen Garden
//!
//! Shared types for command manifests used by:
//! - Rake CLI (for its own commands)
//! - Companions (Cricket, Firefly, etc.)
//! - Moss (for proxying commands and generating help)
//!
//! Philosophy: Single source of truth for command structure.
//! Commands are defined once, metadata is derived from the manifest.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod types;

pub use types::*;

/// Command argument type for validation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ArgType {
    #[default]
    String,
    Integer,
    Boolean,
    Url,
    /// Enum with allowed values
    Enum(Vec<String>),
}

/// Command argument definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArg {
    /// Argument name (e.g., "tune", "volume")
    pub name: String,

    /// Argument type for validation
    #[serde(default)]
    pub arg_type: ArgType,

    /// Whether argument is required
    #[serde(default)]
    pub required: bool,

    /// Human-readable description
    pub description: String,

    /// Optional minimum value (for integers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,

    /// Optional maximum value (for integers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,

    /// Default value (if not required)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl CommandArg {
    /// Create a required string argument
    pub fn required_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arg_type: ArgType::String,
            required: true,
            description: description.into(),
            min: None,
            max: None,
            default: None,
        }
    }

    /// Create an optional string argument
    pub fn optional_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arg_type: ArgType::String,
            required: false,
            description: description.into(),
            min: None,
            max: None,
            default: None,
        }
    }

    /// Create a required integer argument with range
    pub fn required_int(
        name: impl Into<String>,
        description: impl Into<String>,
        min: i64,
        max: i64,
    ) -> Self {
        Self {
            name: name.into(),
            arg_type: ArgType::Integer,
            required: true,
            description: description.into(),
            min: Some(min),
            max: Some(max),
            default: None,
        }
    }

    /// Create a required URL argument
    pub fn required_url(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arg_type: ArgType::Url,
            required: true,
            description: description.into(),
            min: None,
            max: None,
            default: None,
        }
    }
}

/// Command example for documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExample {
    /// What this example demonstrates
    pub description: String,

    /// Full command line
    pub command: String,
}

impl CommandExample {
    pub fn new(description: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            command: command.into(),
        }
    }
}

/// Single command definition in a manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    /// Command name (e.g., "select", "volume")
    pub name: String,

    /// Short description (one line)
    pub description: String,

    /// Long description (optional, multiple paragraphs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_description: Option<String>,

    /// Command arguments
    #[serde(default)]
    pub args: Vec<CommandArg>,

    /// Usage examples
    #[serde(default)]
    pub examples: Vec<CommandExample>,

    /// Related commands (for suggestions)
    #[serde(default)]
    pub see_also: Vec<String>,
}

impl CommandDef {
    /// Create a new command definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            long_description: None,
            args: Vec::new(),
            examples: Vec::new(),
            see_also: Vec::new(),
        }
    }

    /// Add an argument
    pub fn arg(mut self, arg: CommandArg) -> Self {
        self.args.push(arg);
        self
    }

    /// Add an example
    pub fn example(mut self, description: impl Into<String>, command: impl Into<String>) -> Self {
        self.examples
            .push(CommandExample::new(description, command));
        self
    }

    /// Add a see_also reference
    pub fn see_also(mut self, cmd: impl Into<String>) -> Self {
        self.see_also.push(cmd.into());
        self
    }

    /// Set long description
    pub fn long_desc(mut self, desc: impl Into<String>) -> Self {
        self.long_description = Some(desc.into());
        self
    }

    /// Generate args syntax string (e.g., "<tune> [--volume <level>]")
    pub fn args_syntax(&self) -> String {
        self.args
            .iter()
            .map(|a| {
                if a.required {
                    format!("<{}>", a.name)
                } else {
                    format!("[{}]", a.name)
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Command manifest for an Companion or tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandManifest {
    /// Companion/tool identifier (e.g., "cricket", "rake")
    pub id: String,

    /// Display name
    pub name: String,

    /// Version
    pub version: String,

    /// Description
    pub description: String,

    /// Available commands
    pub commands: Vec<CommandDef>,
}

impl CommandManifest {
    /// Create a new command manifest
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: description.into(),
            commands: Vec::new(),
        }
    }

    /// Add a command
    pub fn command(mut self, cmd: CommandDef) -> Self {
        self.commands.push(cmd);
        self
    }

    /// Get command by name
    pub fn get(&self, name: &str) -> Option<&CommandDef> {
        self.commands.iter().find(|c| c.name == name)
    }

    /// Get all command names
    pub fn command_names(&self) -> Vec<&str> {
        self.commands.iter().map(|c| c.name.as_str()).collect()
    }

    /// Build a HashMap for fast lookup
    pub fn as_map(&self) -> HashMap<&str, &CommandDef> {
        self.commands.iter().map(|c| (c.name.as_str(), c)).collect()
    }

    /// Load from JSON file
    pub fn from_json_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let content = crate::utils::strings::strip_bom(&content);
        serde_json::from_str(content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Load from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let json = crate::utils::strings::strip_bom(json);
        serde_json::from_str(json)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize to compact JSON (for --dump-commands)
    pub fn to_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Helper to handle --dump-commands CLI flag
///
/// Usage in Companion's main.rs:
/// ```ignore
/// if args.contains(&"--dump-commands".to_string()) {
///     dump_commands_and_exit(&my_manifest());
/// }
/// ```
///
/// Or use the check_dump_commands helper which checks args automatically.
pub fn dump_commands_and_exit(manifest: &CommandManifest) -> ! {
    match manifest.to_json() {
        Ok(json) => {
            println!("{}", json);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Failed to serialize manifest: {}", e);
            std::process::exit(1);
        }
    }
}

/// Check if --dump-commands was passed and handle it
///
/// Returns true if --dump-commands was handled (program will exit).
/// Returns false if not present, allowing normal execution to continue.
///
/// Usage:
/// ```ignore
/// fn main() {
///     if check_dump_commands(&my_manifest()) {
///         return; // Won't reach here, already exited
///     }
///     // Normal execution continues...
/// }
/// ```
pub fn check_dump_commands(manifest: &CommandManifest) -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--dump-commands") {
        dump_commands_and_exit(manifest);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_manifest_builder() {
        let manifest = CommandManifest::new("cricket", "Cricket", "0.1.0", "Audio Companion")
            .command(
                CommandDef::new("select", "Switch tune")
                    .arg(CommandArg::required_string("tune", "Tune name"))
                    .example(
                        "Switch to mr-robot",
                        "garden-rake hey tell cricket select mr-robot",
                    ),
            )
            .command(
                CommandDef::new("volume", "Set volume").arg(CommandArg::required_int(
                    "level",
                    "Volume 0-100",
                    0,
                    100,
                )),
            );

        assert_eq!(manifest.id, "cricket");
        assert_eq!(manifest.commands.len(), 2);
        assert!(manifest.get("select").is_some());
        assert!(manifest.get("volume").is_some());
    }

    #[test]
    fn test_command_def_args_syntax() {
        let cmd = CommandDef::new("test", "Test command")
            .arg(CommandArg::required_string("name", "Name"))
            .arg(CommandArg::optional_string("extra", "Extra"));

        assert_eq!(cmd.args_syntax(), "<name> [extra]");
    }

    #[test]
    fn test_manifest_json_roundtrip() {
        let manifest = CommandManifest::new("test", "Test", "1.0.0", "Test manifest")
            .command(CommandDef::new("cmd", "A command"));

        let json = manifest.to_json().unwrap();
        let parsed = CommandManifest::from_json(&json).unwrap();

        assert_eq!(parsed.id, manifest.id);
        assert_eq!(parsed.commands.len(), 1);
    }
}
