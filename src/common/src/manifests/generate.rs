//! Manifest generation from image inspection results.
//!
//! Converts OCI image metadata (as returned by the Moss inspect endpoint)
//! into scaffolded manifest files. Used by Rake `manifest init` and
//! potentially by Moss Greenhouse (Phase 3).

use anyhow::{Context, Result};

/// Generated manifest file set — all content as strings, ready to write.
#[derive(Debug, Clone)]
pub struct GeneratedManifest {
    /// Offering name (derived from image reference).
    pub name: String,
    /// `.snippet.yaml` content.
    pub snippet_yaml: String,
    /// `.frontmatter.json` content.
    pub frontmatter_json: String,
    /// `.compatibility.yaml` content (commented template).
    pub compatibility_yaml: String,
    /// `.guidance.md` content (documentation template).
    pub guidance_md: String,
}

/// Generate manifest files from image inspection JSON.
///
/// The `inspection` parameter is the JSON object returned by
/// `GET /api/v1/stone/offerings/inspect?image={ref}`.
///
/// - `name`: override the auto-detected offering name.
/// - `category`: override the default category ("custom").
pub fn generate_from_inspection(
    name: Option<&str>,
    category: Option<&str>,
    inspection: &serde_json::Value,
) -> Result<GeneratedManifest> {
    let image_ref = inspection
        .get("image")
        .or_else(|| inspection.get("image_ref"))
        .and_then(|v| v.as_str())
        .context("Inspection JSON missing 'image' field")?;

    let offering_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| derive_name_from_image(image_ref));

    let category = category.unwrap_or("custom");

    let ports = extract_ports(inspection);
    let volumes = extract_volumes(inspection);
    let environment = extract_environment(inspection);
    let healthcheck = inspection.get("healthcheck");
    let description = extract_description(inspection);
    let architecture = inspection
        .get("architecture")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let snippet_yaml = build_snippet_yaml(
        image_ref,
        &offering_name,
        &ports,
        &volumes,
        &environment,
        healthcheck,
    );

    let frontmatter_json = build_frontmatter_json(
        &offering_name,
        &description,
        category,
        ports.first().map(|p| p.1),
    );

    let compatibility_yaml = build_compatibility_yaml(&offering_name);
    let guidance_md = build_guidance_md(
        &offering_name,
        &description,
        &ports,
        &volumes,
        &environment,
        architecture,
    );

    Ok(GeneratedManifest {
        name: offering_name,
        snippet_yaml,
        frontmatter_json,
        compatibility_yaml,
        guidance_md,
    })
}

/// Derive an offering name from a Docker image reference.
///
/// `ghcr.io/org/myapp:v2` → `myapp`
/// `nginx:latest` → `nginx`
/// `mongo:7` → `mongo`
/// `registry.example.com/tools/builder:3.1` → `builder`
pub fn derive_name_from_image(image_ref: &str) -> String {
    // Strip tag/digest: everything after the last `:` that doesn't contain `/`
    let without_tag = match image_ref.rfind(':') {
        Some(pos) if !image_ref[pos..].contains('/') => &image_ref[..pos],
        _ => image_ref,
    };
    // Strip digest (@sha256:...)
    let without_digest = match without_tag.find('@') {
        Some(pos) => &without_tag[..pos],
        None => without_tag,
    };
    // Take last path segment
    let name = without_digest
        .rsplit('/')
        .next()
        .unwrap_or(without_digest);

    // Sanitize: lowercase, replace non-alphanumeric with hyphens
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();

    sanitized.trim_matches('-').to_string()
}

/// Extract (name, container_port) pairs from inspection JSON.
fn extract_ports(inspection: &serde_json::Value) -> Vec<(String, u16)> {
    let mut ports = Vec::new();
    if let Some(exposed) = inspection.get("exposed_ports").and_then(|v| v.as_array()) {
        for (i, p) in exposed.iter().enumerate() {
            let port = p.as_u64().unwrap_or(0) as u16;
            if port == 0 {
                continue;
            }
            let name = if i == 0 {
                "default".to_string()
            } else {
                format!("port{}", i)
            };
            ports.push((name, port));
        }
    }
    ports
}

/// Extract volume mount points from inspection JSON.
fn extract_volumes(inspection: &serde_json::Value) -> Vec<String> {
    inspection
        .get("volumes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract environment variables from inspection JSON, filtering PATH.
fn extract_environment(inspection: &serde_json::Value) -> Vec<String> {
    inspection
        .get("environment")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.starts_with("PATH="))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract description from OCI labels or fallback.
fn extract_description(inspection: &serde_json::Value) -> String {
    if let Some(labels) = inspection.get("labels").and_then(|v| v.as_object()) {
        // OCI standard label
        if let Some(desc) = labels
            .get("org.opencontainers.image.description")
            .and_then(|v| v.as_str())
        {
            if !desc.is_empty() {
                return desc.to_string();
            }
        }
        // Fallback: description label
        if let Some(desc) = labels.get("description").and_then(|v| v.as_str()) {
            if !desc.is_empty() {
                return desc.to_string();
            }
        }
    }
    String::new()
}

// ============================================================================
// File builders
// ============================================================================

fn build_snippet_yaml(
    image_ref: &str,
    name: &str,
    ports: &[(String, u16)],
    volumes: &[String],
    environment: &[String],
    healthcheck: Option<&serde_json::Value>,
) -> String {
    let mut yaml = String::new();
    yaml.push_str("# Generated by garden-rake manifest init\n");
    yaml.push_str(&format!("image: {image_ref}\n"));
    yaml.push_str(&format!("container_name: {name}\n"));

    // Ports
    if !ports.is_empty() {
        yaml.push_str("ports:\n");
        for (port_name, container_port) in ports {
            yaml.push_str(&format!("  {port_name}: [{container_port}, {container_port}]\n"));
        }
    }

    // Volumes
    if !volumes.is_empty() {
        yaml.push_str("volumes:\n");
        for vol in volumes {
            let slug = vol
                .trim_start_matches('/')
                .replace(['/', '.'], "-");
            yaml.push_str(&format!("  - {name}-{slug}:{vol}\n"));
        }
    }

    // Environment
    if !environment.is_empty() {
        yaml.push_str("environment:\n");
        for env in environment {
            yaml.push_str(&format!("  - {env}\n"));
        }
    }

    // Healthcheck
    if let Some(hc) = healthcheck {
        if !hc.is_null() {
            yaml.push_str("healthcheck:\n");
            if let Some(test) = hc.get("test").and_then(|v| v.as_array()) {
                let parts: Vec<&str> = test.iter().filter_map(|v| v.as_str()).collect();
                if !parts.is_empty() {
                    let formatted: Vec<String> =
                        parts.iter().map(|p| format!("\"{p}\"")).collect();
                    yaml.push_str(&format!("  test: [{}]\n", formatted.join(", ")));
                }
            }
            if let Some(interval) = hc.get("interval_ns").and_then(|v| v.as_i64()) {
                yaml.push_str(&format!("  interval: {}s\n", interval / 1_000_000_000));
            }
            if let Some(timeout) = hc.get("timeout_ns").and_then(|v| v.as_i64()) {
                yaml.push_str(&format!("  timeout: {}s\n", timeout / 1_000_000_000));
            }
            if let Some(retries) = hc.get("retries").and_then(|v| v.as_i64()) {
                yaml.push_str(&format!("  retries: {retries}\n"));
            }
        }
    }

    yaml.push_str("restart: unless-stopped\n");
    yaml.push_str("networks:\n  - zen-garden\n");

    yaml
}

fn build_frontmatter_json(
    name: &str,
    description: &str,
    category: &str,
    default_port: Option<u16>,
) -> String {
    let desc = if description.is_empty() {
        format!("Docker image: {name}")
    } else {
        description.to_string()
    };

    let port_field = match default_port {
        Some(p) => format!("{p}"),
        None => "null".to_string(),
    };

    // Build JSON manually for clean formatting
    format!(
        "{{\n  \"name\": \"{name}\",\n  \"description\": {desc_json},\n  \"category\": \"{category}\",\n  \"tags\": [\"image-direct\"],\n  \"port\": {port_field}\n}}\n",
        desc_json = serde_json::to_string(&desc).unwrap_or_else(|_| format!("\"{desc}\"")),
    )
}

fn build_compatibility_yaml(name: &str) -> String {
    format!(
        r#"# Compatibility rules for {name}
# Add hardware compatibility checks here.
# See: docs/specs/compatibility-rules.md

version: "1"

compatibility_rules: []
#  - name: "example-rule"
#    condition:
#      architectures: ["armv6l"]
#    reason: "Requires ARMv7 or later"
#    fallback:
#      image: "{name}:compatible-tag"
"#
    )
}

fn build_guidance_md(
    name: &str,
    description: &str,
    ports: &[(String, u16)],
    volumes: &[String],
    environment: &[String],
    architecture: &str,
) -> String {
    let title = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string() + &name[1..])
        .unwrap_or_else(|| name.to_string());

    let mut md = format!("# {title}\n\n");

    // Overview
    md.push_str("## Overview\n\n");
    if description.is_empty() {
        md.push_str("_Describe what this offering does._\n\n");
    } else {
        md.push_str(&format!("{description}\n\n"));
    }
    md.push_str(&format!("**Architecture**: {architecture}\n\n"));

    // Configuration
    md.push_str("## Configuration\n\n");

    if !ports.is_empty() {
        md.push_str("### Ports\n\n");
        md.push_str("| Name | Port | Description |\n");
        md.push_str("|------|------|-------------|\n");
        for (port_name, port) in ports {
            md.push_str(&format!("| {port_name} | {port} | |\n"));
        }
        md.push('\n');
    }

    if !volumes.is_empty() {
        md.push_str("### Volumes\n\n");
        md.push_str("| Mount Point | Description |\n");
        md.push_str("|-------------|-------------|\n");
        for vol in volumes {
            md.push_str(&format!("| `{vol}` | |\n"));
        }
        md.push('\n');
    }

    if !environment.is_empty() {
        md.push_str("### Environment Variables\n\n");
        md.push_str("| Variable | Default | Description |\n");
        md.push_str("|----------|---------|-------------|\n");
        for env in environment {
            let (key, val) = env.split_once('=').unwrap_or((env, ""));
            md.push_str(&format!("| `{key}` | `{val}` | |\n"));
        }
        md.push('\n');
    }

    // Usage
    md.push_str("## Usage\n\n");
    if let Some((_, port)) = ports.first() {
        md.push_str(&format!(
            "Connect to this service at `http://{{host}}:{port}`.\n"
        ));
    } else {
        md.push_str("This offering exposes no ports (internal service).\n");
    }

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_inspection() -> serde_json::Value {
        serde_json::json!({
            "image": "nginx:latest",
            "exposed_ports": [80, 443],
            "volumes": ["/usr/share/nginx/html"],
            "environment": ["NGINX_VERSION=1.27.0", "PATH=/usr/local/sbin"],
            "labels": {
                "org.opencontainers.image.description": "The official Nginx Docker image"
            },
            "architecture": "amd64",
            "healthcheck": null
        })
    }

    #[test]
    fn derive_name_simple() {
        assert_eq!(derive_name_from_image("nginx:latest"), "nginx");
        assert_eq!(derive_name_from_image("mongo:7"), "mongo");
        assert_eq!(derive_name_from_image("redis"), "redis");
    }

    #[test]
    fn derive_name_registry() {
        assert_eq!(
            derive_name_from_image("ghcr.io/org/myapp:v2"),
            "myapp"
        );
        assert_eq!(
            derive_name_from_image("registry.example.com/tools/builder:3.1"),
            "builder"
        );
    }

    #[test]
    fn derive_name_with_digest() {
        assert_eq!(
            derive_name_from_image("nginx@sha256:abc123"),
            "nginx"
        );
    }

    #[test]
    fn generate_basic() {
        let inspection = sample_inspection();
        let result = generate_from_inspection(None, None, &inspection).unwrap();

        assert_eq!(result.name, "nginx");
        assert!(result.snippet_yaml.contains("image: nginx:latest"));
        assert!(result.snippet_yaml.contains("default: [80, 80]"));
        assert!(result.snippet_yaml.contains("port1: [443, 443]"));
        assert!(result.snippet_yaml.contains("nginx-usr-share-nginx-html:/usr/share/nginx/html"));
        assert!(result.snippet_yaml.contains("NGINX_VERSION=1.27.0"));
        // PATH= should be filtered
        assert!(!result.snippet_yaml.contains("PATH=/usr/local/sbin"));

        assert!(result.frontmatter_json.contains("\"nginx\""));
        assert!(result.frontmatter_json.contains("official Nginx"));
        assert!(result.frontmatter_json.contains("\"port\": 80"));

        assert!(result.guidance_md.contains("# Nginx"));
        assert!(result.guidance_md.contains("amd64"));
    }

    #[test]
    fn generate_with_custom_name_and_category() {
        let inspection = sample_inspection();
        let result =
            generate_from_inspection(Some("my-web"), Some("web"), &inspection).unwrap();

        assert_eq!(result.name, "my-web");
        assert!(result.snippet_yaml.contains("container_name: my-web"));
        assert!(result.frontmatter_json.contains("\"my-web\""));
        assert!(result.frontmatter_json.contains("\"web\""));
    }

    #[test]
    fn generate_with_healthcheck() {
        let inspection = serde_json::json!({
            "image": "app:latest",
            "exposed_ports": [8080],
            "volumes": [],
            "environment": [],
            "labels": {},
            "architecture": "amd64",
            "healthcheck": {
                "test": ["CMD", "curl", "-f", "http://localhost:8080/health"],
                "interval_ns": 30000000000_i64,
                "timeout_ns": 10000000000_i64,
                "retries": 3
            }
        });
        let result = generate_from_inspection(None, None, &inspection).unwrap();

        assert!(result.snippet_yaml.contains("healthcheck:"));
        assert!(result.snippet_yaml.contains("interval: 30s"));
        assert!(result.snippet_yaml.contains("timeout: 10s"));
        assert!(result.snippet_yaml.contains("retries: 3"));
    }

    #[test]
    fn generate_empty_inspection() {
        let inspection = serde_json::json!({
            "image": "scratch:latest",
            "exposed_ports": [],
            "volumes": [],
            "environment": [],
            "labels": {},
            "architecture": "amd64",
            "healthcheck": null
        });
        let result = generate_from_inspection(None, None, &inspection).unwrap();

        assert_eq!(result.name, "scratch");
        assert!(result.snippet_yaml.contains("image: scratch:latest"));
        // No ports, volumes, or env sections
        assert!(!result.snippet_yaml.contains("ports:"));
        assert!(!result.snippet_yaml.contains("volumes:"));
        assert!(!result.snippet_yaml.contains("environment:"));
    }
}
