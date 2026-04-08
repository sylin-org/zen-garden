//! ADR §Acceptance-5 — canonical-key magic-string guard.
//!
//! Every canonical field path used at runtime must be declared as a
//! `FieldPath` constant in `src/domain/keys/`. String literals matching
//! the canonical-key pattern (`"text\.[a-z]…"`, `"image\.[a-z]…"`,
//! etc.) outside that module are forbidden — they bypass the typed
//! key catalog and break refactor safety.
//!
//! This test walks the source tree, applies the exemption rules from
//! the ADR, and fails the build when it finds a violation. It runs
//! with `cargo test` so any platform that builds the orchestrator
//! also enforces the rule — no separate CI shell step required.
//!
//! Exempt contexts (per ADR text):
//! - `src/domain/keys/*` (the authoritative declarations)
//! - `src/domain/primitive.rs` (primitive→dotted name mapping)
//! - Test modules (`#[cfg(test)]` blocks and `mod tests`)
//! - Doc comments (`//!`, `///`)
//! - Log macros (`tracing::*!`, `info!`, `warn!`, `error!`,
//!   `debug!`, `trace!`)
//! - Format/error macros (`format!`, `panic!`, `write!`, `writeln!`,
//!   `anyhow!`, `bail!`)
//! - `OrchestratorError::new(...)` error message arguments

use std::fs;
use std::path::{Path, PathBuf};

const NAMESPACES: &[&str] = &[
    "text", "image", "audio", "usage", "timing", "meta", "job", "stream",
];

#[test]
fn no_canonical_key_magic_strings_outside_domain_keys() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_root = crate_root.join("src");
    assert!(src_root.is_dir(), "expected src/ at {:?}", src_root);

    let mut violations: Vec<String> = Vec::new();
    let mut scanned: usize = 0;

    walk_rust_files(&src_root, &mut |path| {
        // Skip authoritative declaration sites.
        if is_under(path, &src_root.join("domain").join("keys"))
            || path == src_root.join("domain").join("primitive.rs")
        {
            return;
        }
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        scanned += 1;

        let mut in_test_mod = false;

        for (line_idx, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim_start();

            // Track entry into a test-only block. Tests almost always
            // live at the bottom of the file, so once we see one we
            // treat the rest of the file as test scope. This mirrors
            // the bash guard's behavior.
            if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
                in_test_mod = true;
            }
            if in_test_mod {
                continue;
            }

            // Doc / line comment lines are exempt — they aren't code.
            if line.starts_with("//!") || line.starts_with("///") || line.starts_with("//") {
                continue;
            }

            // Log + format + error macros: the literal isn't being
            // used as a key argument, it's narrative text.
            if line_uses_exempt_macro(line) {
                continue;
            }
            if line.contains("OrchestratorError::new") {
                continue;
            }

            if let Some(hit) = find_canonical_key_literal(line) {
                let rel = path
                    .strip_prefix(&crate_root)
                    .unwrap_or(path)
                    .display();
                violations.push(format!(
                    "{rel}:{}: literal `\"{hit}\"` in: {line}",
                    line_idx + 1
                ));
            }
        }
    });

    assert!(scanned > 0, "guard scanned zero files — bad src root?");

    if !violations.is_empty() {
        let listing = violations.join("\n");
        panic!(
            "\nFound {} canonical-key magic-string violation(s) (\u{00A7}ADR Acceptance-5):\n\n{listing}\n\n\
             Every canonical field path must be a FieldPath constant under src/domain/keys/.\n",
            violations.len()
        );
    }
}

/// Scan a single line for the first string literal whose contents
/// match `<namespace>.<snake_case_segment>...`. Returns the match
/// content (without quotes) so the violation message can show it.
fn find_canonical_key_literal(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            // Find the closing quote (simple — Rust doesn't allow raw
            // newlines inside `"..."` literals so a single-line scan
            // is sufficient for our use case).
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'"' {
                if bytes[end] == b'\\' && end + 1 < bytes.len() {
                    end += 2;
                    continue;
                }
                end += 1;
            }
            if end >= bytes.len() {
                break;
            }
            let inner = &line[start..end];
            if looks_like_canonical_key(inner) {
                return Some(inner.to_string());
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    None
}

fn looks_like_canonical_key(s: &str) -> bool {
    let Some((head, tail)) = s.split_once('.') else {
        return false;
    };
    if !NAMESPACES.contains(&head) {
        return false;
    }
    // Tail must start with [a-z] and contain only [a-z0-9_.].
    let mut chars = tail.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

fn line_uses_exempt_macro(line: &str) -> bool {
    const MACROS: &[&str] = &[
        "tracing::",
        "info!",
        "warn!",
        "error!",
        "debug!",
        "trace!",
        "format!",
        "panic!",
        "write!",
        "writeln!",
        "anyhow!",
        "bail!",
        "println!",
        "eprintln!",
    ];
    MACROS.iter().any(|m| line.contains(m))
}

fn is_under(path: &Path, ancestor: &Path) -> bool {
    path.starts_with(ancestor)
}

fn walk_rust_files(root: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rust_files(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            visit(&path);
        }
    }
}
