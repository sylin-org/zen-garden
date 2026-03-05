//! Tools domain data contracts — GardenTool unified model.
//!
//! TOOLS-0002: Single contract shared by `/garden/tools` and `/garden/services`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

// ── GardenTool ──────────────────────────────────────────────────────────────

/// Unified garden resource — the single domain contract for offerings and
/// seed-banks. Used by `/garden/tools` (streaming) and `/garden/services`
/// (query).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GardenTool {
    /// Bare canonical name: `"mongodb"`, `"mongodb:prod"`, `"ollama:adopted"`.
    /// No `offering:` or `seed-bank:` prefix.
    pub fqid: String,

    /// Tool identity.
    pub tool: ToolIdentity,

    /// Stone hosting this resource.
    pub stone: Stone,

    /// Service runtime state and connection.
    pub service: ServiceInfo,

    /// Capabilities (models, collections, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<Capability>,

    /// Storage-specific metadata. Present only when `tool.category == "storage"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageMetadata>,
}

/// Tool identity — what this resource is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIdentity {
    /// Instance qualifier: `""` for default, `"prod"`, `"dev"`, `"adopted"`.
    #[serde(default)]
    pub name: String,

    /// Offering type: `"mongodb"`, `"ollama"`, `"redis"`, `"seed-bank"`.
    #[serde(rename = "type")]
    pub tool_type: String,

    /// Tool category: `"orchestrator"`, `"offering"`, `"storage"`.
    pub category: String,

    /// Unique identifier (GUIDv7).
    pub id: String,

    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Stone hosting a garden resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stone {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// Service runtime state and connection info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceInfo {
    /// Current status: `"running"`, `"stopped"`, `"degraded"`.
    pub status: String,

    /// Whether the service is ready for traffic.
    pub ready: bool,

    /// Wire protocol: `"mongodb"`, `"http"`, `"s3"`, `"redis"`.
    pub protocol: String,

    /// Connection URIs, ordered by preference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uris: Vec<String>,

    /// Source hostname (preserved from registration, not parsed from URIs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// Source IP (preserved from registration, not parsed from URIs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,

    /// Source port (preserved from registration, not parsed from URIs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// URI template before substitution (e.g. `"mongodb://{host}:{port}/?replicaSet=zen-garden"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_template: Option<String>,
}

/// Typed capability entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    /// Capability type: `"model"`, `"collection"`.
    #[serde(rename = "type")]
    pub cap_type: String,

    /// Items within this capability type.
    pub items: Vec<String>,
}

/// Storage-specific metadata carried by seed-bank entries.
///
/// Present only when `tool.category == "storage"`. Part of the boundary
/// model — no transforms needed, read sites access fields directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageMetadata {
    /// Replication role: `"primary"`, `"dormant"`, or `None` if unassigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Total capacity in bytes.
    pub capacity_bytes: u64,
    /// Used space in bytes.
    pub used_bytes: u64,
    /// Visibility: `"open"`, `"closed"`, `"read-only"`.
    pub visibility: String,
    /// Whether the seed bank is encrypted.
    pub encrypted: bool,
    /// Pinned Primary identifier (STORAGE-0006).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_id: Option<String>,
    /// Supported protocols: `["s3", "storage"]`.
    #[serde(default)]
    pub protocols: Vec<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

impl GardenTool {
    /// Category ordering for result sorting.
    /// Orchestrators pin first, then offerings, then storage.
    pub fn category_priority(&self) -> u8 {
        match self.tool.category.as_str() {
            "orchestrator" => 0,
            "offering" => 1,
            "storage" => 2,
            _ => 3,
        }
    }

    /// Check if this tool has a specific capability.
    pub fn has_capability(&self, cap_type: &str, item: &str) -> bool {
        let t = cap_type.trim().to_ascii_lowercase();
        let i = item.trim().to_ascii_lowercase();
        self.capabilities.iter().any(|cap| {
            cap.cap_type.eq_ignore_ascii_case(&t)
                && cap.items.iter().any(|v| v.eq_ignore_ascii_case(&i))
        })
    }

    /// Extract the offering type from the fqid.
    /// `"mongodb:prod"` → `"mongodb"`, `"redis"` → `"redis"`.
    pub fn offering_type(&self) -> &str {
        &self.tool.tool_type
    }
}

// ── Fqid Matching ───────────────────────────────────────────────────────────

/// Match a query fqid against a tool.
///
/// - Bare name (e.g., `"mongodb"`) matches all tools with `tool.tool_type == "mongodb"`.
/// - Instance-qualified (e.g., `"mongodb::prod"`) matches exact `fqid`.
/// - Legacy V1 queries (`"mongodb:prod"`) are normalized to V2 before matching.
/// - Does NOT prefix-match: `"ollama"` does NOT match `"ollama-cpu"`.
pub fn fqid_matches(query: &str, tool: &GardenTool) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    if q.contains("::") {
        // V2 instance-qualified match
        tool.fqid.eq_ignore_ascii_case(&q)
    } else if q.contains(':') {
        // Could be V1 legacy ("mongodb:prod") or source scheme ("image:nginx").
        // Try normalizing through OfferingFqn::parse to get canonical form.
        if let Ok(parsed) = crate::offerings::OfferingFqn::parse(&q) {
            tool.fqid.eq_ignore_ascii_case(&parsed.fqn())
        } else {
            tool.fqid.eq_ignore_ascii_case(&q)
        }
    } else {
        // Type match: all instances of this offering type
        tool.tool.tool_type.eq_ignore_ascii_case(&q)
    }
}

// ── Deltas & Beacons ────────────────────────────────────────────────────────

/// Delta kind for tool updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDeltaKind {
    Upsert,
    Remove,
}

/// Single tool delta in stream/beacon envelopes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDelta {
    pub event_id: String,
    pub cursor: u64,
    pub timestamp: DateTime<Utc>,
    pub kind: ToolDeltaKind,
    /// Bare fqid: `"mongodb"`, `"ollama:adopted"`.
    pub fqid: String,
    /// Stone-scoped unique key for dedup: `"{stone_id}:{fqid}:{category}"`.
    pub tool_key: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<GardenTool>,
}

/// Inter-Moss tools announcement payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolsBeacon {
    pub stone_id: String,
    pub stone_name: String,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deltas: Vec<ToolDelta>,
    pub timestamp: DateTime<Utc>,
}

impl ToolsBeacon {
    pub fn empty(stone_id: &str, stone_name: &str, endpoint: &str) -> Self {
        Self {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            endpoint: endpoint.to_string(),
            deltas: Vec::new(),
            timestamp: Utc::now(),
        }
    }
}

// ── Tool Key ────────────────────────────────────────────────────────────────

/// Build the cache key for a tool: `"{stone_id}:{fqid}:{category}"`.
///
/// Multiple entries can share an fqid (e.g., `mongodb` on two different stones,
/// or `mongodb` as both orchestrator and offering on the same stone).
/// The key disambiguates.
pub fn build_tool_key(stone_id: &str, fqid: &str, category: &str) -> String {
    format!("{}:{}:{}", stone_id, fqid, category)
}

// ── Capability Types (preserved from TOOLS-0001) ────────────────────────────

/// Complete capability snapshot for filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilitySnapshot {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, Vec<String>>,
}

impl CapabilitySnapshot {
    pub fn contains(&self, cap_type: &str, item: &str) -> bool {
        let t = cap_type.trim().to_ascii_lowercase();
        let i = item.trim().to_ascii_lowercase();
        self.items
            .get(&t)
            .map(|items| items.iter().any(|v| v.eq_ignore_ascii_case(&i)))
            .unwrap_or(false)
    }

    pub fn normalize(&mut self) {
        let mut normalized = BTreeMap::new();
        for (cap_type, values) in &self.items {
            let key = cap_type.trim().to_ascii_lowercase();
            if key.is_empty() {
                continue;
            }
            let mut set = BTreeSet::new();
            for value in values {
                let v = value.trim();
                if !v.is_empty() {
                    set.insert(v.to_string());
                }
            }
            if !set.is_empty() {
                normalized.insert(key, set.into_iter().collect());
            }
        }
        self.items = normalized;
    }
}

/// Optional capability delta for incremental updates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilityDelta {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub added: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub removed: BTreeMap<String, Vec<String>>,
}

/// Parsed capability selector for query filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelector {
    pub cap_type: String,
    pub item: String,
}

// ── Capability Wish Parsing ─────────────────────────────────────────────────

/// Parsed capability wish target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityWish {
    pub offering_fqn: String,
    pub selectors: Vec<CapabilitySelector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityWishParseError(pub String);

impl Display for CapabilityWishParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CapabilityWishParseError {}

/// Parse capability wish syntax:
/// - canonical: `offering[:instance][capability[,capability...]]`
/// - canonical typed: `offering[:instance][cap_type:item[,cap_type:item...]]`
/// - canonical also accepts `|` as separator inside brackets
/// - shorthand: `offering[:instance]:item` (requires default capability type)
pub fn parse_capability_wish(
    input: &str,
    default_capability_type: Option<&str>,
) -> Result<CapabilityWish, CapabilityWishParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CapabilityWishParseError(
            "Capability wish cannot be empty".to_string(),
        ));
    }

    // Canonical bracket syntax: ollama:dev[model:modelv1]
    if let Some(start) = trimmed.find('[') {
        if !trimmed.ends_with(']') {
            return Err(CapabilityWishParseError(
                "Capability wish bracket syntax must end with ']'".to_string(),
            ));
        }
        if start == 0 {
            return Err(CapabilityWishParseError(
                "Offering name is required before capability selector".to_string(),
            ));
        }
        let offering_part = &trimmed[..start];
        let selectors_raw = &trimmed[start + 1..trimmed.len() - 1];

        let offering_fqn = crate::offerings::OfferingFqn::parse(offering_part)
            .map_err(|e| CapabilityWishParseError(e.to_string()))?
            .fqn();

        let selectors = parse_capability_selectors(selectors_raw, default_capability_type)?;

        return Ok(CapabilityWish {
            offering_fqn,
            selectors,
        });
    }

    // Shorthand: ollama:modelv1 or ollama:dev:modelv1 (split by rightmost ':')
    let idx = trimmed.rfind(':').ok_or_else(|| {
        CapabilityWishParseError(
            "Capability wish shorthand requires ':' (example: ollama:modelv1)".to_string(),
        )
    })?;
    let offering_part = trimmed[..idx].trim();
    let item = trimmed[idx + 1..].trim();
    if item.is_empty() {
        return Err(CapabilityWishParseError(
            "Capability item cannot be empty".to_string(),
        ));
    }

    let offering_fqn = crate::offerings::OfferingFqn::parse(offering_part)
        .map_err(|e| CapabilityWishParseError(e.to_string()))?
        .fqn();

    let cap_type = default_capability_type
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CapabilityWishParseError(
                "Shorthand capability wish requires a default capability type".to_string(),
            )
        })?;

    Ok(CapabilityWish {
        offering_fqn,
        selectors: vec![CapabilitySelector {
            cap_type,
            item: item.to_string(),
        }],
    })
}

fn parse_capability_selectors(
    selectors: &str,
    default_capability_type: Option<&str>,
) -> Result<Vec<CapabilitySelector>, CapabilityWishParseError> {
    let mut parsed = Vec::new();
    let mut seen = BTreeSet::new();

    for raw in selectors.split([',', '|']) {
        let selector = parse_capability_selector(raw, default_capability_type)?;
        let key = format!(
            "{}:{}",
            selector.cap_type.to_ascii_lowercase(),
            selector.item.to_ascii_lowercase()
        );
        if seen.insert(key) {
            parsed.push(selector);
        }
    }

    if parsed.is_empty() {
        return Err(CapabilityWishParseError(
            "Capability selector cannot be empty".to_string(),
        ));
    }

    Ok(parsed)
}

fn parse_capability_selector(
    selector: &str,
    default_capability_type: Option<&str>,
) -> Result<CapabilitySelector, CapabilityWishParseError> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(CapabilityWishParseError(
            "Capability selector cannot be empty".to_string(),
        ));
    }

    let default_cap_type = default_capability_type
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CapabilityWishParseError(
                "Untyped selector requires a default capability type".to_string(),
            )
        });

    if let Some((cap_type, item)) = selector.split_once(':') {
        let cap_type = cap_type.trim().to_ascii_lowercase();
        let item = item.trim();
        if cap_type.is_empty() || item.is_empty() {
            return Err(CapabilityWishParseError(
                "Capability selector must be '<type>:<item>'".to_string(),
            ));
        }

        if let Ok(default) = &default_cap_type {
            if cap_type != *default {
                return Ok(CapabilitySelector {
                    cap_type: default.clone(),
                    item: selector.to_string(),
                });
            }
        }

        return Ok(CapabilitySelector {
            cap_type,
            item: item.to_string(),
        });
    }

    let cap_type = default_cap_type?;
    Ok(CapabilitySelector {
        cap_type,
        item: selector.to_string(),
    })
}

// ── Legacy Compatibility ────────────────────────────────────────────────────

/// Tool type discriminator (kept for beacon/cache internals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolType {
    Offering,
    SeedBank,
}

impl ToolType {
    pub fn as_category(self) -> &'static str {
        match self {
            Self::Offering => "offering",
            Self::SeedBank => "storage",
        }
    }
}

impl Display for ToolType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_category())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tool(fqid: &str, tool_type: &str, category: &str) -> GardenTool {
        GardenTool {
            fqid: fqid.to_string(),
            tool: ToolIdentity {
                name: if fqid.contains(':') {
                    fqid.split_once(':').unwrap().1.to_string()
                } else {
                    String::new()
                },
                tool_type: tool_type.to_string(),
                category: category.to_string(),
                id: "test-id".to_string(),
                tags: Vec::new(),
            },
            stone: Stone {
                id: "stone-1".to_string(),
                name: "stone-test".to_string(),
                endpoint: "http://192.168.1.100:7185".to_string(),
            },
            service: ServiceInfo {
                status: "running".to_string(),
                ready: true,
                protocol: tool_type.to_string(),
                uris: Vec::new(),
                hostname: None,
                ip: None,
                port: None,
                uri_template: None,
            },
            capabilities: Vec::new(),
            storage: None,
        }
    }

    #[test]
    fn fqid_bare_matches_type() {
        let tool = sample_tool("mongodb", "mongodb", "offering");
        assert!(fqid_matches("mongodb", &tool));
        assert!(!fqid_matches("redis", &tool));
    }

    #[test]
    fn fqid_v2_matches_named_instance() {
        let tool = sample_tool("mongodb::prod", "mongodb", "offering");
        assert!(fqid_matches("mongodb", &tool)); // type match
        assert!(fqid_matches("mongodb::prod", &tool)); // V2 exact match
        assert!(fqid_matches("mongodb:prod", &tool)); // V1 legacy normalized
        assert!(!fqid_matches("mongodb::dev", &tool)); // wrong instance
    }

    #[test]
    fn fqid_no_prefix_match() {
        let tool = sample_tool("ollama-cpu::adopted", "ollama-cpu", "offering");
        assert!(!fqid_matches("ollama", &tool)); // different type
        assert!(fqid_matches("ollama-cpu", &tool)); // exact type
    }

    #[test]
    fn category_priority_ordering() {
        let orch = sample_tool("mongodb", "mongodb", "orchestrator");
        let offer = sample_tool("mongodb", "mongodb", "offering");
        let storage = sample_tool("seed-test", "seed-bank", "storage");

        assert!(orch.category_priority() < offer.category_priority());
        assert!(offer.category_priority() < storage.category_priority());
    }

    #[test]
    fn has_capability_check() {
        let mut tool = sample_tool("ollama:adopted", "ollama", "offering");
        tool.capabilities = vec![Capability {
            cap_type: "model".to_string(),
            items: vec!["llama3.2".to_string(), "nomic-embed-text".to_string()],
        }];

        assert!(tool.has_capability("model", "llama3.2"));
        assert!(tool.has_capability("Model", "Llama3.2")); // case insensitive
        assert!(!tool.has_capability("model", "gpt-4"));
        assert!(!tool.has_capability("plugin", "llama3.2"));
    }

    #[test]
    fn parse_capability_wish_bracket() {
        let wish = parse_capability_wish("ollama:dev[model:modelv1]", None).unwrap();
        assert_eq!(wish.offering_fqn, "ollama::dev");
        assert_eq!(wish.selectors.len(), 1);
        assert_eq!(wish.selectors[0].cap_type, "model");
        assert_eq!(wish.selectors[0].item, "modelv1");
    }

    #[test]
    fn parse_capability_wish_bracket_multi_untyped() {
        let wish = parse_capability_wish("ollama[model1,model2]", Some("model")).unwrap();
        assert_eq!(wish.offering_fqn, "ollama");
        assert_eq!(wish.selectors.len(), 2);
    }

    #[test]
    fn parse_capability_wish_bracket_multi_pipe_separator() {
        let wish = parse_capability_wish("ollama[model1|model2]", Some("model")).unwrap();
        assert_eq!(wish.selectors.len(), 2);
    }

    #[test]
    fn parse_capability_wish_untyped_token_with_colon() {
        let wish = parse_capability_wish("ollama[llama3:8b]", Some("model")).unwrap();
        assert_eq!(wish.selectors.len(), 1);
        assert_eq!(wish.selectors[0].cap_type, "model");
        assert_eq!(wish.selectors[0].item, "llama3:8b");
    }

    #[test]
    fn parse_capability_wish_shorthand_default_type() {
        let wish = parse_capability_wish("ollama:modelv1", Some("model")).unwrap();
        assert_eq!(wish.offering_fqn, "ollama");
        assert_eq!(wish.selectors[0].item, "modelv1");
    }

    #[test]
    fn parse_capability_wish_shorthand_with_instance() {
        let wish = parse_capability_wish("ollama:dev:modelv1", Some("model")).unwrap();
        assert_eq!(wish.offering_fqn, "ollama::dev");
        assert_eq!(wish.selectors[0].item, "modelv1");
    }

    #[test]
    fn parse_capability_wish_shorthand_requires_default_type() {
        let err = parse_capability_wish("ollama:modelv1", None).unwrap_err();
        assert!(err.to_string().contains("default capability type"));
    }

    #[test]
    fn capability_snapshot_normalizes() {
        let mut snap = CapabilitySnapshot {
            items: BTreeMap::from([(
                "Model".to_string(),
                vec!["llama3".to_string(), "llama3".to_string(), " ".to_string()],
            )]),
        };
        snap.normalize();
        assert_eq!(
            snap.items.get("model").unwrap(),
            &vec!["llama3".to_string()]
        );
    }

    #[test]
    fn build_tool_key_format() {
        let key = build_tool_key("stone-abc", "mongodb", "orchestrator");
        assert_eq!(key, "stone-abc:mongodb:orchestrator");
    }
}
