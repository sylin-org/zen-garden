//! Gateway registration clients — Koi mDNS + Moss gateway API.
//!
//! Two independent HTTP clients for the two-registration model (ORCH-0004):
//! 1. `KoiMdnsClient` — registers an mDNS name via Koi's announce API
//! 2. `MossGatewayClient` — registers a gateway entry via Moss's gateway API

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Koi mDNS Client ────────────────────────────────────────────

/// HTTP client for Koi's mDNS announce/heartbeat/unregister API.
pub struct KoiMdnsClient {
    http: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct AnnounceRequest {
    name: String,
    #[serde(rename = "type")]
    service_type: String,
    port: u16,
    lease_secs: u32,
    txt: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AnnounceResponse {
    registered: RegisteredInfo,
}

#[derive(Debug, Deserialize)]
struct RegisteredInfo {
    id: String,
}

impl KoiMdnsClient {
    pub fn new(koi_endpoint: &str) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
            base_url: koi_endpoint.trim_end_matches('/').to_string(),
        }
    }

    /// Register an mDNS service with Koi. Returns the registration ID.
    pub async fn announce(
        &self,
        name: &str,
        port: u16,
        lease_secs: u32,
        txt: HashMap<String, String>,
    ) -> Result<String> {
        let url = format!("{}/v1/mdns/announce", self.base_url);
        let body = AnnounceRequest {
            name: name.to_string(),
            service_type: "_http._tcp".to_string(),
            port,
            lease_secs,
            txt,
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Koi mDNS announce request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Koi announce failed ({}): {}", status, body);
        }

        let data: AnnounceResponse = resp
            .json()
            .await
            .context("Failed to parse Koi announce response")?;

        Ok(data.registered.id)
    }

    /// Renew an mDNS heartbeat lease.
    pub async fn heartbeat(&self, id: &str) -> Result<()> {
        let url = format!("{}/v1/mdns/heartbeat/{}", self.base_url, id);
        let resp = self
            .http
            .put(&url)
            .send()
            .await
            .context("Koi mDNS heartbeat request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Koi heartbeat failed ({}): {}", status, body);
        }

        Ok(())
    }

    /// Unregister an mDNS service (sends goodbye packets).
    pub async fn unregister(&self, id: &str) -> Result<()> {
        let url = format!("{}/v1/mdns/unregister/{}", self.base_url, id);
        let resp = self
            .http
            .delete(&url)
            .send()
            .await
            .context("Koi mDNS unregister request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Koi unregister failed ({}): {}", status, body);
        }

        Ok(())
    }
}

// ─── Moss Gateway Client ────────────────────────────────────────

/// HTTP client for Moss's gateway registration API.
pub struct MossGatewayClient {
    http: Client,
}

#[derive(Debug, Serialize)]
struct PutGatewayRequest {
    fqn: String,
    hostname: String,
    ip: String,
    port: u16,
    handler_for: Vec<String>,
    protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri_template: Option<String>,
    source: String,
}

#[derive(Debug, Deserialize)]
pub struct PutGatewayResponse {
    pub lease_id: String,
    pub ttl_seconds: u32,
}

/// Parameters for a gateway registration.
pub struct GatewayParams {
    pub fqn: String,
    pub hostname: String,
    pub ip: String,
    pub port: u16,
    pub handler_for: Vec<String>,
    pub protocol: String,
    pub uri_template: Option<String>,
    pub source: String,
}

impl MossGatewayClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Register or refresh a gateway with Moss. Idempotent PUT.
    pub async fn register(
        &self,
        stone_endpoint: &str,
        offering: &str,
        params: &GatewayParams,
    ) -> Result<PutGatewayResponse> {
        let url = format!(
            "{}/api/v1/garden/gateway/{}",
            stone_endpoint.trim_end_matches('/'),
            offering
        );

        let body = PutGatewayRequest {
            fqn: params.fqn.clone(),
            hostname: params.hostname.clone(),
            ip: params.ip.clone(),
            port: params.port,
            handler_for: params.handler_for.clone(),
            protocol: params.protocol.clone(),
            uri_template: params.uri_template.clone(),
            source: params.source.clone(),
        };

        let resp = self
            .http
            .put(&url)
            .json(&body)
            .send()
            .await
            .context("Moss gateway register request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Moss gateway register failed ({}): {}", status, text);
        }

        resp.json()
            .await
            .context("Failed to parse Moss gateway response")
    }

    /// Deregister a gateway from Moss.
    pub async fn deregister(&self, stone_endpoint: &str, offering: &str) -> Result<()> {
        let url = format!(
            "{}/api/v1/garden/gateway/{}",
            stone_endpoint.trim_end_matches('/'),
            offering
        );

        let resp = self
            .http
            .delete(&url)
            .send()
            .await
            .context("Moss gateway deregister request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Moss gateway deregister failed ({}): {}", status, text);
        }

        Ok(())
    }
}
