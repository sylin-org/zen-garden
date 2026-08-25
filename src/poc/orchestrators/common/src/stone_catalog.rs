//! Centralized stone identity catalog for orchestrators.
//!
//! Every stone has many names: bare name (`stone-quartz-fen`), mDNS hostname
//! (`stone-quartz-fen.local`), IP (`192.168.1.5`), and service-specific
//! endpoints (`stone-quartz-fen.local:27017`, `http://192.168.1.5:11434`).
//!
//! `StoneCatalog` is the **single source of truth** that maps all these
//! variants to a canonical `StoneIdentity`.  Consumers resolve any endpoint
//! string through the catalog and get the stone's canonical name + typed
//! service endpoint map — no ad-hoc string matching required.

use std::collections::HashMap;

// ── ServiceKey ──────────────────────────────────────────────────

/// Well-known service endpoint keys.
///
/// Each variant identifies a specific service running on a stone.
/// The catalog stores the full endpoint string for each service.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum ServiceKey {
    /// Moss HTTP API (e.g. `http://stone-quartz-fen.local:7185`).
    Moss,
    /// MongoDB wire protocol (e.g. `stone-quartz-fen.local:27017`).
    Mongo,
    /// Ollama HTTP API (e.g. `http://stone-quartz-fen.local:11434`).
    Ollama,
}

// ── StoneIdentity ───────────────────────────────────────────────

/// Canonical identity of a stone.  All name variants resolve here.
#[derive(Debug, Clone)]
pub struct StoneIdentity {
    /// Primary key — the human-readable stone name (e.g. `stone-quartz-fen`).
    pub stone_name: String,
    /// GUIDv7 stone identifier (filled when available from topology/mDNS).
    pub stone_id: Option<String>,
    /// mDNS hostname (e.g. `stone-quartz-fen.local`).
    pub hostname: String,
    /// LAN IP address (e.g. `192.168.1.5`).  `None` if only hostname is known.
    pub ip: Option<String>,
    /// Service endpoint map — canonical endpoint string per service.
    pub services: HashMap<ServiceKey, String>,
}

// ── StoneCatalog ────────────────────────────────────────────────

/// Centralized stone identity registry.
///
/// Maintains a primary index by `stone_name` and a reverse index from every
/// known endpoint/hostname/IP variant back to the owning stone name.
#[derive(Debug, Clone, Default)]
pub struct StoneCatalog {
    /// Primary index: stone_name → StoneIdentity.
    by_name: HashMap<String, StoneIdentity>,
    /// Reverse index: any known endpoint string variant → stone_name.
    endpoint_to_name: HashMap<String, String>,
}

impl StoneCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update a stone.  Rebuilds its reverse-index entries.
    pub fn upsert(&mut self, identity: StoneIdentity) {
        let name = identity.stone_name.clone();

        // Remove stale reverse entries for this stone (if updating)
        self.remove_reverse_entries(&name);

        // Build fresh reverse entries
        for variant in Self::reverse_variants(&identity) {
            self.endpoint_to_name.insert(variant, name.clone());
        }

        self.by_name.insert(name, identity);
    }

    /// Resolve any endpoint string to its owning stone identity.
    ///
    /// Tries in order: exact match → scheme-stripped → host-only.
    pub fn resolve(&self, endpoint: &str) -> Option<&StoneIdentity> {
        // 1. Exact match (covers full endpoint strings and bare hostnames)
        if let Some(name) = self.endpoint_to_name.get(endpoint) {
            return self.by_name.get(name);
        }

        // 2. Strip scheme (http://, https://) and retry
        let stripped = strip_scheme(endpoint);
        if stripped != endpoint {
            if let Some(name) = self.endpoint_to_name.get(stripped) {
                return self.by_name.get(name);
            }
        }

        // 3. Extract host (strip port) and retry
        let host = strip_port(stripped);
        if host != stripped {
            if let Some(name) = self.endpoint_to_name.get(host) {
                return self.by_name.get(name);
            }
        }

        None
    }

    /// Resolve an endpoint to the owning stone_name.
    pub fn resolve_name(&self, endpoint: &str) -> Option<&str> {
        self.resolve(endpoint).map(|id| id.stone_name.as_str())
    }

    /// Get identity by stone_name (direct primary-index lookup).
    pub fn get(&self, stone_name: &str) -> Option<&StoneIdentity> {
        self.by_name.get(stone_name)
    }

    /// Get the canonical endpoint for a specific service on a stone.
    pub fn service_endpoint(&self, stone_name: &str, key: ServiceKey) -> Option<&str> {
        self.by_name
            .get(stone_name)
            .and_then(|id| id.services.get(&key))
            .map(|s| s.as_str())
    }

    /// Remove a stone and all its reverse-index entries.
    pub fn remove(&mut self, stone_name: &str) {
        self.remove_reverse_entries(stone_name);
        self.by_name.remove(stone_name);
    }

    /// All known stone names.
    pub fn stone_names(&self) -> Vec<&str> {
        self.by_name.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered stones.
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    // ── Internal helpers ────────────────────────────────────────

    /// Generate all reverse-index variants for a stone identity.
    fn reverse_variants(identity: &StoneIdentity) -> Vec<String> {
        let mut variants = Vec::new();

        // stone_name itself
        variants.push(identity.stone_name.clone());

        // hostname variants
        variants.push(identity.hostname.clone());

        // IP variants (if known)
        if let Some(ref ip) = identity.ip {
            variants.push(ip.clone());
        }

        // Service endpoint variants (full + scheme-stripped + host:port)
        for ep in identity.services.values() {
            variants.push(ep.clone());

            let stripped = strip_scheme(ep);
            if stripped != ep.as_str() {
                variants.push(stripped.to_string());
            }

            let host_port = strip_port(stripped);
            if host_port != stripped {
                // host:port is already covered, add host-only
                variants.push(host_port.to_string());
            }
        }

        variants
    }

    /// Remove all reverse-index entries that point to `stone_name`.
    fn remove_reverse_entries(&mut self, stone_name: &str) {
        self.endpoint_to_name
            .retain(|_, name| name != stone_name);
    }
}

// ── String normalization helpers ────────────────────────────────

/// Strip `http://` or `https://` scheme prefix.
fn strip_scheme(s: &str) -> &str {
    s.strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
        .unwrap_or(s)
}

/// Strip `:port` suffix, returning the host portion.
/// Handles IPv6 bracket notation (e.g. `[::1]:8080`).
fn strip_port(s: &str) -> &str {
    // IPv6 bracket notation: [host]:port
    if s.starts_with('[') {
        if let Some(bracket_end) = s.find(']') {
            // Return the part inside brackets (without brackets)
            return &s[1..bracket_end];
        }
    }

    // Regular host:port — find last colon
    match s.rfind(':') {
        Some(pos) => {
            // Only strip if what follows the colon looks like a port number
            let after = &s[pos + 1..];
            if after.chars().all(|c| c.is_ascii_digit()) && !after.is_empty() {
                &s[..pos]
            } else {
                s
            }
        }
        None => s,
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_identity(name: &str, hostname: &str, ip: Option<&str>) -> StoneIdentity {
        StoneIdentity {
            stone_name: name.to_string(),
            stone_id: Some("id-001".to_string()),
            hostname: hostname.to_string(),
            ip: ip.map(|s| s.to_string()),
            services: HashMap::new(),
        }
    }

    fn make_identity_with_services(
        name: &str,
        hostname: &str,
        ip: Option<&str>,
        services: Vec<(ServiceKey, &str)>,
    ) -> StoneIdentity {
        let mut id = make_identity(name, hostname, ip);
        for (key, ep) in services {
            id.services.insert(key, ep.to_string());
        }
        id
    }

    #[test]
    fn upsert_and_get() {
        let mut catalog = StoneCatalog::new();
        let id = make_identity("stone-a", "stone-a.local", Some("192.168.1.5"));
        catalog.upsert(id);

        assert_eq!(catalog.len(), 1);
        let got = catalog.get("stone-a").unwrap();
        assert_eq!(got.hostname, "stone-a.local");
        assert_eq!(got.ip.as_deref(), Some("192.168.1.5"));
    }

    #[test]
    fn resolve_by_name() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity("stone-a", "stone-a.local", Some("192.168.1.5")));

        assert_eq!(catalog.resolve_name("stone-a"), Some("stone-a"));
    }

    #[test]
    fn resolve_by_hostname() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity("stone-a", "stone-a.local", Some("192.168.1.5")));

        assert_eq!(catalog.resolve_name("stone-a.local"), Some("stone-a"));
    }

    #[test]
    fn resolve_by_ip() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity("stone-a", "stone-a.local", Some("192.168.1.5")));

        assert_eq!(catalog.resolve_name("192.168.1.5"), Some("stone-a"));
    }

    #[test]
    fn resolve_by_service_endpoint() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![
                (ServiceKey::Mongo, "stone-a.local:27017"),
                (ServiceKey::Moss, "http://stone-a.local:7185"),
            ],
        ));

        // Exact match
        assert_eq!(
            catalog.resolve_name("stone-a.local:27017"),
            Some("stone-a")
        );
        assert_eq!(
            catalog.resolve_name("http://stone-a.local:7185"),
            Some("stone-a")
        );
    }

    #[test]
    fn resolve_strips_scheme() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![(ServiceKey::Ollama, "http://stone-a.local:11434")],
        ));

        // Resolve with scheme that wasn't explicitly indexed
        assert_eq!(
            catalog.resolve_name("https://stone-a.local:11434"),
            Some("stone-a")
        );
    }

    #[test]
    fn resolve_strips_port() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![(ServiceKey::Mongo, "stone-a.local:27017")],
        ));

        // IP with different port resolves via IP entry in reverse index
        assert_eq!(catalog.resolve_name("192.168.1.5:27017"), Some("stone-a"));
    }

    #[test]
    fn resolve_ip_based_endpoint() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![(ServiceKey::Ollama, "http://stone-a.local:11434")],
        ));

        // IP-based endpoint that wasn't explicitly registered — resolves
        // via strip scheme → strip port → IP match
        assert_eq!(
            catalog.resolve_name("http://192.168.1.5:11434"),
            Some("stone-a")
        );
    }

    #[test]
    fn service_endpoint_lookup() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            None,
            vec![
                (ServiceKey::Mongo, "stone-a.local:27017"),
                (ServiceKey::Moss, "http://stone-a.local:7185"),
            ],
        ));

        assert_eq!(
            catalog.service_endpoint("stone-a", ServiceKey::Mongo),
            Some("stone-a.local:27017")
        );
        assert_eq!(
            catalog.service_endpoint("stone-a", ServiceKey::Moss),
            Some("http://stone-a.local:7185")
        );
        assert_eq!(
            catalog.service_endpoint("stone-a", ServiceKey::Ollama),
            None
        );
    }

    #[test]
    fn remove_cleans_reverse_index() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![(ServiceKey::Mongo, "stone-a.local:27017")],
        ));

        assert!(catalog.resolve("stone-a.local:27017").is_some());
        catalog.remove("stone-a");
        assert!(catalog.resolve("stone-a.local:27017").is_none());
        assert!(catalog.resolve("192.168.1.5").is_none());
        assert_eq!(catalog.len(), 0);
    }

    #[test]
    fn upsert_updates_reverse_index() {
        let mut catalog = StoneCatalog::new();

        // First upsert with IP .5
        catalog.upsert(make_identity("stone-a", "stone-a.local", Some("192.168.1.5")));
        assert_eq!(catalog.resolve_name("192.168.1.5"), Some("stone-a"));

        // Update with IP .6 — old IP should no longer resolve
        catalog.upsert(make_identity("stone-a", "stone-a.local", Some("192.168.1.6")));
        assert_eq!(catalog.resolve_name("192.168.1.6"), Some("stone-a"));
        assert!(catalog.resolve("192.168.1.5").is_none());
    }

    #[test]
    fn multiple_stones() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity_with_services(
            "stone-a",
            "stone-a.local",
            Some("192.168.1.5"),
            vec![(ServiceKey::Mongo, "stone-a.local:27017")],
        ));
        catalog.upsert(make_identity_with_services(
            "stone-b",
            "stone-b.local",
            Some("192.168.1.6"),
            vec![(ServiceKey::Mongo, "stone-b.local:27017")],
        ));

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.resolve_name("stone-a.local:27017"), Some("stone-a"));
        assert_eq!(catalog.resolve_name("192.168.1.6"), Some("stone-b"));
    }

    #[test]
    fn stone_names_returns_all() {
        let mut catalog = StoneCatalog::new();
        catalog.upsert(make_identity("stone-a", "stone-a.local", None));
        catalog.upsert(make_identity("stone-b", "stone-b.local", None));

        let mut names = catalog.stone_names();
        names.sort();
        assert_eq!(names, vec!["stone-a", "stone-b"]);
    }

    #[test]
    fn strip_scheme_helper() {
        assert_eq!(strip_scheme("http://host:80"), "host:80");
        assert_eq!(strip_scheme("https://host:443"), "host:443");
        assert_eq!(strip_scheme("host:80"), "host:80");
    }

    #[test]
    fn strip_port_helper() {
        assert_eq!(strip_port("host:8080"), "host");
        assert_eq!(strip_port("host"), "host");
        assert_eq!(strip_port("192.168.1.5:27017"), "192.168.1.5");
    }
}
