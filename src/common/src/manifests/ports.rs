//! Well-known ports catalog loader
//!
//! Loads the well-known-ports.yaml catalog which provides system-level
//! knowledge about ports that commonly conflict and how to handle them.

use crate::types::WellKnownPortsCatalog;
use std::path::Path;
use std::sync::OnceLock;

/// Global singleton for well-known ports catalog
static PORTS_CATALOG: OnceLock<WellKnownPortsCatalog> = OnceLock::new();

/// Load the well-known ports catalog from a YAML file
pub fn load_ports_catalog(path: &Path) -> Result<WellKnownPortsCatalog, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read ports catalog: {}", e))?;
    let content = crate::utils::strings::strip_bom(&content);

    serde_yaml::from_str(content)
        .map_err(|e| format!("Failed to parse ports catalog: {}", e))
}

/// Initialize the global ports catalog from the given path
pub fn init_ports_catalog(path: &Path) -> Result<(), String> {
    let catalog = load_ports_catalog(path)?;
    PORTS_CATALOG
        .set(catalog)
        .map_err(|_| "Ports catalog already initialized".to_string())
}

/// Get the global ports catalog (returns None if not initialized)
pub fn get_ports_catalog() -> Option<&'static WellKnownPortsCatalog> {
    PORTS_CATALOG.get()
}

/// Load ports catalog from embedded content (for compiled-in manifests)
pub fn load_ports_catalog_from_str(content: &str) -> Result<WellKnownPortsCatalog, String> {
    let content = crate::utils::strings::strip_bom(content);
    serde_yaml::from_str(content)
        .map_err(|e| format!("Failed to parse ports catalog: {}", e))
}

/// Initialize the global ports catalog from embedded content
pub fn init_ports_catalog_from_str(content: &str) -> Result<(), String> {
    let catalog = load_ports_catalog_from_str(content)?;
    PORTS_CATALOG
        .set(catalog)
        .map_err(|_| "Ports catalog already initialized".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ports_catalog() {
        let yaml = r#"
version: "1"
ports:
  53:
    name: dns
    description: "DNS queries"
    linux:
      common_culprit: "systemd-resolved"
      detection: "systemctl is-active --quiet systemd-resolved"
      remediation:
        type: auto
        commands:
          - "systemctl disable --now systemd-resolved"
        files:
          - path: "/etc/resolv.conf"
            content: "nameserver 8.8.8.8"
  80:
    name: http
    description: "HTTP traffic"
    linux:
      common_culprit: "nginx"
      remediation:
        type: manual
        message: "Stop nginx first"
"#;

        let catalog = load_ports_catalog_from_str(yaml).unwrap();
        assert_eq!(catalog.version, "1");
        assert!(catalog.ports.contains_key(&53));
        assert!(catalog.ports.contains_key(&80));

        let dns = catalog.ports.get(&53).unwrap();
        assert_eq!(dns.name, "dns");
        assert!(dns.linux.is_some());
    }
}
