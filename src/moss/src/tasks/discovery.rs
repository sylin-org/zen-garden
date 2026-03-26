//! Service discovery background tasks
//!
//! Handles continuous registration with service discovery systems:
//! - Lantern service registry (centralized discovery)
//! - mDNS broadcasts (local network discovery) - future
//! - Pond synchronization (distributed discovery) - future

use crate::AppState;

/// Lantern registration loop - registers this stone with Lantern every 45 seconds
///
/// Continuously registers this stone with the Lantern service discovery system.
/// Sends POST /api/register with stone ID, name, endpoint, and current service list.
///
/// Only runs if LANTERN_ENDPOINT environment variable is set.
pub async fn lantern_registration_loop(
    stone_id: String,
    stone_name: String,
    endpoint: String,
    lantern_endpoint: String,
    state: AppState,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    use garden_common::{RegisterRequest, RegisterServiceInfo};
    use reqwest::Client;

    tracing::info!(
        stone_id = %stone_id,
        stone_name = %stone_name,
        lantern_endpoint = %lantern_endpoint,
        "Starting Lantern registration loop"
    );

    let client = Client::new();
    let register_url = format!("{}/api/v1/register", lantern_endpoint);

    loop {
        // Build service list from current offerings
        let services = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .map(|o| RegisterServiceInfo {
                    name: o.name.to_string(),
                    service_type: o.offering.clone(),
                    status: o.status.to_string(),
                    connection_string: format!(
                        "{}:{}",
                        endpoint,
                        o.location.port_map.values().next().copied().unwrap_or(0)
                    ),
                })
                .collect()
        };

        let request = RegisterRequest {
            stone_id: Some(stone_id.clone()),
            stone_name: stone_name.clone(),
            endpoint: endpoint.clone(),
            services,
        };

        match client.post(&register_url).json(&request).send().await {
            Ok(response) if response.status().is_success() => {
                tracing::debug!("Registered with Lantern successfully");
            }
            Ok(response) => {
                tracing::warn!(
                    status = ?response.status(),
                    "Lantern registration returned non-success status"
                );
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to register with Lantern");
            }
        }

        // Sleep for 45 seconds before next heartbeat
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(45)) => {}
            _ = token.cancelled() => {
                tracing::info!("Lantern registration loop shutting down (cancellation requested)");
                return Ok(());
            }
        }
    }
}
