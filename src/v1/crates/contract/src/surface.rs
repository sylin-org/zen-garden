//! The surface artifact (ADR-0009 / B1): faces + wire-type schemas,
//! emitted as ONE versioned file — the repo's public contract. Stale
//! truth is a test failure (R4.7): regenerate with
//! `ZG_REGEN_SURFACE=1 cargo test -p garden-contract`.

use schemars::{schema_for, Schema};
use serde_json::{json, Value};

/// The wire-visible types, by name, as JSON Schema.
pub fn type_schemas() -> Vec<( &'static str, Schema)> {
    vec![
        ("Announcement", schema_for!(crate::wire::Announcement)),
        ("ChirpFrame", schema_for!(crate::chirp::ChirpFrame)),
        ("DiscoveryRequest", schema_for!(crate::discovery::DiscoveryRequest)),
        ("DiscoveryResponse", schema_for!(crate::discovery::DiscoveryResponse)),
    ]
}

/// The complete surface: faces + type schemas, as one JSON value.
pub fn surface() -> Value {
    let faces: Vec<Value> = crate::faces::FACES
        .iter()
        .map(|d| {
            json!({
                "face": format!("{:?}", d.face),
                "method": d.method,
                "path": d.path,
                "summary": d.summary,
            })
        })
        .collect();
    let mut types = serde_json::Map::new();
    for (name, schema) in type_schemas() {
        types.insert(name.to_string(), serde_json::to_value(&schema).unwrap_or_default());
    }
    json!({
        "proto": crate::consts::PROTO_V1,
        "faces": faces,
        "types": types,
    })
}

/// The committed artifact, compiled in — servable by any moss without
/// touching the filesystem.
pub const SURFACE_JSON: &str = include_str!("../surface.json");

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// R4.7: stale truth is a test failure. Regenerate with
    /// `ZG_REGEN_SURFACE=1 cargo test -p garden-contract`.
    #[test]
    fn surface_json_matches_the_contract() {
        let rendered = serde_json::to_string_pretty(&surface()).unwrap() + "\n";
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/surface.json");
        if std::env::var_os("ZG_REGEN_SURFACE").is_some() {
            std::fs::write(path, &rendered).unwrap();
            println!("surface.json regenerated");
        }
        let committed = std::fs::read_to_string(path).unwrap();
        assert_eq!(rendered, committed, "surface.json is stale — regenerate with ZG_REGEN_SURFACE=1 cargo test -p garden-contract");
    }

    /// The artifact compiled into the binary is the same contract.
    #[test]
    fn embedded_surface_matches_the_generated() {
        let regenerated = serde_json::to_string_pretty(&surface()).unwrap() + "\n";
        assert_eq!(SURFACE_JSON, regenerated);
    }
}
