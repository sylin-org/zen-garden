//! # Zen Garden Build Utilities
//!
//! Shared build-time utilities for capturing build metadata.
//!
//! ## Usage in build.rs
//!
//! ```no_run
//! garden_build_utils::capture_build_number();
//! ```
//!
//! Then in your code:
//! ```ignore
//! let build_number = env!("BUILD_NUMBER");
//! ```

/// Captures the CARGO_BUILD_NUMBER environment variable and makes it available
/// to the crate being built via BUILD_NUMBER.
///
/// This is typically called from a build.rs script. If CARGO_BUILD_NUMBER is not set,
/// it defaults to "dev".
///
/// Uses legacy `cargo:` instruction syntax for compatibility with Rust 1.75+.
///
/// # Example
///
/// ```no_run
/// garden_build_utils::capture_build_number();
/// ```
pub fn capture_build_number() {
    let build_number = std::env::var("CARGO_BUILD_NUMBER").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=BUILD_NUMBER={}", build_number);
    println!("cargo:rerun-if-env-changed=CARGO_BUILD_NUMBER");
    capture_git_sha();
}

/// Captures the short git commit SHA and exposes it to the crate via `GIT_SHA`,
/// for version→commit traceability in `--version` output.
///
/// Resolution order: the `GIT_SHA` environment variable (set by CI, e.g.
/// `${GITHUB_SHA:0:7}`), then `git rev-parse --short=7 HEAD`, then `"unknown"`
/// when neither is available (e.g. a source tarball with no git). Called by
/// [`capture_build_number`]; safe to call on its own from a build script.
pub fn capture_git_sha() {
    let sha = std::env::var("GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_SHA={sha}");
    println!("cargo:rerun-if-env-changed=GIT_SHA");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_build_number() {
        // This is a build-time utility, so we just verify it compiles
        capture_build_number();
    }
}
