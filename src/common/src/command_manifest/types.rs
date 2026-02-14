//! Response types for command execution
//!
//! Used by Companions (Cricket, Firefly) and Moss command proxy.

use serde::{Deserialize, Serialize};

/// Response status for command execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Success,
    Error,
    Warning,
}

/// Response from Companion command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Result status
    pub status: ResponseStatus,

    /// Primary output text (for data display)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Human-readable result message
    pub message: String,

    /// Suggested next actions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

impl CommandResponse {
    /// Create success response
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Success,
            output: None,
            message: message.into(),
            suggestions: Vec::new(),
        }
    }

    /// Create success response with output
    pub fn success_with_output(message: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Success,
            output: Some(output.into()),
            message: message.into(),
            suggestions: Vec::new(),
        }
    }

    /// Create success response with details (alias for success_with_output)
    pub fn success_with_details(message: impl Into<String>, details: impl Into<String>) -> Self {
        Self::success_with_output(message, details)
    }

    /// Create error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Error,
            output: None,
            message: message.into(),
            suggestions: Vec::new(),
        }
    }

    /// Create not found error (semantic alias for error)
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::error(message)
    }

    /// Create internal error (semantic alias for error)
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::error(message)
    }

    /// Create warning response
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Warning,
            output: None,
            message: message.into(),
            suggestions: Vec::new(),
        }
    }

    /// Add a suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    /// Add multiple suggestions
    pub fn with_suggestions(
        mut self,
        suggestions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.suggestions
            .extend(suggestions.into_iter().map(|s| s.into()));
        self
    }

    /// Check if successful
    pub fn is_success(&self) -> bool {
        matches!(self.status, ResponseStatus::Success)
    }

    /// Check if error
    pub fn is_error(&self) -> bool {
        matches!(self.status, ResponseStatus::Error)
    }
}

/// Request to send command to an Companion (Rake ? Moss)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionCommandRequest {
    /// Target Companion (e.g., "cricket", "firefly")
    pub companion: String,

    /// Raw command arguments (Companion parses internally)
    pub raw_args: Vec<String>,
}

impl CompanionCommandRequest {
    pub fn new(companion: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            companion: companion.into(),
            raw_args: args,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_response_builder() {
        let resp = CommandResponse::success("Done").with_suggestion("Try 'list' next");

        assert!(resp.is_success());
        assert_eq!(resp.suggestions.len(), 1);
    }

    #[test]
    fn test_response_json_serialization() {
        let resp =
            CommandResponse::error("Not found").with_suggestions(["Try 'list'", "Check spelling"]);

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Not found"));

        let parsed: CommandResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_error());
        assert_eq!(parsed.suggestions.len(), 2);
    }
}
