//! Tools domain data contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

/// Tool type discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolType {
    Offering,
    SeedBank,
}

impl ToolType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offering => "offering",
            Self::SeedBank => "seed-bank",
        }
    }
}

impl Display for ToolType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Build canonical tool_fqid = "{tool-type}:{fqid}".
pub fn build_tool_fqid(tool_type: ToolType, fqid: &str) -> Result<String, ToolParseError> {
    let normalized = fqid.trim();
    if normalized.is_empty() {
        return Err(ToolParseError("Tool fqid cannot be empty".to_string()));
    }
    Ok(format!("{}:{}", tool_type.as_str(), normalized))
}

/// Parse canonical tool_fqid = "{tool-type}:{fqid}".
pub fn parse_tool_fqid(input: &str) -> Result<(ToolType, String), ToolParseError> {
    let value = input.trim();
    let (kind, fqid) = value
        .split_once(':')
        .ok_or_else(|| ToolParseError("tool_fqid must include tool type prefix".to_string()))?;
    if fqid.trim().is_empty() {
        return Err(ToolParseError(
            "tool_fqid payload cannot be empty".to_string(),
        ));
    }
    let tool_type = match kind.trim().to_ascii_lowercase().as_str() {
        "offering" => ToolType::Offering,
        "seed-bank" => ToolType::SeedBank,
        other => return Err(ToolParseError(format!("Unsupported tool type: {}", other))),
    };
    Ok((tool_type, fqid.trim().to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolParseError(pub String);

impl Display for ToolParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolParseError {}

/// Tool state vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Ready,
    Degraded,
    Unavailable,
}

/// Optional connection data for a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolConnection {
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub uris: Vec<String>,
}

/// Complete capability snapshot for an offering tool.
///
/// Keys are capability types ("model", "extension"),
/// values are stable sorted item lists.
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

/// Unified tool projection for automation clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolProjection {
    pub tool_fqid: String,
    pub tool_uid: String,
    pub tool_type: ToolType,
    pub state: ToolState,
    pub ready: bool,
    pub revision: u64,
    pub stone_id: String,
    pub stone_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ToolConnection>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capabilities: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub capability_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_delta: Option<CapabilityDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl ToolProjection {
    pub fn normalize_capabilities(&mut self) {
        let mut snapshot = CapabilitySnapshot {
            items: std::mem::take(&mut self.capabilities),
        };
        snapshot.normalize();
        self.capabilities = snapshot.items;
    }
}

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
    pub tool_fqid: String,
    pub tool_uid: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<ToolProjection>,
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

/// Parsed capability selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelector {
    pub cap_type: String,
    pub item: String,
}

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

        let offering_fqn = crate::offerings::parse_offering_fqn(offering_part)
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

    let offering_fqn = crate::offerings::parse_offering_fqn(offering_part)
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

    for raw in selectors.split(|c| c == ',' || c == '|') {
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

        // If a default type is known, treat non-matching prefixes as part of
        // the item so values like "llama3:8b" remain valid untyped selectors.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_fqid_valid() {
        let (kind, fqid) = parse_tool_fqid("offering:ollama:dev").unwrap();
        assert_eq!(kind, ToolType::Offering);
        assert_eq!(fqid, "ollama:dev");
    }

    #[test]
    fn parse_tool_fqid_invalid() {
        let err = parse_tool_fqid("offering").unwrap_err();
        assert!(err.to_string().contains("tool type prefix"));
    }

    #[test]
    fn parse_capability_wish_bracket() {
        let wish = parse_capability_wish("ollama:dev[model:modelv1]", None).unwrap();
        assert_eq!(wish.offering_fqn, "ollama:dev");
        assert_eq!(wish.selectors.len(), 1);
        assert_eq!(wish.selectors[0].cap_type, "model");
        assert_eq!(wish.selectors[0].item, "modelv1");
    }

    #[test]
    fn parse_capability_wish_bracket_multi_untyped() {
        let wish = parse_capability_wish("ollama[model1,model2]", Some("model")).unwrap();
        assert_eq!(wish.offering_fqn, "ollama");
        assert_eq!(wish.selectors.len(), 2);
        assert_eq!(wish.selectors[0].cap_type, "model");
        assert_eq!(wish.selectors[0].item, "model1");
        assert_eq!(wish.selectors[1].cap_type, "model");
        assert_eq!(wish.selectors[1].item, "model2");
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
        assert_eq!(wish.selectors.len(), 1);
        assert_eq!(wish.selectors[0].cap_type, "model");
        assert_eq!(wish.selectors[0].item, "modelv1");
    }

    #[test]
    fn parse_capability_wish_shorthand_with_instance() {
        let wish = parse_capability_wish("ollama:dev:modelv1", Some("model")).unwrap();
        assert_eq!(wish.offering_fqn, "ollama:dev");
        assert_eq!(wish.selectors.len(), 1);
        assert_eq!(wish.selectors[0].cap_type, "model");
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
}
