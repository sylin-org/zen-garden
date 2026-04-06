//! Command execution context (RAKE-0011)
//!
//! Provides shared state and utilities for command handlers.
//! Connected commands receive an `api()` and `endpoint()` that are
//! always available -- no `Option` unwrapping.

use crate::ui::rendering::{OutputWriter, TerminalInfo};
use garden_common::client::StoneApi;

/// Global output format for automation-friendly output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "json" => Self::Json,
            _ => Self::Human,
        })
    }
}

impl OutputFormat {
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Context passed to command handlers.
///
/// For connected commands, `api` and `endpoint` are always set
/// (dispatch enforces this). For local commands, they are `None`.
pub struct Context {
    /// HTTP client with connection pooling
    pub client: reqwest::Client,
    /// Typed Stone API (present for connected commands)
    api: Option<StoneApi>,
    /// Resolved endpoint URL (present for connected commands)
    endpoint: Option<String>,
    /// Stone name (if known)
    stone_name: Option<String>,
    /// Whether to suppress non-essential output
    pub quiet: bool,
    /// Whether to bypass cache
    pub fresh: bool,
    /// Verbose level
    pub verbose: u8,
    /// Terminal info for formatting
    pub term: TerminalInfo,
    /// Output writer for consistent formatting
    pub output: OutputWriter,
    /// Global output format
    pub output_format: OutputFormat,
    /// Optional field path for extracting single values
    pub field: Option<String>,
}

impl Context {
    /// Create a connected context from a Stone reference.
    pub fn from_stone(
        stone: &crate::connection::stone::Stone,
        stone_name: Option<String>,
        client: reqwest::Client,
        quiet: bool,
        fresh: bool,
        verbose: u8,
        output_format: OutputFormat,
        field: Option<String>,
    ) -> Self {
        Self {
            api: Some(StoneApi::new(client.clone(), stone.endpoint().to_string())),
            endpoint: Some(stone.endpoint().to_string()),
            stone_name,
            client,
            quiet,
            fresh,
            verbose,
            term: TerminalInfo::detect(),
            output: OutputWriter::new(),
            output_format,
            field,
        }
    }

    /// Create a local context (no stone).
    pub fn local(
        client: reqwest::Client,
        quiet: bool,
        fresh: bool,
        verbose: u8,
        output_format: OutputFormat,
        field: Option<String>,
    ) -> Self {
        Self {
            client,
            api: None,
            endpoint: None,
            stone_name: None,
            quiet,
            fresh,
            verbose,
            term: TerminalInfo::detect(),
            output: OutputWriter::new(),
            output_format,
            field,
        }
    }

    // ====================================================================
    // Stone access -- dispatch guarantees these for connected commands
    // ====================================================================

    /// Typed Stone API (ARCH-0012).
    /// Dispatch guarantees this is set for connected commands.
    pub fn api(&self) -> &StoneApi {
        self.api
            .as_ref()
            .expect("dispatch guarantees api for connected commands")
    }

    /// Resolved endpoint URL.
    /// Dispatch guarantees this is set for connected commands.
    pub fn endpoint(&self) -> &str {
        self.endpoint
            .as_deref()
            .expect("dispatch guarantees endpoint for connected commands")
    }

    /// Whether this context has a stone connection.
    pub fn has_stone(&self) -> bool {
        self.api.is_some()
    }

    /// Stone name (if known from tending cache or capabilities fetch).
    pub fn stone_name(&self) -> Option<&str> {
        self.stone_name.as_deref()
    }

    // ====================================================================
    // URL helpers
    // ====================================================================

    /// Build URL for API endpoint: `{endpoint}/{path}`
    pub fn api_url(&self, path: &str) -> String {
        let base = self.endpoint().trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }

    /// Build URL for v1 API endpoint: `{endpoint}/api/v1/{path}`
    pub fn api_v1_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        self.api_url(&format!("api/v1/{}", path))
    }

    // ====================================================================
    // Output helpers
    // ====================================================================

    pub fn wants_json(&self) -> bool {
        self.output_format.is_json()
    }

    pub fn has_field(&self) -> bool {
        self.field.is_some()
    }

    pub fn extract_field(&self, value: &serde_json::Value) -> Option<String> {
        let field = self.field.as_ref()?;
        extract_json_field(value, field)
    }
}

/// Extract a field from JSON using dot notation with array indexing
pub fn extract_json_field(value: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = value;

    for segment in path.split('.') {
        if let Some(bracket_pos) = segment.find('[') {
            let field_name = &segment[..bracket_pos];
            let rest = &segment[bracket_pos..];

            if !field_name.is_empty() {
                current = current.get(field_name)?;
            }

            let mut chars = rest.chars().peekable();
            while chars.peek() == Some(&'[') {
                chars.next();
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(c);
                    chars.next();
                }
                let index: usize = index_str.parse().ok()?;
                current = current.get(index)?;
            }
        } else {
            current = current.get(segment)?;
        }
    }

    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
        _ => Some(current.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_extraction_simple() {
        let json = serde_json::json!({
            "name": "mongodb",
            "port": 27017,
            "active": true
        });

        assert_eq!(extract_json_field(&json, "name"), Some("mongodb".to_string()));
        assert_eq!(extract_json_field(&json, "port"), Some("27017".to_string()));
        assert_eq!(extract_json_field(&json, "active"), Some("true".to_string()));
        assert_eq!(extract_json_field(&json, "missing"), None);
    }

    #[test]
    fn test_field_extraction_nested() {
        let json = serde_json::json!({
            "connection": {
                "hostname": "stone-01.local",
                "port": 27017,
                "uris": ["mongodb://stone-01.local:27017"]
            }
        });

        assert_eq!(
            extract_json_field(&json, "connection.hostname"),
            Some("stone-01.local".to_string())
        );
        assert_eq!(
            extract_json_field(&json, "connection.uris[0]"),
            Some("mongodb://stone-01.local:27017".to_string())
        );
    }
}
