//! Offering resolution — bridge between FQN and deployable spec.
//!
//! The resolution pipeline produces a `ResolvedOffering` from any source
//! (manifest, image inspection, or future repo/OCI). The deployment
//! pipeline consumes `ResolvedOffering` without knowing the source.
//!
//! OFFER-0006: This separation is the architectural foundation that makes
//! image-direct deployment first-class rather than a bolt-on.

use crate::docker::ContainerSpec;
use crate::infra::image_inspect::{
    description_from_labels, title_from_labels, ImageHealthcheck, ImageInspection,
};
use garden_common::offerings::OfferingFqn;
use serde::Serialize;

/// Starting host port for auto-assigned image-direct offerings.
const IMAGE_DIRECT_PORT_BASE: u16 = 30000;

/// A fully resolved offering ready for deployment.
///
/// Produced by the resolution pipeline, consumed by the deployment pipeline.
/// Identical structure regardless of source (curated manifest or image inspection).
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedOffering {
    /// Parsed FQN (with source, offering, instance)
    pub fqn: OfferingFqn,
    /// Docker image reference (e.g., "nginx:latest", "mongo:7")
    pub image: String,
    /// Port mappings: (name, host_port, container_port)
    pub ports: Vec<(String, u16, u16)>,
    /// Environment variables in `KEY=VALUE` format
    pub environment: Vec<String>,
    /// Volume mounts: (source, container_path)
    pub volumes: Vec<(String, String)>,
    /// Command override (None = use image default)
    pub command: Option<Vec<String>>,
    /// Embedded healthcheck from image (if any)
    pub healthcheck: Option<ImageHealthcheck>,
    /// Human-readable description (from OCI labels or empty)
    pub description: String,
    /// Offering category
    pub category: String,
    /// Search/discovery tags
    pub tags: Vec<String>,
}

/// Advisory when a curated manifest exists for the same image family.
#[derive(Debug, Clone, Serialize)]
pub struct CuratedAlternative {
    /// Curated offering name (e.g., "mongodb")
    pub offering_name: String,
    /// Description from the curated manifest
    pub description: String,
    /// Whether the curated manifest has hardware compatibility rules
    pub has_compatibility: bool,
    /// Whether the curated manifest has post-install guidance
    pub has_guidance: bool,
    /// Whether the curated manifest has a health check
    pub has_health_check: bool,
}

/// Resolution result — proceed directly or advise of curated alternative.
#[derive(Debug, Clone, Serialize)]
pub enum ResolutionResult {
    /// No curated alternative — proceed with image-direct
    Ready(ResolvedOffering),
    /// A curated manifest exists for this image family
    CuratedAvailable {
        resolved: ResolvedOffering,
        alternative: CuratedAlternative,
    },
}

/// Resolve an image-direct FQN into a deployable offering using inspection results.
///
/// Port assignment: exposed ports get sequential host ports starting from `port_base`.
/// Volume resolution: image volumes get Docker named volumes.
pub fn resolve_from_inspection(
    fqn: &OfferingFqn,
    inspection: &ImageInspection,
    port_base: u16,
) -> ResolvedOffering {
    // Assign host ports to exposed container ports
    let ports: Vec<(String, u16, u16)> = inspection
        .exposed_ports
        .iter()
        .enumerate()
        .map(|(i, &container_port)| {
            let name = if i == 0 {
                "default".to_string()
            } else {
                format!("port{}", i)
            };
            let host_port = port_base + i as u16;
            (name, host_port, container_port)
        })
        .collect();

    // Generate named volumes for image-defined mount points
    let volume_prefix = format!("zen-img-{}", fqn.encoded_for_container());
    let volumes: Vec<(String, String)> = inspection
        .volumes
        .iter()
        .map(|mount_point| {
            let slug = mount_point
                .trim_start_matches('/')
                .replace(['/', '.'], "-");
            let volume_name = format!("{}-{}", volume_prefix, slug);
            (volume_name, mount_point.clone())
        })
        .collect();

    // Filter out internal Docker env vars from the image defaults
    let environment: Vec<String> = inspection
        .environment
        .iter()
        .filter(|env| !env.starts_with("PATH="))
        .cloned()
        .collect();

    // Extract metadata from OCI labels
    let description = description_from_labels(&inspection.labels)
        .unwrap_or_else(|| format!("Image-direct deployment of {}", inspection.image_ref));
    let title = title_from_labels(&inspection.labels);

    let mut tags = vec!["image-direct".to_string()];
    if let Some(t) = title {
        tags.push(t);
    }

    ResolvedOffering {
        fqn: fqn.clone(),
        image: inspection.image_ref.clone(),
        ports,
        environment,
        volumes,
        command: None, // Use image default
        healthcheck: inspection.healthcheck.clone(),
        description,
        category: "custom".to_string(),
        tags,
    }
}

/// Check the curated offerings index for an image family match.
///
/// Compares the image base name (without tag) against known curated offerings.
/// For example, `mongo:7` matches curated offering `mongodb` whose image is `mongo:7`.
pub fn check_curated_collision(
    image_ref: &str,
    offerings: &[super::offerings::CompiledOffering],
) -> Option<CuratedAlternative> {
    let image_base = image_ref
        .rsplit_once(':')
        .map(|(base, _)| base)
        .unwrap_or(image_ref);

    for offering in offerings {
        let offering_image_base = offering
            .image
            .rsplit_once(':')
            .map(|(base, _)| base)
            .unwrap_or(&offering.image);

        if image_base.eq_ignore_ascii_case(offering_image_base) {
            return Some(CuratedAlternative {
                offering_name: offering.name.clone(),
                description: offering.description.clone(),
                has_compatibility: offering.compatibility.decision != "pass"
                    || offering.compatibility.reason.is_some(),
                has_guidance: true, // curated manifests always have guidance templates
                has_health_check: true,
            });
        }
    }

    None
}

/// Convert a `ResolvedOffering` to a `ContainerSpec` for the Docker deployment pipeline.
///
/// This is the key abstraction: the deployment pipeline receives a `ContainerSpec`
/// and doesn't know or care whether it came from a curated manifest or image inspection.
pub fn to_container_spec(resolved: &ResolvedOffering) -> ContainerSpec {
    ContainerSpec {
        image: resolved.image.clone(),
        command: resolved.command.clone(),
        ports: resolved.ports.iter().map(|(_, h, c)| (*h, *c)).collect(),
        environment: resolved.environment.clone(),
        volumes: resolved.volumes.clone(),
        config_files: vec![],
    }
}

/// Default port base for image-direct offerings.
pub fn default_port_base() -> u16 {
    IMAGE_DIRECT_PORT_BASE
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_inspection() -> ImageInspection {
        ImageInspection {
            image_ref: "nginx:latest".to_string(),
            exposed_ports: vec![80, 443],
            volumes: vec!["/usr/share/nginx/html".to_string()],
            environment: vec![
                "PATH=/usr/local/sbin:/usr/local/bin".to_string(),
                "NGINX_VERSION=1.27.0".to_string(),
            ],
            command: Some(vec!["nginx".to_string(), "-g".to_string(), "daemon off;".to_string()]),
            entrypoint: None,
            labels: {
                let mut m = HashMap::new();
                m.insert(
                    "org.opencontainers.image.description".to_string(),
                    "Official Nginx image".to_string(),
                );
                m
            },
            healthcheck: None,
            architecture: Some("amd64".to_string()),
        }
    }

    #[test]
    fn resolve_assigns_ports_sequentially() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        let inspection = sample_inspection();
        let resolved = resolve_from_inspection(&fqn, &inspection, 30000);

        assert_eq!(resolved.ports.len(), 2);
        assert_eq!(resolved.ports[0], ("default".to_string(), 30000, 80));
        assert_eq!(resolved.ports[1], ("port1".to_string(), 30001, 443));
    }

    #[test]
    fn resolve_generates_named_volumes() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        let inspection = sample_inspection();
        let resolved = resolve_from_inspection(&fqn, &inspection, 30000);

        assert_eq!(resolved.volumes.len(), 1);
        assert!(resolved.volumes[0].0.starts_with("zen-img-"));
        assert_eq!(resolved.volumes[0].1, "/usr/share/nginx/html");
    }

    #[test]
    fn resolve_filters_path_env() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        let inspection = sample_inspection();
        let resolved = resolve_from_inspection(&fqn, &inspection, 30000);

        // PATH= should be filtered out
        assert_eq!(resolved.environment.len(), 1);
        assert!(resolved.environment[0].starts_with("NGINX_VERSION="));
    }

    #[test]
    fn resolve_extracts_oci_description() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        let inspection = sample_inspection();
        let resolved = resolve_from_inspection(&fqn, &inspection, 30000);

        assert_eq!(resolved.description, "Official Nginx image");
    }

    #[test]
    fn to_container_spec_conversion() {
        let fqn = OfferingFqn::image_direct("nginx:latest").unwrap();
        let inspection = sample_inspection();
        let resolved = resolve_from_inspection(&fqn, &inspection, 30000);
        let spec = to_container_spec(&resolved);

        assert_eq!(spec.image, "nginx:latest");
        assert_eq!(spec.ports, vec![(30000, 80), (30001, 443)]);
        assert!(spec.config_files.is_empty());
    }

    #[test]
    fn curated_collision_detects_image_family() {
        use crate::domain::compatibility::CompiledCompatibility;

        let offerings = vec![super::super::offerings::CompiledOffering {
            name: "mongodb".to_string(),
            category: "data".to_string(),
            description: "MongoDB with replica set support".to_string(),
            tags: vec!["database".to_string()],
            image: "mongo:7".to_string(),
            ports: HashMap::new(),
            environment: vec![],
            volumes: vec![],
            compatibility: CompiledCompatibility {
                decision: "pass".to_string(),
                reason: None,
                original_image: None,
                fallback_image: None,
                fallback_name: None,
                suggestion: None,
            },
            tasks: HashMap::new(),
            network: Default::default(),
            coordination: Default::default(),
        }];

        // Same image family, different tag
        let collision = check_curated_collision("mongo:8", &offerings);
        assert!(collision.is_some());
        assert_eq!(collision.unwrap().offering_name, "mongodb");

        // Unrelated image
        let collision = check_curated_collision("nginx:latest", &offerings);
        assert!(collision.is_none());
    }
}
