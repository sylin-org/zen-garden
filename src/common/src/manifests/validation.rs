//! Manifest validation — security and schema rules.
//!
//! Reusable by both Rake (local validation) and Moss (server-side validation
//! before test deployment). Pure functions, no external dependencies beyond
//! serde for YAML/JSON parsing.

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

    // Check version field
    if value.get("version").is_none() {
        findings.push(ValidationFinding {
            file: filename.to_string(),
            line: 0,
            severity: Severity::Warning,
            code: "COMPAT002".to_string(),
            message: "Missing 'version' field — defaulting to version 1".to_string(),
        });
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
            files_checked += 1;
        } else if name.ends_with(".frontmatter.json") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            findings.extend(validate_frontmatter(&content, &name));
            files_checked += 1;
        } else if name.ends_with(".compatibility.yaml") {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read {}", path.display()))?;
            findings.extend(validate_compatibility(&content, &name));
            files_checked += 1;
        }
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
        let yaml = "version: \"1\"\ncompatibility_rules: []\n";
        let findings = validate_compatibility(yaml, "test.compatibility.yaml");
        assert!(findings.is_empty());
    }

    #[test]
    fn warns_missing_compatibility_version() {
        let yaml = "compatibility_rules: []\n";
        let findings = validate_compatibility(yaml, "test.compatibility.yaml");
        assert!(findings.iter().any(|f| f.code == "COMPAT002"));
    }
}
