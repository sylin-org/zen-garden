//! The compiler's evaluation half (OFFERINGS.md §6.4): compatibility rules
//! are pure data evaluated against a facts generation. Order-independent
//! severity — deny > fallback > place — and every rule lands in the
//! decision report, matched or not.
//!
//! Unit law: stored facts are CANONICAL (bytes). A rule path's final
//! segment may name an operand unit (`ram.total.mb` = "compare in MB");
//! the operand is converted to bytes before comparison. Unknown suffixes
//! fail at manifest validation, not here.

use super::facts::Generation;
use super::manifest::{CompatRule, CondOp, Decide};
use serde::Serialize;

/// One rule's outcome, recorded even when it didn't match.
#[derive(Debug, Clone, Serialize)]
pub struct RuleOutcome {
    pub rule: String,
    pub decide: Decide,
    /// matched | no_match | unknown (a referenced fact was absent).
    pub result: &'static str,
    pub because: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggest: Option<String>,
}

/// The compile verdict over a manifest's rules.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionReport {
    pub outcomes: Vec<RuleOutcome>,
    /// The binding decision: deny > fallback > place.
    pub decision: Decide,
    /// The effective image after any fallback swap.
    pub image: String,
}

/// Path suffix → operand multiplier into bytes. Only byte-quantities carry
/// these; everything else compares raw.
fn unit_multiplier(last_segment: &str) -> Option<u64> {
    match last_segment {
        "kb" => Some(1024),
        "mb" => Some(1024 * 1024),
        "gb" => Some(1024 * 1024 * 1024),
        _ => None,
    }
}

/// Split "ram.total.mb" into ("ram.total.bytes", 1 MiB);
/// "machine.architecture" passes through untouched.
fn resolve_path(path: &str) -> (String, u64) {
    match path.rsplit_once('.') {
        Some((base, last)) => match unit_multiplier(last) {
            Some(mult) => (format!("{base}.bytes"), mult),
            None => (path.to_string(), 1),
        },
        None => (path.to_string(), 1),
    }
}

/// Compare one condition against the generation. Err = a referenced fact
/// was absent — tri-state, never folded silently into no-match.
fn check(cond: &super::manifest::Condition, facts: &Generation) -> Result<bool, ()> {
    let (path, mult) = resolve_path(&cond.path);
    let fact = facts.resolve(&path).ok_or(())?;

    // YAML operands → JSON for uniform comparison.
    let want = serde_json::to_value(&cond.value).map_err(|_| ())?;

    if let Some(n) = want.as_i64().or_else(|| want.as_u64().map(|u| u as i64)) {
        let operand = n as f64 * mult as f64;
        let v = fact.as_f64().ok_or(())?;
        return Ok(match cond.op {
            CondOp::Eq => v == operand,
            CondOp::Ge => v >= operand,
            CondOp::Lt => v < operand,
            CondOp::In | CondOp::Lacks => false,
        });
    }
    if let Some(s) = want.as_str() {
        return Ok(match cond.op {
            CondOp::Eq => fact.as_str() == Some(s),
            CondOp::Lacks => match fact {
                serde_json::Value::Array(items) => !items
                    .iter()
                    .any(|i| i.as_str().map(|e| e.contains(s)).unwrap_or(false)),
                other => other.as_str().map(|e| !e.contains(s)).unwrap_or(true),
            },
            _ => false,
        });
    }
    if let Some(list) = want.as_array() {
        return Ok(match cond.op {
            CondOp::In => list.iter().any(|w| w == fact),
            _ => false,
        });
    }
    Err(())
}

/// Evaluate all rules; severity decides the verdict, not order.
pub fn evaluate(
    rules: &[CompatRule],
    facts: &Generation,
    default_image: &str,
) -> DecisionReport {
    let mut outcomes = Vec::new();
    let mut best: Option<(u8, &CompatRule)> = None; // severity: place=0 fallback=1 deny=2

    for rule in rules {
        let mut all_known = true;
        let mut all_true = true;
        for cond in &rule.when {
            match check(cond, facts) {
                Ok(true) => {}
                Ok(false) => all_true = false,
                Err(()) => {
                    all_known = false;
                    all_true = false;
                }
            }
        }
        let result = if !all_known {
            "unknown"
        } else if all_true {
            "matched"
        } else {
            "no_match"
        };
        outcomes.push(RuleOutcome {
            rule: rule.name.clone(),
            decide: rule.decide,
            result,
            because: rule.because.clone(),
            source: rule.source.clone(),
            suggest: rule.suggest.clone(),
        });

        if result == "matched" {
            let severity = match rule.decide {
                Decide::Deny => 2u8,
                Decide::Fallback => 1,
                Decide::Place => 0,
            };
            if best.map(|(s, _)| severity > s).unwrap_or(true) {
                best = Some((severity, rule));
            }
        }
    }

    let decision = best.map(|(_, r)| r.decide).unwrap_or(Decide::Place);
    let image = match best {
        Some((_, r)) if r.decide == Decide::Fallback => r
            .into
            .as_ref()
            .map(|f| f.image.clone())
            .unwrap_or_else(|| default_image.to_string()),
        _ => default_image.to_string(),
    };

    DecisionReport { outcomes, decision, image }
}
