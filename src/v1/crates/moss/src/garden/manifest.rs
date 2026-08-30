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
    /// Detection intent (OFFERINGS.md §5.1): how the stone recognizes
    /// this offering already running on the host. Parsed by the detection
    /// domain (offerings::detect); rides the ADR-0008 catalog layers like
    /// every other manifest section.
    pub adopted: Option<AdoptIntent>,
    pub borrowed: Option<serde_yaml::Value>,
    #[serde(default)]
    pub compatibility: Vec<CompatRule>,
    /// Declared install form (OFFERINGS.md §5.1): ask/secret/default.
    #[serde(default)]
    pub inputs: BTreeMap<String, InputField>,
    /// Capability types this offering holds (OFFERINGS.md §5.1 — the
    /// reserved `capabilities:` grammar, unparked). Observed read-only by
    /// the stone's capability sweep; W1 ships the list channel only.
    #[serde(default)]
    pub capabilities: Vec<CapabilityDecl>,
    /// The living will (ADR-0005 §1): how this offering asks to be
    /// remembered. Lifecycle intent — never hashed into plans (§7).
    #[serde(default)]
    pub capture: Option<super::will::policy::CapturePolicy>,
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

/// One capability type an offering holds (W1: observe-only; the
/// add/remove/upgrade operations wait for W2's job semantics — L11).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDecl {
    /// Capability type identifier ("model"). Lowercase plain name; the
    /// same word a wish selector speaks (`ollama[model:llama3]`).
    pub r#type: String,
    /// The wish shorthand's default type when an offering declares
    /// several. At most one per manifest.
    #[serde(default)]
    pub default: bool,
    /// How to observe what this offering holds right now.
    pub list: CapabilityList,
    /// How to GROW this capability (W2): an in-container command; `{{item}}`
    /// is replaced by the capability's name. Long by nature (model pulls) —
    /// runs as a journaled job, never inline.
    #[serde(default)]
    pub add: Option<CapabilityMutation>,
    /// How to remove one item (W2). Same grammar, short budget.
    #[serde(default)]
    pub remove: Option<CapabilityMutation>,
}

/// A mutation channel (W2): exec only — a mutation is the garden
/// operating its OWN placed work, and exec into the offering's container
/// is the one world-honest way to do that. `{{item}}` must appear at
/// least once: a command that ignores what it was told to touch is a
/// lie, and lies are refused at load.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityMutation {
    pub exec: Vec<String>,
    #[serde(default = "default_add_timeout")]
    pub timeout_secs: u64,
}

/// PoC parity: model pulls run for hours (capabilities.rs:224).
fn default_add_timeout() -> u64 {
    7200
}

/// The list channel: exactly ONE of exec or http (validated at load).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityList {
    /// Run inside the offering's container (HookRunner seam); stdout is
    /// the payload. Read-only inspection — observation, not operation.
    #[serde(default)]
    pub exec: Option<Vec<String>>,
    /// GET `http://localhost:<offering port><path>`; the JSON answer is
    /// the payload. Localhost only — a capability endpoint is the
    /// offering's own self-description.
    #[serde(default)]
    pub http: Option<HttpList>,
}

/// The http channel's JSON grammar: where the item array lives and which
/// field of each element names the item. Dot notation, explicit beats
/// clever — no transform DSL (gate-4 ruling).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpList {
    pub path: String,
    /// Dot path to the item array (e.g. "models").
    pub item_path: String,
    /// Dot path to the item's name within each element (e.g. "name").
    pub value_path: String,
}

/// Detection rules for the adopted mode: regular expressions matched
/// against the host world's container facts (name, image). The stem is
/// implicit — the manifest IS the stem — so no name rule is needed here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptIntent {
    /// Regex over the world's container name ("ollama", "ollama-1").
    #[serde(default)]
    pub container_name_pattern: Option<String>,
    /// Regex over the image reference ("ollama/ollama:0.5").
    #[serde(default)]
    pub image_pattern: Option<String>,
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

    /// Detection rules (adopted mode): typed, at least one pattern, and
    /// every pattern a valid regex — a rule matching everything would
    /// adopt the whole host, and a broken regex must die at load, never
    /// mid-sweep.
    #[test]
    fn adopt_rules_need_a_pattern_and_a_valid_regex() {
        let good = GOOD.replace(
            "managed:",
            "adopted:\n  container_name_pattern: '^redis(-.+)?$'\n  image_pattern: '^redis:'\nmanaged:",
        );
        let adopt = Catalog::parse("redis", &good).unwrap().adopted.unwrap();
        assert_eq!(adopt.container_name_pattern.as_deref(), Some("^redis(-.+)?$"));
        assert_eq!(adopt.image_pattern.as_deref(), Some("^redis:"));

        // Neither pattern: refused.
        let empty = GOOD.replace("managed:", "adopted: {}\nmanaged:");
        assert!(
            Catalog::parse("redis", &empty).is_err(),
            "a patternless rule would adopt the whole host"
        );

        // A pattern that cannot compile is a load-time refusal.
        let bad = GOOD.replace(
            "managed:",
            "adopted:\n  container_name_pattern: '^redis('\nmanaged:",
        );
        assert!(Catalog::parse("redis", &bad).is_err());

        // Detection alone is a legal manifest: recognizable without being
        // placeable.
        let adopt_only = "\
kind: software
name: redis
category: data
description: d
adopted:
  container_name_pattern: '^redis'
";
        let m = Catalog::parse("redis", adopt_only).unwrap();
        assert!(m.managed.is_none() && m.adopted.is_some());
    }

    /// Capability declarations (W1): one list channel per type, plain
    /// lowercase types, at most one default, no duplicates — all refused
    /// at load, never mid-sweep.
    #[test]
    fn capability_declarations_are_validated_at_load() {
        let good = GOOD.replace(
            "managed:",
            "capabilities:
  - type: model
    default: true
    list:
      http: { path: /api/tags, item_path: models, value_path: name }
managed:",
        );
        let m = Catalog::parse("redis", &good).unwrap();
        assert_eq!(m.capabilities.len(), 1);
        assert_eq!(m.capabilities[0].r#type, "model");
        assert!(m.capabilities[0].default);

        // Two channels: refused.
        let both = good.replace(
            "      http: { path: /api/tags, item_path: models, value_path: name }",
            "      http: { path: /api/tags, item_path: models, value_path: name }
      exec: [list, things]",
        );
        assert!(Catalog::parse("redis", &both).is_err());

        // Neither channel: refused.
        let none = good.replace(
            "      http: { path: /api/tags, item_path: models, value_path: name }",
            "      http: null",
        );
        assert!(Catalog::parse("redis", &none).is_err());

        // Duplicate type and two defaults: refused.
        let dup = GOOD.replace(
            "managed:",
            "capabilities:
  - type: model
    list: { exec: [a] }
  - type: model
    list: { exec: [b] }
managed:",
        );
        assert!(Catalog::parse("redis", &dup).is_err());
        let twodefaults = GOOD.replace(
            "managed:",
            "capabilities:
  - type: model
    default: true
    list: { exec: [a] }
  - type: other
    default: true
    list: { exec: [b] }
managed:",
        );
        assert!(Catalog::parse("redis", &twodefaults).is_err());

        // Uppercase type: refused.
        let upper = GOOD.replace(
            "managed:",
            "capabilities:
  - type: Model
    list: { exec: [a] }
managed:",
        );
        assert!(Catalog::parse("redis", &upper).is_err());

        // A mutation that never names {{item}} ignores what it was told
        // to touch — refused at load.
        let noitem = GOOD.replace(
            "managed:",
            "capabilities:\n  - type: model\n    list: { exec: [a] }\n    add: { exec: [ollama, pull] }\nmanaged:",
        );
        assert!(Catalog::parse("redis", &noitem).is_err());

        // With {{item}} it loads; timeout rides.
        let withitem = GOOD.replace(
            "managed:",
            "capabilities:\n  - type: model\n    list: { exec: [a] }\n    add: { exec: [ollama, pull, \"{{item}}\"], timeout_secs: 60 }\nmanaged:",
        );
        let m = Catalog::parse("redis", &withitem).unwrap();
        assert_eq!(m.capabilities[0].add.as_ref().unwrap().timeout_secs, 60);
    }

    /// D15's dead debt stays dead: a manifest with volumes MUST state
    /// its living will. An undeclared capture on stateful work is
    /// untrusted-consistency - never silently tarred, and never silent.
    #[test]
    fn every_stateful_manifest_declares_its_living_will() {
        let catalog = Catalog::load_fully_layered(None, &[]);
        let undeclared: Vec<String> = catalog
            .names()
            .into_iter()
            .filter(|name| {
                let m = catalog.get(name).expect("named entry exists");
                m.managed
                    .as_ref()
                    .map(|mi| !mi.volumes.is_empty())
                    .unwrap_or(false)
                    && m.capture.is_none()
            })
            .collect();
        assert!(
            undeclared.is_empty(),
            "stateful manifests without a living will: {undeclared:?}"
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
        // Detection rules: at least one pattern (a rule matching
        // everything would adopt the whole host), and every pattern must
        // compile — a broken regex dies at load, never at sweep time.
        if let Some(adopt) = &m.adopted {
            if adopt.container_name_pattern.is_none() && adopt.image_pattern.is_none() {
                return Err(format!(
                    "manifest '{}': adopted section declares no patterns — \
                     give a container_name_pattern or an image_pattern",
                    m.name
                ));
            }
            for pattern in [&adopt.container_name_pattern, &adopt.image_pattern]
                .into_iter()
                .flatten()
            {
                if let Err(e) = regex::Regex::new(pattern) {
                    return Err(format!(
                        "manifest '{}': adopted pattern '{pattern}' is not a valid regex: {e}",
                        m.name
                    ));
                }
            }
        }
        // Capability declarations: plain lowercase types, exactly one
        // list channel each, at most one default type, no duplicates —
        // every refusal dies at load, never mid-sweep (§5.1 law).
        let mut default_types = 0usize;
        let mut seen_types = std::collections::BTreeSet::new();
        for cap in &m.capabilities {
            let valid_name = !cap.r#type.is_empty()
                && cap
                    .r#type
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !valid_name {
                return Err(format!(
                    "manifest '{}': capability type '{}' must be a lowercase plain name",
                    m.name, cap.r#type
                ));
            }
            if !seen_types.insert(cap.r#type.clone()) {
                return Err(format!(
                    "manifest '{}': capability type '{}' declared twice",
                    m.name, cap.r#type
                ));
            }
            if cap.default {
                default_types += 1;
            }
            let channels =
                cap.list.exec.is_some() as usize + cap.list.http.is_some() as usize;
            if channels != 1 {
                return Err(format!(
                    "manifest '{}': capability '{}' list needs exactly one channel (exec or http)",
                    m.name, cap.r#type
                ));
            }
            if let Some(http) = &cap.list.http {
                if !http.path.starts_with('/') {
                    return Err(format!(
                        "manifest '{}': capability '{}' http path must start with '/'",
                        m.name, cap.r#type
                    ));
                }
                for p in [&http.item_path, &http.value_path] {
                    if p.is_empty()
                        || p.starts_with('.')
                        || p.ends_with('.')
                        || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                    {
                        return Err(format!(
                            "manifest '{}': capability '{}' json path '{p}' is not a dot path",
                            m.name, cap.r#type
                        ));
                    }
                }
            }
            for (op, mutation) in [("add", &cap.add), ("remove", &cap.remove)] {
                let Some(mutation) = mutation else { continue };
                if mutation.exec.is_empty() {
                    return Err(format!(
                        "manifest '{}': capability '{}' {op} exec is empty",
                        m.name, cap.r#type
                    ));
                }
                if mutation.timeout_secs == 0 {
                    return Err(format!(
                        "manifest '{}': capability '{}' {op} timeout must be positive",
                        m.name, cap.r#type
                    ));
                }
                if !mutation.exec.iter().any(|arg| arg.contains("{{item}}")) {
                    return Err(format!(
                        "manifest '{}': capability '{}' {op} never names {{{{item}}}} — it would ignore what it was told to touch",
                        m.name, cap.r#type
                    ));
                }
            }
        }
        if default_types > 1 {
            return Err(format!(
                "manifest '{}': at most one capability type may be default",
                m.name
            ));
        }
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
