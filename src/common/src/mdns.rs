//! Shared mDNS utilities for `_http._tcp` service registration.
//!
//! Provides TXT property builders that all zen-garden components use
//! when registering their web UIs as `_http._tcp` DNS-SD services.

use std::collections::HashMap;

use crate::constants;

/// Component types for `_http._tcp` TXT records.
pub enum HttpServiceComponent {
    /// Moss stone daemon (portrait web UI at `/`)
    Moss,
    /// Lantern dashboard daemon (React SPA at `/`)
    Lantern,
    /// Orchestrator dashboard (at `/`)
    Orchestrator { offering: String },
}

/// Build TXT record properties for an `_http._tcp` mDNS registration.
///
/// Returns a `HashMap` suitable for use with `koi_embedded::RegisterPayload::txt`
/// or `KoiMdnsClient::announce()`.
///
/// RFC 6763 specifies the `path` key for `_http._tcp`. We also include
/// garden-specific metadata keys (`garden-component`, `garden-role`, `version`).
pub fn build_http_txt(
    component: &HttpServiceComponent,
    path: &str,
    version: &str,
) -> HashMap<String, String> {
    let mut txt = HashMap::new();

    // RFC 6763 standard key
    txt.insert(constants::TXT_PATH.to_string(), path.to_string());

    // Version
    txt.insert("version".to_string(), version.to_string());

    // Component-specific keys
    match component {
        HttpServiceComponent::Moss => {
            txt.insert(constants::TXT_COMPONENT.to_string(), "moss".to_string());
            txt.insert("garden-role".to_string(), "stone-portrait".to_string());
        }
        HttpServiceComponent::Lantern => {
            txt.insert(constants::TXT_COMPONENT.to_string(), "lantern".to_string());
            txt.insert("garden-role".to_string(), "dashboard".to_string());
        }
        HttpServiceComponent::Orchestrator { offering } => {
            txt.insert(
                constants::TXT_COMPONENT.to_string(),
                "orchestrator".to_string(),
            );
            txt.insert("garden-role".to_string(), "orchestrator".to_string());
            txt.insert("garden-offering".to_string(), offering.clone());
        }
    }

    txt
}
