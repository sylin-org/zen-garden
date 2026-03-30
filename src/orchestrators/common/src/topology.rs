//! Generic topology query for orchestrators.
//!
//! Queries `GET /api/v1/garden/topology` on a tended stone and returns all
//! stones that have a running instance of a specific offering.

use crate::http::check_response;
use anyhow::{Context, Result};
use garden_common::offerings::OfferingFqn;
use garden_common::types::HardwareCapabilities;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// A stone discovered via the topology endpoint that runs a specific offering.
#[derive(Debug, Clone)]
pub struct TopologyOfferingStone {
    pub stone_id: String,
    pub stone_name: String,
    pub ip: String,
    /// mDNS hostname, e.g. `stone-quartz-fen.local`.
    pub hostname: String,
    pub moss_port: u16,
    /// Fully-qualified name of the offering instance (e.g. `mongodb::prod`).
    pub fqn: OfferingFqn,
    /// Full hardware capabilities from the chirp payload.
    pub capabilities: Option<HardwareCapabilities>,
    /// Actual host ports from the topology (e.g., `{"default": 8000}`).
    /// Empty when ports match manifest defaults.
    pub ports: std::collections::HashMap<String, u16>,
}

impl TopologyOfferingStone {
    /// Moss API endpoint using the resolved IP address.
    ///
    /// Prefers IP over `.local` hostname because mDNS resolution is
    /// unreliable inside Docker containers on Windows.
    pub fn moss_endpoint(&self) -> String {
        format!("http://{}:{}", self.ip, self.moss_port)
    }
}

/// Query the topology endpoint on a tended stone and return all stones that
/// have a running instance of the specified offering.
///
/// Hardware capabilities are extracted directly from the chirp payload — no
/// separate portrait fetch required.
pub async fn query_topology_for_offering(
    stone_endpoint: &str,
    offering_name: &str,
) -> Result<Vec<TopologyOfferingStone>> {
    use garden_common::types::topology::TopologyEntry;

    #[derive(Deserialize)]
    struct TopologyResponse {
        data: Vec<TopologyEntry>,
    }

    let url = format!(
        "{}/api/v1/garden/topology",
        stone_endpoint.trim_end_matches('/')
    );

    tracing::info!(url = %url, offering = %offering_name, "querying topology");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("connect to topology endpoint at {url}"))?;
    let response = check_response(response, "topology query").await?;

    let topo: TopologyResponse = response.json().await.context("parse topology response")?;

    let mut results = Vec::new();
    for entry in &topo.data {
        let sn = &entry.stone_name;
        let hostname = if sn.contains('.') {
            sn.clone()
        } else {
            format!("{}.local", sn)
        };

        // Emit one entry per matching service instance (a stone may host
        // multiple instances of the same offering, e.g. mongodb + mongodb::prod).
        for svc in &entry.services {
            if svc.offering == offering_name && svc.status == "running" {
                results.push(TopologyOfferingStone {
                    stone_id: entry.stone_id.clone(),
                    stone_name: entry.stone_name.clone(),
                    ip: entry.address.ip.to_string(),
                    hostname: hostname.clone(),
                    moss_port: entry.address.port,
                    fqn: svc.name.clone(),
                    capabilities: entry.capabilities.clone(),
                    ports: svc.ports.clone(),
                });
            }
        }
    }

    tracing::info!(
        count = results.len(),
        offering = %offering_name,
        stones = ?results.iter().map(|s| &s.stone_name).collect::<Vec<_>>(),
        "topology query complete"
    );

    Ok(results)
}
