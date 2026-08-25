//! Manifest validation — security and schema rules.
//!
//! Reusable by both Rake (local validation) and Moss (server-side validation
//! before test deployment). Pure functions; the only in-crate helpers used are
//! serde (YAML/JSON parsing), the compatibility predicate parser
//! (`crate::compatibility::Predicate`), and the category registry.

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// Severity level for validation findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A single validation finding (error, warning, or info).
#[derive(Debug, Clone, Serialize)]
pub struct ValidationFinding {
    /// Which file the finding relates to.
    pub file: String,
    /// Line number (0 if not applicable).
    pub line: usize,
    /// Severity.
    pub severity: Severity,
    /// Finding code (e.g., "SEC001", "SCHEMA002").
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Result of validating a manifest directory or file set.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub findings: Vec<ValidationFinding>,
    pub files_checked: usize,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        !self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }
}

/// Sensitive volume mount targets that should never appear in manifests.
const SENSITIVE_MOUNTS: &[&str] = &["/", "/etc", "/proc", "/sys", "/var/run/docker.sock", "/dev"];

// ============================================================================
// Snippet validation
// ============================================================================

/// Validate a snippet YAML string.
///
/// Checks security rules (privileged, host network, sensitive mounts) and
/// schema rules (required fields, port ranges).
pub fn validate_snippet(content: &str, filename: &str) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();

    // Parse YAML
    let value: serde_yml::Value = match serde_yml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Error,
                code: "YAML001".to_string(),
                message: format!("Invalid YAML: {e}"),
            });
            return findings;
        }
    };

    // If it has a top-level "services" key, unwrap to the first service
    let service = if let Some(services) = value.get("services") {
        services
            .as_mapping()
            .and_then(|m| m.values().next())
            .unwrap_or(&value)
    } else {
        &value
    };

    // --- Schema rules ---

    // SCHEMA001: Missing image field
    match service.get("image") {
        None => {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Error,
                code: "SCHEMA001".to_string(),
                message: "Missing required 'image' field".to_string(),
            });
        }
        Some(img) => {
            // SCHEMA002: Empty image value
            if img.as_str().is_none_or(|s| s.trim().is_empty()) {
                findings.push(ValidationFinding {
                    file: filename.to_string(),
                    line: 0,
                    severity: Severity::Error,
                    code: "SCHEMA002".to_string(),
                    message: "The 'image' field is empty".to_string(),
                });
            }
        }
    }

    // SCHEMA003: No ports (info only)
    if service.get("ports").is_none() {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Info,
            code: "SCHEMA003".to_string(),
            message: "No ports defined — offering will be internal-only".to_string(),
        });
    }

    // --- Security rules ---

    // SEC001: privileged
    if let Some(priv_val) = service.get("privileged")
        && priv_val.as_bool() == Some(true)
    {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Error,
            code: "SEC001".to_string(),
            message: "Container runs in privileged mode — this is a security risk".to_string(),
        });
    }

    // SEC002: host network
    if let Some(net) = service.get("network_mode")
        && net.as_str() == Some("host")
    {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Error,
            code: "SEC002".to_string(),
            message: "Container uses host networking — bypasses network isolation".to_string(),
        });
    }

    // SEC003: Sensitive volume mounts
    if let Some(volumes) = service.get("volumes")
        && let Some(vols) = volumes.as_sequence()
    {
        for vol in vols {
            let vol_str = vol.as_str().unwrap_or("");
            // Volume format: "source:target[:options]" or just "target"
            let target = vol_str
                .split(':')
                .nth(1)
                .unwrap_or(vol_str)
                .trim_end_matches(":ro")
                .trim_end_matches(":rw");

            for sensitive in SENSITIVE_MOUNTS {
                if target == *sensitive {
                    findings.push(ValidationFinding {
                        file: filename.to_string(),
                        line: 0,
                        severity: Severity::Error,
                        code: "SEC003".to_string(),
                        message: format!(
                            "Sensitive volume mount to '{sensitive}' — potential host compromise"
                        ),
                    });
                }
            }
        }
    }

    // SEC004 + SEC005: Port validation
    validate_ports(service, filename, &mut findings);

    findings
}

/// Validate port definitions in a snippet.
fn validate_ports(
    service: &serde_yml::Value,
    filename: &str,
    findings: &mut Vec<ValidationFinding>,
) {
    let ports = match service.get("ports") {
        Some(p) => p,
        None => return,
    };

    let mut seen_host_ports: HashSet<u16> = HashSet::new();

    // Ports can be a mapping (named ports) or sequence
    let port_pairs: Vec<(u16, u16)> = if let Some(mapping) = ports.as_mapping() {
        mapping.values().filter_map(extract_port_pair).collect()
    } else if let Some(seq) = ports.as_sequence() {
        seq.iter().filter_map(extract_port_pair).collect()
    } else {
        return;
    };

    for (host, container) in port_pairs {
        // SEC004: Port range
        if host == 0 || container == 0 {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Error,
                code: "SEC004".to_string(),
                message: format!("Port 0 is invalid (host={host}, container={container})"),
            });
        }

        // SEC005: Duplicate host ports
        if !seen_host_ports.insert(host) {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Warning,
                code: "SEC005".to_string(),
                message: format!("Duplicate host port {host}"),
            });
        }
    }
}

/// Extract a (host_port, container_port) pair from a YAML value.
///
/// Supports:
/// - `[host, container]` (array of two integers)
/// - `"host:container"` (string format)
fn extract_port_pair(v: &serde_yml::Value) -> Option<(u16, u16)> {
    // Array format: [8080, 80]
    if let Some(seq) = v.as_sequence()
        && seq.len() == 2
    {
        let host = seq[0].as_u64()? as u16;
        let container = seq[1].as_u64()? as u16;
        return Some((host, container));
    }
    // String format: "8080:80"
    if let Some(s) = v.as_str() {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            let host: u16 = parts[0].parse().ok()?;
            let container: u16 = parts[1].parse().ok()?;
            return Some((host, container));
        }
    }
    // Single integer (container port only — host = same)
    if let Some(n) = v.as_u64() {
        let port = n as u16;
        return Some((port, port));
    }
    None
}

// ============================================================================
// Frontmatter validation
// ============================================================================

/// Validate a frontmatter JSON string.
pub fn validate_frontmatter(content: &str, filename: &str) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();

    // FM001: Parse JSON
    let value: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Error,
                code: "FM001".to_string(),
                message: format!("Invalid JSON: {e}"),
            });
            return findings;
        }
    };

    // FM002: Missing name
    if value.get("name").and_then(|n| n.as_str()).is_none() {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Error,
            code: "FM002".to_string(),
            message: "Missing required 'name' field".to_string(),
        });
    }

    // FM003: Missing description
    if value.get("description").and_then(|d| d.as_str()).is_none() {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Warning,
            code: "FM003".to_string(),
            message: "Missing 'description' field — offering will show no description".to_string(),
        });
    }

    // FM004: Port range
    if let Some(port) = value.get("port").and_then(|p| p.as_u64())
        && (port == 0 || port > 65535)
    {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Error,
            code: "FM004".to_string(),
            message: format!("Port {port} is outside valid range (1–65535)"),
        });
    }

    // FM005: Unknown category (alias-aware). Skipped when the registry is empty
    // (e.g. unit tests / minimal deployments) to avoid false positives.
    if let Some(category) = value.get("category").and_then(|c| c.as_str()) {
        let registry = crate::manifests::category::get_category_registry();
        let known = registry.category_names();
        if !known.is_empty() && registry.resolve_token(category).is_none() {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Warning,
                code: "FM005".to_string(),
                message: format!("Unknown category '{category}'. Known: {}", known.join(", ")),
            });
        }
    }

    // FM007: Unknown top-level frontmatter keys.
    const KNOWN_FRONTMATTER_KEYS: &[&str] = &[
        "name",
        "description",
        "category",
        "tags",
        "port",
        "modes",
        "volumes",
        "gpu_recommended",
        "minimum_memory_gb",
        "connection",
        "manageable_env",
        "homepage",
        "documentation",
        "icon",
        "coordination",
        "ceremony",
    ];
    if let Some(obj) = value.as_object() {
        for key in obj.keys() {
            if !KNOWN_FRONTMATTER_KEYS.contains(&key.as_str()) {
                findings.push(ValidationFinding {
                    file: filename.to_string(),
                    line: 0,
                    severity: Severity::Warning,
                    code: "FM007".to_string(),
                    message: format!("Unknown frontmatter key '{key}'"),
                });
            }
        }
    }

    findings
}

// ============================================================================
// Compatibility validation
// ============================================================================

/// Validate a compatibility YAML string.
pub fn validate_compatibility(content: &str, filename: &str) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();

    // Parse YAML
    let value: serde_yml::Value = match serde_yml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            findings.push(ValidationFinding {
                file: filename.to_string(),
                line: 0,
                severity: Severity::Error,
                code: "COMPAT001".to_string(),
                message: format!("Invalid YAML: {e}"),
            });
            return findings;
        }
    };

    // COMPAT003: parse every compatibility_rules[].when predicate. Walking the
    // untyped Value keeps one bad rule from aborting the whole check and no-ops
    // on the hw/ recommendations schema (which has no compatibility_rules).
    if let Some(rules) = value.get("compatibility_rules").and_then(|r| r.as_sequence()) {
        for rule in rules {
            let rule_name = rule
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("<unnamed>");
            let Some(when) = rule.get("when").and_then(|w| w.as_sequence()) else {
                continue;
            };
            for expr in when {
                let Some(expr) = expr.as_str() else { continue };
                if let Err(e) = crate::compatibility::Predicate::parse(expr) {
                    findings.push(ValidationFinding {
                        file: filename.to_string(),
                        line: 0,
                        severity: Severity::Error,
                        code: "COMPAT003".to_string(),
                        message: format!(
                            "Rule '{rule_name}' has invalid predicate '{expr}': {e}"
                        ),
                    });
                }
            }
        }
    }

    findings
}

// ============================================================================
// Directory validation
// ============================================================================

/// Validate a complete manifest directory.
///
/// Scans for `.snippet.yaml`, `.frontmatter.json`, and `.compatibility.yaml`
/// files and validates each one. Returns an aggregate result.
pub fn validate_manifest_dir(dir: &Path) -> anyhow::Result<ValidationResult> {
    use anyhow::Context;

    let mut findings = Vec::new();
    let mut files_checked = 0;
    let mut snippet_content: Option<String> = None;
    let mut frontmatter_content: Option<String> = None;

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Cannot read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if name.ends_with(".snippet.yaml") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            findings.extend(validate_snippet(&content, &name));
            snippet_content = Some(content);
            files_checked += 1;
        } else if name.ends_with(".frontmatter.json") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            findings.extend(validate_frontmatter(&content, &name));
            frontmatter_content = Some(content);
            files_checked += 1;
        } else if name.ends_with(".compatibility.yaml") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            findings.extend(validate_compatibility(&content, &name));
            files_checked += 1;
        }
    }

    // FM006: cross-file port consistency (requires both files).
    if let (Some(snippet), Some(frontmatter)) = (&snippet_content, &frontmatter_content) {
        findings.extend(validate_ports_match(snippet, frontmatter));
    }

    if files_checked == 0 {
        findings.push(ValidationFinding {
            file: dir.display().to_string(),
            line: 0,
            severity: Severity::Warning,
            code: "DIR001".to_string(),
            message: "No manifest files found in directory".to_string(),
        });
    }

    Ok(ValidationResult {
        findings,
        files_checked,
    })
}

/// Cross-file check (FM006): the frontmatter `port` should match the snippet's
/// `ports.default` host port. Both inputs are raw file contents; returns empty
/// when either file is unparseable or the frontmatter has no `port`.
pub fn validate_ports_match(
    snippet_content: &str,
    frontmatter_content: &str,
) -> Vec<ValidationFinding> {
    let mut findings = Vec::new();

    let Ok(snippet) = serde_yml::from_str::<serde_yml::Value>(snippet_content) else {
        return findings;
    };
    let Ok(fm) = serde_json::from_str::<serde_json::Value>(frontmatter_content) else {
        return findings;
    };
    let Some(fm_port) = fm.get("port").and_then(|p| p.as_u64()) else {
        return findings;
    };

    // Unwrap an optional `services:` compose wrapper.
    let service = snippet
        .get("services")
        .and_then(|s| s.as_mapping())
        .and_then(|m| m.values().next())
        .unwrap_or(&snippet);

    let default = service.get("ports").and_then(|p| p.get("default"));
    if let Some((host, _container)) = default.and_then(extract_port_pair)
        && host as u64 != fm_port
    {
        findings.push(ValidationFinding {
            file: "frontmatter".to_string(),
            line: 0,
            severity: Severity::Warning,
            code: "FM006".to_string(),
            message: format!(
                "Frontmatter port {fm_port} does not match snippet ports.default host port {host}"
            ),
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_snippet_passes() {
        let yaml = r#"
image: nginx:latest
container_name: nginx
ports:
  default: [80, 80]
volumes:
  - nginx-data:/usr/share/nginx/html
restart: unless-stopped
networks:
  - zen-garden
"#;
        let findings = validate_snippet(yaml, "nginx.snippet.yaml");
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn rejects_missing_image() {
        let yaml = "container_name: test\nports:\n  default: [80, 80]\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SCHEMA001"));
    }

    #[test]
    fn rejects_empty_image() {
        let yaml = "image: \"\"\nports:\n  default: [80, 80]\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SCHEMA002"));
    }

    #[test]
    fn rejects_privileged() {
        let yaml = "image: test:latest\nprivileged: true\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SEC001"));
    }

    #[test]
    fn rejects_host_network() {
        let yaml = "image: test:latest\nnetwork_mode: host\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SEC002"));
    }

    #[test]
    fn rejects_sensitive_volume_mounts() {
        let yaml = "image: test:latest\nvolumes:\n  - /host/path:/var/run/docker.sock\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SEC003"));
    }

    #[test]
    fn rejects_root_mount() {
        let yaml = "image: test:latest\nvolumes:\n  - data:/\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SEC003"));
    }

    #[test]
    fn warns_duplicate_ports() {
        let yaml = "image: test:latest\nports:\n  http: [8080, 80]\n  alt: [8080, 8080]\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SEC005"));
    }

    #[test]
    fn info_no_ports() {
        let yaml = "image: test:latest\n";
        let findings = validate_snippet(yaml, "test.snippet.yaml");
        assert!(findings.iter().any(|f| f.code == "SCHEMA003"));
    }

    #[test]
    fn valid_frontmatter_passes() {
        let json = r#"{"name": "nginx", "description": "Web server", "port": 80}"#;
        let findings = validate_frontmatter(json, "nginx.frontmatter.json");
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn rejects_invalid_json() {
        let findings = validate_frontmatter("{bad json", "test.frontmatter.json");
        assert!(findings.iter().any(|f| f.code == "FM001"));
    }

    #[test]
    fn rejects_missing_name() {
        let json = r#"{"description": "test"}"#;
        let findings = validate_frontmatter(json, "test.frontmatter.json");
        assert!(findings.iter().any(|f| f.code == "FM002"));
    }

    #[test]
    fn warns_missing_description() {
        let json = r#"{"name": "test"}"#;
        let findings = validate_frontmatter(json, "test.frontmatter.json");
        assert!(findings.iter().any(|f| f.code == "FM003"));
    }

    #[test]
    fn rejects_invalid_port_in_frontmatter() {
        let json = r#"{"name": "test", "description": "test", "port": 99999}"#;
        let findings = validate_frontmatter(json, "test.frontmatter.json");
        assert!(findings.iter().any(|f| f.code == "FM004"));
    }

    #[test]
    fn validates_compose_format() {
        let yaml = "services:\n  myapp:\n    image: myapp:latest\n    ports:\n      default: [8080, 8080]\n";
        let findings = validate_snippet(yaml, "myapp.snippet.yaml");
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "Expected no errors for compose format, got: {errors:?}"
        );
    }

    #[test]
    fn validates_compatibility_yaml() {
        let yaml = "compatibility_rules:\n  - name: ok\n    when:\n      - host.ram.total.mb < 512\n    reason: low\n";
        let findings = validate_compatibility(yaml, "test.compatibility.yaml");
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn rejects_invalid_predicate() {
        let yaml = "compatibility_rules:\n  - name: bad\n    when:\n      - host.architcture IS armv6l\n    reason: typo\n";
        let findings = validate_compatibility(yaml, "test.compatibility.yaml");
        assert!(findings.iter().any(|f| f.code == "COMPAT003"));
    }

    #[test]
    fn warns_unknown_frontmatter_key() {
        let json = r#"{"name":"x","description":"y","bogus_key":true}"#;
        let findings = validate_frontmatter(json, "x.frontmatter.json");
        assert!(findings.iter().any(|f| f.code == "FM007"));
    }

    #[test]
    fn warns_port_mismatch() {
        let snippet = "image: x:1\nports:\n  default: [8080, 80]\n";
        let fm = r#"{"name":"x","description":"y","port":9090}"#;
        let findings = validate_ports_match(snippet, fm);
        assert!(findings.iter().any(|f| f.code == "FM006"));
    }

    #[test]
    fn port_match_produces_no_warning() {
        let snippet = "image: x:1\nports:\n  default: [8080, 80]\n";
        let fm = r#"{"name":"x","description":"y","port":8080}"#;
        let findings = validate_ports_match(snippet, fm);
        assert!(findings.is_empty(), "got: {findings:?}");
    }
}
