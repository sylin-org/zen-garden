// O2 wires the compiler into plant (§5.3 placed records); fixtures pin it.
#![allow(dead_code)]

//! The compiler (OFFERINGS.md §5): manifest × stone facts × inputs →
//! PlacementPlan. Compatibility evaluation lives in evaluate.rs; this
//! module sequences it and shapes the plan.

use super::evaluate::{self};
use super::directory::OfferingDir;
use super::facts::Generation;
use super::manifest::{Decide, Manifest};
use super::ports::{self, Claim, Intent, Pool, Tier};
use crate::offerings::model::{PortAllocation, WorkloadSpec};
use serde::Serialize;
use std::collections::BTreeMap;

/// Why compile refused.
#[derive(Debug)]
pub enum CompileError {
    /// Not a placeable manifest (no managed section).
    NotPlaceable(String),
    /// Compatibility said no.
    Denied { because: String, suggest: Option<String> },
    /// Manifest or inputs malformed.
    Invalid(String),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPlaceable(n) => write!(f, "'{n}' declares no managed placement"),
            Self::Denied { because, suggest } => {
                write!(f, "compatibility denied: {because}")?;
                if let Some(s) = suggest {
                    write!(f, " — {s}")?;
                }
                Ok(())
            }
            Self::Invalid(e) => write!(f, "{e}"),
        }
    }
}

/// One recorded choice (the decision log's atom).
#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub rule: String,
    pub chose: String,
    pub because: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The resolved instructions for THIS stone — stored inside ManagedData,
/// hashed for drift detection, rendered by `rake explain`.
#[derive(Debug, Clone, Serialize)]
pub struct PlacementPlan {
    pub workload: WorkloadSpec,
    pub decisions: Vec<Decision>,
    pub plan_hash: u64,
    pub facts_generation: u64,
}

/// Replace `${input.k}` tokens in every string leaf of a JSON value.
fn substitute(v: &mut serde_json::Value, inputs: &BTreeMap<String, String>) {
    match v {
        serde_json::Value::String(s) => {
            for (k, val) in inputs {
                let token = format!("${{{k}}}");
                if s.contains(&token) {
                    *s = s.replace(&token, val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for i in items {
                substitute(i, inputs);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, child) in map.iter_mut() {
                substitute(child, inputs);
            }
        }
        _ => {}
    }
}

/// Compile a manifest into a plan against current facts.
///
/// `claims` is the stone's address ledger (every other offering's held
/// ports — rested and adopted alike) and `pool` the stone's service pool;
/// both feed the ADR-0002 allocator BEFORE any socket is touched. The
/// resulting homes ride inside `workload.allocations`, hashed into
/// `plan_hash`, so drift detection sees addresses as part of intent.
#[allow(clippy::too_many_arguments)]
pub fn compile(
    m: &Manifest,
    facts_gen: &Generation,
    inputs: &BTreeMap<String, String>,
    dir: &OfferingDir,
    claims: &[Claim],
    pool: Pool,
) -> Result<PlacementPlan, CompileError> {
    let Some(managed) = &m.managed else {
        return Err(CompileError::NotPlaceable(m.name.clone()));
    };

    // Inputs: declared defaults fill gaps; unknown provided keys are ignored
    // here (validated against `inputs` at the surface).
    let mut effective_inputs = BTreeMap::new();
    for (k, def) in &m.inputs {
        let value = inputs.get(k).cloned().or_else(|| def.default.clone()).unwrap_or_default();
        effective_inputs.insert(k.clone(), value);
    }

    let report = evaluate::evaluate(&m.compatibility, facts_gen, &managed.image);
    if report.decision == Decide::Deny {
        let denied = report
            .outcomes
            .iter()
            .find(|o| o.result == "matched" && o.decide == Decide::Deny);
        return Err(CompileError::Denied {
            because: denied.map(|d| d.because.clone()).unwrap_or_else(|| "incompatible".into()),
            suggest: denied.and_then(|d| d.suggest.clone()),
        });
    }

    // Spec from intent, image possibly swapped by compatibility.
    let mut workload = WorkloadSpec {
        image: report.image.clone(),
        named_ports: managed.ports.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        ..Default::default()
    };
    workload.restart = managed.restart.clone();
    for v in &managed.volumes {
        workload.volumes.push(crate::offerings::model::VolumeMount {
            host_path: dir.volumes().join(&v.name).to_string_lossy().into_owned(),
            container_path: v.mount.clone(),
        });
    }
    // Config materialization: declared shape → rendered content, staged
    // inside the offering directory's configs/ (OFFERINGS.md §5).
    for cfg in &managed.config_files {
        let mount = cfg.get("mount").and_then(|x| x.as_str()).unwrap_or("");
        let format = cfg.get("format").and_then(|x| x.as_str()).unwrap_or("raw");
        if mount.is_empty() {
            continue;
        }
        let content = match format {
            "yaml" => "# Managed by Zen Garden\n{}\n",
            "json" => "{}\n",
            "toml" => "# Managed by Zen Garden\n",
            "ini" => "; Managed by Zen Garden\n",
            _ => "",
        };
        let file_name = mount.split('/').next_back().unwrap_or("config");
        workload.configs.push(crate::offerings::model::ConfigMount {
            host_path: dir.configs().join(file_name).to_string_lossy().into_owned(),
            container_path: mount.to_string(),
            content: content.into(),
        });
    }
    for (k, v) in &managed.env {
        workload.env.insert(k.clone(), v.clone());
    }

    // Input substitution across every string leaf of the spec.
    let mut spec_value = serde_json::to_value(&workload)
        .map_err(|e| CompileError::Invalid(format!("spec encode: {e}")))?;
    substitute(&mut spec_value, &effective_inputs);
    let mut workload: WorkloadSpec = serde_json::from_value(spec_value)
        .map_err(|e| CompileError::Invalid(format!("spec decode after inputs: {e}")))?;

    // ---- Addresses: the ledger speaks before any socket (ADR-0002) --------
    // Every container role gets a stable HOME — declared pins honored,
    // everything else drawn from the stone's service pool. This is pure
    // policy: reality is probed later at the create edge.
    let mut intents = BTreeMap::new();
    for role in managed.ports.keys() {
        let intent = managed
            .host_ports
            .get(role)
            .map(|d| Intent {
                tier: if d.strict { Tier::Strict } else { Tier::Soft },
                home: Some(d.port),
            })
            .unwrap_or(Intent {
                tier: Tier::Flexible,
                home: None,
            });
        intents.insert(role.clone(), intent);
    }
    let homes = match ports::allocate(&intents, claims, pool) {
        Ok(h) => h,
        Err(super::ports::AllocError::ClaimConflict { port, holder }) => {
            return Err(CompileError::Denied {
                because: format!("host port {port} is held by garden member '{holder}'"),
                suggest: Some(
                    "uproot or rest that offering first, or pick another pinned port".into(),
                ),
            });
        }
        Err(e) => {
            return Err(CompileError::Invalid(format!(
                "address allocation refused: {e}"
            )));
        }
    };
    workload.allocations = homes
        .iter()
        .map(|(role, home)| {
            (
                role.clone(),
                PortAllocation {
                    home: *home,
                    tier: intents[role].tier,
                },
            )
        })
        .collect();

    // Advanced bag passes through as raw YAML→JSON (adapters may consume).
    if !managed.advanced.is_null()
        && let Ok(v) = serde_json::to_value(&managed.advanced)
    {
        workload.advanced = v;
    }

    // §5.2/§6.4: the plan records EVERY compatibility outcome — passing
    // rules included — so explain shows the whole evaluation, blind spots
    // included, not just refusals. `chose` names the outcome.
    let mut decisions: Vec<Decision> = report
        .outcomes
        .iter()
        .map(|o| Decision {
            rule: o.rule.clone(),
            chose: if o.result == "matched" {
                if o.decide == Decide::Fallback {
                    format!("image -> {}", report.image)
                } else {
                    "place".into()
                }
            } else {
                o.result.into()
            },
            because: o.because.clone(),
            source: o.source.clone(),
        })
        .collect();

    // §5.2 records EVERY choice — port assignments included (ADR-0002):
    // each role's home lands in the log with its provenance, so explain
    // answers "why :7300?" without archaeology.
    for (role, home) in &homes {
        decisions.push(Decision {
            rule: format!("address.{role}"),
            chose: home.to_string(),
            because: allocation_reason(&intents[role], *home),
            source: Some("ledger".into()),
        });
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    serde_json::to_string(&workload)
        .map_err(|e| CompileError::Invalid(format!("plan encode: {e}")))?
        .hash(&mut hasher);
    let plan_hash = hasher.finish();

    Ok(PlacementPlan {
        workload,
        decisions,
        plan_hash,
        facts_generation: facts_gen.id,
    })
}

/// Why a role holds its home — the decision log's address provenance.
fn allocation_reason(intent: &Intent, home: u16) -> String {
    match (intent.tier, intent.home) {
        (Tier::Strict, Some(pin)) if pin == home => {
            format!("identity-critical host pin :{pin} per manifest")
        }
        (Tier::Soft, Some(pin)) if pin == home => {
            format!("preferred host home :{pin} per manifest")
        }
        (Tier::Soft, Some(pin)) => {
            format!("preferred home :{pin} unavailable in the ledger; service pool draw")
        }
        _ => "service pool draw".to_string(),
    }
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::facts::{Contributor, FactValue, Factsheet};
    use crate::offerings::manifest::Catalog;
    use std::collections::BTreeMap;

    struct Static(BTreeMap<String, FactValue>);

    #[async_trait::async_trait]
    impl Contributor for Static {
        fn concern(&self) -> &'static str { "static" }
        async fn measure(&self) -> BTreeMap<String, FactValue> { self.0.clone() }
    }

    const MDB: &str = include_str!("../../../../catalog/sw/data/mongodb.offering.yaml");

    #[tokio::test]
    async fn mongodb_compiles_with_config_and_volume_paths() {
        let manifest = Catalog::parse("mongodb", MDB).unwrap();
        assert!(!manifest.managed.as_ref().unwrap().config_files.is_empty(),
            "fixture must declare config_files");

        let factsheet = Factsheet::empty();
        let mut nodes = BTreeMap::new();
        nodes.insert("machine.architecture".to_string(), serde_json::json!("x86_64"));
        nodes.insert(
            "cpu.features".to_string(),
            serde_json::json!(["sse4_2", "avx"]),
        );
        nodes.insert("ram.total.bytes".to_string(), serde_json::json!(8589934592u64));
        factsheet
            .collect(&[std::sync::Arc::new(Static(nodes))])
            .await;

        let facts_snapshot = factsheet.snapshot();
        let inputs = BTreeMap::new();
        let dir = crate::offerings::directory::OfferingsRoot::new(std::path::PathBuf::from(
            ".zen-garden-probe",
        ))
        .dir_for("mongodb");
        let plan = compile(&manifest, &facts_snapshot, &inputs, &dir, &[], Pool::default())
            .unwrap();

        assert_eq!(plan.workload.image, "mongo:7", "AVX present: no fallback");
        // §5.2: EVERY compatibility outcome rides in the plan — passing
        // rules included — so explain shows the whole evaluation. With
        // AVX+sse4_2 present and 8GB RAM, all three refusals are live
        // candidates evaluated as no_match.
        let by_rule: BTreeMap<String, String> = plan
            .decisions
            .iter()
            .map(|d| (d.rule.clone(), d.chose.clone()))
            .collect();
        assert_eq!(by_rule.len(), 4, "{by_rule:?}");
        assert_eq!(by_rule["missing-avx-feature"], "no_match");
        assert_eq!(by_rule["missing-sse42-feature"], "no_match");
        assert_eq!(by_rule["insufficient-memory"], "no_match");

        // ADR-0002: even a pin-less manifest leaves the stone a ledgered
        // home — the pool's lowest free slot, recorded as a decision.
        let first_slot = Pool::default().start.to_string();
        assert_eq!(
            plan.workload.allocations["default"].home,
            Pool::default().start
        );
        assert_eq!(by_rule["address.default"], first_slot);

        let cfg = plan.workload.configs.first().expect("config must materialize");
        assert!(
            cfg.host_path.ends_with("mongod.conf"),
            "host_path={}",
            cfg.host_path
        );
        assert_eq!(cfg.container_path, "/etc/mongod.conf");
        assert!(!cfg.content.is_empty());

        let vol = plan.workload.volumes.first().unwrap();
        assert!(
            vol.host_path.contains(&format!("volumes{}mongo-data", std::path::MAIN_SEPARATOR)),
            "host_path={}",
            vol.host_path
        );

        // Cleanup probe artifacts.
        let _ = std::fs::remove_dir_all(dir.root.clone());
    }

    const SMALL: &str = "\
kind: software
name: small
category: misc
description: Rules all present
managed:
  world: oci
  image: small:1
compatibility:
  - name: tiny-ram-noted
    when: [{ path: ram.total.mb, op: lt, value: 128 }]
    decide: place
    because: tiny machines deserve caching too
  - name: no-unknown-facts-here
    when: [{ path: gpu.present, op: eq, value: true }]
    decide: deny
    because: gpu stones should use the GPU offering
";

    /// Tri-state fidelity (§6.4): matched, unknown and the absent-rule
    /// outcomes each land in the plan under their own chose label.
    #[tokio::test]
    async fn plan_records_every_outcome_state() {
        let manifest = Catalog::parse("small", SMALL).unwrap();

        // Facts: ram BELOW 128 MB (canonical bytes) so tiny-ram matches;
        // gpu.present deliberately ABSENT (tri-state coverage).
        let factsheet = Factsheet::empty();
        let mut nodes = BTreeMap::new();
        nodes.insert("ram.total.bytes".to_string(), serde_json::json!(67108864u64));
        factsheet
            .collect(&[std::sync::Arc::new(Static(nodes))])
            .await;
        let facts_snapshot = factsheet.snapshot();

        let inputs = BTreeMap::new();
        let dir = crate::offerings::directory::OfferingsRoot::new(std::path::PathBuf::from(
            ".zen-garden-probe",
        ))
        .dir_for("small");
        let plan = compile(&manifest, &facts_snapshot, &inputs, &dir, &[], Pool::default())
            .unwrap();

        let by_rule: BTreeMap<String, String> = plan
            .decisions
            .iter()
            .map(|d| (d.rule.clone(), d.chose.clone()))
            .collect();
        assert_eq!(by_rule.len(), 2, "both outcomes recorded: {by_rule:?}");
        assert_eq!(by_rule["tiny-ram-noted"], "place");
        // The gpu rule addresses an unprobed fact — unknown, never folded.
        assert_eq!(by_rule["no-unknown-facts-here"], "unknown");
        // No ports section ⇒ no roles ⇒ no addresses to allocate.
        assert!(plan.workload.allocations.is_empty());

        let _ = std::fs::remove_dir_all(dir.root.clone());
    }

    const PINNED: &str = "\
kind: software
name: pinned
category: net
description: Host-port grammar exercised end to end
managed:
  world: oci
  image: pinned:1
  ports: { dns: 53, ui: 8080 }
  host_ports:
    dns: { port: 53, strict: true }
    ui: { port: 8080 }
";

    /// ADR-0002 end-to-end through compile: strict pin honored exactly,
    /// soft pin honored when free — both carrying their provenance into
    /// the decision log and the hashed workload.
    #[tokio::test]
    async fn pins_land_as_allocations_with_provenance() {
        let manifest = Catalog::parse("pinned", PINNED).unwrap();
        let factsheet = Factsheet::empty();
        let nodes = BTreeMap::new();
        factsheet
            .collect(&[std::sync::Arc::new(Static(nodes))])
            .await;
        let snapshot = factsheet.snapshot();
        let dir = crate::offerings::directory::OfferingsRoot::new(std::path::PathBuf::from(
            ".zen-garden-probe",
        ))
        .dir_for("pinned");

        let plan = compile(&manifest, &snapshot, &BTreeMap::new(), &dir, &[], Pool::default())
            .unwrap();
        assert_eq!(plan.workload.allocations["dns"].home, 53);
        assert_eq!(plan.workload.allocations["dns"].tier, Tier::Strict);
        assert_eq!(plan.workload.allocations["ui"].home, 8080);
        assert_eq!(plan.workload.allocations["ui"].tier, Tier::Soft);

        let by_rule: BTreeMap<String, String> = plan
            .decisions
            .iter()
            .map(|d| (d.rule.clone(), d.chose.clone()))
            .collect();
        assert_eq!(by_rule["address.dns"], "53");
        assert_eq!(by_rule["address.ui"], "8080");
        let why_ui = plan
            .decisions
            .iter()
            .find(|d| d.rule == "address.ui")
            .map(|d| d.because.clone())
            .unwrap_or_default();
        assert!(why_ui.contains("preferred"), "{why_ui}");

        let _ = std::fs::remove_dir_all(dir.root.clone());
    }

    /// A strict pin colliding with a LEDGER claim refuses loudly at compile
    /// time, naming the holder — before any socket is ever touched.
    #[tokio::test]
    async fn strict_pin_versus_ledger_claim_denies_with_holder_named() {
        let manifest = Catalog::parse("pinned", PINNED).unwrap();
        let factsheet = Factsheet::empty();
        let nodes = BTreeMap::new();
        factsheet
            .collect(&[std::sync::Arc::new(Static(nodes))])
            .await;
        let claims = vec![Claim::new("technitium", 53)];
        let dir = crate::offerings::directory::OfferingsRoot::new(std::path::PathBuf::from(
            ".zen-garden-probe",
        ))
        .dir_for("pinned2");

        let err = compile(
            &manifest,
            &factsheet.snapshot(),
            &BTreeMap::new(),
            &dir,
            &claims,
            Pool::default(),
        )
        .unwrap_err();
        match err {
            CompileError::Denied { because, .. } => {
                assert!(because.contains("'technitium'"), "{because}");
            }
            other => {
                assert!(
                    matches!(other, CompileError::Denied { .. }),
                    "expected a refusal naming the holder, got {other:?}"
                );
            }
        }

        let _ = std::fs::remove_dir_all(dir.root.clone());
    }
}