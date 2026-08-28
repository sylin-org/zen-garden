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
    /// The living will (ADR-0005 §1): how this offering asks to be
    /// remembered. Lifecycle intent — never hashed into plans (§7).
    #[serde(default)]
    pub capture: Option<super::capture::CapturePolicy>,
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
    /// Host-side allocation intent per named role (ADR-0002 §5.1). Absent
    /// roles are flexible pool members; a declared role's `port` is its
    /// preferred home, and `strict: true` marks the address identity-
    /// critical (occupied ⇒ refuse plant).
    #[serde(default)]
    pub host_ports: BTreeMap<String, HostPortDecl>,
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

/// One role's host allocation declaration. Every declared role carries its
/// port; strictness is the only escalation (ADR-0002 ruling 2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostPortDecl {
    pub port: u16,
    #[serde(default)]
    pub strict: bool,
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
        assert!(
            Catalog::parse("not-redis", GOOD).is_err(),
            "stem must match name"
        );
    }

    /// ADR-0002 §5.1 grammar: declared host roles must map onto real
    /// container roles; strict is just an escalation of a plain pin.
    #[test]
    fn host_ports_must_reference_known_roles() {
        let bad = GOOD.replace(
            "  ports: { default: 6379 }\n",
            "  ports: { default: 6379 }\n  host_ports: { dns: { port: 53, strict: true } }\n",
        );
        let err = Catalog::parse("redis", &bad).unwrap_err();
        assert!(err.contains("'dns'"), "{err}");

        let good = GOOD.replace(
            "  ports: { default: 6379 }\n",
            "  ports: { default: 6379 }\n  host_ports: { default: { port: 16379 } }\n",
        );
        let m = Catalog::parse("redis", &good).unwrap();
        let managed = m.managed.as_ref().unwrap();
        assert_eq!(managed.host_ports["default"].port, 16379);
        assert!(!managed.host_ports["default"].strict);
    }

    #[test]
    fn unknown_fields_in_host_port_decls_are_rejected() {
        let bad = GOOD.replace(
            "  ports: { default: 6379 }\n",
            "  ports: { default: 6379 }\n  host_ports: { default: { por: 1 } }\n",
        );
        assert!(
            Catalog::parse("redis", &bad).is_err(),
            "typo'd decl keys must fail loudly, not silently degrade"
        );
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

    /// ADR-0008: the embedded approved catalog is the floor — a moss
    /// with NO directories boots knowing the approved set; an operator
    /// layer overrides by name; the floor's other entries survive.
    #[test]
    fn embedded_floor_carries_first_light_and_layers_override_by_name() {
        let catalog = Catalog::load_fully_layered(None, &[]);
        assert!(
            catalog.get("memcached").is_some(),
            "the embedded floor carries the approved set"
        );
        assert!(catalog.get("redis").is_some());
        assert!(catalog.get("mongodb").is_some());

        // The operator layer: same name, different description — the
        // override wins; a sibling entry is ADDED, not lost.
        let dir = std::env::temp_dir().join(format!("moss-catalog-emb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("memcached.offering.yaml"),
            GOOD.replace("name: redis", "name: memcached")
                .replace("category: data", "category: cache")
                .replace("description: In-memory cache", "description: operator's own memcached"),
        )
        .unwrap();
        std::fs::write(
            dir.join("private.offering.yaml"),
            GOOD.replace("name: redis", "name: private"),
        )
        .unwrap();
        let layered = Catalog::load_fully_layered(None, std::slice::from_ref(&dir));
        let mem = layered.get("memcached").unwrap();
        assert_eq!(mem.description, "operator's own memcached", "override by name");
        assert!(layered.get("private").is_some(), "operator additions land");
        assert!(layered.get("redis").is_some(), "floor siblings survive");
        let _ = std::fs::remove_dir_all(&dir);
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

/// The approved catalog, compiled into the binary (ADR-0008 layer 0):
/// the release tagged what the moss knows how to place.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../catalog"]
struct ApprovedCatalog;

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
        Self::validate(&m)?;
        Ok(m)
    }

    /// Cross-section rules that serde can't express (ADR-0002 §5.1):
    /// a host-port declaration must name an EXISTING container port role —
    /// there is nothing to expose otherwise.
    fn validate(m: &Manifest) -> Result<(), String> {
        // The living will is a managed offering's will: capture without
        // placed work has nothing to preserve and nothing to hook into.
        if let Some(policy) = &m.capture {
            if m.managed.is_none() {
                return Err(format!(
                    "manifest '{}': capture requires a managed section — adoption and borrow have their own laws",
                    m.name
                ));
            }
            let volumes: Vec<String> = Self::managed_volume_names(m);
            let roles: Vec<String> = m
                .managed
                .as_ref()
                .map(|managed| managed.ports.keys().cloned().collect())
                .unwrap_or_default();
            policy.validate(&m.name, &volumes, &roles)?;
        }
        let Some(managed) = &m.managed else {
            return Ok(());
        };
        for role in managed.host_ports.keys() {
            if !managed.ports.contains_key(role) {
                return Err(format!(
                    "manifest '{}': host_ports declares role '{role}' with no matching ports entry",
                    m.name
                ));
            }
        }
        Ok(())
    }

    /// Declared volume names for template validation (ADR-0005 §1).
    fn managed_volume_names(m: &Manifest) -> Vec<String> {
        m.managed
            .as_ref()
            .map(|managed| managed.volumes.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default()
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

    /// Layer 0 (ADR-0008): the approved catalog, compiled into the
    /// binary. The floor every moss boots with — the release tagged what
    /// it knows how to place.
    fn ingest_embedded(&mut self) -> usize {
        let mut added = 0usize;
        for path in ApprovedCatalog::iter() {
            let path = path.as_ref();
            let Some(name) = path.rsplit('/').next() else { continue };
            let Some(stem) = name.strip_suffix(".offering.yaml") else { continue };
            if let Some(file) = ApprovedCatalog::get(path) {
                match Self::parse(stem, &String::from_utf8_lossy(&file.data)) {
                    Ok(m) => {
                        self.entries.insert(m.name.clone(), m);
                        added += 1;
                    }
                    Err(e) => tracing::warn!(error = %e, "embedded manifest skipped"),
                }
            }
        }
        added
    }

    /// The ADR-0008 layering: the embedded approved catalog is the floor;
    /// the operator catalog dir adds and overrides by name; the manifests
    /// overlay is highest. The filesystem base is OPTIONAL now — absence
    /// no longer means an empty garden (the floor carries it). Missing
    /// layers are noted, never fatal.
    pub fn load_fully_layered(
        base: Option<&std::path::Path>,
        overlays: &[std::path::PathBuf],
    ) -> Self {
        let mut catalog = Self::default();
        let embedded = catalog.ingest_embedded();
        tracing::info!(offerings = embedded, "embedded approved catalog loaded");

        let mut added = 0usize;
        let mut overrode = 0usize;
        if let Some(base) = base {
            if base.is_dir() {
                let (a, o) = catalog.ingest_counting(base);
                added += a;
                overrode += o;
            } else {
                tracing::info!(
                    root = %base.display(),
                    "operator catalog absent; the floor carries it"
                );
            }
        }
        for overlay in overlays {
            if !overlay.is_dir() {
                tracing::info!(overlay = %overlay.display(), "overlay absent; nothing applied");
                continue;
            }
            let (a, o) = catalog.ingest_counting(overlay);
            added += a;
            overrode += o;
        }
        tracing::info!(overlays = overlays.len(), added, overrode, "catalog layers applied");
        catalog
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
