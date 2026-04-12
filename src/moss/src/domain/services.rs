//! Service counting utilities for placement evaluation
//!
//! Reusable functions for counting services on local and remote stones.

use anyhow::{Context, Result};
use std::time::Duration;

/// Get count of running services on local stone
///
/// Fast, zero-latency check of local offerings registry.
pub async fn get_local_service_count(state: &crate::AppState) -> Result<usize> {
    let count = state
        .offerings
        .with_active(|offerings| {
            offerings
                .iter()
                .filter(|o| o.status == garden_common::OfferingStatus::Running)
                .count()
        })
        .await;

    Ok(count)
}

/// Fetch service count from remote stone via HTTP
///
/// Calls the `/api/v1/services` endpoint and counts running services.
pub async fn fetch_remote_service_count(endpoint: &str, timeout: Duration) -> Result<usize> {
    let services_url = format!("{}/api/v1/services", endpoint.trim_end_matches('/'));
    let response = crate::http::HTTP
        .get(&services_url)
        .timeout(timeout)
        .send()
        .await
        .context("Failed to fetch services from remote stone")?;

    if !response.status().is_success() {
        anyhow::bail!("Remote stone returned error: {}", response.status());
    }

    let services: Vec<garden_common::ServiceInfo> = response
        .json()
        .await
        .context("Failed to parse services response")?;

    // Count running services
    let count = services
        .iter()
        .filter(|svc| svc.status == garden_common::ServiceStatus::Running)
        .count();

    Ok(count)
}

#[cfg(test)]
mod tests {
    // Note: get_local_service_count requires AppState, so we test it via integration tests
    // fetch_remote_service_count requires a live HTTP server, so we test it manually

    #[test]
    fn test_service_counting_compiles() {
        // This test just ensures the module compiles correctly
        assert!(true);
    }
}
