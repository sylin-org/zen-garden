//! Connection string resolution utilities
//!
//! Provides reusable functions for resolving service connection URIs.
//! Supports hostname-first resolution with IP fallback for resilience.

use garden_common::manifests::{get_category_registry, ConnectionProfile};
use serde::{Deserialize, Serialize};

/// Resolved connection information for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConnection {
    /// Hostname (e.g., "stone-02.local")
    pub hostname: String,
    /// IP address (e.g., "192.168.1.102")
    pub ip: String,
    /// Service port
    pub port: u16,
    /// Protocol (e.g., "mongodb", "postgresql", "redis")
    pub protocol: String,
    /// Connection URIs - hostname-first, then IP (for resilience)
    pub uris: Vec<String>,
}

/// Default connection templates by protocol
///
/// Used when offering manifest doesn't specify a connection profile.
pub fn default_template(protocol: &str) -> String {
    match protocol.to_lowercase().as_str() {
        "mongodb" => "mongodb://{host}:{port}".to_string(),
        "postgresql" | "postgres" => "postgresql://{host}:{port}".to_string(),
        "mysql" | "mariadb" => "mysql://{host}:{port}".to_string(),
        "redis" => "redis://{host}:{port}".to_string(),
        "elasticsearch" => "http://{host}:{port}".to_string(),
        "meilisearch" => "http://{host}:{port}".to_string(),
        "minio" | "s3" => "http://{host}:{port}".to_string(),
        "nats" => "nats://{host}:{port}".to_string(),
        "rabbitmq" | "amqp" => "amqp://{host}:{port}".to_string(),
        "http" | "https" => "{protocol}://{host}:{port}".to_string(),
        _ => "{protocol}://{host}:{port}".to_string(),
    }
}

/// Extract protocol scheme from a connection template.
///
/// Example: `mongodb://{host}:{port}` -> `mongodb`
pub fn protocol_from_template(template: &str) -> Option<String> {
    let trimmed = template.trim();
    if let Some(scheme_end) = trimmed.find("://") {
        let scheme = trimmed[..scheme_end].trim().to_ascii_lowercase();
        if !scheme.is_empty() && is_literal_uri_scheme(&scheme) {
            return Some(scheme);
        }
    }

    // Some manifests provide structured templates (for example JSON blobs)
    // that embed URLs instead of being raw URI templates.
    if let Some(scheme) = find_embedded_uri_scheme(trimmed) {
        return Some(scheme);
    }

    None
}

fn is_literal_uri_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.'
}

fn find_embedded_uri_scheme(template: &str) -> Option<String> {
    let mut search_from = 0usize;
    while search_from < template.len() {
        let rel = template[search_from..].find("://")?;
        let scheme_end = search_from + rel;

        let mut scheme_start = scheme_end;
        while scheme_start > 0 {
            let ch = template.as_bytes()[scheme_start - 1] as char;
            if is_scheme_char(ch) {
                scheme_start -= 1;
            } else {
                break;
            }
        }

        let candidate = template[scheme_start..scheme_end]
            .trim()
            .to_ascii_lowercase();
        if is_literal_uri_scheme(&candidate) {
            return Some(candidate);
        }

        search_from = scheme_end + 3;
    }

    None
}

/// Infer protocol using manifest metadata (connection profile + category).
///
/// Priority:
/// 1. `connection.protocol` when present
/// 2. `connection.uri_template` scheme when present
/// 3. category connection profile (protocol or uri_template)
/// 3. `"tcp"` fallback with warning
pub fn infer_protocol_from_manifest_metadata(
    offering_name: &str,
    category: &str,
    connection: Option<&ConnectionProfile>,
) -> String {
    if let Some(conn) = connection {
        if let Some(protocol) = conn.protocol.as_deref() {
            if !protocol.trim().is_empty() {
                return protocol.trim().to_string();
            }
        }
        if let Some(template) = conn.uri_template.as_deref() {
            if let Some(protocol) = protocol_from_template(template) {
                return protocol;
            }
        }
    }

    if let Some(category_conn) = get_category_registry().connection(category) {
        if let Some(protocol) = category_conn.protocol.as_deref() {
            if !protocol.trim().is_empty() {
                return protocol.trim().to_string();
            }
        }
        if let Some(template) = category_conn.uri_template.as_deref() {
            if let Some(protocol) = protocol_from_template(template) {
                return protocol;
            }
        }
    }

    tracing::warn!(
        offering = %offering_name,
        category = %category,
        "No protocol found in manifest or category, using 'tcp'"
    );
    "tcp".to_string()
}

/// Select the best URI template from offering or category connection profiles.
pub fn select_uri_template(
    connection: Option<&ConnectionProfile>,
    category: &str,
) -> Option<String> {
    if let Some(conn) = connection {
        if let Some(template) = conn.uri_template.as_deref() {
            if !template.trim().is_empty() {
                return Some(template.to_string());
            }
        }
    }

    if let Some(category_conn) = get_category_registry().connection(category) {
        if let Some(template) = category_conn.uri_template.as_deref() {
            if !template.trim().is_empty() {
                return Some(template.to_string());
            }
        }
    }

    None
}

/// Infer protocol from offering category and name
///
/// Infer protocol from offering manifest or category registry
///
/// Looks up offering's connection profile to determine protocol,
/// falls back to category defaults, then "tcp" as last resort.
pub async fn infer_protocol(
    offering_name: &str,
    category: &str,
    state: &crate::app_state::AppState,
) -> String {
    let connection = state
        .manifest_registry
        .get_offering(offering_name)
        .and_then(|offering| offering.connection.as_ref());

    infer_protocol_from_manifest_metadata(offering_name, category, connection)
}

/// Extract IP address from endpoint URL
///
/// # Example
/// ```ignore
/// let ip = extract_ip("http://192.168.1.102:7185");
/// assert_eq!(ip, "192.168.1.102");
/// ```
pub fn extract_ip(endpoint: &str) -> String {
    // Remove protocol prefix
    let without_protocol = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);

    // Extract host:port or just host
    let host_port = without_protocol
        .split('/')
        .next()
        .unwrap_or(without_protocol);

    // Remove port if present
    if let Some(bracket_end) = host_port.find(']') {
        // IPv6 address like [::1]:8080
        host_port[1..bracket_end].to_string()
    } else if let Some(colon_pos) = host_port.rfind(':') {
        // Check if it's a port separator (not part of IPv6)
        let potential_host = &host_port[..colon_pos];
        if potential_host.contains(':') {
            // IPv6 without brackets
            host_port.to_string()
        } else {
            potential_host.to_string()
        }
    } else {
        host_port.to_string()
    }
}

/// Build hostname from stone name
///
/// Appends `.local` suffix for mDNS resolution.
pub fn build_hostname(stone_name: &str) -> String {
    if stone_name.contains('.') {
        // Already has domain suffix
        stone_name.to_string()
    } else {
        format!("{}.local", stone_name)
    }
}

/// Resolve connection URIs from template
///
/// Applies template substitution and returns both hostname and IP-based URIs.
///
/// # Arguments
/// * `template` - Connection template with placeholders (e.g., "mongodb://{host}:{port}")
/// * `hostname` - mDNS hostname (e.g., "stone-02.local")
/// * `ip` - IP address (e.g., "192.168.1.102")
/// * `port` - Service port
/// * `protocol` - Protocol name (used for {protocol} placeholder)
///
/// # Returns
/// Vector of URIs with IP-based first (reliable in containers), hostname-based second (human-readable)
pub fn resolve_uris(
    template: &str,
    hostname: &str,
    ip: &str,
    port: u16,
    protocol: &str,
) -> Vec<String> {
    let uri_ip = template
        .replace("{host}", ip)
        .replace("{port}", &port.to_string())
        .replace("{protocol}", protocol);

    let uri_hostname = template
        .replace("{host}", hostname)
        .replace("{port}", &port.to_string())
        .replace("{protocol}", protocol);

    // IP first (reliable — .local mDNS fails in Docker on Windows),
    // hostname second (human-readable fallback)
    if uri_ip != uri_hostname {
        vec![uri_ip, uri_hostname]
    } else {
        vec![uri_ip]
    }
}

/// Full connection resolution from service and stone info
///
/// This is the main entry point for resolving a complete connection.
pub fn resolve_connection(
    stone_name: &str,
    stone_endpoint: &str,
    port: u16,
    protocol: &str,
    template: Option<&str>,
) -> ResolvedConnection {
    let hostname = build_hostname(stone_name);
    let ip = extract_ip(stone_endpoint);

    let effective_template = template
        .map(|t| t.to_string())
        .unwrap_or_else(|| default_template(protocol));

    let uris = resolve_uris(&effective_template, &hostname, &ip, port, protocol);

    ResolvedConnection {
        hostname,
        ip,
        port,
        protocol: protocol.to_string(),
        uris,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ip_with_port() {
        assert_eq!(extract_ip("http://192.168.1.102:7185"), "192.168.1.102");
    }

    #[test]
    fn test_extract_ip_without_port() {
        assert_eq!(extract_ip("http://192.168.1.102"), "192.168.1.102");
    }

    #[test]
    fn test_extract_ip_no_protocol() {
        assert_eq!(extract_ip("192.168.1.102:7185"), "192.168.1.102");
    }

    #[test]
    fn test_extract_ip_with_path() {
        assert_eq!(
            extract_ip("http://192.168.1.102:7185/api/v1"),
            "192.168.1.102"
        );
    }

    #[test]
    fn test_build_hostname_simple() {
        assert_eq!(build_hostname("stone-02"), "stone-02.local");
    }

    #[test]
    fn test_build_hostname_already_qualified() {
        assert_eq!(build_hostname("stone-02.local"), "stone-02.local");
        assert_eq!(build_hostname("server.example.com"), "server.example.com");
    }

    #[test]
    fn test_resolve_uris_mongodb() {
        let uris = resolve_uris(
            "mongodb://{host}:{port}",
            "stone-02.local",
            "192.168.1.102",
            27017,
            "mongodb",
        );
        assert_eq!(uris.len(), 2);
        assert_eq!(uris[0], "mongodb://192.168.1.102:27017");
        assert_eq!(uris[1], "mongodb://stone-02.local:27017");
    }

    #[test]
    fn test_resolve_uris_with_protocol_placeholder() {
        let uris = resolve_uris(
            "{protocol}://{host}:{port}",
            "stone-01.local",
            "10.0.0.1",
            8080,
            "http",
        );
        assert_eq!(uris[0], "http://10.0.0.1:8080");
        assert_eq!(uris[1], "http://stone-01.local:8080");
    }

    #[test]
    fn test_resolve_connection_full() {
        let conn = resolve_connection(
            "stone-02",
            "http://192.168.1.102:7185",
            27017,
            "mongodb",
            Some("mongodb://{host}:{port}"),
        );

        assert_eq!(conn.hostname, "stone-02.local");
        assert_eq!(conn.ip, "192.168.1.102");
        assert_eq!(conn.port, 27017);
        assert_eq!(conn.protocol, "mongodb");
        assert_eq!(conn.uris.len(), 2);
        assert_eq!(conn.uris[0], "mongodb://192.168.1.102:27017");
    }

    #[test]
    fn test_resolve_connection_default_template() {
        let conn = resolve_connection("stone-01", "http://10.0.0.1:7185", 6379, "redis", None);

        assert_eq!(conn.uris[0], "redis://10.0.0.1:6379");
    }

    #[test]
    fn test_protocol_from_template() {
        assert_eq!(
            protocol_from_template("mongodb://{host}:{port}"),
            Some("mongodb".to_string())
        );
        assert_eq!(
            protocol_from_template(" HTTPS://example "),
            Some("https".to_string())
        );
        assert_eq!(protocol_from_template("{protocol}://{host}:{port}"), None);
        assert_eq!(
            protocol_from_template(
                r#"{
  "base_url": "http://{{ host }}:{{ port }}",
  "tags_url": "http://{{ host }}:{{ port }}/api/tags"
}"#
            ),
            Some("http".to_string())
        );
        assert_eq!(protocol_from_template(""), None);
    }

    #[test]
    fn test_is_literal_uri_scheme() {
        assert!(is_literal_uri_scheme("http"));
        assert!(is_literal_uri_scheme("mongodb"));
        assert!(is_literal_uri_scheme("redis+tls"));
        assert!(!is_literal_uri_scheme("{protocol}"));
        assert!(!is_literal_uri_scheme("1http"));
        assert!(!is_literal_uri_scheme(""));
    }

    #[test]
    fn test_find_embedded_uri_scheme() {
        assert_eq!(
            find_embedded_uri_scheme(r#"{ "endpoint": "nats://{host}:{port}" }"#),
            Some("nats".to_string())
        );
        assert_eq!(find_embedded_uri_scheme("{protocol}://{host}:{port}"), None);
        assert_eq!(find_embedded_uri_scheme("no uri here"), None);
    }

    #[test]
    fn test_infer_protocol_from_manifest_metadata_prefers_template() {
        let profile = ConnectionProfile {
            protocol: None,
            uri_template: Some("mongodb://{host}:{port}".to_string()),
            endpoints: std::collections::BTreeMap::new(),
        };
        let protocol = infer_protocol_from_manifest_metadata("mongodb", "data", Some(&profile));
        assert_eq!(protocol, "mongodb");
    }

    #[test]
    fn test_infer_protocol_from_manifest_metadata_unknown_category_falls_back_to_tcp() {
        let protocol =
            infer_protocol_from_manifest_metadata("mystery", "category-that-does-not-exist", None);
        assert_eq!(protocol, "tcp");
    }

    #[test]
    fn test_default_template() {
        assert!(default_template("mongodb").contains("mongodb://"));
        assert!(default_template("postgresql").contains("postgresql://"));
        assert!(default_template("redis").contains("redis://"));
        assert!(default_template("unknown").contains("{protocol}://"));
    }
}
