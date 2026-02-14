//! JSON utilities
//!
//! Helpers for JSON serialization/deserialization with consistent error handling.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Parse JSON string with context
pub fn parse<T: DeserializeOwned>(json: &str) -> Result<T> {
    serde_json::from_str(json).context("Failed to parse JSON")
}

/// Parse JSON string with custom error context
pub fn parse_with_context<T: DeserializeOwned>(json: &str, context: &str) -> Result<T> {
    serde_json::from_str(json).with_context(|| format!("Failed to parse JSON: {}", context))
}

/// Serialize to JSON string
pub fn stringify<T: Serialize>(data: &T) -> Result<String> {
    serde_json::to_string(data).context("Failed to serialize to JSON")
}

/// Serialize to pretty-printed JSON string
pub fn stringify_pretty<T: Serialize>(data: &T) -> Result<String> {
    serde_json::to_string_pretty(data).context("Failed to serialize to JSON")
}

/// Parse JSON value (untyped)
pub fn parse_value(json: &str) -> Result<serde_json::Value> {
    parse(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_parse_and_stringify() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let json = stringify(&data).expect("Should serialize");
        let parsed: TestData = parse(&json).expect("Should parse");

        assert_eq!(parsed, data);
    }

    #[test]
    fn test_stringify_pretty() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let json = stringify_pretty(&data).expect("Should serialize");
        assert!(json.contains('\n'));
        assert!(json.contains("  "));
    }

    #[test]
    fn test_parse_with_context() {
        let result: Result<TestData> = parse_with_context("invalid", "test data");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test data"));
    }

    #[test]
    fn test_parse_value() {
        let json = r#"{"key": "value", "num": 123}"#;
        let value = parse_value(json).expect("Should parse");

        assert!(value.is_object());
        assert_eq!(value["key"], "value");
        assert_eq!(value["num"], 123);
    }
}
