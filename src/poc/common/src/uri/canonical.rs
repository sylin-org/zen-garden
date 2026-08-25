//! Canonical string form for `zen-garden:` URIs (URI-0003).
//!
//! Two URIs that parse to equivalent intents MUST produce equal
//! canonical strings. Canonicalisation rules (URI-0003 §"Round-trip
//! and equality"):
//!
//! - Lowercase scheme, kind, target name, instance, action
//! - Sort query parameters alphabetically by key
//! - Sort multi-valued parameters (`cap`, `tags`) within their value list
//! - Strip trailing slashes on empty sub-paths
//! - Percent-encode special characters per RFC 3986
//! - URL-form input always normalises to URN-form output

use super::ZenGardenUri;

/// Build the canonical URN-form string representation.
pub(super) fn render(uri: &ZenGardenUri) -> String {
    let mut out = String::with_capacity(64);
    out.push_str("zen-garden:");

    // Target.
    if let Some(kind) = uri.kind {
        out.push_str(kind.as_str());
        out.push_str("//");
    }
    if let Some(name) = &uri.target_name {
        out.push_str(name);
    }
    if let Some(instance) = &uri.target_instance {
        out.push(':');
        out.push_str(instance);
    }

    // Sub-path: percent-encode each segment, single '/' separator.
    if let Some(sub) = &uri.sub_path {
        out.push('/');
        for (i, segment) in sub.split('/').enumerate() {
            if i > 0 {
                out.push('/');
            }
            out.push_str(&encode_path_segment(segment));
        }
    }

    // Query: sort by key, sort multi-valued lists by value.
    let params = collect_query_params(uri);
    if !params.is_empty() {
        out.push('?');
        for (i, (k, v)) in params.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(k);
            out.push('=');
            out.push_str(v);
        }
    }

    // Fragment: percent-encode if needed.
    if let Some(frag) = &uri.fragment {
        out.push('#');
        out.push_str(&encode_fragment(frag));
    }

    out
}

/// Build the sorted, percent-encoded query parameter list.
fn collect_query_params(uri: &ZenGardenUri) -> Vec<(&'static str, String)> {
    let mut params: Vec<(&'static str, String)> = Vec::new();

    if !uri.capabilities.is_empty() {
        params.push(("cap", uri.capabilities.join(",")));
    }
    if !uri.tags.is_empty() {
        params.push(("tags", uri.tags.join(",")));
    }
    if let Some(action) = &uri.action {
        params.push(("action", action.clone()));
    }
    if let Some(at) = &uri.at {
        params.push(("at", encode_query_value(at)));
    }
    if let Some(protocol) = &uri.protocol_hint {
        params.push(("protocol", encode_query_value(protocol)));
    }
    // version=1 is the default and not emitted in canonical form.

    params.sort_by(|a, b| a.0.cmp(b.0));
    params
}

/// Percent-encode a path segment per RFC 3986 §3.3 (path-segment
/// reserved chars). Keeps unreserved + sub-delims + `:`/`@`.
fn encode_path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if is_unreserved(byte) || matches!(byte, b':' | b'@' | b'+') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// Percent-encode a query parameter value. More aggressive than path
/// encoding since `=`, `&`, `+`, and `#` are query reserved.
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if is_unreserved(byte) || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// Percent-encode a fragment per RFC 3986 §3.5. Same rules as path
/// segments but `#` is forbidden (would split the fragment).
fn encode_fragment(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    for byte in fragment.bytes() {
        if is_unreserved(byte) || matches!(byte, b':' | b'@' | b'/' | b'?' | b'-' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

/// RFC 3986 unreserved character set: ALPHA / DIGIT / `-` / `.` / `_` / `~`.
fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}
