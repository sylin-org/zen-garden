//! Validation utilities
//!
//! Common validation patterns for names, paths, and inputs.

use std::path::Path;

/// Validate offering/service name
///
/// Rules:
/// - Must be 1-64 characters
/// - Lowercase alphanumeric + hyphens only
/// - Must start and end with alphanumeric
/// - No consecutive hyphens
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    if name.len() > 64 {
        return Err(format!("Name too long: {} characters (max 64)", name.len()));
    }

    if !name.chars().next().unwrap().is_ascii_alphanumeric() {
        return Err("Name must start with alphanumeric character".to_string());
    }

    if !name.chars().last().unwrap().is_ascii_alphanumeric() {
        return Err("Name must end with alphanumeric character".to_string());
    }

    if name.contains("--") {
        return Err("Name cannot contain consecutive hyphens".to_string());
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("Name can only contain lowercase letters, digits, and hyphens".to_string());
    }

    Ok(())
}

/// Check if name is valid (boolean version)
pub fn is_valid_name(name: &str) -> bool {
    validate_name(name).is_ok()
}

/// Validate port number
pub fn validate_port(port: u16) -> Result<(), String> {
    if port < 1024 {
        return Err(format!("Port {} is in privileged range (< 1024)", port));
    }
    Ok(())
}

/// Validate URL format (basic check)
pub fn validate_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    if url.len() < 10 {
        return Err("URL too short".to_string());
    }

    Ok(())
}

/// Validate path is safe (no parent directory traversal)
pub fn validate_safe_path<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let path_str = path.as_ref().to_string_lossy();

    if path_str.contains("..") {
        return Err("Path cannot contain '..' (parent directory)".to_string());
    }

    if path_str.starts_with('/') && !cfg!(target_os = "windows") {
        return Err("Path cannot be absolute".to_string());
    }

    if path_str.contains('\\') && !cfg!(target_os = "windows") {
        return Err("Path cannot contain backslashes".to_string());
    }

    Ok(())
}

/// Validate JSON string is well-formed
pub fn validate_json(json: &str) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(json)
        .map(|_| ())
        .map_err(|e| format!("Invalid JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name() {
        // Valid names
        assert!(validate_name("mongodb").is_ok());
        assert!(validate_name("redis-cache").is_ok());
        assert!(validate_name("app123").is_ok());
        assert!(validate_name("my-app-v2").is_ok());

        // Invalid names
        assert!(validate_name("").is_err());
        assert!(validate_name("MongoDB").is_err()); // Uppercase
        assert!(validate_name("-redis").is_err()); // Starts with hyphen
        assert!(validate_name("redis-").is_err()); // Ends with hyphen
        assert!(validate_name("my--app").is_err()); // Consecutive hyphens
        assert!(validate_name("app_name").is_err()); // Underscore
        assert!(validate_name(&"a".repeat(65)).is_err()); // Too long
    }

    #[test]
    fn test_is_valid_name() {
        assert!(is_valid_name("valid-name"));
        assert!(!is_valid_name("Invalid-Name"));
    }

    #[test]
    fn test_validate_port() {
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(1024).is_ok());
        assert!(validate_port(65535).is_ok());

        assert!(validate_port(80).is_err());
        assert!(validate_port(443).is_err());
        assert!(validate_port(1023).is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("http://example.com").is_ok());
        assert!(validate_url("https://example.com:8080/path").is_ok());

        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("http://").is_err());
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn test_validate_safe_path() {
        assert!(validate_safe_path("data/file.txt").is_ok());
        assert!(validate_safe_path("subdir/data/file.txt").is_ok());

        assert!(validate_safe_path("../etc/passwd").is_err());
        assert!(validate_safe_path("data/../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_json() {
        assert!(validate_json(r#"{"key": "value"}"#).is_ok());
        assert!(validate_json(r#"[1, 2, 3]"#).is_ok());
        assert!(validate_json(r#"null"#).is_ok());

        assert!(validate_json("").is_err());
        assert!(validate_json("{invalid}").is_err());
        assert!(validate_json("not json").is_err());
    }
}
