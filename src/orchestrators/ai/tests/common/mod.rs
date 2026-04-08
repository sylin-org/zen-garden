//! Shared fixture for all integration / acceptance tests.
//!
//! Exports a builder-style [`Fixture`] that wires an [`AppState`]
//! with a configurable mock provider. Each test binary in
//! `tests/*.rs` imports this via `mod common;`.
//!
//! Docker-era tests that speak HTTP to a running orchestrator use
//! [`garden_probe::GardenHandle`] instead of [`Fixture`].

#![allow(dead_code)]

pub mod garden_probe;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::stream::{self, BoxStream};
use tempfile::TempDir;
use tokio::sync::{watch, Mutex};

use zen_garden_ai_orchestrator::{
    app_state::AppState,
    domain::{
        directory::Directory,
        events::EventBus,
        field_path::FieldPath,
        idempotency::IdempotencyStore,
        ids::{ProviderName, RegistrationId},
        jobs::JobStore,
        keys,
        resources::Resources,
        media::{MediaDelivery, MediaStore},
        output::Output,
        primitive::Primitive,
        provider::{
            HonoredField, MediaInputSpec, Provider, ProviderError, ProviderHealth,
            ProviderOutcome, ProviderState, ProviderStatePublisher, Registration,
            RegistrationStrategy,
        },
        request::OrchestratorRequest,
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
        recommendation::{DemandLedger, PinRegistry, RecommendationEngine},
    },
};

// ── Mock provider with scripted behavior ──────────────────────

pub type Scripted = dyn Fn(OrchestratorRequest) -> Result<ProviderOutcome, ProviderError>
    + Send
    + Sync
    + 'static;

pub struct MockProvider {
    name: ProviderName,
    publisher: ProviderStatePublisher,
    script: Mutex<Arc<Scripted>>,
}

impl MockProvider {
    pub fn new(name: &str, registrations: Vec<Registration>) -> Arc<Self> {
        let name = ProviderName::new(name);
        let initial = ProviderState {
            health: ProviderHealth::Healthy,
            registrations,
            models: Vec::new(),
            performance_hints: Vec::new(),
        };
        let default_script: Arc<Scripted> = Arc::new(|request: OrchestratorRequest| {
            // Default: echo the user prompt as the response.
            let prompt = request
                .payload
                .pointer("/text/prompt/user")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut out = Output::new();
            out.set(
                &keys::text::RESPONSE,
                format!("mock-reply: {prompt}"),
            );
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
            publisher: ProviderStatePublisher::new(initial),
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
        let script = self.script.lock().await.clone();
        script(request)
    }
}

// ── Registration builders ─────────────────────────────────────

pub fn bare_text_chat(name: &str) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: ProviderName::new(name),
        primitive: Primitive::TextChat,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::text::PROMPT_USER).required(),
            HonoredField::new(keys::text::PROMPT_SYSTEM),
            HonoredField::new(keys::text::SAMPLING_TEMPERATURE),
        ],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

pub fn bare_image_analyze_base64(name: &str) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: ProviderName::new(name),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![HonoredField::new(keys::image::SOURCE).required()],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Base64,
            accepted_types: vec!["image/png".to_string(), "image/jpeg".to_string()],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

pub fn bare_image_analyze_by_id(name: &str) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: ProviderName::new(name),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![HonoredField::new(keys::image::SOURCE).required()],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::ById,
            accepted_types: vec!["image/png".to_string()],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

pub fn bare_image_analyze_transfer(name: &str) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: ProviderName::new(name),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![HonoredField::new(keys::image::SOURCE).required()],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Transfer,
            accepted_types: vec!["image/png".to_string()],
            overlay: None,
        }],
        media_outputs: Vec::new(),
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
    pub directory: Arc<Directory>,
    pub media_store: Arc<dyn MediaStore>,
    pub job_store: Arc<dyn JobStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub demand: Arc<DemandLedger>,
    pub recommendation: Arc<RecommendationEngine>,
    pub capability_directory: Arc<CapabilityDirectory>,
    pub directory_subscriber: Arc<DirectorySubscriber>,
}

pub async fn fixture_with_provider(provider: Arc<dyn Provider>) -> Fixture {
    let tmp = TempDir::new().expect("tmp");
    let data_dir = tmp.path().to_path_buf();

    let media_store = DiskMediaStore::load(&data_dir).await.expect("media") as Arc<dyn MediaStore>;
    let job_store = DiskJobStore::load(&data_dir).await.expect("jobs") as Arc<dyn JobStore>;
    let idempotency_store =
        Arc::new(InMemoryIdempotencyStore::new()) as Arc<dyn IdempotencyStore>;

    let vocabularies = VocabularyRegistry::build();
    let directory = Directory::new();

    let pins = Arc::new(PinRegistry::load(&data_dir).await);
    let demand = Arc::new(DemandLedger::new());
    let recommendation =
        RecommendationEngine::new(directory.clone(), pins.clone(), demand.clone());

    let resolver: Arc<
        dyn zen_garden_ai_orchestrator::services::contextualizer::RecommendationResolver,
    > = recommendation.clone();
    let contextualizer = Arc::new(Contextualizer::new(vocabularies.clone(), Some(resolver)));
    let media_resolver = Arc::new(MediaResolver);
    let dispatcher = Arc::new(Dispatcher::new(
        directory.clone(),
        contextualizer,
        media_resolver,
        idempotency_store.clone(),
        demand.clone(),
        job_store.clone(),
        media_store.clone(),
    ));

    let skills = zen_garden_ai_orchestrator::services::skills::Skills::new();
    let provisioning =
        zen_garden_ai_orchestrator::services::skills::ProvisioningQueue::with_default_concurrency();
    let events = EventBus::new();
    let resources = Resources::new(events.clone());
    let catalog = CatalogBuilder::new(
        directory.clone(),
        vocabularies.clone(),
        skills.clone(),
        events.clone(),
    );

    directory.register(provider).await.expect("register");
    directory.rebuild_snapshot().await;
    recommendation.rebuild().await;

    let capability_directory = CapabilityDirectory::new();
    let directory_subscriber =
        DirectorySubscriber::new(capability_directory.clone(), events.clone());

    let state = AppState {
        directory: directory.clone(),
        vocabularies,
        media_store: media_store.clone(),
        job_store: job_store.clone(),
        idempotency_store: idempotency_store.clone(),
        dispatcher,
        recommendation: recommendation.clone(),
        catalog,
        skills,
        provisioning,
        data_dir: data_dir.clone(),
        events,
        resources,
        capability_directory: capability_directory.clone(),
    };

    Fixture {
        state,
        tmp,
        directory,
        media_store,
        job_store,
        idempotency_store,
        demand,
        recommendation,
        capability_directory,
        directory_subscriber,
    }
}

pub async fn fixture_with_mock_chat() -> (Fixture, Arc<MockProvider>) {
    let mock = MockProvider::new("mockchat", vec![bare_text_chat("mockchat")]);
    let fixture = fixture_with_provider(mock.clone()).await;
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
