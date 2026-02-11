//! HTTP client for communicating with Moss REST APIs
//!
//! Used by the aggregation layer to poll stone endpoints
//! and by the action proxy to forward commands.

use anyhow::{Context, Result};
use reqwest::Client;

/// Client for Moss stone REST APIs
#[derive(Clone)]
pub struct MossClient {
    client: Client,
}

impl MossClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Fetch JSON from a Moss endpoint
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Moss returned {} from {}: {}", status, url, body);
        }

        resp.json::<T>()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))
    }

    /// POST JSON to a Moss endpoint and return the response
    pub async fn post_json<T: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<R> {
        let resp = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Moss returned {} from {}: {}", status, url, body_text);
        }

        resp.json::<R>()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))
    }

    /// Forward a raw request to a Moss endpoint, returning the raw response body
    pub async fn proxy_post(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let resp = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to proxy to {}", url))?;

        let status = resp.status();
        let response_body: serde_json::Value = resp
            .json()
            .await
            .unwrap_or(serde_json::Value::Null);

        Ok((status, response_body))
    }

    /// Forward a DELETE request to a Moss endpoint
    pub async fn proxy_delete(
        &self,
        url: &str,
    ) -> Result<(reqwest::StatusCode, serde_json::Value)> {
        let resp = self
            .client
            .delete(url)
            .send()
            .await
            .with_context(|| format!("Failed to proxy DELETE to {}", url))?;

        let status = resp.status();
        let response_body: serde_json::Value = resp
            .json()
            .await
            .unwrap_or(serde_json::Value::Null);

        Ok((status, response_body))
    }
}
