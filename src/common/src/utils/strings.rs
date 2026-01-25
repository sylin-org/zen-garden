//! String utilities
//!
//! Helper functions for common string operations.

/// Truncate string to maximum length, adding ellipsis if needed
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let mut result: String = s.chars().take(max_len - 3).collect();
        result.push_str("...");
        result
    }
}

/// Truncate string to maximum length with custom suffix
pub fn truncate_with_suffix(s: &str, max_len: usize, suffix: &str) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let suffix_chars = suffix.chars().count();
        if max_len <= suffix_chars {
            s.chars().take(max_len).collect()
        } else {
            let mut result: String = s.chars().take(max_len - suffix_chars).collect();
            result.push_str(suffix);
            result
        }
    }
}

/// Join strings with separator, filtering out empty strings
pub fn join_non_empty(parts: &[&str], separator: &str) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(separator)
}

/// Convert string to kebab-case (lowercase with hyphens)
pub fn to_kebab_case(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_uppercase() {
                format!("-{}", c.to_lowercase())
            } else if c.is_whitespace() {
                "-".to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<String>()
        .trim_start_matches('-')
        .to_string()
}

/// Convert string to snake_case (lowercase with underscores)
pub fn to_snake_case(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_uppercase() {
                format!("_{}", c.to_lowercase())
            } else if c.is_whitespace() {
                "_".to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<String>()
        .trim_start_matches('_')
        .to_string()
}

/// Check if string is a valid identifier (alphanumeric + underscore/hyphen)
pub fn is_valid_identifier(s: &str) -> bool {
    !s.is_empty() 
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        && !s.starts_with(|c: char| c.is_numeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hello world", 5), "he...");
        assert_eq!(truncate("hi", 2), "hi");
    }

    #[test]
    fn test_truncate_with_suffix() {
        assert_eq!(truncate_with_suffix("hello world", 10, "…"), "hello wor…");
        assert_eq!(truncate_with_suffix("hello", 10, "…"), "hello");
        assert_eq!(truncate_with_suffix("test", 3, "…"), "te…");
    }

    #[test]
    fn test_join_non_empty() {
        assert_eq!(
            join_non_empty(&["hello", "", "world", ""], ", "),
            "hello, world"
        );
        assert_eq!(join_non_empty(&["", "", ""], ", "), "");
        assert_eq!(join_non_empty(&["only"], ", "), "only");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("HelloWorld"), "hello-world");
        assert_eq!(to_kebab_case("hello world"), "hello-world");
        assert_eq!(to_kebab_case("helloWorld"), "hello-world");
        assert_eq!(to_kebab_case("already-kebab"), "already-kebab");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("hello world"), "hello_world");
        assert_eq!(to_snake_case("helloWorld"), "hello_world");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("valid_name"));
        assert!(is_valid_identifier("valid-name"));
        assert!(is_valid_identifier("valid123"));
        assert!(is_valid_identifier("_valid"));
        
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123invalid"));
        assert!(!is_valid_identifier("invalid name"));
        assert!(!is_valid_identifier("invalid.name"));
    }
}
