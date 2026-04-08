//! Shared fixture for all integration / acceptance tests
//! (ORCH-0030 R2 M3 shape).
//!
//! Exports a builder-style [`Fixture`] that wires an [`AppState`]
//! with a configurable mock provider. Each test binary in
//! `tests/*.rs` imports this via `mod common;`.
//!
//! All M1 tests are in-process: they call `app.oneshot(req)` against
//! a router built from a synthetic [`Fixture`]. Live HTTP tests
//! against a running orchestrator land in M5.

#![allow(dead_code)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::stream::{self, BoxStream};
use tempfile::TempDir;
use tokio::sync::Mutex;

use zen_garden_ai_orchestrator::{
    app_state::AppState,
    domain::{
        capability_announcement::{
            Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput,
        },
        events::EventBus,
        field_path::FieldPath,
        idempotency::IdempotencyStore,
        ids::ProviderName,
        jobs::JobStore,
        keys,
        media::{MediaDelivery, MediaStore},
        output::Output,
        primitive::Primitive,
        provider::{Provider, ProviderError, ProviderOutcome},
        request::OrchestratorRequest,
        resources::Resources,
        vocabulary::VocabularyRegistry,
    },
    services::{
        catalog_builder::CatalogBuilder,
        contextualizer::Contextualizer,
        directory_subscriber::{CapabilityDirectory, DirectorySubscriber},
        dispatcher::Dispatcher,
        idempotency_store::InMemoryIdempotencyStore,
        job_store::DiskJobStore,
        media_resolver::MediaResolver,
        media_store::DiskMediaStore,
        provider_registry::ProviderRegistry,
    },
};

// ── Mock provider with scripted behavior ──────────────────────

pub type Scripted = dyn Fn(OrchestratorRequest) -> Result<ProviderOutcome, ProviderError>
    + Send
    + Sync
    + 'static;

/// Test-only mock provider implementing the lean ORCH-0030 R2 M3
/// `Provider` trait. Each instance carries a swappable script.
pub struct MockProvider {
    name: ProviderName,
    script: Mutex<Arc<Scripted>>,
}

impl MockProvider {
    /// Construct a mock provider that publishes the given capabilities
    /// when the fixture wires it into the `CapabilityDirectory`.
    pub fn new(name: &str) -> Arc<Self> {
        let name = ProviderName::new(name);
        let default_script: Arc<Scripted> = Arc::new(|request: OrchestratorRequest| {
            // Default: echo the user prompt as the response.
            let prompt = request
                .payload
                .pointer("/text/prompt/user")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut out = Output::new();
            out.set(&keys::text::RESPONSE, format!("mock-reply: {prompt}"));
            out.set(
                &keys::text::FINISH_REASON,
                keys::text::values::FINISH_REASON_STOP,
            );
            out.set(&keys::usage::TOKENS_INPUT, 3);
            out.set(&keys::usage::TOKENS_OUTPUT, 4);
            Ok(ProviderOutcome::Sync(out))
        });
        Arc::new(Self {
            name,
            script: Mutex::new(default_script),
        })
    }

    /// Replace the scripted behavior at runtime.
    pub async fn set_script<F>(&self, f: F)
    where
        F: Fn(OrchestratorRequest) -> Result<ProviderOutcome, ProviderError>
            + Send
            + Sync
            + 'static,
    {
        *self.script.lock().await = Arc::new(f);
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let script = self.script.lock().await.clone();
        script(request)
    }
}

// ── Capability builders ───────────────────────────────────────
//
// Replace the legacy `Registration` builders. Each builder produces
// a `Capability` for one primitive, with the appropriate
// `CapabilityMediaInput`s. Tests assemble a `CapabilityAnnouncement`
// from these and publish it via the fixture.

pub fn cap_text_chat() -> AnnCapability {
    AnnCapability::new(Primitive::TextChat)
}

pub fn cap_image_analyze_base64() -> AnnCapability {
    AnnCapability {
        primitive: Primitive::ImageAnalyze,
        media_inputs: vec![CapabilityMediaInput {
            field: keys::image::SOURCE.as_str().to_string(),
            delivery: MediaDelivery::Base64,
            accepted_types: vec!["image/png".to_string(), "image/jpeg".to_string()],
            overlay: None,
        }],
    }
}

pub fn cap_image_analyze_by_id() -> AnnCapability {
    AnnCapability {
        primitive: Primitive::ImageAnalyze,
        media_inputs: vec![CapabilityMediaInput {
            field: keys::image::SOURCE.as_str().to_string(),
            delivery: MediaDelivery::ById,
            accepted_types: vec!["image/png".to_string()],
            overlay: None,
        }],
    }
}

pub fn cap_image_analyze_transfer() -> AnnCapability {
    AnnCapability {
        primitive: Primitive::ImageAnalyze,
        media_inputs: vec![CapabilityMediaInput {
            field: keys::image::SOURCE.as_str().to_string(),
            delivery: MediaDelivery::Transfer,
            accepted_types: vec!["image/png".to_string()],
            overlay: None,
        }],
    }
}

// ── Scripted outcome helpers ──────────────────────────────────

pub fn sync_outcome(response: &str) -> ProviderOutcome {
    let mut out = Output::new();
    out.set(&keys::text::RESPONSE, response);
    out.set(
        &keys::text::FINISH_REASON,
        keys::text::values::FINISH_REASON_STOP,
    );
    ProviderOutcome::Sync(out)
}

pub fn async_outcome(job_id_str: &str) -> ProviderOutcome {
    let mut out = Output::new();
    out.set(&keys::job::ID, job_id_str);
    out.set(&keys::job::STATUS, keys::job::values::STATUS_RUNNING);
    ProviderOutcome::Async(out)
}

pub fn streaming_outcome_two_chunks() -> ProviderOutcome {
    let mut initial = Output::new();
    initial.set(&keys::text::RESPONSE, "");
    let mut a = Output::new();
    a.set(&keys::text::RESPONSE, "hello");
    let mut b = Output::new();
    b.set(&keys::text::RESPONSE, " world");
    let stream: BoxStream<'static, Result<Output, ProviderError>> =
        Box::pin(stream::iter(vec![Ok(a), Ok(b)]));
    ProviderOutcome::Streaming { initial, stream }
}

// ── Fixture builder ───────────────────────────────────────────

pub struct Fixture {
    pub state: AppState,
    pub tmp: TempDir,
    pub provider_registry: Arc<ProviderRegistry>,
    pub media_store: Arc<dyn MediaStore>,
    pub job_store: Arc<dyn JobStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub capability_directory: Arc<CapabilityDirectory>,
    pub directory_subscriber: Arc<DirectorySubscriber>,
    pub events: Arc<EventBus>,
}

/// Build a `Fixture` populated with `provider`, which is registered
/// in the `ProviderRegistry` and announced via a synthetic
/// `CapabilityAnnouncement` containing the supplied capabilities.
pub async fn fixture_with_provider_capabilities(
    provider: Arc<dyn Provider>,
    capabilities: Vec<AnnCapability>,
) -> Fixture {
    let tmp = TempDir::new().expect("tmp");
    let data_dir = tmp.path().to_path_buf();

    let media_store =
        DiskMediaStore::load(&data_dir).await.expect("media") as Arc<dyn MediaStore>;
    let job_store = DiskJobStore::load(&data_dir).await.expect("jobs") as Arc<dyn JobStore>;
    let idempotency_store =
        Arc::new(InMemoryIdempotencyStore::new()) as Arc<dyn IdempotencyStore>;

    let vocabularies = VocabularyRegistry::build();

    let events = EventBus::new();
    let resources = Resources::new(events.clone());
    let provisioning =
        zen_garden_ai_orchestrator::services::skills::ProvisioningQueue::with_default_concurrency();

    let capability_directory = CapabilityDirectory::new();
    let directory_subscriber =
        DirectorySubscriber::new(capability_directory.clone(), events.clone());

    let provider_registry = ProviderRegistry::new();
    let provider_name = provider.name();
    provider_registry.register(provider).await;

    // Publish a synthetic announcement so the directory knows what
    // the mock provider can serve.
    let announcement = CapabilityAnnouncement {
        provider: provider_name,
        enabled: true,
        capabilities,
        skills: Vec::new(),
    };
    directory_subscriber
        .apply(announcement)
        .await
        .expect("apply test announcement");

    let contextualizer = Arc::new(Contextualizer::new(vocabularies.clone()));
    let media_resolver = Arc::new(MediaResolver);
    let dispatcher = Arc::new(Dispatcher::new(
        capability_directory.clone(),
        provider_registry.clone(),
        contextualizer,
        media_resolver,
        idempotency_store.clone(),
        job_store.clone(),
        media_store.clone(),
    ));

    let catalog = CatalogBuilder::new(
        capability_directory.clone(),
        vocabularies.clone(),
        events.clone(),
    );

    let state = AppState {
        vocabularies,
        media_store: media_store.clone(),
        job_store: job_store.clone(),
        idempotency_store: idempotency_store.clone(),
        dispatcher,
        catalog,
        provisioning,
        data_dir: data_dir.clone(),
        events: events.clone(),
        resources,
        capability_directory: capability_directory.clone(),
        provider_registry: provider_registry.clone(),
    };

    Fixture {
        state,
        tmp,
        provider_registry,
        media_store,
        job_store,
        idempotency_store,
        capability_directory,
        directory_subscriber,
        events,
    }
}

/// Convenience: provider that serves only `text.chat`.
pub async fn fixture_with_mock_chat() -> (Fixture, Arc<MockProvider>) {
    let mock = MockProvider::new("mockchat");
    let fixture =
        fixture_with_provider_capabilities(mock.clone(), vec![cap_text_chat()]).await;
    (fixture, mock)
}

// ── HTTP helpers ──────────────────────────────────────────────

pub async fn body_json(body: axum::body::Body) -> serde_json::Value {
    let bytes = axum::body::to_bytes(body, 16 * 1024 * 1024)
        .await
        .expect("body bytes");
    serde_json::from_slice(&bytes).expect("json")
}

pub fn post_json(path: &str, body: serde_json::Value) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::post(path)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&body).expect("serialize"),
        ))
        .expect("build")
}

pub fn get(path: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::get(path)
        .body(axum::body::Body::empty())
        .expect("build")
}

/// Silence unused-warning on utility re-imports.
fn _keep_alive() {
    let _ = (Utc::now, FieldPath::new("x"));
}
