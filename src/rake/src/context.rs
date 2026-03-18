//! Command execution context
//!
//! Provides shared state and utilities for command handlers.
//! This eliminates repetitive setup code in each command.

use crate::ui::rendering::{OutputWriter, TerminalInfo};

/// Global output format for automation-friendly output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Human-readable output (default)
    #[default]
    Human,
    /// JSON output for scripts/automation
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

/// Context passed to command handlers
///
/// Contains all the shared state needed to execute a command:
/// - HTTP client for API calls
/// - Resolved endpoint (if applicable)
/// - Stone name (if resolved)
/// - Output formatting utilities
/// - Mode flags (quiet, fresh, verbose)
/// - Automation: output_format, field extraction
pub struct Runtime {
    /// HTTP client with connection pooling
    pub client: reqwest::Client,
    /// Resolved stone endpoint (e.g., "http://10.0.0.5:7185")
    pub endpoint: Option<String>,
    /// Stone name (e.g., "stone-01")
    pub stone: Option<String>,
    /// Whether to suppress non-essential output
    pub quiet: bool,
    /// Whether to bypass cache
    pub fresh: bool,
    /// Verbose level (0=off, 1=-v, 2=-vv, etc.)
    pub verbose: u8,
    /// Terminal info for formatting
    pub term: TerminalInfo,
    /// Output writer for consistent formatting
    pub output: OutputWriter,
    /// Global output format (human or json) for automation
    pub output_format: OutputFormat,
    /// Optional field path for extracting single values (e.g., "connection.uris[0]")
    pub field: Option<String>,
}

impl Runtime {
    /// Create context with resolved endpoint
    pub fn with_endpoint(
        client: reqwest::Client,
        endpoint: String,
        stone: Option<String>,
        quiet: bool,
        fresh: bool,
        verbose: u8,
    ) -> Self {
        let term = TerminalInfo::detect();
        let output = OutputWriter::new();
        Self {
            client,
            endpoint: Some(endpoint),
            stone,
            quiet,
            fresh,
            verbose,
            term,
            output,
            output_format: OutputFormat::default(),
            field: None,
        }
    }

    /// Create context with all options including automation flags
    #[allow(clippy::too_many_arguments)]
    pub fn with_automation(
        client: reqwest::Client,
        endpoint: Option<String>,
        stone: Option<String>,
        quiet: bool,
        fresh: bool,
        verbose: u8,
        output_format: OutputFormat,
        field: Option<String>,
    ) -> Self {
        let term = TerminalInfo::detect();
        let output = OutputWriter::new();
        Self {
            client,
            endpoint,
            stone,
            quiet,
            fresh,
            verbose,
            term,
            output,
            output_format,
            field,
        }
    }

    /// Create context without endpoint (for local-only commands)
    pub fn without_endpoint(
        client: reqwest::Client,
        quiet: bool,
        fresh: bool,
        verbose: u8,
    ) -> Self {
        let term = TerminalInfo::detect();
        let output = OutputWriter::new();
        Self {
            client,
            endpoint: None,
            stone: None,
            quiet,
            fresh,
            verbose,
            term,
            output,
            output_format: OutputFormat::default(),
            field: None,
        }
    }

    /// Get endpoint, returning error if not resolved
    pub fn endpoint(&self) -> anyhow::Result<&str> {
        self.endpoint
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No stone endpoint available"))
    }

    /// Build URL for API endpoint
    pub fn api_url(&self, path: &str) -> anyhow::Result<String> {
        let base = self.endpoint()?;
        let base = base.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        Ok(format!("{}/{}", base, path))
    }

    /// Build URL for v1 API endpoint
    pub fn api_v1_url(&self, path: &str) -> anyhow::Result<String> {
        let path = path.trim_start_matches('/');
        self.api_url(&format!("api/v1/{}", path))
    }

    /// Check if we should output JSON (either --output json or command-specific --format json)
    pub fn wants_json(&self) -> bool {
        self.output_format.is_json()
    }

    /// Check if we have a field extraction request
    pub fn has_field(&self) -> bool {
        self.field.is_some()
    }

    /// Extract a field from a JSON value using dot notation
    ///
    /// Supports:
    /// - Simple paths: "name", "connection.port"
    /// - Array indexing: "uris[0]", "services[0].name"
    ///
    /// Returns the extracted value as a string, or None if not found.
    pub fn extract_field(&self, value: &serde_json::Value) -> Option<String> {
        let field = self.field.as_ref()?;
        extract_json_field(value, field)
    }
}

/// Extract a field from JSON using dot notation with array indexing
///
/// Examples:
/// - "name" -> value["name"]
/// - "connection.port" -> value["connection"]["port"]
/// - "services[0].name" -> value["services"][0]["name"]
/// - "uris[0]" -> value["uris"][0]
pub fn extract_json_field(value: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = value;

    for segment in path.split('.') {
        // Check for array indexing: "field[0]"
        if let Some(bracket_pos) = segment.find('[') {
            let field_name = &segment[..bracket_pos];
            let rest = &segment[bracket_pos..];

            // Get the field first (if not empty)
            if !field_name.is_empty() {
                current = current.get(field_name)?;
            }

            // Parse array indices like "[0]" or "[0][1]"
            let mut chars = rest.chars().peekable();
            while chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next(); // consume ']'
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

    // Convert to string representation
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => Some("null".to_string()),
        // For objects/arrays, return compact JSON
        _ => Some(current.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url_building() {
        let ctx = Runtime::with_endpoint(
            reqwest::Client::new(),
            "http://10.0.0.5:7185".to_string(),
            Some("stone-01".to_string()),
            false,
            false,
            0,
        );

        assert_eq!(
            ctx.api_url("health").unwrap(),
            "http://10.0.0.5:7185/health"
        );
        assert_eq!(
            ctx.api_v1_url("services").unwrap(),
            "http://10.0.0.5:7185/api/v1/services"
        );
        assert_eq!(
            ctx.api_v1_url("/services").unwrap(),
            "http://10.0.0.5:7185/api/v1/services"
        );
    }

    #[test]
    fn test_endpoint_without_resolution() {
        let ctx = Runtime::without_endpoint(reqwest::Client::new(), false, false, 0);

        assert!(ctx.endpoint().is_err());
        assert!(ctx.api_url("health").is_err());
    }

    #[test]
    fn test_field_extraction_simple() {
        let json = serde_json::json!({
            "name": "mongodb",
            "port": 27017,
            "active": true
        });

        assert_eq!(
            extract_json_field(&json, "name"),
            Some("mongodb".to_string())
        );
        assert_eq!(extract_json_field(&json, "port"), Some("27017".to_string()));
        assert_eq!(
            extract_json_field(&json, "active"),
            Some("true".to_string())
        );
        assert_eq!(extract_json_field(&json, "missing"), None);
    }

    #[test]
    fn test_field_extraction_nested() {
        let json = serde_json::json!({
            "connection": {
                "hostname": "stone-01.local",
                "port": 27017,
                "uris": ["mongodb://stone-01.local:27017", "mongodb://10.0.0.5:27017"]
            }
        });

        assert_eq!(
            extract_json_field(&json, "connection.hostname"),
            Some("stone-01.local".to_string())
        );
        assert_eq!(
            extract_json_field(&json, "connection.port"),
            Some("27017".to_string())
        );
        assert_eq!(
            extract_json_field(&json, "connection.uris[0]"),
            Some("mongodb://stone-01.local:27017".to_string())
        );
        assert_eq!(
            extract_json_field(&json, "connection.uris[1]"),
            Some("mongodb://10.0.0.5:27017".to_string())
        );
    }

    #[test]
    fn test_field_extraction_array_root() {
        let json = serde_json::json!({
            "services": [
                { "name": "mongodb", "port": 27017 },
                { "name": "redis", "port": 6379 }
            ]
        });

        assert_eq!(
            extract_json_field(&json, "services[0].name"),
            Some("mongodb".to_string())
        );
        assert_eq!(
            extract_json_field(&json, "services[1].port"),
            Some("6379".to_string())
        );
    }
}
