//! Service discovery domain logic
//!
//! Provides service discovery across the garden with connection string resolution.
//! Supports search by name, category, or tags.
//!
//! All discovery queries go through the unified tool registry (TOOLS-0003).
//! The registry holds all entries — Local, Gateway, and Announced — so a single
//! `snapshot(&query)` replaces the previous three-stage pipeline.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::domain::connection::ResolvedConnection;
use crate::domain::tool::registry::ToolQuery;
use garden_common::manifests::get_category_registry;
use garden_common::tools::GardenTool;

/// Search criteria for service discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSearchCriteria {
    /// Search by exact service name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Search by category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Search by tag (any match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,

    /// Required sub-capabilities (all must match)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<SubCapabilityFilter>,
}

/// Filter for sub-capability search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubCapabilityFilter {
    /// Capability type (e.g., "model", "collection")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap_type: Option<String>,
    /// Capability item to match (e.g., "llama2")
    pub item: String,
}

impl ServiceSearchCriteria {
    pub fn by_name(name: &str) -> Self {
        Self {
            name: Some(name.to_string()),
            category: None,
            tag: None,
            required_capabilities: Vec::new(),
        }
    }

    pub fn by_category(category: &str) -> Self {
        Self {
            name: None,
            category: Some(category.to_string()),
            tag: None,
            required_capabilities: Vec::new(),
        }
    }

    pub fn by_tag(tag: &str) -> Self {
        Self {
            name: None,
            category: None,
            tag: Some(tag.to_string()),
            required_capabilities: Vec::new(),
        }
    }

    pub fn by_sub_capability(cap_type: Option<&str>, item: &str) -> Self {
        Self {
            name: None,
            category: None,
            tag: None,
            required_capabilities: vec![SubCapabilityFilter {
                cap_type: cap_type.map(String::from),
                item: item.to_string(),
            }],
        }
    }

    /// Create a name search with sub-capability filter
    /// E.g., ollama[llama2,mistral]
    pub fn by_name_with_sub_capabilities(
        name: &str,
        required_capabilities: Vec<SubCapabilityFilter>,
    ) -> Self {
        Self {
            name: Some(name.to_string()),
            category: None,
            tag: None,
            required_capabilities,
        }
    }

    /// Parse search query with prefix detection
    ///
    /// Supports:
    /// - `mongodb` - name search (or implicit category if known)
    /// - `c:database`, `cat:database`, `category:database` - category search
    /// - `t:nosql`, `tag:nosql`, `tags:nosql` - tag search
    /// - `model:llama2` - sub-capability search (type:item)
    /// - `ollama[llama2,mistral]` - name with required capabilities (AND semantics)
    pub fn parse(query: &str) -> Self {
        let query = query.trim();

        // Check for sub-capability syntax: name[item]
        // E.g., "ollama[llama2,mistral]"
        if let Some((name_part, rest)) = query.split_once('[')
            && let Some(item) = rest.strip_suffix(']')
        {
            let required_capabilities = parse_capability_requirements(item);
            if !required_capabilities.is_empty() {
                return Self::by_name_with_sub_capabilities(
                    name_part.trim(),
                    required_capabilities,
                );
            }
        }

        // Check for category prefix
        if let Some(cat) = query
            .strip_prefix("c:")
            .or_else(|| query.strip_prefix("cat:"))
            .or_else(|| query.strip_prefix("category:"))
        {
            return Self::by_category(cat);
        }

        // Check for tag prefix
        if let Some(tag) = query
            .strip_prefix("t:")
            .or_else(|| query.strip_prefix("tag:"))
            .or_else(|| query.strip_prefix("tags:"))
        {
            return Self::by_tag(tag);
        }

        // Check for sub-capability prefix: model:item, cap:item
        // E.g., "model:llama2,llama3" or "collection:embeddings"
        if let Some(item) = query.strip_prefix("model:") {
            return Self {
                name: None,
                category: None,
                tag: None,
                required_capabilities: parse_capability_items(Some("model"), item),
            };
        }
        if let Some(item) = query.strip_prefix("collection:") {
            return Self {
                name: None,
                category: None,
                tag: None,
                required_capabilities: parse_capability_items(Some("collection"), item),
            };
        }
        if let Some(item) = query.strip_prefix("cap:") {
            // Generic capability search (any type). Multiple values are supported.
            return Self {
                name: None,
                category: None,
                tag: None,
                required_capabilities: parse_capability_items(None, item),
            };
        }

        // Check if it's a known category (implicit category search)
        // Uses data-driven category registry instead of hardcoded list
        let lower = query.to_lowercase();
        if get_category_registry().resolve_token(&lower).is_some() {
            return Self::by_category(&lower);
        }

        // Default to name search
        Self::by_name(query)
    }

    /// Check if this is a name-based search (exact match required)
    pub fn is_name_search(&self) -> bool {
        self.name.is_some()
    }

    /// Check if this search includes sub-capability filter
    pub fn has_sub_capability_filter(&self) -> bool {
        !self.required_capabilities.is_empty()
    }
}

fn parse_capability_items(cap_type: Option<&str>, items: &str) -> Vec<SubCapabilityFilter> {
    items
        .split([',', '|'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| SubCapabilityFilter {
            cap_type: cap_type.map(String::from),
            item: item.to_string(),
        })
        .collect()
}

fn parse_capability_requirements(input: &str) -> Vec<SubCapabilityFilter> {
    input
        .split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| SubCapabilityFilter {
            cap_type: None,
            item: token.to_string(),
        })
        .collect()
}

// Re-export response types from garden-common (canonical definitions)
pub use garden_common::discovery::{FoundService, ServiceDiscoveryResponse, StoneRef};

// ── Registry-backed discovery ────────────────────────────────────────────────

/// List all local services for the `/api/v1/stone/services` endpoint.
///
/// Returns all offerings on this stone from the tool registry.
/// Includes both running and non-running entries.
pub async fn list_all_local_services(state: &AppState) -> ServiceDiscoveryResponse {
    let query = ToolQuery {
        stone_id: Some(state.current.stone.id.clone()),
        ..Default::default()
    };

    let (_, tools) = state.tool.snapshot(&query).await;

    let services: Vec<FoundService> = tools
        .into_iter()
        .filter(|t| t.tool.category != garden_common::constants::CATEGORY_STORAGE)
        .map(garden_tool_to_found_service)
        .collect();

    ServiceDiscoveryResponse {
        found: !services.is_empty(),
        services,
        source: "local".to_string(),
        cache_age_seconds: None,
        timestamp: Utc::now(),
    }
}

/// Find services across the garden (all origins: Local, Gateway, Announced).
///
/// Single registry query replaces the previous three-stage pipeline
/// (gateway check + local offerings + topology cache).
pub async fn find_services(
    criteria: &ServiceSearchCriteria,
    state: &AppState,
) -> ServiceDiscoveryResponse {
    let start = std::time::Instant::now();

    // Build ToolQuery from criteria (coarse filter the registry can evaluate)
    let query = criteria_to_tool_query(criteria);

    // Single registry query — all origins (Local, Gateway, Announced)
    let (_, tools) = state.tool.snapshot(&query).await;

    // Convert GardenTool -> FoundService, applying fine-grained filters
    let mut all_services: Vec<FoundService> = tools
        .into_iter()
        // Exclude seed-banks — they have their own API path
        .filter(|t| t.tool.category != garden_common::constants::CATEGORY_STORAGE)
        // Fine-grained criteria that ToolQuery cannot express (tags, sub-capabilities,
        // partial name/offering match)
        .filter(|t| matches_search_criteria(criteria, t))
        .map(garden_tool_to_found_service)
        .collect();

    // Sort per discovery ordering contract
    sort_found_services(&mut all_services, criteria);

    let elapsed = start.elapsed();
    tracing::debug!(
        criteria = ?criteria,
        found = all_services.len(),
        duration_ms = elapsed.as_millis(),
        "Service discovery completed"
    );

    // Compute cache age from the most recent topology entry's last_seen
    let cache_age_seconds = {
        let map = state.current.topology.cache.read().await;
        map.values().map(|e| e.last_seen).max().map(|newest| {
            let age = Utc::now().signed_duration_since(newest);
            age.num_seconds().max(0) as u64
        })
    };

    ServiceDiscoveryResponse {
        found: !all_services.is_empty(),
        services: all_services,
        source: "registry".to_string(),
        cache_age_seconds,
        timestamp: Utc::now(),
    }
}

// ── Conversion helpers ───────────────────────────────────────────────────────

/// Convert `ServiceSearchCriteria` to `ToolQuery` for coarse registry filtering.
///
/// Only maps fields that `ToolQuery` can natively filter (fqid, category, status).
/// Tag matching and sub-capability filtering happen post-query in
/// `matches_search_criteria`.
fn criteria_to_tool_query(criteria: &ServiceSearchCriteria) -> ToolQuery {
    ToolQuery {
        // For name searches, use fqid filter (bare name = type match in fqid_matches)
        fqid: criteria.name.clone(),
        // For category searches, use category filter
        category: criteria.category.clone(),
        // Only return running services for discovery
        status: Some(garden_common::constants::SERVICE_RUNNING.to_string()),
        ..Default::default()
    }
}

/// Fine-grained filtering that `ToolQuery` cannot express.
///
/// `ToolQuery` already handles fqid (name/offering-type) and category matching.
/// This function checks the remaining criteria dimensions:
/// - Tag matching (tags live on the tool, not in ToolQuery)
/// - Sub-capability matching (AND semantics across capability types)
fn matches_search_criteria(criteria: &ServiceSearchCriteria, tool: &GardenTool) -> bool {
    // Tag match (any tag matches)
    if let Some(ref search_tag) = criteria.tag {
        let lower_search = search_tag.to_lowercase();
        let has_tag = tool
            .tool
            .tags
            .iter()
            .any(|t| t.to_lowercase() == lower_search);
        // Orchestrators implicitly carry their category as a tag
        let implicit_tag = tool.tool.category.to_lowercase() == lower_search;
        if !has_tag && !implicit_tag {
            return false;
        }
    }

    // Required sub-capabilities (AND semantics)
    for filter in &criteria.required_capabilities {
        let lower_item = filter.item.to_lowercase();

        let has_cap = tool.capabilities.iter().any(|cap| {
            if let Some(ref cap_type) = filter.cap_type
                && cap.cap_type.to_lowercase() != cap_type.to_lowercase()
            {
                return false;
            }
            cap.items.iter().any(|i| i.to_lowercase() == lower_item)
        });

        if !has_cap {
            return false;
        }
    }

    true
}

/// Convert a `GardenTool` to `FoundService` for API compatibility.
fn garden_tool_to_found_service(tool: GardenTool) -> FoundService {
    let svc = &tool.service;
    let conn = ResolvedConnection {
        hostname: svc
            .hostname
            .clone()
            .unwrap_or_else(|| tool.stone.name.clone()),
        ip: svc.ip.clone().unwrap_or_default(),
        port: svc.port.unwrap_or(0),
        protocol: svc.protocol.clone(),
        uris: svc.uris.clone(),
    };

    // Convert Capability -> SubCapability (same structure, different type names)
    let sub_capabilities: Vec<garden_common::SubCapability> = tool
        .capabilities
        .iter()
        .map(|cap| garden_common::SubCapability {
            cap_type: cap.cap_type.clone(),
            items: cap.items.clone(),
            discovered_at: None,
        })
        .collect();

    FoundService {
        offering_id: tool.tool.id.clone(),
        name: tool.fqid.clone(),
        offering: tool.tool.tool_type.clone(),
        category: tool.tool.category.clone(),
        tags: tool.tool.tags.clone(),
        status: tool.service.status.clone(),
        stone: StoneRef {
            id: tool.stone.id,
            name: tool.stone.name,
            endpoint: tool.stone.endpoint,
        },
        connection: conn,
        sub_capabilities,
        source: tool.tool.source.clone(),
    }
}

/// Get default port from offering manifest.
///
/// Looks up the offering and returns the default port.
/// Returns 8080 as fallback if not found.
pub async fn get_offering_port(offering: &str, state: &AppState) -> u16 {
    if let Some(offering_def) = state.manifest_registry.get_offering(offering) {
        let port = offering_def.default_host_port();
        if port != 8080 {
            // 8080 is the generic default
            return port;
        }
    }

    tracing::warn!(
        offering = %offering,
        "Offering not found or has no port mappings, using default 8080"
    );
    8080
}

/// Sort found services according to the discovery ordering contract.
///
/// For name-based searches:
///   1. Exact name matches first (service `name` == query).
///   2. Partial matches second (only `offering` matches the query).
///   3. Within each tier, orchestrators (category == "orchestrator") first.
///   4. Stable tie-break: service name, then stone name.
///
/// For non-name searches (category, tag) the registry's built-in sort
/// (category priority -> fqid -> stone name) already applies, so we only
/// apply the stable tie-break.
fn sort_found_services(services: &mut [FoundService], criteria: &ServiceSearchCriteria) {
    if let Some(ref search_name) = criteria.name {
        let lower_search = search_name.to_lowercase();

        services.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == lower_search;
            let b_exact = b.name.to_lowercase() == lower_search;

            // Primary: exact name match first
            match (a_exact, b_exact) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }

            let a_orch = a.category == garden_common::constants::CATEGORY_ORCHESTRATOR;
            let b_orch = b.category == garden_common::constants::CATEGORY_ORCHESTRATOR;

            // Secondary: orchestrators before data services
            match (a_orch, b_orch) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }

            // Tertiary: alphabetical by name, then stone
            a.name.cmp(&b.name).then(a.stone.name.cmp(&b.stone.name))
        });
    }
    // Non-name searches: orchestrators are already first via registry sort;
    // no additional reordering needed.
}

/// Check if a service matches the search criteria (used by tests).
#[cfg(test)]
fn matches_criteria(
    criteria: &ServiceSearchCriteria,
    name: &str,
    offering: &str,
    category: &str,
    tags: &[String],
    sub_capabilities: &[garden_common::SubCapability],
) -> bool {
    // Name match (exact or offering match)
    if let Some(ref search_name) = criteria.name {
        let lower_search = search_name.to_lowercase();
        let lower_name = name.to_lowercase();
        let lower_offering = offering.to_lowercase();

        if lower_name != lower_search && lower_offering != lower_search {
            return false;
        }
    }

    // Category match
    if let Some(ref search_cat) = criteria.category {
        if category.to_lowercase() != search_cat.to_lowercase() {
            return false;
        }
    }

    // Tag match (any tag matches)
    if let Some(ref search_tag) = criteria.tag {
        let lower_search = search_tag.to_lowercase();
        let has_tag = tags.iter().any(|t| t.to_lowercase() == lower_search);
        if !has_tag {
            return false;
        }
    }

    // Required sub-capabilities (AND semantics)
    for filter in &criteria.required_capabilities {
        let lower_item = filter.item.to_lowercase();

        let has_cap = sub_capabilities.iter().any(|cap| {
            if let Some(ref cap_type) = filter.cap_type {
                if cap.cap_type.to_lowercase() != cap_type.to_lowercase() {
                    return false;
                }
            }
            cap.items.iter().any(|i| i.to_lowercase() == lower_item)
        });

        if !has_cap {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_name_search() {
        let criteria = ServiceSearchCriteria::parse("mongodb");
        assert_eq!(criteria.name, Some("mongodb".to_string()));
        assert!(criteria.category.is_none());
        assert!(criteria.tag.is_none());
    }

    #[test]
    fn test_parse_category_prefix() {
        let criteria = ServiceSearchCriteria::parse("c:database");
        assert!(criteria.name.is_none());
        assert_eq!(criteria.category, Some("database".to_string()));

        let criteria = ServiceSearchCriteria::parse("cat:cache");
        assert_eq!(criteria.category, Some("cache".to_string()));

        let criteria = ServiceSearchCriteria::parse("category:search");
        assert_eq!(criteria.category, Some("search".to_string()));
    }

    #[test]
    fn test_parse_tag_prefix() {
        let criteria = ServiceSearchCriteria::parse("t:nosql");
        assert!(criteria.name.is_none());
        assert_eq!(criteria.tag, Some("nosql".to_string()));

        let criteria = ServiceSearchCriteria::parse("tag:document");
        assert_eq!(criteria.tag, Some("document".to_string()));

        let criteria = ServiceSearchCriteria::parse("tags:realtime");
        assert_eq!(criteria.tag, Some("realtime".to_string()));
    }

    #[test]
    fn test_parse_implicit_category() {
        // Skip test if category registry is empty (not loaded from manifests)
        // This can happen in test environments without the manifests directory
        if get_category_registry().category_names().is_empty() {
            eprintln!("Skipping test_parse_implicit_category: no category registry loaded");
            // Just verify that unknown words default to name search
            let criteria = ServiceSearchCriteria::parse("myservice");
            assert_eq!(criteria.name, Some("myservice".to_string()));
            return;
        }

        // Known categories should be detected (only if registry is loaded)
        let criteria = ServiceSearchCriteria::parse("database");
        assert_eq!(criteria.category, Some("database".to_string()));

        let criteria = ServiceSearchCriteria::parse("cache");
        assert_eq!(criteria.category, Some("cache".to_string()));

        // Unknown words default to name search
        let criteria = ServiceSearchCriteria::parse("myservice");
        assert_eq!(criteria.name, Some("myservice".to_string()));
    }

    #[test]
    fn test_matches_criteria_name() {
        let criteria = ServiceSearchCriteria::by_name("mongodb");

        assert!(matches_criteria(
            &criteria,
            "mongodb",
            "mongodb",
            "database",
            &[],
            &[]
        ));
        assert!(matches_criteria(
            &criteria,
            "mongodb:dev",
            "mongodb",
            "database",
            &[],
            &[]
        ));
        assert!(!matches_criteria(
            &criteria,
            "redis",
            "redis",
            "cache",
            &[],
            &[]
        ));
    }

    #[test]
    fn test_matches_criteria_category() {
        let criteria = ServiceSearchCriteria::by_category("database");

        assert!(matches_criteria(
            &criteria,
            "mongodb",
            "mongodb",
            "database",
            &[],
            &[]
        ));
        assert!(matches_criteria(
            &criteria,
            "postgres",
            "postgres",
            "database",
            &[],
            &[]
        ));
        assert!(!matches_criteria(
            &criteria,
            "redis",
            "redis",
            "cache",
            &[],
            &[]
        ));
    }

    #[test]
    fn test_matches_criteria_tag() {
        let criteria = ServiceSearchCriteria::by_tag("nosql");

        assert!(matches_criteria(
            &criteria,
            "mongodb",
            "mongodb",
            "database",
            &["document".to_string(), "nosql".to_string()],
            &[]
        ));
        assert!(!matches_criteria(
            &criteria,
            "postgres",
            "postgres",
            "database",
            &["sql".to_string(), "relational".to_string()],
            &[]
        ));
    }

    #[test]
    fn test_matches_criteria_sub_capability() {
        use garden_common::SubCapability;

        let caps = vec![SubCapability::new(
            "model",
            vec!["llama2".to_string(), "mistral".to_string()],
        )];

        // Search for model:llama2
        let criteria = ServiceSearchCriteria::by_sub_capability(Some("model"), "llama2");
        assert!(matches_criteria(
            &criteria,
            "ollama",
            "ollama",
            "ai",
            &[],
            &caps
        ));
        assert!(!matches_criteria(
            &criteria,
            "ollama",
            "ollama",
            "ai",
            &[],
            &[]
        ));

        // Search for ollama[mistral]
        let criteria = ServiceSearchCriteria::by_name_with_sub_capabilities(
            "ollama",
            vec![SubCapabilityFilter {
                cap_type: None,
                item: "mistral".to_string(),
            }],
        );
        assert!(matches_criteria(
            &criteria,
            "ollama",
            "ollama",
            "ai",
            &[],
            &caps
        ));
        assert!(!matches_criteria(
            &criteria,
            "redis",
            "redis",
            "cache",
            &[],
            &caps
        ));

        // Generic cap: search (any type)
        let criteria = ServiceSearchCriteria::by_sub_capability(None, "llama2");
        assert!(matches_criteria(
            &criteria,
            "ollama",
            "ollama",
            "ai",
            &[],
            &caps
        ));

        // Multi-capability (AND semantics): requires both llama2 and mistral
        let criteria = ServiceSearchCriteria::by_name_with_sub_capabilities(
            "ollama",
            vec![
                SubCapabilityFilter {
                    cap_type: None,
                    item: "llama2".to_string(),
                },
                SubCapabilityFilter {
                    cap_type: None,
                    item: "mistral".to_string(),
                },
            ],
        );
        assert!(matches_criteria(
            &criteria,
            "ollama",
            "ollama",
            "ai",
            &[],
            &caps
        ));
    }

    #[test]
    fn test_parse_sub_capability_syntax() {
        // Test ollama[llama2] syntax
        let criteria = ServiceSearchCriteria::parse("ollama[llama2]");
        assert_eq!(criteria.name, Some("ollama".to_string()));
        assert_eq!(criteria.required_capabilities.len(), 1);
        assert_eq!(criteria.required_capabilities[0].item, "llama2");
        assert!(criteria.required_capabilities[0].cap_type.is_none());

        // Test multi-capability syntax
        let criteria = ServiceSearchCriteria::parse("ollama[llama2,mistral]");
        assert_eq!(criteria.required_capabilities.len(), 2);
        assert_eq!(criteria.required_capabilities[0].item, "llama2");
        assert_eq!(criteria.required_capabilities[1].item, "mistral");

        // Test model:llama2 syntax
        let criteria = ServiceSearchCriteria::parse("model:llama2");
        assert_eq!(criteria.required_capabilities.len(), 1);
        assert_eq!(
            criteria.required_capabilities[0].cap_type,
            Some("model".to_string())
        );
        assert_eq!(criteria.required_capabilities[0].item, "llama2");

        // Test model with multi-values
        let criteria = ServiceSearchCriteria::parse("model:llama2,mistral");
        assert_eq!(criteria.required_capabilities.len(), 2);
        assert_eq!(
            criteria.required_capabilities[0].cap_type,
            Some("model".to_string())
        );
        assert_eq!(
            criteria.required_capabilities[1].cap_type,
            Some("model".to_string())
        );

        // Test collection:embeddings syntax
        let criteria = ServiceSearchCriteria::parse("collection:embeddings");
        assert_eq!(criteria.required_capabilities.len(), 1);
        assert_eq!(
            criteria.required_capabilities[0].cap_type,
            Some("collection".to_string())
        );
    }

    #[test]
    fn test_is_name_search() {
        assert!(ServiceSearchCriteria::by_name("mongodb").is_name_search());
        assert!(!ServiceSearchCriteria::by_category("database").is_name_search());
        assert!(!ServiceSearchCriteria::by_tag("nosql").is_name_search());
    }

    #[test]
    fn test_has_sub_capability_filter() {
        assert!(
            ServiceSearchCriteria::by_sub_capability(Some("model"), "llama2")
                .has_sub_capability_filter()
        );
        assert!(
            ServiceSearchCriteria::by_name_with_sub_capabilities(
                "ollama",
                vec![SubCapabilityFilter {
                    cap_type: None,
                    item: "llama2".to_string(),
                }],
            )
            .has_sub_capability_filter()
        );
        assert!(!ServiceSearchCriteria::by_name("mongodb").has_sub_capability_filter());
    }

    /// Helper to build a minimal FoundService for sort tests.
    fn stub_service(name: &str, offering: &str, category: &str, stone: &str) -> FoundService {
        FoundService {
            offering_id: String::new(),
            name: name.to_string(),
            offering: offering.to_string(),
            category: category.to_string(),
            tags: vec![],
            status: "running".to_string(),
            stone: StoneRef {
                id: stone.to_string(),
                name: stone.to_string(),
                endpoint: format!("http://{}", stone),
            },
            connection: ResolvedConnection {
                hostname: stone.to_string(),
                ip: "127.0.0.1".to_string(),
                port: 27017,
                protocol: "mongodb".to_string(),
                uris: vec![],
            },
            sub_capabilities: vec![],
            source: String::new(),
        }
    }

    #[test]
    fn test_sort_exact_name_matches_first() {
        let criteria = ServiceSearchCriteria::by_name("mongodb");

        // Simulate the real scenario: orchestrators collected first, then data.
        let mut services = vec![
            stub_service("mongodb::legacy", "mongodb", "orchestrator", "stone-a"),
            stub_service("mongodb", "mongodb", "orchestrator", "stone-b"),
            stub_service("mongodb", "mongodb", "data", "stone-c"),
            stub_service("mongodb", "mongodb", "data", "stone-d"),
            stub_service("mongodb::legacy", "mongodb", "data", "stone-a"),
        ];

        sort_found_services(&mut services, &criteria);

        let names: Vec<(&str, &str)> = services
            .iter()
            .map(|s| (s.name.as_str(), s.category.as_str()))
            .collect();

        // Exact matches first (mongodb), then partials (mongodb::legacy).
        // Within each tier, orchestrators before data.
        assert_eq!(
            names,
            vec![
                ("mongodb", "orchestrator"),
                ("mongodb", "data"),
                ("mongodb", "data"),
                ("mongodb::legacy", "orchestrator"),
                ("mongodb::legacy", "data"),
            ]
        );
    }

    #[test]
    fn test_sort_no_reorder_for_category_search() {
        let criteria = ServiceSearchCriteria::by_category("database");

        let mut services = vec![
            stub_service("mongodb::legacy", "mongodb", "orchestrator", "stone-a"),
            stub_service("mongodb", "mongodb", "data", "stone-b"),
        ];

        let original_order: Vec<String> = services.iter().map(|s| s.name.clone()).collect();

        sort_found_services(&mut services, &criteria);

        let after: Vec<String> = services.iter().map(|s| s.name.clone()).collect();
        assert_eq!(original_order, after, "category search should not reorder");
    }

    #[test]
    fn test_criteria_to_tool_query_name() {
        let criteria = ServiceSearchCriteria::by_name("mongodb");
        let query = criteria_to_tool_query(&criteria);
        assert_eq!(query.fqid, Some("mongodb".to_string()));
        assert!(query.category.is_none());
        assert_eq!(query.status, Some("running".to_string()));
        assert!(query.stone_id.is_none());
    }

    #[test]
    fn test_criteria_to_tool_query_category() {
        let criteria = ServiceSearchCriteria::by_category("database");
        let query = criteria_to_tool_query(&criteria);
        assert!(query.fqid.is_none());
        assert_eq!(query.category, Some("database".to_string()));
        assert_eq!(query.status, Some("running".to_string()));
    }

    #[test]
    fn test_matches_search_criteria_tag() {
        use garden_common::tools::{GardenTool, ServiceInfo, Stone, ToolIdentity};

        let tool = GardenTool {
            fqid: "mongodb".to_string(),
            tool: ToolIdentity {
                name: String::new(),
                tool_type: "mongodb".to_string(),
                category: "offering".to_string(),
                id: "test-id".to_string(),
                tags: vec!["nosql".to_string(), "document".to_string()],
                source: String::new(),
            },
            stone: Stone {
                id: "stone-a".to_string(),
                name: "stone-a".to_string(),
                endpoint: "http://stone-a:7185".to_string(),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: "mongodb".to_string(),
                uris: Vec::new(),
                hostname: None,
                ip: None,
                port: Some(27017),
                uri_template: None,
            },
            capabilities: Vec::new(),
            storage: None,
        };

        let criteria = ServiceSearchCriteria::by_tag("nosql");
        assert!(matches_search_criteria(&criteria, &tool));

        let criteria = ServiceSearchCriteria::by_tag("relational");
        assert!(!matches_search_criteria(&criteria, &tool));
    }

    #[test]
    fn test_matches_search_criteria_sub_capability() {
        use garden_common::tools::{Capability, GardenTool, ServiceInfo, Stone, ToolIdentity};

        let tool = GardenTool {
            fqid: "ollama".to_string(),
            tool: ToolIdentity {
                name: String::new(),
                tool_type: "ollama".to_string(),
                category: "offering".to_string(),
                id: "test-id".to_string(),
                tags: Vec::new(),
                source: String::new(),
            },
            stone: Stone {
                id: "stone-a".to_string(),
                name: "stone-a".to_string(),
                endpoint: "http://stone-a:7185".to_string(),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: "ollama".to_string(),
                uris: Vec::new(),
                hostname: None,
                ip: None,
                port: Some(11434),
                uri_template: None,
            },
            capabilities: vec![Capability {
                cap_type: "model".to_string(),
                items: vec!["llama2".to_string(), "mistral".to_string()],
            }],
            storage: None,
        };

        let criteria = ServiceSearchCriteria::by_sub_capability(Some("model"), "llama2");
        assert!(matches_search_criteria(&criteria, &tool));

        let criteria = ServiceSearchCriteria::by_sub_capability(Some("model"), "gpt-4");
        assert!(!matches_search_criteria(&criteria, &tool));
    }

    #[test]
    fn test_garden_tool_to_found_service() {
        use garden_common::tools::{Capability, GardenTool, ServiceInfo, Stone, ToolIdentity};

        let tool = GardenTool {
            fqid: "mongodb::prod".to_string(),
            tool: ToolIdentity {
                name: "prod".to_string(),
                tool_type: "mongodb".to_string(),
                category: "offering".to_string(),
                id: "uid-123".to_string(),
                tags: vec!["database".to_string()],
                source: String::new(),
            },
            stone: Stone {
                id: "stone-a".to_string(),
                name: "stone-a".to_string(),
                endpoint: "http://stone-a:7185".to_string(),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: "mongodb".to_string(),
                uris: vec!["mongodb://stone-a:27017".to_string()],
                hostname: Some("stone-a.local".to_string()),
                ip: Some("192.168.1.10".to_string()),
                port: Some(27017),
                uri_template: None,
            },
            capabilities: vec![Capability {
                cap_type: "collection".to_string(),
                items: vec!["users".to_string()],
            }],
            storage: None,
        };

        let found = garden_tool_to_found_service(tool);

        assert_eq!(found.name, "mongodb::prod");
        assert_eq!(found.offering, "mongodb");
        assert_eq!(found.category, "offering");
        assert_eq!(found.offering_id, "uid-123");
        assert_eq!(found.tags, vec!["database".to_string()]);
        assert_eq!(found.status, "running");
        assert_eq!(found.stone.id, "stone-a");
        assert_eq!(found.stone.name, "stone-a");
        assert_eq!(found.connection.hostname, "stone-a.local");
        assert_eq!(found.connection.ip, "192.168.1.10");
        assert_eq!(found.connection.port, 27017);
        assert_eq!(found.connection.protocol, "mongodb");
        assert_eq!(found.connection.uris, vec!["mongodb://stone-a:27017"]);
        assert_eq!(found.sub_capabilities.len(), 1);
        assert_eq!(found.sub_capabilities[0].cap_type, "collection");
        assert_eq!(found.sub_capabilities[0].items, vec!["users"]);
    }
}
