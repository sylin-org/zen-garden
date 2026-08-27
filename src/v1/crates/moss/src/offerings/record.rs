//! The on-media offering record, v3 (S5.5): sections hold facts (R3.9).
//!
//! Rootspace: `identity · state · location · mode · registered_at ·
//! updated_at`. Every nesting level is a nameable noun. The domain type
//! ([`super::model::Offering`]) keeps its v2-flat serde — that flat shape
//! is the LEGACY reader/writer, and its bytes on disk are what this module
//! migrates: load detects v2, renames the source `*.json.migrated`, and
//! writes the sectioned truth fresh (the `.migrated` pattern — each file
//! moves once, evidence preserved).
//!
//! HTTP renders offerings through this same view: one shape, disk and
//! wire alike (R3.9, B1).

use super::model::{Location, Offering, Status};
use serde::{Deserialize, Serialize};

/// WHO the offering is — immutable identity plus catalog provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Stable identity; survives renames.
    pub offering_id: String,
    /// Fully-qualified name (`redis::default`).
    pub name: String,
    /// Catalog stem it grew from.
    pub stem: String,
    /// Catalog category (glossary noun).
    pub category: String,
}

/// HOW IT FARES right now — the lifecycle claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub status: Status,
}

/// The sectioned record. `mode` holds the internally-tagged
/// [`super::model::ModeData`] verbatim (its `mode` tag spells the kind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferingRecord {
    pub identity: Identity,
    pub state: State,
    pub location: Location,
    #[serde(rename = "mode")]
    pub mode_data: super::model::ModeData,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl OfferingRecord {
    pub fn from_domain(o: &Offering) -> Self {
        Self {
            identity: Identity {
                offering_id: o.offering_id.clone(),
                name: o.name.clone(),
                stem: o.offering.clone(),
                category: o.category.clone(),
            },
            state: State { status: o.status },
            location: o.location.clone(),
            mode_data: o.mode_data.clone(),
            registered_at: o.registered_at,
            updated_at: o.updated_at,
        }
    }

    pub fn into_domain(self) -> Offering {
        Offering {
            offering_id: self.identity.offering_id,
            name: self.identity.name,
            offering: self.identity.stem,
            category: self.identity.category,
            status: self.state.status,
            location: self.location,
            mode_data: self.mode_data,
            registered_at: self.registered_at,
            updated_at: self.updated_at,
        }
    }
}

/// Migrate a stored plan VALUE from the v2 flat shape to v3 sections:
/// `{workload, decisions, plan_hash, facts_generation}` →
/// `{workload, decisions, meta{plan_hash, facts_generation}}`. v3 values
/// pass through untouched; anything else (not a plan at all) likewise.
pub fn migrate_plan_value(v: serde_json::Value) -> serde_json::Value {
    let is_v2 = v.get("meta").is_none()
        && (v.get("plan_hash").is_some() || v.get("facts_generation").is_some());
    if !is_v2 {
        return v;
    }
    let mut out = serde_json::Map::new();
    let mut meta = serde_json::Map::new();
    if let serde_json::Value::Object(fields) = v {
        for (k, val) in fields {
            match k.as_str() {
                "plan_hash" | "facts_generation" => {
                    meta.insert(k, val);
                }
                other => {
                    out.insert(other.to_string(), val);
                }
            }
        }
    }
    out.insert("meta".into(), serde_json::Value::Object(meta));
    serde_json::Value::Object(out)
}

/// Migrate an Offering's mode payload in place: the embedded plan (when
/// present) re-sections, so record embed and plan.json sidecar speak one
/// shape.
pub fn migrate_embedded_plan(o: &mut Offering) {
    if let super::model::ModeData::Managed(m) = &mut o.mode_data
        && let Some(plan) = &mut m.plan
    {
        *plan = migrate_plan_value(plan.clone());
    }
}
