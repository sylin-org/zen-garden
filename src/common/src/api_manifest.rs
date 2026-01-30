//! API Manifest - structured metadata for all Moss HTTP endpoints
//!
//! Similar to CommandManifest for Companions, this provides:
//! - Single source of truth for API documentation
//! - Structured metadata that developers reference when creating endpoints
//! - Dynamic generation of API specs for tools/docs
//!
//! ## Usage
//!
//! **Define endpoint metadata:**
//! ```rust
//! use garden_common::api_manifest::*;
//!
//! pub fn manifest() -> EndpointSpec {
//!     EndpointSpec::new("GET", "/api/v1/services", "services")
//!         .description("List all services running on this Stone")
//!         .query_param("q", "Search query (name, category, tag)", false)
//!         .query_param("fresh", "Force network scan", false)
//!         .response_type("Array<ServiceInfo>")
//!         .example(
//!             "List all services",
//!             "curl http://stone-01:7185/api/v1/services",
//!             r#"{"data": [{"name": "mongodb", "status": "Running"}]}"#
//!         )
//! }
//! ```
//!
//! **In Moss handlers:**
//! ```rust,ignore
//! // Generate manifest from endpoint specs in handler
//! fn build_manifest(base_url: &str) -> ApiManifest {
//!     let mut endpoints = Vec::new();
//!     endpoints.push(EndpointSpec::new("GET", "/health", "health"));
//!     // ... add more endpoints
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Global endpoint registry
static ENDPOINT_REGISTRY_INSTANCE: OnceLock<EndpointRegistry> = OnceLock::new();

/// Complete API manifest for all endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiManifest {
    pub version: String,
    pub base_url: String,
    pub categories: Vec<ApiCategory>,
    pub endpoints: Vec<EndpointSpec>,
}

/// API category grouping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCategory {
    pub name: String,
    pub description: String,
    pub endpoints: Vec<String>, // endpoint paths
}

/// Single endpoint specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointSpec {
    pub method: String,
    pub path: String,
    pub category: String,
    pub description: String,
    pub path_params: Vec<ParamSpec>,
    pub query_params: Vec<ParamSpec>,
    pub body_schema: Option<String>,
    pub response_type: String,
    pub examples: Vec<EndpointExample>,
    pub notes: Vec<String>,
}

/// Parameter specification (path or query)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub description: String,
    pub param_type: String, // "string", "number", "boolean"
    pub required: bool,
    pub default: Option<String>,
}

/// Usage example
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointExample {
    pub title: String,
    pub description: Option<String>,
    pub curl: String,
    pub response: String,
}

impl EndpointSpec {
    /// Create a new endpoint spec
    pub fn new(method: impl Into<String>, path: impl Into<String>, category: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            category: category.into(),
            description: String::new(),
            path_params: Vec::new(),
            query_params: Vec::new(),
            body_schema: None,
            response_type: String::from("GardenApiResponse<T>"),
            examples: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add path parameter
    pub fn path_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        param_type: impl Into<String>,
    ) -> Self {
        self.path_params.push(ParamSpec {
            name: name.into(),
            description: description.into(),
            param_type: param_type.into(),
            required: true, // path params always required
            default: None,
        });
        self
    }

    /// Add query parameter
    pub fn query_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        self.query_params.push(ParamSpec {
            name: name.into(),
            description: description.into(),
            param_type: String::from("string"),
            required,
            default: None,
        });
        self
    }

    /// Set request body schema
    pub fn body_schema(mut self, schema: impl Into<String>) -> Self {
        self.body_schema = Some(schema.into());
        self
    }

    /// Set response type
    pub fn response_type(mut self, response_type: impl Into<String>) -> Self {
        self.response_type = response_type.into();
        self
    }

    /// Add usage example
    pub fn example(
        mut self,
        title: impl Into<String>,
        curl: impl Into<String>,
        response: impl Into<String>,
    ) -> Self {
        self.examples.push(EndpointExample {
            title: title.into(),
            description: None,
            curl: curl.into(),
            response: response.into(),
        });
        self
    }

    /// Add a note
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Endpoint registry for collecting all endpoints
#[derive(Debug, Default)]
pub struct EndpointRegistry {
    endpoints: Vec<EndpointSpec>,
}

impl EndpointRegistry {
    /// Get or initialize the global registry
    pub fn global() -> &'static EndpointRegistry {
        ENDPOINT_REGISTRY_INSTANCE.get_or_init(|| {
            let registry = EndpointRegistry::default();
            // Register all endpoints during initialization
            // This happens when the module is first loaded
            registry
        })
    }

    /// Register an endpoint (called during module init)
    pub fn register(&mut self, spec: EndpointSpec) {
        self.endpoints.push(spec);
    }

    /// Get all registered endpoints
    pub fn endpoints(&self) -> &[EndpointSpec] {
        &self.endpoints
    }

    /// Generate complete API manifest
    pub fn generate_manifest(&self, base_url: impl Into<String>) -> ApiManifest {
        let mut categories_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        // Group endpoints by category
        for endpoint in &self.endpoints {
            categories_map
                .entry(endpoint.category.clone())
                .or_insert_with(Vec::new)
                .push(endpoint.path.clone());
        }

        let categories = vec![
            ApiCategory {
                name: "health".into(),
                description: "Health and monitoring endpoints".into(),
                endpoints: categories_map.get("health").cloned().unwrap_or_default(),
            },
            ApiCategory {
                name: "offerings".into(),
                description: "Human-layer service templates (plant/remove)".into(),
                endpoints: categories_map.get("offerings").cloned().unwrap_or_default(),
            },
            ApiCategory {
                name: "services".into(),
                description: "Technical-layer container management".into(),
                endpoints: categories_map.get("services").cloned().unwrap_or_default(),
            },
            ApiCategory {
                name: "stone".into(),
                description: "Stone-level operations (upgrade, Companions, presence)".into(),
                endpoints: categories_map.get("stone").cloned().unwrap_or_default(),
            },
            ApiCategory {
                name: "garden".into(),
                description: "Cross-stone topology and orchestration".into(),
                endpoints: categories_map.get("garden").cloned().unwrap_or_default(),
            },
            ApiCategory {
                name: "admin".into(),
                description: "Administrative operations (shutdown, reboot)".into(),
                endpoints: categories_map.get("admin").cloned().unwrap_or_default(),
            },
        ];

        ApiManifest {
            version: env!("CARGO_PKG_VERSION").into(),
            base_url: base_url.into(),
            categories,
            endpoints: self.endpoints.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_spec_builder() {
        let spec = EndpointSpec::new("GET", "/api/v1/services", "services")
            .description("List services")
            .query_param("q", "Search query", false)
            .response_type("Array<ServiceInfo>")
            .example(
                "Basic list",
                "curl http://stone:7185/api/v1/services",
                r#"{"data": []}"#,
            );

        assert_eq!(spec.method, "GET");
        assert_eq!(spec.path, "/api/v1/services");
        assert_eq!(spec.query_params.len(), 1);
        assert_eq!(spec.examples.len(), 1);
    }

    #[test]
    fn test_manifest_generation() {
        let mut registry = EndpointRegistry::default();
        
        registry.register(
            EndpointSpec::new("GET", "/health", "health")
                .description("Health check")
        );

        registry.register(
            EndpointSpec::new("GET", "/api/v1/services", "services")
                .description("List services")
        );

        let manifest = registry.generate_manifest("http://localhost:7185");

        assert_eq!(manifest.endpoints.len(), 2);
        assert_eq!(manifest.base_url, "http://localhost:7185");
        assert!(manifest.categories.iter().any(|c| c.name == "health"));
        assert!(manifest.categories.iter().any(|c| c.name == "services"));
    }
}
