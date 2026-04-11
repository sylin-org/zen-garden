//! Configuration composition engine for managed services.
//!
//! Composes a manifest `ServiceTemplate` (base layer) with zero or more
//! `ConfigPatch` overlays (from orchestrators, admins, etc.) to produce
//! the effective container configuration.
//!
//! Pure domain logic — no I/O, no Docker, fully testable.

use anyhow::{Result, bail};
use garden_common::manifests::offering::ServiceTemplate;
use garden_common::types::ConfigPatch;

// ============================================================================
// Effective Config
// ============================================================================

/// The effective container configuration after composing manifest + patches.
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    /// Docker image (always from the manifest — patches cannot change this).
    pub image: String,
    /// Container command override. `None` means use image default CMD.
    pub command: Option<Vec<String>>,
    /// Port mappings (host, container). Always from the manifest.
    pub ports: Vec<(u16, u16)>,
    /// Environment variables in `KEY=VALUE` format for Docker.
    pub environment: Vec<String>,
    /// Volume mounts (host_path, container_path).
    pub volumes: Vec<(String, String)>,
    /// Config file mappings from the manifest template.
    /// Carried through so install_service can create empty files and bind-mount them.
    pub config_files: Vec<garden_common::manifests::offering::ConfigFileMapping>,
}

// ============================================================================
// Composition
// ============================================================================

/// Compose a manifest template with config patches to produce the effective config.
///
/// Patches are applied in order of `applied_at` (oldest first). The result
/// should have been validated at PATCH time, so conflicts here are programming
/// errors (we still handle them gracefully).
pub fn compose(template: &ServiceTemplate, patches: &[ConfigPatch]) -> Result<EffectiveConfig> {
    let mut effective = EffectiveConfig {
        image: template.image.clone(),
        command: template.command.clone(),
        ports: template.ports_vec(),
        environment: template.environment.clone(),
        volumes: template.volumes.clone(),
        config_files: template.config_files.clone(),
    };

    // Sort patches by applied_at for deterministic order
    let mut sorted_patches: Vec<&ConfigPatch> = patches.iter().collect();
    sorted_patches.sort_by_key(|p| p.applied_at);

    for patch in sorted_patches {
        // Command: last writer wins (validated at PATCH time to be single-owner)
        if let Some(ref cmd) = patch.command {
            effective.command = Some(cmd.clone());
        }

        // Environment: additive merge, patch values override template values with same key
        for (key, value) in &patch.environment {
            // Remove any existing entry with the same key from template env
            effective
                .environment
                .retain(|e| !e.starts_with(&format!("{}=", key)));
            effective.environment.push(format!("{}={}", key, value));
        }

        // Volumes: additive, append new mounts
        for volume in &patch.volumes {
            // Skip if container path already exists (validated at PATCH time)
            let already_mounted = effective
                .volumes
                .iter()
                .any(|(_, container)| container == &volume.1);
            if !already_mounted {
                effective.volumes.push(volume.clone());
            }
        }
    }

    Ok(effective)
}

// ============================================================================
// Validation
// ============================================================================

/// Validate a new or updated patch against existing patches from other owners.
///
/// Returns `Ok(())` if the patch can be safely applied, or an error describing
/// the conflict. The patch's own owner is excluded from conflict checks
/// (since it would replace the existing patch from that owner).
pub fn validate_patch(existing: &[ConfigPatch], new_patch: &ConfigPatch) -> Result<()> {
    let others: Vec<&ConfigPatch> = existing
        .iter()
        .filter(|p| p.owner != new_patch.owner)
        .collect();

    // Command: singular — only one owner may set it
    if new_patch.command.is_some() {
        for other in &others {
            if other.command.is_some() {
                bail!(
                    "command conflict: already set by '{}'. Only one owner may override the container command.",
                    other.owner
                );
            }
        }
    }

    // Environment: check for key conflicts across owners
    for key in new_patch.environment.keys() {
        for other in &others {
            if other.environment.contains_key(key) {
                bail!(
                    "environment conflict: key '{}' already set by '{}'",
                    key,
                    other.owner
                );
            }
        }
    }

    // Volumes: check for container_path conflicts across owners
    for (_, container_path) in &new_patch.volumes {
        for other in &others {
            for (_, other_container_path) in &other.volumes {
                if container_path == other_container_path {
                    bail!(
                        "volume conflict: container path '{}' already mounted by '{}'",
                        container_path,
                        other.owner
                    );
                }
            }
        }
    }

    // Config files: check for container_path conflicts across owners
    for config_path in new_patch.config.keys() {
        for other in &others {
            if other.config.contains_key(config_path) {
                bail!(
                    "config file conflict: path '{}' already set by '{}'",
                    config_path,
                    other.owner
                );
            }
        }
    }

    Ok(())
}

/// Merge config file content from all patches for a given container path.
///
/// Returns the merged content string if any patch contributes to this file,
/// or `None` if no patches touch this file. Currently uses last-writer-wins
/// per file (one owner per file is the common case).
pub fn merge_config_content(patches: &[ConfigPatch], container_path: &str) -> Option<String> {
    let mut sorted_patches: Vec<&ConfigPatch> = patches.iter().collect();
    sorted_patches.sort_by_key(|p| p.applied_at);

    let mut content: Option<String> = None;
    for patch in sorted_patches {
        if let Some(c) = patch.config.get(container_path) {
            content = Some(c.clone());
        }
    }
    content
}

/// Return the list of owners who have applied patches.
pub fn patch_owners(patches: &[ConfigPatch]) -> Vec<String> {
    patches.iter().map(|p| p.owner.clone()).collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::manifests::offering::ServiceTemplate;

    fn test_template() -> ServiceTemplate {
        ServiceTemplate {
            image: "mongo:7".to_string(),
            command: None,
            config_files: vec![],
            ports: {
                let mut m = std::collections::HashMap::new();
                m.insert("default".to_string(), (27017, 27017));
                m
            },
            environment: vec!["FOO=bar".to_string()],
            volumes: vec![("/data/db".to_string(), "/data/db".to_string())],
            compatibility: None,
            tasks: std::collections::HashMap::new(),
            network: Default::default(),
            device_requests: vec![],
        }
    }

    fn patch(owner: &str) -> ConfigPatch {
        ConfigPatch {
            owner: owner.to_string(),
            description: None,
            applied_at: chrono::Utc::now(),
            command: None,
            environment: std::collections::HashMap::new(),
            volumes: vec![],
            config: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn compose_no_patches() {
        let tmpl = test_template();
        let eff = compose(&tmpl, &[]).unwrap();
        assert_eq!(eff.image, "mongo:7");
        assert!(eff.command.is_none());
        assert_eq!(eff.ports, vec![(27017, 27017)]);
        assert_eq!(eff.environment, vec!["FOO=bar"]);
    }

    #[test]
    fn compose_with_command() {
        let tmpl = test_template();
        let mut p = patch("mongodb-orchestrator");
        p.command = Some(vec![
            "--replSet".to_string(),
            "zen-garden".to_string(),
            "--bind_ip_all".to_string(),
        ]);

        let eff = compose(&tmpl, &[p]).unwrap();
        assert_eq!(
            eff.command,
            Some(vec![
                "--replSet".to_string(),
                "zen-garden".to_string(),
                "--bind_ip_all".to_string()
            ])
        );
    }

    #[test]
    fn compose_with_env_merge() {
        let tmpl = test_template();
        let mut p = patch("orchestrator");
        p.environment
            .insert("EXTRA".to_string(), "value".to_string());

        let eff = compose(&tmpl, &[p]).unwrap();
        assert!(eff.environment.contains(&"FOO=bar".to_string()));
        assert!(eff.environment.contains(&"EXTRA=value".to_string()));
    }

    #[test]
    fn compose_env_override_template_key() {
        let tmpl = test_template();
        let mut p = patch("orchestrator");
        p.environment
            .insert("FOO".to_string(), "overridden".to_string());

        let eff = compose(&tmpl, &[p]).unwrap();
        assert!(!eff.environment.contains(&"FOO=bar".to_string()));
        assert!(eff.environment.contains(&"FOO=overridden".to_string()));
    }

    #[test]
    fn compose_with_volumes() {
        let tmpl = test_template();
        let mut p = patch("orchestrator");
        p.volumes
            .push(("/extra".to_string(), "/mnt/extra".to_string()));

        let eff = compose(&tmpl, &[p]).unwrap();
        assert_eq!(eff.volumes.len(), 2);
    }

    #[test]
    fn compose_volume_no_duplicate_container_path() {
        let tmpl = test_template();
        let mut p = patch("orchestrator");
        // Same container path as template — should be skipped
        p.volumes
            .push(("/other/host".to_string(), "/data/db".to_string()));

        let eff = compose(&tmpl, &[p]).unwrap();
        assert_eq!(eff.volumes.len(), 1); // Original preserved, duplicate skipped
    }

    #[test]
    fn validate_no_conflict() {
        let existing = vec![];
        let p = patch("orchestrator");
        assert!(validate_patch(&existing, &p).is_ok());
    }

    #[test]
    fn validate_command_conflict() {
        let mut existing_patch = patch("orchestrator-a");
        existing_patch.command = Some(vec!["--flag".to_string()]);

        let mut new_patch = patch("orchestrator-b");
        new_patch.command = Some(vec!["--other".to_string()]);

        assert!(validate_patch(&[existing_patch], &new_patch).is_err());
    }

    #[test]
    fn validate_command_same_owner_ok() {
        let mut existing_patch = patch("orchestrator");
        existing_patch.command = Some(vec!["--flag".to_string()]);

        let mut new_patch = patch("orchestrator"); // Same owner
        new_patch.command = Some(vec!["--updated".to_string()]);

        assert!(validate_patch(&[existing_patch], &new_patch).is_ok());
    }

    #[test]
    fn validate_env_conflict() {
        let mut existing_patch = patch("owner-a");
        existing_patch
            .environment
            .insert("KEY".to_string(), "a".to_string());

        let mut new_patch = patch("owner-b");
        new_patch
            .environment
            .insert("KEY".to_string(), "b".to_string());

        assert!(validate_patch(&[existing_patch], &new_patch).is_err());
    }

    #[test]
    fn validate_env_different_keys_ok() {
        let mut existing_patch = patch("owner-a");
        existing_patch
            .environment
            .insert("KEY_A".to_string(), "a".to_string());

        let mut new_patch = patch("owner-b");
        new_patch
            .environment
            .insert("KEY_B".to_string(), "b".to_string());

        assert!(validate_patch(&[existing_patch], &new_patch).is_ok());
    }

    #[test]
    fn validate_volume_conflict() {
        let mut existing_patch = patch("owner-a");
        existing_patch
            .volumes
            .push(("/host/a".to_string(), "/mnt/shared".to_string()));

        let mut new_patch = patch("owner-b");
        new_patch
            .volumes
            .push(("/host/b".to_string(), "/mnt/shared".to_string()));

        assert!(validate_patch(&[existing_patch], &new_patch).is_err());
    }

    #[test]
    fn validate_volume_different_paths_ok() {
        let mut existing_patch = patch("owner-a");
        existing_patch
            .volumes
            .push(("/host/a".to_string(), "/mnt/a".to_string()));

        let mut new_patch = patch("owner-b");
        new_patch
            .volumes
            .push(("/host/b".to_string(), "/mnt/b".to_string()));

        assert!(validate_patch(&[existing_patch], &new_patch).is_ok());
    }

    #[test]
    fn two_patches_non_overlapping_env() {
        let tmpl = test_template();
        let mut p1 = patch("owner-a");
        p1.environment.insert("A".to_string(), "1".to_string());

        let mut p2 = patch("owner-b");
        p2.environment.insert("B".to_string(), "2".to_string());

        let eff = compose(&tmpl, &[p1, p2]).unwrap();
        assert!(eff.environment.contains(&"A=1".to_string()));
        assert!(eff.environment.contains(&"B=2".to_string()));
        assert!(eff.environment.contains(&"FOO=bar".to_string()));
    }

    #[test]
    fn patch_owners_returns_all() {
        let patches = vec![patch("alpha"), patch("beta")];
        let owners = patch_owners(&patches);
        assert_eq!(owners, vec!["alpha", "beta"]);
    }
}
