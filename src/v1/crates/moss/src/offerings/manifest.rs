// Fields/sections land with their consuming slices (OFFERINGS.md §5/§7).
#![allow(dead_code)]

//! The manifest format (garden manifest v1, OFFERINGS.md §5.1) and the
//! catalog that loads it.
//!
//! One `<name>.offering.yaml` per offering; identity stated once (stem must
//! equal `name` — validated). Compatibility rules carry citations; unknown
//! fact paths or units fail at load, not at placement.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// The manifest document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub kind: String, // software | hardware
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub managed: Option<ManagedIntent>,
    pub adopted: Option<serde_yaml::Value>, // detection DSL lands in O3
    pub borrowed: Option<serde_yaml::Value>,
    #[serde(default)]
    pub compatibility: Vec<CompatRule>,
    /// Declared install form (OFFERINGS.md §5.1): ask/secret/default.
    #[serde(default)]
    pub inputs: BTreeMap<String, InputField>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputField {
    pub ask: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub default: Option<String>,
}

/// Placement intent — everything compile needs to produce a WorkloadSpec.
#[derive(Debug, Clone, Deserialize)]
pub struct ManagedIntent {
    #[serde(default = "default_world")]
    pub world: String,
    pub image: String,
    /// name → container port. Host mapping is the world's craft (PORT-0001).
    #[serde(default)]
    pub ports: BTreeMap<String, u16>,
    #[serde(default)]
    pub volumes: Vec<VolumeDecl>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub config_files: Vec<serde_yaml::Value>,
    #[serde(default)]
    pub healthcheck: Option<serde_yaml::Value>,
    #[serde(default = "default_restart")]
    pub restart: String,
    /// Rare-but-real passthroughs: cap_add, shm_size, sysctls, ulimits...
    #[serde(default)]
    pub advanced: serde_yaml::Value,
    /// Placement hints consumed at compile time.
    #[serde(default)]
    pub placement: Option<serde_yaml::Value>,
    /// Cron maintenance commands.
    #[serde(default)]
    pub tasks: Option<serde_yaml::Value>,
}

fn default_world() -> String {
    "oci".into()
}

fn default_restart() -> String {
    "unless-stopped".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct VolumeDecl {
    pub name: String,
    pub mount: String,
}

/// One compatibility rule. `decide` is explicit — absence-as-deny was a PoC
/// scar (clever, invisible).
#[derive(Debug, Clone, Deserialize)]
pub struct CompatRule {
    pub name: String,
    pub when: Vec<Condition>,
    pub decide: Decide,
    /// Fallback target for `decide: fallback`.
    #[serde(default)]
    pub into: Option<FallbackInto>,
    pub because: String,
    #[serde(default)]
    pub suggest: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decide {
    Place,
    Fallback,
    Deny,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FallbackInto {
    pub image: String,
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    const GOOD: &str = "\
kind: software
name: redis
category: data
description: In-memory cache
managed:
  world: oci
  image: redis:7-alpine
  ports: { default: 6379 }
";

    #[test]
    fn parses_and_enforces_stem_identity() {
        let m = Catalog::parse("redis", GOOD).unwrap();
        assert_eq!(m.name, "redis");
        assert!(Catalog::parse("not-redis", GOOD).is_err(), "stem must match name");
    }

    /// The catalog derives from the directory: good manifests load, bad
    /// ones are skipped with warnings, siblings survive.
    #[test]
    fn load_dir_derives_catalog_from_tree() {
        let dir = std::env::temp_dir().join(format!("moss-catalog-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sw/data")).unwrap();
        std::fs::write(dir.join("sw/data/redis.offering.yaml"), GOOD).unwrap();
        std::fs::write(dir.join("sw/data/broken.offering.yaml"), "kind: [unclosed").unwrap();
        // stem mismatch → rejected at parse
        std::fs::write(
            dir.join("sw/data/mismatch.offering.yaml"),
            GOOD.replace("name: redis", "name: other"),
        )
        .unwrap();

        let catalog = Catalog::load_dir(&dir);
        assert_eq!(catalog.names(), vec!["redis".to_string()]);
        assert_eq!(catalog.get("redis").unwrap().category, "data");
    }

    /// Overlays aim one stone's own manifests at the catalog without
    /// forking the base: later layers override by NAME, siblings survive.
    #[test]
    fn load_layered_overrides_by_name_and_keeps_siblings() {
        let root = std::env::temp_dir().join(format!("moss-catalog-base-{}", std::process::id()));
        let overlay = std::env::temp_dir().join(format!("moss-catalog-over-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&overlay);

        // Base: two offerings.
        std::fs::create_dir_all(root.join("sw/data")).unwrap();
        std::fs::write(
            root.join("sw/data/redis.offering.yaml"),
            GOOD.replace("description: In-memory cache", "description: BASE redis"),
        )
        .unwrap();
        std::fs::write(root.join("sw/data/mongo.offering.yaml"), GOOD.replace("name: redis", "name: mongo").replace("redis:7-alpine", "mongo:7")).unwrap();
        // Overlay: redis redefined, extra sibling, junk skipped.
        std::fs::create_dir_all(overlay.join("sw/cache")).unwrap();
        std::fs::write(
            overlay.join("sw/cache/redis.offering.yaml"),
            GOOD.replace("description: In-memory cache", "description: OPERATOR redis"),
        )
        .unwrap();
        std::fs::write(overlay.join("sw/cache/valkey.offering.yaml"), GOOD.replace("name: redis", "name: valkey").replace("redis:7-alpine", "valkey:8")).unwrap();
        std::fs::write(overlay.join("sw/cache/broken.offering.yaml"), "kind: [oops").unwrap();

        let catalog = Catalog::load_layered(&root, &[overlay]);
        let mut names = catalog.names();
        names.sort();
        assert_eq!(names, ["mongo", "redis", "valkey"], "base + overlay siblings");

        // The override won — and it won on content, not just presence.
        let redis = catalog.get("redis").unwrap();
        assert_eq!(redis.description, "OPERATOR redis");
    }

    /// An absent overlay layer is routine (operators mostly run the base).
    #[test]
    fn missing_overlays_are_not_fatal() {
        let root = std::env::temp_dir().join(format!("moss-catalog-missing-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sw")).unwrap();
        std::fs::write(root.join("sw/redis.offering.yaml"), GOOD).unwrap();

        let ghost = root.parent().unwrap().join("no-such-overlay-dir");
        let catalog = Catalog::load_layered(&root, &[ghost]);
        assert_eq!(catalog.names(), vec!["redis".to_string()]);
    }
}

/// A single condition over the facts tree (§6.2 grammar). Units ride on the
/// path suffix (.mb/.gb); comparisons convert to canonical bytes.
#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    pub path: String,
    pub op: CondOp,
    pub value: serde_yaml::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CondOp {
    Eq,
    Ge,
    Lt,
    In,
    Lacks,
}

// ---------------------------------------------------------------------------
// The catalog: every manifest this moss can offer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: BTreeMap<String, Manifest>,
}

impl Catalog {
    /// Parse one manifest document. `expected_stem` enforces identity-stem
    /// equality (path redundant, not authoritative).
    pub fn parse(expected_stem: &str, yaml: &str) -> Result<Manifest, String> {
        let m: Manifest =
            serde_yaml::from_str(yaml).map_err(|e| format!("manifest '{expected_stem}': {e}"))?;
        if m.name != expected_stem {
            return Err(format!(
                "manifest name '{}' does not match file stem '{expected_stem}'",
                m.name
            ));
        }
        Ok(m)
    }

    /// A catalog seeded from documents — test convenience over `load_dir`.
    pub fn embedded<I, S>(docs: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (S, &'static str)>,
        S: AsRef<str>,
    {
        let mut catalog = Self::default();
        for (stem, yaml) in docs {
            let m = Self::parse(stem.as_ref(), yaml)?;
            catalog.entries.insert(m.name.clone(), m);
        }
        Ok(catalog)
    }

    /// Derive the catalog from a directory tree: every `*.offering.yaml`
    /// found below `root` is parsed, its stem taken from the filename.
    /// Unreadable/unparseable files are skipped with a warning — one bad
    /// manifest must not silence its siblings. A missing root yields an
    /// empty catalog (a stone may simply place nothing from catalogs).
    pub fn load_dir(root: &std::path::Path) -> Self {
        let mut catalog = Self::default();
        if !root.is_dir() {
            tracing::warn!(root = %root.display(), "catalog dir not found; no catalog offerings");
            return catalog;
        }
        let found = catalog.ingest(root);
        tracing::info!(root = %root.display(), offerings = found, "catalog loaded");
        catalog
    }

    /// Base tree first, then every overlay layer on top: later layers
    /// OVERRIDE earlier entries by name (identity is the stem; `sw/<cat>/`
    /// nesting is traversal detail). Operators aim one stone's manifests at
    /// `{data_dir}/manifests/sw/<category>/` without forking the whole base
    /// catalog. Missing layers are noted, never fatal.
    pub fn load_layered(
        base: &std::path::Path,
        overlays: &[std::path::PathBuf],
    ) -> Self {
        let mut catalog = Self::default();
        if !base.is_dir() {
            tracing::warn!(root = %base.display(), "catalog dir not found; no catalog offerings");
        } else {
            let found = catalog.ingest(base);
            tracing::info!(root = %base.display(), offerings = found, "catalog loaded");
        }

        let mut added = 0usize;
        let mut overrode = 0usize;
        for overlay in overlays {
            if !overlay.is_dir() {
                tracing::info!(overlay = %overlay.display(), "overlay absent; nothing applied");
                continue;
            }
            let (a, o) = catalog.ingest_counting(overlay);
            added += a;
            overrode += o;
        }
        tracing::info!(overlays = overlays.len(), added, overrode, "catalog overlays applied");
        catalog
    }

    /// Parse-and-insert everything under `root`; returns how many landed.
    fn ingest(&mut self, root: &std::path::Path) -> usize {
        let (added, _) = self.ingest_counting(root);
        added
    }

    /// Insert-count form used by the layered loader: it must know how many
    /// entries REPLACED something (the honest override report).
    fn ingest_counting(&mut self, root: &std::path::Path) -> (usize, usize) {
        let mut added = 0usize;
        let mut overrode = 0usize;
        visit_dir(root, &mut |stem, yaml| match Self::parse(stem, yaml) {
            Ok(m) => {
                if self.entries.insert(m.name.clone(), m).is_some() {
                    overrode += 1;
                }
                added += 1;
            }
            Err(e) => tracing::warn!(error = %e, "manifest skipped"),
        });
        (added, overrode)
    }

    pub fn get(&self, name: &str) -> Option<&Manifest> {
        self.entries.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Recursively visit a directory, handing every `*.offering.yaml` to `f`
/// with its stem (filename minus the `.offering.yaml` suffix).
fn visit_dir(dir: &std::path::Path, f: &mut impl FnMut(&str, &str)) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path, f);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && let Some(stem) = name.strip_suffix(".offering.yaml")
        {
            match std::fs::read_to_string(&path) {
                Ok(yaml) => f(stem, &yaml),
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "catalog file unreadable"),
            }
        }
    }
}
