//! Docling provider — `image.analyze.ocr`.
//!
//! Docling (https://github.com/DS4SD/docling) runs document layout
//! analysis and OCR on uploaded images/PDFs. Registered as a
//! skill-oriented action under `image.analyze/ocr`; callers hit
//! `POST /v1/image/analyze/ocr` with a media reference and get
//! back extracted text.
//!
//! Wire: `POST /v1/convert/file` with a multipart `files` field.
//! Returns JSON with `document.md_content`, `document.text_content`,
//! and other per-element metadata.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::watch;

use crate::domain::ids::{ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::MediaDelivery;
use crate::domain::moniker::Moniker;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaInputSpec, Provider, ProviderError, ProviderHealth, ProviderOutcome,
    ProviderState, ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use crate::services::garden_discovery::GardenDiscovery;
use tokio_util::sync::CancellationToken;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

const FQNS: &[&'static str] = &["docling"];

#[derive(Debug, Clone, Default)]
pub struct DoclingConfig;

pub struct DoclingProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    http: Client,
    publisher: ProviderStatePublisher,
}

fn build_registration(name: &ProviderName) -> Registration {
    let moniker = Moniker::new("ocr").expect("valid skill moniker");
    Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Skill {
            moniker,
            display_name: "Docling OCR".to_string(),
            description: Some(
                "Document layout + OCR via Docling. Returns extracted text and markdown."
                    .to_string(),
            ),
        },
        honored_fields: vec![HonoredField::new(keys::image::SOURCE).required()],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Transfer,
            accepted_types: vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/tiff".to_string(),
                "application/pdf".to_string(),
            ],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

impl DoclingProvider {
    pub fn new(
        _config: DoclingConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::DOCLING);
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
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no docling instances in the garden".to_string())
        })
    }
}

impl DoclingProvider {
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

fn spawn_subscriber(
    provider: Arc<DoclingProvider>,
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
                    let urls: Vec<String> = event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten());
                }
            }
        }
    });
}

#[async_trait]
impl Provider for DoclingProvider {
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
        if request.action.primitive != Primitive::ImageAnalyze {
            return Err(ProviderError::Unsupported(format!(
                "docling does not serve {}",
                request.action.primitive.dotted()
            )));
        }

        let media_ref = request
            .media
            .find_at_field(&keys::image::SOURCE)
            .ok_or_else(|| {
                ProviderError::Unsupported("image.source media reference missing".to_string())
            })?;
        let bytes = request
            .context
            .media_store
            .get_bytes(&media_ref.id)
            .await
            .map_err(|e| ProviderError::Internal(format!("media fetch: {e}")))?;
        let meta = request
            .context
            .media_store
            .get_metadata(&media_ref.id)
            .await
            .map_err(|e| ProviderError::Internal(format!("media meta: {e}")))?;

        let filename = format!(
            "{}{}",
            media_ref.id,
            match meta.content_type.as_str() {
                "image/png" => ".png",
                "image/jpeg" => ".jpg",
                "image/tiff" => ".tiff",
                "application/pdf" => ".pdf",
                _ => ".bin",
            }
        );
        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(filename)
            .mime_str(&meta.content_type)
            .map_err(|e| ProviderError::Internal(format!("mime: {e}")))?;
        let form = reqwest::multipart::Form::new().part("files", part);

        let base = self.pick()?;
        let endpoint = format!(
            "{}/v1/convert/file",
            base.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&endpoint)
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "docling convert").await?;
        let wire: ConvertResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut text = wire
            .document
            .as_ref()
            .and_then(|d| d.md_content.clone())
            .unwrap_or_default();
        if text.is_empty() {
            text = wire
                .document
                .as_ref()
                .and_then(|d| d.text_content.clone())
                .unwrap_or_default();
        }

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, text);
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ConvertResponse {
    #[serde(default)]
    document: Option<ConvertDocument>,
}

#[derive(Debug, Deserialize)]
struct ConvertDocument {
    #[serde(default)]
    md_content: Option<String>,
    #[serde(default)]
    text_content: Option<String>,
}
