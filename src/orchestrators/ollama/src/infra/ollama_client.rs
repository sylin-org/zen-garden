//! HTTP client for Ollama API operations.
//!
//! Encapsulates all direct communication with Ollama instances.
//! Every method is fallible and includes timeouts.

use crate::domain::types::*;
use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::Client;
use std::time::Duration;

/// Timeout for discovery/profiling queries.
const PROFILE_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout for model show queries (per model).
const SHOW_TIMEOUT: Duration = Duration::from_secs(5);

/// Client for a single Ollama instance.
#[derive(Clone)]
pub struct OllamaClient {
    http: Client,
}

impl OllamaClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(300)) // long for inference
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { http }
    }

    // ── Discovery / Profiling ────────────────────────────────────

    /// Fetch the list of all locally available models.
    pub async fn get_tags(&self, endpoint: &str) -> Result<OllamaTagsResponse> {
        let url = format!("{endpoint}/api/tags");
        let resp = self
            .http
            .get(&url)
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/tags")?
            .error_for_status()
            .context("GET /api/tags status")?;
        resp.json().await.context("parse /api/tags")
    }

    /// Fetch models currently loaded in memory.
    pub async fn get_ps(&self, endpoint: &str) -> Result<OllamaPsResponse> {
        let url = format!("{endpoint}/api/ps");
        let resp = self
            .http
            .get(&url)
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/ps")?
            .error_for_status()
            .context("GET /api/ps status")?;
        resp.json().await.context("parse /api/ps")
    }

    /// Fetch detailed model metadata.
    pub async fn show_model(&self, endpoint: &str, model: &str) -> Result<OllamaShowResponse> {
        let url = format!("{endpoint}/api/show");
        let resp = self
            .http
            .post(&url)
            .timeout(SHOW_TIMEOUT)
            .json(&serde_json::json!({"model": model}))
            .send()
            .await
            .context("POST /api/show")?
            .error_for_status()
            .context("POST /api/show status")?;
        resp.json().await.context("parse /api/show")
    }

    /// Fetch Ollama version.
    pub async fn get_version(&self, endpoint: &str) -> Result<OllamaVersionResponse> {
        let url = format!("{endpoint}/api/version");
        let resp = self
            .http
            .get(&url)
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/version")?
            .error_for_status()
            .context("GET /api/version status")?;
        resp.json().await.context("parse /api/version")
    }

    // ── Proxy Forwarding ─────────────────────────────────────────

    /// Forward an inference request and return the raw response for streaming.
    pub async fn forward_request(
        &self,
        endpoint: &str,
        path: &str,
        method: reqwest::Method,
        body: Bytes,
        headers: reqwest::header::HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = format!("{endpoint}{path}");
        let mut builder = self.http.request(method, &url).body(body);

        // Forward relevant headers
        for (key, value) in headers.iter() {
            let name = key.as_str();
            if name == "content-type" || name == "accept" || name == "authorization" {
                builder = builder.header(key, value);
            }
        }

        builder
            .send()
            .await
            .context("forward request to Ollama")
    }

    // ── Model Management ─────────────────────────────────────────

    /// Pull a model on a specific instance. Returns a stream of progress events.
    pub async fn pull_model(
        &self,
        endpoint: &str,
        model: &str,
    ) -> Result<impl futures_util::Stream<Item = Result<Bytes, reqwest::Error>>> {
        let url = format!("{endpoint}/api/pull");
        let resp = self
            .http
            .post(&url)
            .json(&serde_json::json!({"model": model, "stream": true}))
            .send()
            .await
            .context("POST /api/pull")?
            .error_for_status()
            .context("POST /api/pull status")?;
        Ok(resp.bytes_stream())
    }

    /// Delete a model on a specific instance.
    pub async fn delete_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let url = format!("{endpoint}/api/delete");
        self.http
            .delete(&url)
            .json(&serde_json::json!({"model": model}))
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("DELETE /api/delete")?
            .error_for_status()
            .context("DELETE /api/delete status")?;
        Ok(())
    }

    /// Load a model into VRAM (empty prompt trick).
    pub async fn load_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let url = format!("{endpoint}/api/generate");
        self.http
            .post(&url)
            .json(&serde_json::json!({"model": model, "stream": false}))
            .send()
            .await
            .context("load model")?
            .error_for_status()
            .context("load model status")?;
        Ok(())
    }

    /// Unload a model from VRAM (keep_alive: 0 trick).
    pub async fn unload_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let url = format!("{endpoint}/api/generate");
        self.http
            .post(&url)
            .json(&serde_json::json!({"model": model, "keep_alive": 0, "stream": false}))
            .send()
            .await
            .context("unload model")?
            .error_for_status()
            .context("unload model status")?;
        Ok(())
    }

    /// Health check: can we reach the instance?
    pub async fn health_check(&self, endpoint: &str) -> bool {
        self.get_tags(endpoint).await.is_ok()
    }

    // ── Benchmarking ─────────────────────────────────────────────

    /// Benchmark timeout: if inference takes > 2 min for 80 tokens, it's Vetoed.
    const BENCH_TIMEOUT: Duration = Duration::from_secs(120);
    /// Embed benchmark timeout.
    const EMBED_TIMEOUT: Duration = Duration::from_secs(60);

    /// Run a non-streaming generate request and return timing data.
    ///
    /// Used by the fitness profiler to measure cold-start and tok/s.
    pub async fn benchmark_generate(
        &self,
        endpoint: &str,
        model: &str,
        prompt: &str,
        num_predict: u32,
    ) -> Result<OllamaInferenceFinal> {
        let url = format!("{endpoint}/api/generate");
        let resp = self
            .http
            .post(&url)
            .timeout(Self::BENCH_TIMEOUT)
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": false,
                "options": { "num_predict": num_predict }
            }))
            .send()
            .await
            .context("generate request timed out or unreachable")?
            .error_for_status()
            .context("generate returned HTTP error")?;
        resp.json().await.context("generate response parse failed")
    }

    /// Run a non-streaming generate request with base64 images (vision).
    pub async fn benchmark_generate_vision(
        &self,
        endpoint: &str,
        model: &str,
        prompt: &str,
        images_b64: &[String],
        num_predict: u32,
    ) -> Result<OllamaInferenceFinal> {
        let url = format!("{endpoint}/api/generate");
        let resp = self
            .http
            .post(&url)
            .timeout(Self::BENCH_TIMEOUT)
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "images": images_b64,
                "stream": false,
                "options": { "num_predict": num_predict }
            }))
            .send()
            .await
            .context("vision request timed out or unreachable")?
            .error_for_status()
            .context("vision returned HTTP error")?;
        resp.json().await.context("vision response parse failed")
    }

    /// Run an embed request and return timing data.
    pub async fn benchmark_embed(
        &self,
        endpoint: &str,
        model: &str,
        input: &str,
    ) -> Result<OllamaEmbedResponse> {
        let url = format!("{endpoint}/api/embed");
        let resp = self
            .http
            .post(&url)
            .timeout(Self::EMBED_TIMEOUT)
            .json(&serde_json::json!({
                "model": model,
                "input": input
            }))
            .send()
            .await
            .context("embed request timed out or unreachable")?
            .error_for_status()
            .context("embed returned HTTP error")?;
        resp.json().await.context("embed response parse failed")
    }

    // ── Full Instance Profiling ──────────────────────────────────

    /// Profile an Ollama instance: tags + ps + version + show per model.
    ///
    /// Returns (models_available, models_loaded, model_infos, version).
    pub async fn full_profile(
        &self,
        endpoint: &str,
    ) -> Result<(
        Vec<String>,
        Vec<LoadedModel>,
        Vec<ModelInfo>,
        Option<String>,
    )> {
        // Step 1: parallel inventory + load state + version
        let (tags_result, ps_result, version_result) = tokio::join!(
            self.get_tags(endpoint),
            self.get_ps(endpoint),
            self.get_version(endpoint),
        );

        let tags = tags_result.context("profiling: tags")?;
        let ps = ps_result.context("profiling: ps")?;
        let version = version_result.ok().map(|v| v.version);

        let models_available: Vec<String> = tags.models.iter().map(|t| t.name.clone()).collect();

        let models_loaded: Vec<LoadedModel> = ps
            .models
            .iter()
            .map(|m| LoadedModel {
                name: m.name.clone(),
                size_vram: m.size_vram,
                expires_at: m.expires_at.clone(),
            })
            .collect();

        // Build a map of loaded model VRAM for authoritative values
        let loaded_vram: std::collections::HashMap<&str, u64> =
            ps.models.iter().map(|m| (m.name.as_str(), m.size_vram)).collect();

        // Step 2: deep model profiles (parallel, one per model)
        let mut model_infos = Vec::new();
        let mut show_futures = Vec::new();

        for tag in &tags.models {
            let name = tag.name.clone();
            let endpoint = endpoint.to_string();
            let client = self.clone();
            show_futures.push(async move {
                let show = client.show_model(&endpoint, &name).await;
                (name, tag.clone(), show)
            });
        }

        let show_results = futures_util::future::join_all(show_futures).await;

        for (name, tag, show_result) in show_results {
            let (param_count, capabilities) = match show_result {
                Ok(show) => (show.parameter_count(), show.capabilities),
                Err(e) => {
                    tracing::warn!(model = %name, error = %e, "failed to query /api/show");
                    (None, vec![])
                }
            };

            let details = tag.details.as_ref();
            let quant = details.and_then(|d| d.quantization_level.as_deref());
            let param_size = details.and_then(|d| d.parameter_size.clone());
            let format = details.and_then(|d| d.format.clone());

            // VRAM: authoritative from /api/ps ONLY.  If the model is not
            // currently loaded we do NOT guess — vram_bytes stays None until
            // an actual load provides a real measurement.
            let vram_bytes = loaded_vram.get(name.as_str()).copied();

            model_infos.push(ModelInfo {
                name: name.clone(),
                parameter_count: param_count,
                parameter_size: param_size,
                quantization_level: quant.map(|s| s.to_string()),
                family: details.and_then(|d| d.family.clone()),
                families: details.map(|d| d.families.clone()).unwrap_or_default(),
                capabilities,
                format,
                size_disk: tag.size,
                vram_bytes,
            });
        }

        Ok((models_available, models_loaded, model_infos, version))
    }
}
