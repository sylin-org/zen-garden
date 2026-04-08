//! LibreTranslate provider — `text.translate`.
//!
//! Wire API:
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

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::domain::ids::{ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    Provider, ProviderError, ProviderHealth, ProviderOutcome, ProviderState,
    ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

/// Garden offering FQNs this adapter claims. LibreTranslate has a
/// single canonical offering name; variants would be added here.
const FQNS: &[&'static str] = &["libretranslate"];

/// Static configuration for a LibreTranslate provider. The instance
/// list is supplied by the garden discovery service at runtime —
/// the only piece needed up front is an optional API key
/// (LibreTranslate public instances may require one).
#[derive(Debug, Clone, Default)]
pub struct LibreTranslateConfig {
    pub api_key: Option<String>,
}

pub struct LibreTranslateProvider {
    name: ProviderName,
    config: LibreTranslateConfig,
    instances: Arc<InstancePool>,
    http: Client,
    publisher: ProviderStatePublisher,
}

fn build_registration(name: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::TextTranslate,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            crate::domain::provider::HonoredField::new(keys::text::BODY).required(),
            crate::domain::provider::HonoredField::new(keys::text::LANGUAGE_TARGET).required(),
            crate::domain::provider::HonoredField::new(keys::text::LANGUAGE_SOURCE),
        ],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

impl LibreTranslateProvider {
    /// Construct the adapter and immediately spawn its garden
    /// discovery subscriber. The provider starts in `Offline` and
    /// flips to `Healthy` when discovery delivers its first
    /// non-empty event.
    pub fn new(
        config: LibreTranslateConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::LIBRETRANSLATE);
        let initial = ProviderState {
            health: ProviderHealth::Offline {
                reason: "no garden instances discovered yet".to_string(),
            },
            registrations: vec![build_registration(&name)],
            models: Vec::new(),
            performance_hints: Vec::new(),
        };
        let provider = Arc::new(Self {
            name,
            config,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
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

impl LibreTranslateProvider {
    /// Apply a fresh merged URL list (called by the subscriber task
    /// whenever the garden discovery service emits an event for any
    /// of the FQNs this adapter claims).
    fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        let count = self.instances.len();
        let name = self.name.clone();
        self.publisher.modify(move |mut state| {
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state.registrations = vec![build_registration(&name)];
            state
        });
    }
}

/// Subscribe this adapter to its declared FQNs and update the pool
/// on every event.
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
                    let urls: Vec<String> = event
                        .instances
                        .into_iter()
                        .map(|i| i.url)
                        .collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten());
                }
            }
        }
    });
}

#[async_trait]
impl Provider for LibreTranslateProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    fn state(&self) -> Arc<ProviderState> {
        self.publisher.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>> {
        self.publisher.subscribe()
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
        let response = self.call_translate(&endpoint, &payload).await?;

        let mut out = Output::new();
        out.set(&keys::text::TRANSLATED, response.translated_text);
        if source == "auto" {
            if let Some(detected) = response.detected_language {
                out.set(&keys::text::DETECTED_LANGUAGE, detected.language);
            }
        }
        out.set(
            &keys::usage::CHARACTERS,
            body.chars().count() as u64,
        );
        Ok(ProviderOutcome::Sync(out))
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
