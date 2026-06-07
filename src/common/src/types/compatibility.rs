//! Compatibility system types — rules, conditions, healthchecks.
//!
//! Rules use the COMPAT-0002 predicate DSL:
//! ```yaml
//! compatibility_rules:
//!   - name: no-nvidia-use-rocm
//!     when:
//!       - host.ai.runtime LACKS cuda
//!       - host.ai.runtime HAS rocm
//!     fallback:
//!       image: "yanwk/comfyui-boot:rocm"
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityRules {
    #[serde(default)]
    pub compatibility_rules: Vec<CompatibilityRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install_healthcheck: Option<PostInstallHealthcheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityRule {
    pub name: String,
    /// Predicate DSL expressions (AND'd together). Each string is parsed
    /// by `Predicate::parse()` at evaluation time.
    #[serde(default)]
    pub when: Vec<String>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackConfig>,
    /// If true, this rule produces a warning instead of failing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub warn_only: bool,
    /// If true, evaluation continues to subsequent rules after this warning.
    /// Only meaningful when `warn_only` is true.
    #[serde(
        default,
        rename = "continue",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub continue_eval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    pub image: String,
    /// Suggested offering instance name when this fallback applies.
    /// e.g. `"legacy"` → FQN becomes `mongodb::legacy` instead of `mongodb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostInstallHealthcheck {
    pub enabled: bool,
    pub scan_log_lines: usize,
    pub timeout_seconds: u64,
    pub patterns: Vec<HealthcheckPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckPattern {
    pub pattern: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackConfig>,
}

impl PostInstallHealthcheck {
    /// First pattern whose regex matches `logs` (first-match-wins, mirroring
    /// pre-flight rule evaluation). Patterns whose regex fails to compile are
    /// skipped. Used by Moss to scan a freshly-deployed container's logs for
    /// crash signatures (COMPAT-0003).
    pub fn scan<'a>(&'a self, logs: &str) -> Option<&'a HealthcheckPattern> {
        self.patterns
            .iter()
            .find(|p| regex::Regex::new(&p.pattern).is_ok_and(|re| re.is_match(logs)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hc(patterns: Vec<HealthcheckPattern>) -> PostInstallHealthcheck {
        PostInstallHealthcheck {
            enabled: true,
            scan_log_lines: 100,
            timeout_seconds: 30,
            patterns,
        }
    }

    #[test]
    fn scan_matches_pattern_with_fallback() {
        let h = hc(vec![HealthcheckPattern {
            pattern: "Illegal instruction".to_string(),
            reason: "missing AVX".to_string(),
            suggestion: None,
            fallback: Some(FallbackConfig {
                image: "mongo:4.4".to_string(),
                name: Some("legacy".to_string()),
            }),
        }]);
        let m = h
            .scan("mongod: Illegal instruction (core dumped)")
            .expect("should match");
        assert_eq!(m.fallback.as_ref().unwrap().image, "mongo:4.4");
    }

    #[test]
    fn scan_no_match_returns_none() {
        let h = hc(vec![HealthcheckPattern {
            pattern: "Illegal instruction".to_string(),
            reason: "x".to_string(),
            suggestion: None,
            fallback: None,
        }]);
        assert!(h.scan("listening on 27017").is_none());
    }

    #[test]
    fn scan_skips_invalid_regex() {
        let h = hc(vec![HealthcheckPattern {
            pattern: "[unclosed".to_string(),
            reason: "x".to_string(),
            suggestion: None,
            fallback: None,
        }]);
        assert!(h.scan("anything").is_none());
    }
}
