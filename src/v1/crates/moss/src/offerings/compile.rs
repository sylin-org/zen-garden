// O2 wires the compiler into plant (§5.3 placed records); fixtures pin it.
#![allow(dead_code)]

//! The compiler (OFFERINGS.md §5): manifest × stone facts × inputs →
//! PlacementPlan. Compatibility evaluation lives in evaluate.rs; this
//! module sequences it and shapes the plan.

use super::evaluate::{self};
use super::facts::Generation;
use super::manifest::{Decide, Manifest};
use crate::offerings::model::WorkloadSpec;
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
pub fn compile(
    m: &Manifest,
    facts_gen: &Generation,
    inputs: &BTreeMap<String, String>,
    volumes_root: &std::path::Path,
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
            host_path: volumes_root
                .join(&m.name)
                .join(&v.name)
                .to_string_lossy()
                .into_owned(),
            container_path: v.mount.clone(),
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

    // Advanced bag passes through as raw YAML→JSON (adapters may consume).
    if !managed.advanced.is_null()
        && let Ok(v) = serde_json::to_value(&managed.advanced)
    {
        workload.advanced = v;
    }

    let decisions: Vec<Decision> = report
        .outcomes
        .iter()
        .filter(|o| o.result == "matched")
        .map(|o| Decision {
            rule: o.rule.clone(),
            chose: if o.decide == Decide::Fallback { format!("image -> {}", report.image) } else { "place".into() },
            because: o.because.clone(),
            source: o.source.clone(),
        })
        .collect();

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
