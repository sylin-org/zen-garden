//! Zen Common Types
//! Core data structures for service discovery, health, resources, and registry

pub mod compatibility;
pub mod discovery;
pub mod error;
pub mod hardware;
pub mod health;
pub mod lantern;
pub mod offering;
pub mod orchestration;
pub mod peer_address;
pub mod pond;
pub mod ports_catalog;
pub mod service;
pub mod task;
pub mod topology;

// Re-export all types for backward compatibility
pub use compatibility::*;
pub use discovery::*;
pub use error::*;
pub use hardware::*;
pub use health::*;
pub use lantern::*;
pub use offering::*;
pub use orchestration::*;
pub use pond::*;
pub use ports_catalog::*;
pub use service::*;
pub use peer_address::*;
pub use task::*;
pub use topology::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_status_serde() {
        let status = ServiceStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ServiceStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_service_health_status_serde() {
        let health = ServiceHealthStatus::Healthy;
        let json = serde_json::to_string(&health).unwrap();
        let deserialized: ServiceHealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(health, deserialized);
    }

    #[test]
    fn test_service_info_serde() {
        let info = ServiceInfo {
            offering_id: "018d3c8f-1a2b-7c3d-8e4f-5a6b7c8d9e0f".into(),
            name: "mongodb".into(),
            offering: "mongodb".into(),
            version: "7.0".into(),
            status: ServiceStatus::Running,
            health: ServiceHealthStatus::Healthy,
            ports: Ports {
                native: 27017,
                agnostic: Some(8080),
            },
            resources: None,
            job_id: None,
            sub_capabilities: Vec::new(),
            guidance: None,
            customized_by: Vec::new(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.name, deserialized.name);
        assert_eq!(info.status, deserialized.status);
        assert_eq!(info.offering_id, deserialized.offering_id);
    }

    #[test]
    fn test_service_info_offering_id_migration() {
        // Test that existing services without offering_id deserialize correctly
        // (serde default should provide empty string)
        let json = r#"{
            "name": "mongodb",
            "offering": "mongodb",
            "version": "7.0",
            "status": "Running",
            "health": "Healthy",
            "ports": {"native": 27017, "agnostic": 8080}
        }"#;
        let deserialized: ServiceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(deserialized.offering_id, "");
        assert_eq!(deserialized.name, "mongodb");
    }

    #[test]
    fn test_discovery_request_serde() {
        let req = DiscoveryRequest {
            discover: "moss".into(),
            request_id: "test-123".into(),
            requester: "rake".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: DiscoveryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.discover, deserialized.discover);
        assert_eq!(req.request_id, deserialized.request_id);
    }

    #[test]
    fn test_discovery_response_serde() {
        let resp = DiscoveryResponse {
            stone_id: Some("01234567-89ab-cdef-0123-456789abcdef".into()),
            stone_name: "stone-01".into(),
            address: crate::PeerAddress::new("127.0.0.1".parse().unwrap(), 3001),
            moss_version: "0.1.0".into(),
            lantern_endpoint: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: DiscoveryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.stone_name, deserialized.stone_name);
    }

    #[test]
    fn test_pond_config_defaults() {
        let config = PondConfig {
            enabled: false,
            keystone_path: None,
            require_mtls: false,
        };
        assert!(!config.enabled);
        assert!(!config.require_mtls);
    }

    #[test]
    fn test_stone_invite_request() {
        let req = StoneInviteRequest {
            stone_name: "stone-02".into(),
            expiry_hours: Some(24),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("stone-02"));
    }

    #[test]
    fn test_offering_mode_serde() {
        let mode = OfferingMode::Adopted;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"adopted\"");
        let deserialized: OfferingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }

    #[test]
    fn test_adopted_control_level_default() {
        let default = AdoptedControlLevel::default();
        assert_eq!(default, AdoptedControlLevel::Monitor);
    }

    #[test]
    fn test_adopted_control_level_serde() {
        let level = AdoptedControlLevel::Full;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"full\"");
        let deserialized: AdoptedControlLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(level, deserialized);
    }

    #[test]
    fn test_health_method_serde() {
        let method = HealthMethod::Http;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, "\"http\"");
        let deserialized: HealthMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(method, deserialized);
    }
}
