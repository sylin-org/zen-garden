//! Shared storage access control.
//!
//! Defines which path names are blocked from cloud drive listings and writes
//! across all storage access paths (WebDAV, cloud filter, file API).

/// Directory names that must never appear in listings or be writable.
///
/// Applies at any depth — checked by both name (single component)
/// and path (full relative path) helpers below.
pub const BLOCKED: &[&str] = &[
    ".zen-garden",               // Zen Garden metadata / manifest
    "Zen Garden",                // Windows display alias for .zen-garden
    "$RECYCLE.BIN",              // Windows recycle bin
    "System Volume Information", // Windows volume metadata
];

/// Returns `true` if `name` (a single path component) is blocked.
pub fn is_blocked_name(name: &str) -> bool {
    BLOCKED.contains(&name)
}

/// Returns `true` if any component of `rel_path` matches a blocked name.
///
/// `rel_path` is relative to the storage root; both `/` and `\` separators
/// are accepted.
pub fn is_blocked_path(rel_path: &str) -> bool {
    rel_path
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .any(|component| BLOCKED.contains(&component))
}

/// Returns the full blocked-name list.
pub fn blocked_paths() -> &'static [&'static str] {
    BLOCKED
}

/// Returns `true` if `value` contains path traversal segments.
///
/// Rejects `..`, root dirs, Windows prefixes, and backslash separators.
/// Shared across all storage access paths (file API, S3 gateway, WebDAV).
pub fn has_path_traversal(value: &str) -> bool {
    if value.contains('\\') {
        return true;
    }
    std::path::Path::new(value).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}
