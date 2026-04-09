//! LibreTranslate provider — capability-event driven (ORCH-0030 R2 M3).
//!
//! LibreTranslate exposes a single primitive, [`Primitive::TextTranslate`],
//! backed by a small set of in-garden instances discovered via
//! [`GardenDiscovery`]. The adapter maintains a round-robin
//! [`InstancePool`] and publishes a [`CapabilityAnnouncement`] to the
//! event bus on every pool change so the
//! [`CapabilityDirectory`](crate::services::directory_subscriber::CapabilityDirectory)
//! can route `text.translate` requests to it.
//!
//! LibreTranslate has no model concept — the engine is fixed per
//! instance — so `onboard` ignores `request.selectors.model` entirely.
//!
//! # Wire API
//!
//! ```text
//! POST /translate
//! { "q": "...", "source": "auto", "target": "en", "format": "text" }
//! ```
//!
//! Response:
//!
//! ```text
//! { "translatedText": "...", "detectedLanguage": {"language": "fr", "confidence": 0.99} }
//! ```
//!
//! Language codes are passed through to LibreTranslate untouched. An
//! unsupported language is surfaced as an upstream error rather than
//! being pre-validated — probing every instance's `/languages` would
//! double per-request cost for negligible benefit.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

/// Garden offering FQNs this adapter claims. LibreTranslate has a
/// single canonical offering name; variants (e.g. `libretranslate::dev`)
/// are picked up automatically by the discovery base-name match.
const FQNS: &[&'static str] = &["libretranslate"];

/// Static configuration for a LibreTranslate provider. The instance
/// list is supplied by the garden discovery service at runtime — the
/// only piece needed up front is an optional API key (public
/// LibreTranslate instances may require one).
#[derive(Debug, Clone, Default)]
pub struct LibreTranslateConfig {
    pub api_key: Option<String>,
}

pub struct LibreTranslateProvider {
    name: ProviderName,
    config: LibreTranslateConfig,
    instances: Arc<InstancePool>,
    http: Client,
    events: Arc<EventBus>,
}

impl LibreTranslateProvider {
    /// Construct the adapter and immediately spawn its garden
    /// discovery subscriber. The adapter publishes a disabled
    /// capability announcement until discovery delivers its first
    /// non-empty event, at which point it flips to `enabled: true`.
    pub fn new(
        config: LibreTranslateConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::LIBRETRANSLATE);
        let provider = Arc::new(Self {
            name,
            config,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Publish the current pool state as a capability announcement.
    /// Called by the discovery subscriber on every pool change.
    async fn publish_capabilities(&self) {
        let announcement =
            build_capability_announcement(&self.name, !self.instances.is_empty());
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Apply a fresh merged URL list. Returns `true` if the pool
    /// changed structurally (so the caller can publish a fresh
    /// announcement).
    fn apply_merged(&self, urls: Vec<String>) -> bool {
        self.instances.set(urls)
    }

    async fn call_translate(
        &self,
        endpoint: &str,
        payload: &WirePayload<'_>,
    ) -> Result<WireResponse, ProviderError> {
        let url = format!("{}/translate", endpoint.trim_end_matches('/'));
        let resp = self
            .http
            .post(&url)
            .json(payload)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "libretranslate translate").await?;
        resp.json::<WireResponse>()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))
    }
}

// ── Discovery subscriber ─────────────────────────────────────

/// Subscribe this adapter to its declared FQNs and update the pool
/// on every event. Publishes a fresh capability announcement after
/// every accepted pool update so the CapabilityDirectory mirrors the
/// adapter's current routability.
fn spawn_subscriber(
    provider: Arc<LibreTranslateProvider>,
    discovery: Arc<GardenDiscovery>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let pool = PerFqnInstances::new();
        let mut rx = discovery.subscribe(FQNS).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    let urls: Vec<String> =
                        event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    if provider.apply_merged(pool.flatten()) {
                        provider.publish_capabilities().await;
                    }
                }
            }
        }
    });
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for LibreTranslateProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let endpoint = self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(
                "no libretranslate instances are running in the garden".to_string(),
            )
        })?;

        let body = request
            .payload
            .pointer("/text/body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing text.body".to_string()))?;
        let target = request
            .payload
            .pointer("/text/language/target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ProviderError::Unsupported("missing text.language.target".to_string())
            })?;
        let source = request
            .payload
            .pointer("/text/language/source")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        let payload = WirePayload {
            q: body,
            source,
            target,
            format: "text",
            api_key: self.config.api_key.as_deref(),
        };

        tracing::debug!(
            provider = %self.name,
            request_id = %request.id,
            instance = %endpoint,
            source = %source,
            target = %target,
            "libretranslate translate dispatch",
        );

        let response = self.call_translate(&endpoint, &payload).await?;

        let mut out = Output::new();
        out.set(&keys::text::TRANSLATED, response.translated_text);
        if source == "auto" {
            if let Some(detected) = response.detected_language {
                out.set(&keys::text::DETECTED_LANGUAGE, detected.language);
            }
        }
        out.set(&keys::usage::CHARACTERS, body.chars().count() as u64);
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement LibreTranslate publishes given
/// the current instance pool state. LibreTranslate serves exactly
/// one primitive (`text.translate`), has no media inputs, and
/// publishes no skills.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
) -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled: has_instances,
        capabilities: vec![AnnCapability {
            primitive: Primitive::TextTranslate,
            media_inputs: Vec::new(),
            parameters: vec![],
        }],
        skills: Vec::new(),
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WirePayload<'a> {
    q: &'a str,
    source: &'a str,
    target: &'a str,
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "api_key")]
    api_key: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
    #[serde(default, rename = "detectedLanguage")]
    detected_language: Option<DetectedLanguage>,
}

#[derive(Debug, Deserialize)]
struct DetectedLanguage {
    language: String,
    #[serde(default)]
    #[allow(dead_code)]
    confidence: f64,
}

// ── Tests (ORCH-0030 R2 M4) ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::LIBRETRANSLATE)
    }

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "libretranslate");
        assert!(!ann.enabled);
    }

    #[test]
    fn announcement_enabled_when_instances_present() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert!(ann.enabled);
    }

    #[test]
    fn announcement_declares_text_translate_with_no_media_inputs() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.capabilities.len(), 1);
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::TextTranslate);
        // text.translate is text-in / text-out — no media references.
        assert!(cap.media_inputs.is_empty());
    }

    #[test]
    fn announcement_publishes_no_skills() {
        let ann = build_capability_announcement(&provider_name(), true);
        // LibreTranslate has no per-skill specializations; the
        // primitive is the entire surface.
        assert!(ann.skills.is_empty());
    }
}
