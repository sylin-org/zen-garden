//! Docling provider — capability-event driven (ORCH-0030 R2 M3).
//!
//! Docling (https://github.com/DS4SD/docling) runs document layout
//! analysis and OCR on uploaded images/PDFs. After M3, Docling
//! publishes a [`CapabilityAnnouncement`] on the bus declaring the
//! `image.analyze` primitive plus the `ocr` skill, and the
//! `CapabilityDirectory` (populated by `DirectorySubscriber`) is the
//! authoritative view of what Docling can serve.
//!
//! # Wire
//!
//! `POST /v1/convert/file` with a multipart `files` field. Returns
//! JSON with `document.md_content`, `document.text_content`, and
//! other per-element metadata. Docling has no model concept — the
//! adapter ignores `selectors.model` entirely.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput, ParameterType,
    ParameterWidget, SkillDeclaration, SkillDisplay, SkillParameter,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::media::MediaDelivery;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderMeta, ProviderResult};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, truncate_str, InstancePool,
    PerFqnInstances,
};

/// Docling base name. Discovery's base-name match picks up `docling`
/// and any `docling::adopted`, `docling::dev`, etc. variants.
const FQNS: &[&'static str] = &["docling"];

#[derive(Debug, Clone, Default)]
pub struct DoclingConfig;

pub struct DoclingProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    http: Client,
    events: Arc<EventBus>,
}

impl DoclingProvider {
    pub fn new(
        _config: DoclingConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::DOCLING);
        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Round-robin pick of the next instance base URL.
    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable("no docling instances in the garden".to_string())
        })
    }

    /// Publish the current instance pool state as a capability
    /// announcement. Called every time the instance pool changes.
    async fn publish_capabilities(&self) {
        let announcement =
            build_capability_announcement(&self.name, !self.instances.is_empty());
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Apply a merged URL list from discovery. Publishes a fresh
    /// capability announcement if the pool structurally changed.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.publish_capabilities().await;
    }
}

// ── Discovery subscriber ─────────────────────────────────────

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
                    let urls: Vec<String> =
                        event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten()).await;
                }
            }
        }
    });
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for DoclingProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderResult, ProviderError> {
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
        let endpoint = format!("{}/v1/convert/file", base.trim_end_matches('/'));
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

        let summary = format!("→ '{}'", truncate_str(&text, 30));
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, text);
        Ok(ProviderResult::sync_with(
            out,
            ProviderMeta {
                instance: Some(base),
                summary: Some(summary),
                ..Default::default()
            },
        ))
    }
}

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement Docling publishes given the
/// current instance pool state. Pure function — no IO, no &self —
/// so unit tests can exercise the wire shape directly.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
) -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled: has_instances,
        capabilities: vec![AnnCapability {
            primitive: Primitive::ImageAnalyze,
            media_inputs: vec![CapabilityMediaInput {
                field: keys::image::SOURCE.as_str().to_string(),
                delivery: MediaDelivery::Transfer,
                accepted_types: vec![
                    "image/png".to_string(),
                    "image/jpeg".to_string(),
                    "image/tiff".to_string(),
                    "application/pdf".to_string(),
                ],
                overlay: None,
            }],
            parameters: vec![
                SkillParameter { field: "image.source".into(), required: true, label: Some("Document".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::File), ..Default::default() },
            ],
        }],
        skills: vec![SkillDeclaration {
            id: "ocr".to_string(),
            primitive: Primitive::ImageAnalyze,
            display: SkillDisplay::new("Docling OCR").with_description(
                "Document layout + OCR via Docling. Returns extracted text and markdown.",
            ),
            parameters: vec![SkillParameter {
                field: keys::image::SOURCE.as_str().to_string(),
                required: true,
                description: Some("The document/image to analyze.".into()),
                default: None,
                auto: None,
                pinnable: false,
                label: None,
                field_type: None,
                widget: None,
                min: None,
                max: None,
                step: None,
                options: None,
                placeholder: None,
            }],
        }],
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

// ── Tests (ORCH-0030 R2 M4) ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::DOCLING)
    }

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "docling");
        assert!(!ann.enabled);
    }

    #[test]
    fn announcement_enabled_when_instances_present() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert!(ann.enabled);
    }

    #[test]
    fn announcement_declares_image_analyze_with_transfer_media_input() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.capabilities.len(), 1);
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::ImageAnalyze);
        assert_eq!(cap.media_inputs.len(), 1);
        let media = &cap.media_inputs[0];
        assert_eq!(media.field, "image.source");
        assert!(matches!(media.delivery, MediaDelivery::Transfer));
        assert!(media.accepted_types.contains(&"image/png".to_string()));
        assert!(media.accepted_types.contains(&"application/pdf".to_string()));
    }

    #[test]
    fn announcement_declares_one_ocr_skill() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.skills.len(), 1);
        let skill = &ann.skills[0];
        assert_eq!(skill.id, "ocr");
        assert_eq!(skill.primitive, Primitive::ImageAnalyze);
        assert_eq!(skill.display.name, "Docling OCR");
        assert!(skill.display.description.is_some());
        // The skill exposes one parameter for image.source.
        assert_eq!(skill.parameters.len(), 1);
        let param = &skill.parameters[0];
        assert_eq!(param.field, "image.source");
        assert!(param.required);
        assert!(!param.pinnable);
    }
}

