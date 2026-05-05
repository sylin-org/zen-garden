//! Parser for `zen-garden:` URIs (URI-0003).
//!
//! See [`super::ZenGardenUri::parse`] for the public entry point.

use std::collections::BTreeSet;

use super::error::UriError;
use super::kind::Kind;
use super::ZenGardenUri;
use crate::constants::reserved_names;

const SCHEME: &str = "zen-garden";

/// Parse a `zen-garden:` URI string into a structured [`ZenGardenUri`].
///
/// See URI-0003 for the full grammar. Briefly:
/// - Scheme matches case-insensitively (`zen-garden:`).
/// - Optional leading `//` is tolerated as a URL-form alias and stripped.
/// - Target may be a bare name, `<kind>//<name>`, or empty (only when
///   the URI carries a `cap=` query).
/// - `:instance` qualifier may appear on the name.
/// - `/<sub-path>` may follow the target.
/// - `?<query>` and `#<fragment>` follow standard URI syntax.
pub fn parse(input: &str) -> Result<ZenGardenUri, UriError> {
    let trimmed = input.trim();

    // Step 1: Strip and validate scheme (case-insensitive).
    let colon_idx = trimmed.find(':').ok_or_else(|| UriError::InvalidScheme {
        found: trimmed.to_string(),
    })?;
    let scheme = &trimmed[..colon_idx];
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return Err(UriError::InvalidScheme {
            found: scheme.to_string(),
        });
    }
    let mut rest = &trimmed[colon_idx + 1..];

    // Step 2: Strip optional leading "//" (URL-form alias tolerance).
    if let Some(stripped) = rest.strip_prefix("//") {
        rest = stripped;
    }

    // Step 3: Split off fragment on the first '#'.
    let (rest, fragment_raw) = match rest.find('#') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    // Step 4: Split off query on the first '?'.
    let (target_path, query_raw) = match rest.find('?') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    // Step 5: Parse target and sub-path.
    let target_parsed = parse_target_path(target_path)?;

    // Step 6: Parse query parameters.
    let params = parse_query(query_raw)?;

    // Step 7: Validate version.
    if let Some(v) = params.version
        && v != 1
    {
        return Err(UriError::UnsupportedVersion { version: v });
    }

    // Step 8: Empty target requires cap= query.
    if target_parsed.target_name.is_none() && params.capabilities.is_empty() {
        return Err(UriError::EmptyTargetNoCap);
    }

    // Step 9: Decode percent-encoding in sub_path and fragment.
    let sub_path = match target_parsed.sub_path {
        Some(s) => Some(decode(&s)?),
        None => None,
    };
    let fragment = match fragment_raw {
        Some(f) => Some(decode(f)?),
        None => None,
    };

    Ok(ZenGardenUri {
        kind: target_parsed.kind,
        target_name: target_parsed.target_name,
        target_instance: target_parsed.target_instance,
        sub_path,
        capabilities: params.capabilities,
        action: params.action,
        at: params.at,
        tags: params.tags,
        protocol_hint: params.protocol_hint,
        fragment,
        version: params.version.unwrap_or(1),
    })
}

struct TargetParsed {
    kind: Option<Kind>,
    target_name: Option<String>,
    target_instance: Option<String>,
    sub_path: Option<String>,
}

/// Parse the target-and-sub-path portion of the URI (everything before
/// the first `?` or `#`).
fn parse_target_path(input: &str) -> Result<TargetParsed, UriError> {
    // Empty or just slashes → empty target.
    let stripped = input.trim_end_matches('/');
    if stripped.is_empty() {
        return Ok(TargetParsed {
            kind: None,
            target_name: None,
            target_instance: None,
            sub_path: None,
        });
    }

    // Detect explicit-kind form by the first '//' separator.
    if let Some(double_slash_idx) = stripped.find("//") {
        let kind_str = &stripped[..double_slash_idx];
        let after_kind = &stripped[double_slash_idx + 2..];

        let kind = Kind::parse(kind_str).ok_or_else(|| UriError::InvalidKind {
            kind: kind_str.to_string(),
        })?;

        // After the kind, we expect <name>[:<instance>][/<sub-path>].
        // A second '//' is not permitted anywhere here.
        if after_kind.contains("//") {
            return Err(UriError::MalformedTarget {
                detail: "extra '//' after the name in explicit-kind form".to_string(),
            });
        }

        let (name_part, sub_path) = split_first_slash(after_kind);
        let (name, instance) = split_name_instance(name_part)?;

        if name.is_empty() {
            return Err(UriError::MalformedTarget {
                detail: "explicit-kind form has empty name".to_string(),
            });
        }

        return Ok(TargetParsed {
            kind: Some(kind),
            target_name: Some(name.to_ascii_lowercase()),
            target_instance: instance,
            sub_path,
        });
    }

    // Bare-name form.
    let (name_part, sub_path) = split_first_slash(stripped);
    let (name, instance) = split_name_instance(name_part)?;

    if name.is_empty() {
        return Err(UriError::MalformedTarget {
            detail: "empty target".to_string(),
        });
    }

    let name_lower = name.to_ascii_lowercase();

    // Reject reserved keywords as bare cascade targets — they may only
    // appear in the explicit-kind form.
    if reserved_names::is_reserved(&name_lower) {
        return Err(UriError::ReservedNameAsTarget { name: name_lower });
    }

    Ok(TargetParsed {
        kind: None,
        target_name: Some(name_lower),
        target_instance: instance,
        sub_path,
    })
}

/// Split on the first `/`. Sub-path is `Some` only when there is content
/// after the slash.
fn split_first_slash(input: &str) -> (&str, Option<String>) {
    match input.find('/') {
        Some(idx) => {
            let head = &input[..idx];
            let tail = &input[idx + 1..];
            let tail = tail.trim_end_matches('/');
            if tail.is_empty() {
                (head, None)
            } else {
                (head, Some(tail.to_string()))
            }
        }
        None => (input, None),
    }
}

/// Split a name part on its first `:` to separate `<name>` from
/// `:<instance>`. The instance is lowercased.
fn split_name_instance(input: &str) -> Result<(&str, Option<String>), UriError> {
    match input.find(':') {
        Some(idx) => {
            let name = &input[..idx];
            let instance_part = &input[idx + 1..];
            if instance_part.is_empty() {
                Err(UriError::MalformedTarget {
                    detail: "empty instance after ':'".to_string(),
                })
            } else if instance_part.contains(':') {
                Err(UriError::MalformedTarget {
                    detail: "multiple ':' in name; instance must be a single segment".to_string(),
                })
            } else {
                Ok((name, Some(instance_part.to_ascii_lowercase())))
            }
        }
        None => Ok((input, None)),
    }
}

#[derive(Default)]
struct QueryParsed {
    capabilities: Vec<String>,
    action: Option<String>,
    at: Option<String>,
    tags: Vec<String>,
    protocol_hint: Option<String>,
    version: Option<u32>,
}

/// Parse the query string (everything after `?` and before `#`).
///
/// Multi-valued parameters (`cap`, `tags`) accept both `cap=a,b` and
/// `cap=a&cap=b`. Output sets are sorted and deduplicated.
fn parse_query(input: Option<&str>) -> Result<QueryParsed, UriError> {
    let Some(query) = input else {
        return Ok(QueryParsed::default());
    };

    // Empty query string after '?' is malformed.
    if query.is_empty() {
        return Err(UriError::MalformedQuery {
            detail: "empty query string after '?'".to_string(),
        });
    }

    let mut caps: BTreeSet<String> = BTreeSet::new();
    let mut tags: BTreeSet<String> = BTreeSet::new();
    let mut action: Option<String> = None;
    let mut at: Option<String> = None;
    let mut protocol_hint: Option<String> = None;
    let mut version: Option<u32> = None;

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }

        let eq_idx = pair.find('=').ok_or_else(|| UriError::MalformedQuery {
            detail: format!("query parameter '{pair}' has no '='"),
        })?;
        let raw_key = &pair[..eq_idx];
        let raw_value = &pair[eq_idx + 1..];

        let key = decode(raw_key)?.to_ascii_lowercase();
        let value = decode(raw_value)?;

        match key.as_str() {
            "cap" => {
                for v in split_multi(&value) {
                    caps.insert(v);
                }
            }
            "tags" => {
                for v in split_multi(&value) {
                    tags.insert(v);
                }
            }
            "action" => action = Some(value.to_ascii_lowercase()),
            "at" => at = Some(value),
            "protocol" => protocol_hint = Some(value),
            "v" => {
                let parsed: u32 =
                    value.parse().map_err(|_| UriError::MalformedQuery {
                        detail: format!("v= must be a non-negative integer, got '{value}'"),
                    })?;
                version = Some(parsed);
            }
            // Forward-compatibility: unknown keys are silently ignored.
            _ => {}
        }
    }

    Ok(QueryParsed {
        capabilities: caps.into_iter().collect(),
        action,
        at,
        tags: tags.into_iter().collect(),
        protocol_hint,
        version,
    })
}

/// Split a multi-valued query parameter on `,` or `|`, trimming and
/// lowercasing each non-empty token.
fn split_multi(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split([',', '|'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
}

/// Percent-decode a URI component. Maps decoding errors to
/// [`UriError::MalformedEncoding`].
fn decode(s: &str) -> Result<String, UriError> {
    urlencoding::decode(s)
        .map(|cow| cow.into_owned())
        .map_err(|e| UriError::MalformedEncoding {
            detail: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(uri: &str) -> ZenGardenUri {
        parse(uri).unwrap_or_else(|e| panic!("expected '{uri}' to parse, got {e:?}"))
    }

    fn err(uri: &str) -> UriError {
        parse(uri).expect_err(&format!("expected '{uri}' to fail"))
    }

    #[test]
    fn bare_offering() {
        let u = ok("zen-garden:mongodb");
        assert_eq!(u.target_name.as_deref(), Some("mongodb"));
        assert!(u.kind.is_none());
        assert!(u.sub_path.is_none());
    }

    #[test]
    fn url_form_alias() {
        let urn = ok("zen-garden:mongodb");
        let url = ok("zen-garden://mongodb");
        assert_eq!(urn.canonical(), url.canonical());
    }

    #[test]
    fn explicit_kind() {
        let u = ok("zen-garden:offering//mongodb");
        assert_eq!(u.kind, Some(Kind::Offering));
        assert_eq!(u.target_name.as_deref(), Some("mongodb"));
    }

    #[test]
    fn explicit_kind_double_slash_after_name_rejected() {
        assert!(matches!(
            err("zen-garden:offering//mongodb//db-a"),
            UriError::MalformedTarget { .. }
        ));
    }

    #[test]
    fn bare_double_slash_invalid_kind() {
        assert!(matches!(
            err("zen-garden:mongodb//db-a"),
            UriError::InvalidKind { .. }
        ));
    }

    #[test]
    fn capability_only() {
        let u = ok("zen-garden:?cap=s3");
        assert!(u.target_name.is_none());
        assert_eq!(u.capabilities, vec!["s3"]);
    }

    #[test]
    fn empty_target_no_cap_rejected() {
        assert_eq!(err("zen-garden:"), UriError::EmptyTargetNoCap);
        assert_eq!(err("zen-garden:?action=wish"), UriError::EmptyTargetNoCap);
    }

    #[test]
    fn reserved_name_as_target_rejected() {
        assert!(matches!(
            err("zen-garden:offering"),
            UriError::ReservedNameAsTarget { .. }
        ));
    }

    #[test]
    fn trailing_slash_stripped() {
        let u = ok("zen-garden:mongodb/");
        assert!(u.sub_path.is_none());
    }
}
