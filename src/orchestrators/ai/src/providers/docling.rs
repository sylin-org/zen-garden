//! Docling provider — document-to-markdown conversion via docling-serve.
//!
//! Docling-serve API (custom, NOT OpenAI-compatible):
//! - Health: GET /docs (returns 200; no /health endpoint)
//! - Convert: POST /v1/convert/source with JSON `{"source": "base64://...", "options": {"to_format": "md"}}`
//! - Single service, no model listing — enumerate returns one model named "docling"
//!
//! The provider accepts an InferenceRequest, extracts base64 content from message
//! content parts (image_url data URIs), calls Docling's convert endpoint, and
//! returns the extracted markdown as an InferenceResponse.

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const INFER_TIMEOUT: Duration = Duration::from_secs(300);

const DOCLING_CAPABILITIES: &[Capability] = &[Capability::Ocr];

pub struct DoclingProvider {
    http: Client,
}

impl DoclingProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Provider for DoclingProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Docling
    }

    fn capabilities(&self) -> &[Capability] {
        DOCLING_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "docling".to_string(),
        }
    }

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            // Docling-serve has no /health — use /docs as liveness check
            let resp = self
                .http
                .get(format!("{endpoint}/docs"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await
                .context("probe docling /docs")?;

            if !resp.status().is_success() {
                anyhow::bail!("docling health check failed: HTTP {}", resp.status());
            }

            Ok(ProbeResult {
                version: None,
                capabilities: DOCLING_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({"provider": "docling"}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        // Docling is a single-service offering — no model listing API.
        // Return one model named "docling" if the service is alive.
        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/docs"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await;

            if resp.is_err() || !resp.unwrap().status().is_success() {
                return Ok(vec![]);
            }

            Ok(vec![ServiceModel {
                name: "docling".to_string(),
                capabilities: DOCLING_CAPABILITIES.to_vec(),
                specializations: vec![],
                vram_bytes: None,
                metadata: serde_json::json!({
                    "provider": "docling",
                    "note": "Single document conversion service — no model selection",
                }),
            }])
        })
    }

    fn infer(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let base64_data = extract_base64_source(&req)
                .context("no base64 document content found in request messages")?;

            let body = serde_json::json!({
                "source": format!("base64://{base64_data}"),
                "options": {
                    "to_format": "md"
                }
            });

            let resp = self
                .http
                .post(format!("{endpoint}/v1/convert/source"))
                .json(&body)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST docling /v1/convert/source")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("docling convert HTTP {status}: {text}");
            }

            let result: serde_json::Value =
                resp.json().await.context("parse docling response")?;

            let md_content = result
                .get("md_content")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    // Alternative response shape: nested under document.md_content
                    result
                        .get("document")
                        .and_then(|d| d.get("md_content"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("")
                .to_string();

            Ok(InferenceResponse {
                id: format!("docling-{}", chrono::Utc::now().timestamp_millis()),
                object: "chat.completion".to_string(),
                model: "docling".to_string(),
                choices: vec![InferenceChoice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: Some(serde_json::Value::String(md_content)),
                        tool_calls: None,
                        tool_call_id: None,
                        extra: serde_json::Map::new(),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            })
        })
    }

    // ── Form Schema (ORCH-0017) ──────────────────────────────────

    fn form_schema(&self, _model: &str, capability: Capability) -> FormSchema {
        match capability {
            Capability::Ocr => FormSchema {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "title": "URL or description",
                            "description": "Paste a URL to a PDF/image, or upload a file via the file area"
                        }
                    },
                    "required": ["message"]
                }),
                ui_schema: serde_json::json!({
                    "message": {
                        "ui:widget": "textarea",
                        "ui:options": { "rows": 3 }
                    }
                }),
            },
            _ => FormSchema::default(),
        }
    }
}

/// Extract base64 data from the last user message's content parts.
///
/// Looks for:
/// 1. An `image_url` content part with a `data:...;base64,...` URI
/// 2. Fallback: plain string content treated as raw base64
fn extract_base64_source(req: &InferenceRequest) -> Option<String> {
    let last_user = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")?;

    let content = last_user.content.as_ref()?;

    // Case 1: content is an array of parts (OpenAI multimodal format)
    if let Some(parts) = content.as_array() {
        for part in parts {
            if part.get("type").and_then(|t| t.as_str()) == Some("image_url") {
                if let Some(url) = part
                    .get("image_url")
                    .and_then(|u| u.get("url"))
                    .and_then(|u| u.as_str())
                {
                    // Strip data URI prefix: "data:application/pdf;base64,..." → base64 payload
                    if let Some((_prefix, data)) = url.split_once(";base64,") {
                        return Some(data.to_string());
                    }
                    // Already raw base64
                    return Some(url.to_string());
                }
            }
        }
    }

    // Case 2: content is a plain string — treat as raw base64
    if let Some(text) = content.as_str() {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = DoclingProvider::new();
        assert_eq!(p.kind(), OfferingKind::Docling);
        assert_eq!(p.capabilities(), &[Capability::Ocr]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let p = DoclingProvider::new();
        match p.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "docling");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn extract_base64_from_image_url_part() {
        let req = InferenceRequest {
            model: "docling".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: Some(serde_json::json!([
                    {"type": "text", "text": "Extract text from this document"},
                    {"type": "image_url", "image_url": {"url": "data:application/pdf;base64,AQIDBA=="}}
                ])),
                tool_calls: None,
                tool_call_id: None,
                extra: serde_json::Map::new(),
            }],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
            stream: false,
            extra: serde_json::Map::new(),
        };

        let data = extract_base64_source(&req).unwrap();
        assert_eq!(data, "AQIDBA==");
    }
}
