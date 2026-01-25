//! ID generation utilities
//!
//! Thin wrappers around the uuid crate for generating GUIDv7 (RFC 9562).
//! All IDs use timestamp-based UUIDs that sort naturally by creation time.

/// Generate a GUIDv7 (RFC 9562) using the uuid crate
///
/// Format: xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx (36 characters)
/// Example: "018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f"
pub fn generate_guidv7() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Generate a GUIDv7 with a domain prefix
///
/// Format: {prefix}-{guidv7}
/// Example: "mongodb-018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f"
pub fn generate_id(prefix: &str) -> String {
    format!("{}-{}", prefix, generate_guidv7())
}

/// Alias for generate_guidv7()
pub fn generate_timestamp_id() -> String {
    generate_guidv7()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_guidv7() {
        let id = generate_id("mongodb");
        
        assert!(id.starts_with("mongodb-"));
        
        // After prefix, should have standard GUIDv7
        let guid_part = id.strip_prefix("mongodb-").unwrap();
        assert_eq!(guid_part.len(), 36);
        assert_eq!(guid_part.matches('-').count(), 4);
        
        // Should be unique
        let id2 = generate_id("mongodb");
        assert_ne!(id, id2);
    }

    #[test]
    fn test_generate_timestamp_id() {
        let id = generate_timestamp_id();
        
        // Should be same as guidv7
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }

    #[test]
    fn test_guidv7_natural_sorting() {
        use std::thread;
        use std::time::Duration;
        
        let id1 = generate_guidv7();
        thread::sleep(Duration::from_millis(2));
        let id2 = generate_guidv7();
        
        // Later IDs should sort after earlier ones (lexicographic)
        assert!(id2 > id1);
    }

    #[test]
    fn test_guidv7_format_consistency() {
        let id1 = generate_id("offering");
        let id2 = generate_id("offering");
        
        // Both should have same structure
        assert_eq!(
            id1.matches('-').count(),
            id2.matches('-').count()
        );
        
        // Prefix should be consistent
        assert!(id1.starts_with("offering-"));
        assert!(id2.starts_with("offering-"));
        
        // Should have 5 hyphens total (1 after prefix + 4 in UUID)
        assert_eq!(id1.matches('-').count(), 5);
        assert_eq!(id2.matches('-').count(), 5);
    }
}
