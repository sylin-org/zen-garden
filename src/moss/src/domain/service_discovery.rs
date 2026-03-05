//! Service discovery domain logic
//!
//! Provides service discovery across the garden with connection string resolution.
//! Supports search by name, category, or tags with cache-first architecture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::domain::connection::{self, ResolvedConnection};
use crate::domain::{topology, TopologyEntry};
use crate::AppState;
use garden_common::manifests::get_category_registry;
use garden_common::{OfferingStatus, ServiceStatus};

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
        if let Some((name_part, rest)) = query.split_once('[') {
            if let Some(item) = rest.strip_suffix(']') {
                let required_capabilities = parse_capability_requirements(item);
                if !required_capabilities.is_empty() {
                    return Self::by_name_with_sub_capabilities(
                        name_part.trim(),
                        required_capabilities,
                    );
                }
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

/// Found service with connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundService {
    /// Unique identifier for this offering instance (GUIDv7)
    /// Survives renames, migrations, used for backup keying.
    #[serde(default)]
    pub offering_id: String,

    /// Service name
    pub name: String,

    /// Offering type (e.g., "mongodb", "redis")
    pub offering: String,

    /// Service category
    pub category: String,

    /// Service tags
    pub tags: Vec<String>,

    /// Current status
    pub status: String,

    /// Stone hosting this service
    pub stone: StoneRef,

    /// Resolved connection information
    pub connection: ResolvedConnection,

    /// Sub-capabilities (e.g., models, collections)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sub_capabilities: Vec<garden_common::SubCapability>,
}

/// Reference to a stone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneRef {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// Service discovery response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryResponse {
    /// Whether services were found
    pub found: bool,

    /// Found services
    pub services: Vec<FoundService>,

    /// Data source ("cache" or "fresh")
    pub source: String,

    /// Cache age in seconds (if from cache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_seconds: Option<u64>,

    /// Response timestamp
    pub timestamp: DateTime<Utc>,
}

/// Find services matching criteria on local stone
///
/// Zero-latency local search using offerings and offerings index.
pub async fn find_local_services(
    criteria: &ServiceSearchCriteria,
    state: &AppState,
) -> Vec<FoundService> {
    let self_endpoint = state.self_entry.read().await.address.http_base();
    let offerings = state.offerings.read().await;
    let offerings_index = state.offerings_index.read().await;

    let mut results = Vec::new();

    for offering in offerings.iter() {
        // Skip non-running offerings
        if offering.status != OfferingStatus::Running {
            continue;
        }

        // Get offering metadata (category, tags)
        let (category, tags) = offerings_index
            .as_ref()
            .and_then(|idx| idx.offerings.iter().find(|o| o.name == offering.offering))
            .map(|o| (o.category.clone(), o.tags.clone()))
            .unwrap_or_else(|| (offering.offering.clone(), vec![]));

        // Check if matches criteria
        if !matches_criteria(
            criteria,
            &offering.name.to_string(),
            &offering.offering,
            &category,
            &tags,
            &offering.sub_capabilities,
        ) {
            continue;
        }

        // Resolve connection
        let protocol = connection::infer_protocol(&offering.offering, &category, state).await;
        let port = offering.location.port;

        let connection_profile = state
            .manifest_registry
            .get_offering(&offering.offering)
            .and_then(|entry| entry.connection.as_ref());
        let uri_template = connection::select_uri_template(connection_profile, &category);

        let conn = connection::resolve_connection(
            &state.stone_name,
            &self_endpoint,
            port,
            &protocol,
            uri_template.as_deref(),
        );

        results.push(FoundService {
            offering_id: offering.offering_id.clone(),
            name: offering.name.to_string(),
            offering: offering.offering.clone(),
            category,
            tags,
            status: format!("{}", offering.status),
            stone: StoneRef {
                id: state.stone_id.clone(),
                name: state.stone_name.clone(),
                endpoint: self_endpoint.clone(),
            },
            connection: conn,
            sub_capabilities: offering.sub_capabilities.clone(),
        });
    }

    results
}

/// List all local services (regardless of criteria) for the unified /api/v1/services endpoint
///
/// Returns all offerings from unified registry with full connection info.
/// Includes both running and non-running offerings.
pub async fn list_all_local_services(state: &AppState) -> ServiceDiscoveryResponse {
    let self_endpoint = state.self_entry.read().await.address.http_base();
    let offerings = state.offerings.read().await;
    let offerings_index = state.offerings_index.read().await;

    let mut services = Vec::new();

    for offering in offerings.iter() {
        // Get offering metadata (category, tags)
        let (category, tags) = offerings_index
            .as_ref()
            .and_then(|idx| idx.offerings.iter().find(|o| o.name == offering.offering))
            .map(|o| (o.category.clone(), o.tags.clone()))
            .unwrap_or_else(|| (offering.offering.clone(), vec![]));

        // Resolve connection
        let protocol = connection::infer_protocol(&offering.offering, &category, state).await;
        let port = offering.location.port;

        let connection_profile = state
            .manifest_registry
            .get_offering(&offering.offering)
            .and_then(|entry| entry.connection.as_ref());
        let uri_template = connection::select_uri_template(connection_profile, &category);

        let conn = connection::resolve_connection(
            &state.stone_name,
            &self_endpoint,
            port,
            &protocol,
            uri_template.as_deref(),
        );

        services.push(FoundService {
            offering_id: offering.offering_id.clone(),
            name: offering.name.to_string(),
            offering: offering.offering.clone(),
            category,
            tags,
            status: format!("{}", offering.status),
            stone: StoneRef {
                id: state.stone_id.clone(),
                name: state.stone_name.clone(),
                endpoint: self_endpoint.clone(),
            },
            connection: conn,
            sub_capabilities: offering.sub_capabilities.clone(),
        });
    }

    ServiceDiscoveryResponse {
        found: !services.is_empty(),
        services,
        source: "local".to_string(),
        cache_age_seconds: None,
        timestamp: Utc::now(),
    }
}

/// Find services across the garden (local + remote stones)
///
/// Always checks both local registry and topology cache.
/// The `fresh` parameter controls whether to do active network discovery
/// (UDP broadcast) in addition to checking the cache.
pub async fn find_services(
    criteria: &ServiceSearchCriteria,
    state: &AppState,
    fresh: bool,
) -> ServiceDiscoveryResponse {
    let start = std::time::Instant::now();
    let mut all_services = Vec::new();

    // ── Gateway check (ORCH-0004) ────────────────────────────────
    // Gateways appear first (structural priority — routed endpoint before raw).

    // Check local gateway registrations from the unified registry
    {
        let reg = state.registry.read().await;
        for entry in reg.gateway_entries() {
            let tool = &entry.tool;
            let offering = &tool.tool.tool_type;
            let gw_category = &tool.tool.category;
            let gw_tags = if tool.tool.tags.is_empty() {
                vec!["orchestrator".to_string()]
            } else {
                tool.tool.tags.clone()
            };

            if !matches_criteria(
                criteria,
                &tool.fqid,
                offering,
                gw_category,
                &gw_tags,
                &[],
            ) {
                continue;
            }

            // Use preserved source fields — no URI parsing needed.
            let svc = &tool.service;
            let conn = ResolvedConnection {
                hostname: svc.hostname.clone().unwrap_or_else(|| tool.stone.name.clone()),
                ip: svc.ip.clone().unwrap_or_default(),
                port: svc.port.unwrap_or(0),
                protocol: svc.protocol.clone(),
                uris: svc.uris.clone(),
            };

            all_services.push(FoundService {
                offering_id: String::new(),
                name: tool.fqid.clone(),
                offering: offering.clone(),
                category: gw_category.to_string(),
                tags: gw_tags,
                status: garden_common::SERVICE_RUNNING.to_string(),
                stone: StoneRef {
                    id: tool.stone.id.clone(),
                    name: tool.stone.name.clone(),
                    endpoint: tool.stone.endpoint.clone(),
                },
                connection: conn,
                sub_capabilities: vec![],
            });
        }
    }

    // TOOLS-0003: Remote gateways are now in the registry (via tools beacon).
    // The topology cache gateway path is removed — the registry is the single
    // source of truth for gateway entries. The old path duplicated entries
    // because chirped gateways appeared on every stone's topology entry.

    // 1. Search local stone first (zero latency)
    let local_services = find_local_services(criteria, state).await;
    all_services.extend(local_services);

    // 2. Always check topology cache for remote services
    // The cache is populated by chirps from other stones
    let cached_services = find_services_in_topology_cache(criteria, state).await;
    all_services.extend(cached_services);

    // 3. If fresh requested, do active network discovery
    // This triggers UDP broadcast and waits for responses
    if fresh {
        // TODO: Implement active discovery that triggers UDP broadcast
        // For now, fresh just ensures we check the cache (which we always do now)
        tracing::debug!("Fresh mode: topology cache already checked");
    }

    let elapsed = start.elapsed();
    tracing::debug!(
        criteria = ?criteria,
        found = all_services.len(),
        duration_ms = elapsed.as_millis(),
        "Service discovery completed"
    );

    // Compute cache age from the most recent topology entry's last_seen
    let cache_age_seconds = {
        let map = state.topology_cache.read().await;
        map.values().map(|e| e.last_seen).max().map(|newest| {
            let age = Utc::now().signed_duration_since(newest);
            age.num_seconds().max(0) as u64
        })
    };

    ServiceDiscoveryResponse {
        found: !all_services.is_empty(),
        services: all_services,
        source: if fresh { "fresh" } else { "cache" }.to_string(),
        cache_age_seconds,
        timestamp: Utc::now(),
    }
}

/// Find services from topology cache (populated by chirps from other stones)
///
/// This is the primary method for cross-garden discovery.
/// Each stone chirps its services every 30s, and we cache that data.
/// No network requests needed - just read from cache.
async fn find_services_in_topology_cache(
    criteria: &ServiceSearchCriteria,
    state: &AppState,
) -> Vec<FoundService> {
    let stones = topology::get_online_stones(&state.topology_cache).await;
    let mut results = Vec::new();

    for stone in stones {
        // Skip self — local services are already covered by find_local_services
        if stone.stone_id == state.stone_id {
            continue;
        }

        // Skip if no services (stone hasn't chirped yet or has none)
        if stone.services.is_empty() {
            continue;
        }

        for svc in &stone.services {
            // Only include running services
            if svc.status != garden_common::SERVICE_RUNNING {
                continue;
            }

            // Check if matches criteria
            // Note: Remote services don't include sub_capabilities in chirps yet
            // Sub-capability filtering only works for local services
            if !matches_criteria(criteria, &svc.name.to_string(), &svc.offering, &svc.category, &[], &[]) {
                continue;
            }

            // Infer protocol and resolve connection
            let protocol = connection::infer_protocol(&svc.offering, &svc.category, state).await;

            // PORT-0001: Use actual remapped port from chirp if available,
            // otherwise fall back to manifest default.
            let port = if let Some(&p) = svc.ports.get("default") {
                p
            } else {
                get_offering_port(&svc.offering, state).await
            };
            let connection_profile = state
                .manifest_registry
                .get_offering(&svc.offering)
                .and_then(|entry| entry.connection.as_ref());
            let uri_template = connection::select_uri_template(connection_profile, &svc.category);

            let conn = connection::resolve_connection(
                &stone.stone_name,
                &stone.address.http_base(),
                port,
                &protocol,
                uri_template.as_deref(),
            );

            results.push(FoundService {
                offering_id: svc.offering_id.clone(),
                name: svc.name.to_string(),
                offering: svc.offering.clone(),
                category: svc.category.clone(),
                tags: vec![],
                status: svc.status.clone(),
                stone: StoneRef {
                    id: stone.stone_id.clone(),
                    name: stone.stone_name.clone(),
                    endpoint: stone.address.http_base(),
                },
                connection: conn,
                sub_capabilities: vec![], // Remote services don't include sub_capabilities in chirps
            });
        }
    }

    results
}

/// Get default port from offering manifest
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

/// Find services on remote stones via HTTP requests (legacy, slower)
///
/// This is the fallback method that makes HTTP requests to each stone.
/// Prefer find_services_in_topology_cache for better performance.
#[allow(dead_code)]
async fn find_remote_services(
    criteria: &ServiceSearchCriteria,
    state: &AppState,
) -> Vec<FoundService> {
    let stones = topology::get_online_stones(&state.topology_cache).await;
    let mut results = Vec::new();

    // Query each remote stone in parallel
    let timeout = Duration::from_secs(2);
    let tasks: Vec<_> = stones
        .into_iter()
        .map(|stone| {
            let criteria = criteria.clone();
            let state_clone = state.clone();
            tokio::spawn(async move {
                fetch_remote_services(
                    &stone.address.http_base(),
                    &criteria,
                    &stone,
                    timeout,
                    &state_clone,
                )
                .await
            })
        })
        .collect();

    for task in tasks {
        match task.await {
            Ok(Ok(services)) => results.extend(services),
            Ok(Err(e)) => {
                tracing::debug!(error = ?e, "Failed to fetch services from remote stone");
            }
            Err(e) => {
                tracing::debug!(error = ?e, "Task join error while fetching remote services");
            }
        }
    }

    results
}

/// Fetch services from a single remote stone
async fn fetch_remote_services(
    endpoint: &str,
    criteria: &ServiceSearchCriteria,
    stone: &TopologyEntry,
    timeout: Duration,
    state: &AppState,
) -> anyhow::Result<Vec<FoundService>> {
    let client = reqwest::Client::builder().timeout(timeout).build()?;

    // Build query URL
    let mut url = format!("{}/api/v1/services", endpoint.trim_end_matches('/'));
    let mut query_params = Vec::new();

    if let Some(ref name) = criteria.name {
        query_params.push(format!("name={}", name));
    }
    if let Some(ref category) = criteria.category {
        query_params.push(format!("category={}", category));
    }
    if let Some(ref tag) = criteria.tag {
        query_params.push(format!("tag={}", tag));
    }

    if !query_params.is_empty() {
        url = format!("{}?{}", url, query_params.join("&"));
    }

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Remote stone returned error: {}", response.status());
    }

    // Parse response
    let services: Vec<garden_common::ServiceInfo> = response.json().await?;

    // Convert to FoundService with connection resolution
    let mut results = Vec::new();
    for service in services {
        if service.status != ServiceStatus::Running {
            continue;
        }

        // Infer protocol and resolve connection
        let category = service.offering.clone(); // Use offering as category fallback
        let protocol = connection::infer_protocol(&service.offering, &category, state).await;
        let connection_profile = state
            .manifest_registry
            .get_offering(&service.offering)
            .and_then(|entry| entry.connection.as_ref());
        let uri_template = connection::select_uri_template(connection_profile, &category);

        let conn = connection::resolve_connection(
            &stone.stone_name,
            &stone.address.http_base(),
            service.ports.native,
            &protocol,
            uri_template.as_deref(),
        );

        results.push(FoundService {
            offering_id: service.offering_id,
            name: service.name,
            offering: service.offering,
            category,
            tags: vec![],
            status: format!("{:?}", service.status),
            stone: StoneRef {
                id: stone.stone_id.clone(),
                name: stone.stone_name.clone(),
                endpoint: stone.address.http_base(),
            },
            connection: conn,
            sub_capabilities: service.sub_capabilities,
        });
    }

    Ok(results)
}

/// Check if a service matches the search criteria
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
        assert!(ServiceSearchCriteria::by_name_with_sub_capabilities(
            "ollama",
            vec![SubCapabilityFilter {
                cap_type: None,
                item: "llama2".to_string(),
            }],
        )
        .has_sub_capability_filter());
        assert!(!ServiceSearchCriteria::by_name("mongodb").has_sub_capability_filter());
    }
}
