//! Ollama HTTP client — all Ollama-specific API calls.
//!
//! Harvested from ollama-orchestrator infra/ollama_client.rs. Timeout
//! values and error handling preserved exactly.

use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::header::HeaderMap;

use super::types::*;
use crate::domain::types::{LoadedModel, ModelInfo};

/// Timeout for discovery/profiling queries.
const PROFILE_TIMEOUT: Duration = Duration::from_secs(10);
/// Timeout for per-model show queries.
const SHOW_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout for generate benchmarks (2 min for ~80 tokens).
const BENCH_TIMEOUT: Duration = Duration::from_secs(120);
/// Timeout for embedding benchmarks.
const EMBED_TIMEOUT: Duration = Duration::from_secs(60);
/// Timeout for sustained generation (Think capability).
const THINK_TIMEOUT: Duration = Duration::from_secs(300);

/// HTTP client for the Ollama REST API.
#[derive(Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }

    // ── Discovery / Profiling ───────────────────────────────────

    pub async fn get_tags(&self, endpoint: &str) -> Result<TagsResponse> {
        let resp = self
            .http
            .get(format!("{endpoint}/api/tags"))
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/tags")?;
        orchestrator_common::http::check_response(resp, "GET /api/tags")
            .await?
            .json()
            .await
            .context("parse tags response")
    }

    pub async fn get_ps(&self, endpoint: &str) -> Result<PsResponse> {
        let resp = self
            .http
            .get(format!("{endpoint}/api/ps"))
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/ps")?;
        orchestrator_common::http::check_response(resp, "GET /api/ps")
            .await?
            .json()
            .await
            .context("parse ps response")
    }

    pub async fn show_model(&self, endpoint: &str, model: &str) -> Result<ShowResponse> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/show"))
            .json(&serde_json::json!({"name": model}))
            .timeout(SHOW_TIMEOUT)
            .send()
            .await
            .context("POST /api/show")?;
        orchestrator_common::http::check_response(resp, "POST /api/show")
            .await?
            .json()
            .await
            .context("parse show response")
    }

    pub async fn get_version(&self, endpoint: &str) -> Result<VersionResponse> {
        let resp = self
            .http
            .get(format!("{endpoint}/api/version"))
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /api/version")?;
        orchestrator_common::http::check_response(resp, "GET /api/version")
            .await?
            .json()
            .await
            .context("parse version response")
    }

    // ── Proxy Forwarding ────────────────────────────────────────

    /// Forward an inference request, returning the raw response for streaming.
    pub async fn forward_request(
        &self,
        endpoint: &str,
        path: &str,
        method: reqwest::Method,
        body: Bytes,
        headers: HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = format!("{endpoint}{path}");
        let mut builder = self.http.request(method, &url).body(body);

        // Forward select headers.
        for key in ["content-type", "accept", "authorization"] {
            if let Some(val) = headers.get(key) {
                if let Ok(val) = val.to_str() {
                    builder = builder.header(key, val);
                }
            }
        }

        let resp = builder.send().await.context("forward request")?;
        Ok(resp)
    }

    // ── Model Management ────────────────────────────────────────

    /// Pull a model, returning a stream of progress events.
    pub async fn pull_model(
        &self,
        endpoint: &str,
        model: &str,
    ) -> Result<impl futures_util::Stream<Item = Result<Bytes, reqwest::Error>> + use<>> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/pull"))
            .json(&serde_json::json!({"name": model, "stream": true}))
            .send()
            .await
            .context("POST /api/pull")?;
        let resp =
            orchestrator_common::http::check_response(resp, "POST /api/pull").await?;
        Ok(resp.bytes_stream())
    }

    pub async fn delete_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let resp = self
            .http
            .delete(format!("{endpoint}/api/delete"))
            .json(&serde_json::json!({"name": model}))
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("DELETE /api/delete")?;
        orchestrator_common::http::check_response(resp, "DELETE /api/delete").await?;
        Ok(())
    }

    /// Load a model into VRAM using the empty-prompt trick.
    pub async fn load_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/generate"))
            .json(&serde_json::json!({"model": model, "prompt": "", "stream": false}))
            .send()
            .await
            .context("load model")?;
        orchestrator_common::http::check_response(resp, "load model").await?;
        Ok(())
    }

    /// Unload a model from VRAM using `keep_alive: 0`.
    pub async fn unload_model(&self, endpoint: &str, model: &str) -> Result<()> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/generate"))
            .json(&serde_json::json!({
                "model": model,
                "prompt": "",
                "stream": false,
                "keep_alive": 0
            }))
            .send()
            .await
            .context("unload model")?;
        orchestrator_common::http::check_response(resp, "unload model").await?;
        Ok(())
    }

    pub async fn health_check(&self, endpoint: &str) -> bool {
        self.get_tags(endpoint).await.is_ok()
    }

    // ── Benchmarking ────────────────────────────────────────────

    pub async fn benchmark_generate(
        &self,
        endpoint: &str,
        model: &str,
        prompt: &str,
        num_predict: u32,
    ) -> Result<InferenceFinal> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/generate"))
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": false,
                "options": {"num_predict": num_predict}
            }))
            .timeout(BENCH_TIMEOUT)
            .send()
            .await
            .context("benchmark generate")?;
        orchestrator_common::http::check_response(resp, "benchmark generate")
            .await?
            .json()
            .await
            .context("parse benchmark response")
    }

    pub async fn benchmark_generate_vision(
        &self,
        endpoint: &str,
        model: &str,
        prompt: &str,
        images_b64: &[String],
        num_predict: u32,
    ) -> Result<InferenceFinal> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/generate"))
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "images": images_b64,
                "stream": false,
                "options": {"num_predict": num_predict}
            }))
            .timeout(BENCH_TIMEOUT)
            .send()
            .await
            .context("benchmark vision")?;
        orchestrator_common::http::check_response(resp, "benchmark vision")
            .await?
            .json()
            .await
            .context("parse vision benchmark response")
    }

    pub async fn benchmark_generate_long(
        &self,
        endpoint: &str,
        model: &str,
        prompt: &str,
        num_predict: u32,
    ) -> Result<InferenceFinal> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/generate"))
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": false,
                "options": {"num_predict": num_predict}
            }))
            .timeout(THINK_TIMEOUT)
            .send()
            .await
            .context("benchmark long generate")?;
        orchestrator_common::http::check_response(resp, "benchmark long generate")
            .await?
            .json()
            .await
            .context("parse long benchmark response")
    }

    pub async fn benchmark_chat_tools(
        &self,
        endpoint: &str,
        model: &str,
        user_message: &str,
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/chat"))
            .json(&serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": user_message}],
                "tools": tools,
                "stream": false
            }))
            .timeout(BENCH_TIMEOUT)
            .send()
            .await
            .context("benchmark tools")?;
        orchestrator_common::http::check_response(resp, "benchmark tools")
            .await?
            .json()
            .await
            .context("parse tools benchmark response")
    }

    pub async fn benchmark_embed(
        &self,
        endpoint: &str,
        model: &str,
        input: &str,
    ) -> Result<EmbedResponse> {
        let resp = self
            .http
            .post(format!("{endpoint}/api/embed"))
            .json(&serde_json::json!({"model": model, "input": input}))
            .timeout(EMBED_TIMEOUT)
            .send()
            .await
            .context("benchmark embed")?;
        orchestrator_common::http::check_response(resp, "benchmark embed")
            .await?
            .json()
            .await
            .context("parse embed benchmark response")
    }

    // ── Full Instance Profile ───────────────────────────────────

    /// Profile an entire Ollama instance: tags + ps + version + per-model show.
    ///
    /// Returns (models_available, models_loaded, model_infos, version).
    pub async fn full_profile(
        &self,
        endpoint: &str,
    ) -> Result<(Vec<String>, Vec<LoadedModel>, Vec<ModelInfo>, Option<String>)> {
        // Parallel: tags + ps + version
        let (tags_result, ps_result, version_result) = tokio::join!(
            self.get_tags(endpoint),
            self.get_ps(endpoint),
            self.get_version(endpoint),
        );

        let tags = tags_result?;
        let ps = ps_result?;
        let version = version_result.ok().map(|v| v.version);

        let models_available: Vec<String> = tags.models.iter().map(|m| m.name.clone()).collect();

        let models_loaded: Vec<LoadedModel> = ps
            .models
            .iter()
            .map(|m| LoadedModel {
                name: m.name.clone(),
                vram_bytes: m.size_vram,
                expires_at: m.expires_at.clone(),
            })
            .collect();

        // Parallel: show per model.
        // Build a lookup map for loaded model VRAM, avoiding closure move issues.
        let loaded_vram: std::collections::HashMap<String, u64> = ps
            .models
            .iter()
            .map(|r| (r.name.clone(), r.size_vram))
            .collect();

        let show_futures: Vec<_> = tags
            .models
            .iter()
            .map(|tag| {
                let model_name = tag.name.clone();
                let size_disk = tag.size;
                let details = tag.details.clone();
                let endpoint = endpoint.to_string();
                let client = self.clone();
                let vram = loaded_vram.get(&model_name).copied();
                async move {
                    let show = client.show_model(&endpoint, &model_name).await.ok();

                    ModelInfo {
                        name: model_name,
                        parameter_count: show.as_ref().and_then(|s| s.parameter_count()),
                        parameter_size: details
                            .as_ref()
                            .and_then(|d| d.parameter_size.clone()),
                        quantization_level: details
                            .as_ref()
                            .and_then(|d| d.quantization_level.clone()),
                        family: details.as_ref().and_then(|d| d.family.clone()),
                        families: details
                            .as_ref()
                            .map(|d| d.families.clone())
                            .unwrap_or_default(),
                        capabilities: show
                            .as_ref()
                            .map(|s| s.capabilities.clone())
                            .unwrap_or_default(),
                        format: details.as_ref().and_then(|d| d.format.clone()),
                        size_disk,
                        vram_bytes: vram,
                        context_length: show.as_ref().and_then(|s| s.context_length()),
                    }
                }
            })
            .collect();

        let model_infos = futures_util::future::join_all(show_futures).await;

        Ok((models_available, models_loaded, model_infos, version))
    }
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}
