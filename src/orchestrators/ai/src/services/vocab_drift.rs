//! Vocabulary drift auditor (§ADR Acceptance-9).
//!
//! Walks an [`Output`] produced by a provider and reports any keys
//! that aren't declared in the corresponding primitive's output
//! vocabulary (or in one of the opted-in shared namespaces, or in the
//! `x_*` passthrough escape hatch).
//!
//! The auditor is intentionally a *report*, not a build gate. It runs
//! per-provider, accumulates findings across a suite of test
//! requests, and returns a structured [`DriftReport`] callers can
//! serialize for CI artifacts. Empty reports mean the provider is
//! perfectly aligned with its declared vocabulary.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Serialize;

use crate::domain::ids::ProviderName;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{SharedNamespace, Vocabulary};

/// Per-provider drift report assembled across many test calls.
#[derive(Debug, Default, Clone, Serialize)]
pub struct DriftReport {
    pub provider: String,
    pub primitive: String,
    /// Sorted union of every drifting key seen across the request suite.
    pub drifting_keys: BTreeSet<String>,
    /// Sorted union of every key seen, including the well-known ones.
    pub all_observed_keys: BTreeSet<String>,
    /// How many requests fed into this report.
    pub samples: u64,
}

impl DriftReport {
    pub fn new(provider: &ProviderName, primitive: Primitive) -> Self {
        Self {
            provider: provider.as_str().to_string(),
            primitive: primitive.dotted().to_string(),
            drifting_keys: BTreeSet::new(),
            all_observed_keys: BTreeSet::new(),
            samples: 0,
        }
    }

    /// Fold one provider response into the rolling report.
    pub fn observe(&mut self, output: &Output, vocabulary: &Vocabulary) {
        self.samples += 1;
        let known = build_known_set(vocabulary);
        let shared = shared_namespace_prefixes(vocabulary);
        for key in collect_keys(output) {
            self.all_observed_keys.insert(key.clone());
            if is_drift(&key, &known, &shared) {
                self.drifting_keys.insert(key);
            }
        }
    }

    pub fn is_clean(&self) -> bool {
        self.drifting_keys.is_empty()
    }
}

/// Multi-provider audit. Maps `(provider, primitive)` to its rolling
/// report so a CI step can dump all of it as one artifact.
#[derive(Debug, Default, Clone, Serialize)]
pub struct DriftAudit {
    pub reports: BTreeMap<String, DriftReport>,
}

impl DriftAudit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one observation into the audit. The report key is
    /// `<provider>::<primitive>` so the same provider can be audited
    /// across multiple primitives in one report file.
    pub fn observe(
        &mut self,
        provider: &ProviderName,
        primitive: Primitive,
        output: &Output,
        vocabulary: &Vocabulary,
    ) {
        let key = format!("{}::{}", provider.as_str(), primitive.dotted());
        let report = self
            .reports
            .entry(key)
            .or_insert_with(|| DriftReport::new(provider, primitive));
        report.observe(output, vocabulary);
    }

    pub fn drifting_reports(&self) -> impl Iterator<Item = &DriftReport> {
        self.reports.values().filter(|r| !r.is_clean())
    }

    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| format!("{{\"error\": \"serialization failed: {e}\"}}"))
    }
}

// ── helpers ────────────────────────────────────────────────────

fn build_known_set(vocabulary: &Vocabulary) -> HashSet<String> {
    let mut known = HashSet::new();
    for spec in vocabulary
        .output
        .required
        .iter()
        .chain(vocabulary.output.optional.iter())
    {
        known.insert(spec.path.as_str().to_string());
    }
    known
}

fn shared_namespace_prefixes(vocabulary: &Vocabulary) -> Vec<&'static str> {
    vocabulary
        .output
        .shared_namespaces
        .iter()
        .map(|ns| match ns {
            SharedNamespace::Usage => "usage",
            SharedNamespace::Timing => "timing",
            SharedNamespace::Meta => "meta",
            SharedNamespace::Job => "job",
            SharedNamespace::Stream => "stream",
        })
        .collect()
}

fn is_drift(key: &str, known: &HashSet<String>, shared_prefixes: &[&'static str]) -> bool {
    if known.contains(key) {
        return false;
    }
    if shared_prefixes
        .iter()
        .any(|p| key == *p || key.starts_with(&format!("{p}.")))
    {
        return false;
    }
    // `x_`-prefixed segments are explicitly passthrough per the ADR.
    if key.starts_with("x_") || key.contains(".x_") {
        return false;
    }
    true
}

fn collect_keys(output: &Output) -> Vec<String> {
    let value = serde_json::to_value(output).unwrap_or(serde_json::Value::Null);
    let mut keys = Vec::new();
    walk(&String::new(), &value, &mut keys);
    keys
}

fn walk(prefix: &str, value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(map) = value.as_object() {
        for (k, v) in map {
            let full = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if v.is_object() {
                walk(&full, v, out);
            } else {
                out.push(full);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::field_path::FieldPath;
    use crate::domain::keys;
    use crate::domain::vocabulary::VocabularyRegistry;

    fn text_chat_vocab() -> Vocabulary {
        VocabularyRegistry::build().get(Primitive::TextChat).clone()
    }

    #[test]
    fn declared_keys_are_not_drift() {
        let mut report = DriftReport::new(&ProviderName::new("p"), Primitive::TextChat);
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "hi");
        out.set(&keys::text::FINISH_REASON, "stop");
        out.set(&keys::usage::TOKENS_INPUT, 1);
        report.observe(&out, &text_chat_vocab());
        assert!(report.is_clean(), "expected clean, got {:?}", report.drifting_keys);
        assert_eq!(report.samples, 1);
    }

    #[test]
    fn unknown_key_is_drift_but_x_prefix_is_passthrough() {
        let mut report = DriftReport::new(&ProviderName::new("p"), Primitive::TextChat);
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, "hi");
        let drift = FieldPath::parse("text.ghost_field").unwrap();
        out.set(&drift, "leak");
        let xpass = FieldPath::parse("text.x_provider_extension").unwrap();
        out.set(&xpass, "ok");
        report.observe(&out, &text_chat_vocab());
        assert!(report.drifting_keys.contains("text.ghost_field"));
        assert!(!report.drifting_keys.contains("text.x_provider_extension"));
    }

    #[test]
    fn multi_sample_audit_unions_drift() {
        let mut audit = DriftAudit::new();
        let provider = ProviderName::new("p");
        let mut a = Output::new();
        let drift_a = FieldPath::parse("text.first_extra").unwrap();
        a.set(&drift_a, 1);
        let mut b = Output::new();
        let drift_b = FieldPath::parse("text.second_extra").unwrap();
        b.set(&drift_b, 2);
        audit.observe(&provider, Primitive::TextChat, &a, &text_chat_vocab());
        audit.observe(&provider, Primitive::TextChat, &b, &text_chat_vocab());
        let report = audit.reports.get("p::text.chat").expect("report exists");
        assert_eq!(report.samples, 2);
        assert!(report.drifting_keys.contains("text.first_extra"));
        assert!(report.drifting_keys.contains("text.second_extra"));
    }
}
