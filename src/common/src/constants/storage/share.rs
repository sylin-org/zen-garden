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
    BLOCKED.iter().any(|&b| b == name)
}

/// Returns `true` if any component of `rel_path` matches a blocked name.
///
/// `rel_path` is relative to the storage root; both `/` and `\` separators
/// are accepted.
pub fn is_blocked_path(rel_path: &str) -> bool {
    let normalized = rel_path.trim_start_matches('/').trim_start_matches('\\');
    BLOCKED.iter().any(|&blocked| {
        normalized == blocked
            || normalized.starts_with(&format!("{}/", blocked))
            || normalized.starts_with(&format!("{}\\", blocked))
            || normalized.contains(&format!("/{}/", blocked))
            || normalized.contains(&format!("\\{}/", blocked))
            || normalized.ends_with(&format!("/{}", blocked))
            || normalized.ends_with(&format!("\\{}", blocked))
    })
}

/// Returns the full blocked-name list.
pub fn blocked_paths() -> &'static [&'static str] {
    BLOCKED
}
