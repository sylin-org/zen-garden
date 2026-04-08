//! The Dispatcher — a ten-line coordinator that owns the request
//! pipeline from raw caller intent through provider handoff.
//!
//! Responsibilities (ADR §Pipeline + §Job model):
//! - Pre-create a `Job` record before calling `onboard`.
//! - Assemble the `ExecutionContext` (media store, job sink, cancel,
//!   span).
//! - Run the contextualizer.
//! - Consult the idempotency cache.
//! - Run the media resolver.
//! - Look up the provider by resolved name.
//! - Call `onboard`.
//! - Reserve referenced media for Async/Streaming outcomes (bound
//!   to the job id).
//! - Persist a synchronous result in the idempotency cache or, for
//!   async outcomes, the job reference.
//! - Update the demand ledger.
//!
//! No per-primitive logic; no per-provider branching.

use std::sync::Arc;

use serde_json::Value;

use crate::domain::directory::Directory;
use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::idempotency::{
    CachedResponse, ContentFingerprint, IdempotencyKey, IdempotencyRecord, IdempotencyStore,
};
use crate::domain::ids::JobId;
use crate::domain::jobs::{JobCategory, JobSink, JobStore};
use crate::domain::keys;
use crate::domain::media::{MediaReservation, SharedMediaStore};
use crate::domain::provider::ProviderOutcome;
use crate::domain::request::{ExecutionContext, OrchestratorRequest, RawRequest};
use crate::services::contextualizer::Contextualizer;
use crate::services::media_resolver::MediaResolver;
use crate::services::recommendation::{DemandKey, DemandLedger};

/// The outcome returned to HTTP handlers.
pub enum DispatchResult {
    Fresh(ProviderOutcome, OrchestratorRequest),
    Cached(IdempotencyRecord, OrchestratorRequest),
}

impl std::fmt::Debug for DispatchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispatchResult::Fresh(_, req) => f
                .debug_struct("DispatchResult::Fresh")
                .field("request_id", &req.id)
                .field("action", &req.action.dotted())
                .finish(),
            DispatchResult::Cached(rec, req) => f
                .debug_struct("DispatchResult::Cached")
                .field("request_id", &req.id)
                .field("action", &req.action.dotted())
                .field("cached_at", &rec.stored_at)
                .finish(),
        }
    }
}

pub struct Dispatcher {
    directory: Arc<Directory>,
    contextualizer: Arc<Contextualizer>,
    media_resolver: Arc<MediaResolver>,
    idempotency: Arc<dyn IdempotencyStore>,
    demand: Arc<DemandLedger>,
    job_store: Arc<dyn JobStore>,
    media_store: SharedMediaStore,
}

impl Dispatcher {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        directory: Arc<Directory>,
        contextualizer: Arc<Contextualizer>,
        media_resolver: Arc<MediaResolver>,
        idempotency: Arc<dyn IdempotencyStore>,
        demand: Arc<DemandLedger>,
        job_store: Arc<dyn JobStore>,
        media_store: SharedMediaStore,
    ) -> Self {
        Self {
            directory,
            contextualizer,
            media_resolver,
            idempotency,
            demand,
            job_store,
            media_store,
        }
    }

    pub async fn dispatch(
        &self,
        raw: RawRequest,
    ) -> Result<DispatchResult, OrchestratorError> {
        // 1. Pre-create a job record. Every request has a job from
        //    the moment it enters the pipeline so observers can poll
        //    it regardless of outcome mode.
        let job = self
            .job_store
            .create(
                raw.correlation_id.clone(),
                JobCategory::Api,
                None,
                Some(raw.action.clone()),
                Value::Null,
            )
            .await
            .map_err(|e| {
                OrchestratorError::new(
                    ErrorCode::InternalError,
                    format!("failed to create job record: {e}"),
                )
            })?;
        let job_id = job.id.clone();
        let job_sink = Arc::new(JobSink::new(job_id.clone(), self.job_store.clone()));

        // 2. Build the execution context from dispatcher-held deps.
        let context = ExecutionContext {
            media_store: self.media_store.clone(),
            job_sink,
            cancel: raw.cancel.clone(),
            span: raw.span.clone(),
        };

        let request = OrchestratorRequest {
            id: raw.id,
            correlation_id: raw.correlation_id,
            received_at: raw.received_at,
            action: raw.action,
            payload: raw.payload,
            selectors: raw.selectors,
            constraints: raw.constraints,
            media: crate::domain::request::MediaContext::default(),
            resolved_provider: None,
            resolved_model: None,
            context,
        };

        // 3. Contextualize.
        let snapshot = self.directory.snapshot();
        let request = match self.contextualizer.resolve(request, &snapshot).await {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .job_store
                    .fail(
                        &job_id,
                        serde_json::json!({
                            "code": e.code.as_str(),
                            "message": e.message,
                        }),
                    )
                    .await;
                return Err(e);
            }
        };

        // 4. Check idempotency cache.
        //
        // Two-hash scheme (§ADR Acceptance-7):
        //   - `idem_key`     identifies the cache slot (header + action)
        //   - `idem_print`   captures what the dispatcher would actually
        //                    have executed (canonicalized payload +
        //                    selectors). Equal slots with differing
        //                    fingerprints are a `422 idempotency_conflict`.
        let (idem_key, idem_print) = match self.build_idempotency_pair(&request) {
            Some(pair) => (Some(pair.0), Some(pair.1)),
            None => (None, None),
        };
        if let (Some(key), Some(fingerprint)) = (idem_key.as_ref(), idem_print.as_ref()) {
            match self.idempotency.lookup(key).await {
                Ok(Some(record)) => {
                    if &record.fingerprint != fingerprint {
                        // Same key, different content — the user broke
                        // their own promise. 422 per the ADR taxonomy.
                        let _ = self.job_store.cancel(&job_id).await;
                        return Err(OrchestratorError::new(
                            ErrorCode::IdempotencyConflict,
                            "Idempotency-Key reused with a different request body.",
                        )
                        .with_details(serde_json::json!({
                            "idempotency_key": key.as_str(),
                            "stored_fingerprint": record.fingerprint.as_str(),
                            "request_fingerprint": fingerprint.as_str(),
                        })));
                    }
                    // Cache hit — mark the pre-created job cancelled
                    // so it doesn't linger as Queued forever.
                    let _ = self.job_store.cancel(&job_id).await;
                    return Ok(DispatchResult::Cached(record, request));
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = self.job_store.cancel(&job_id).await;
                    return Err(OrchestratorError::new(
                        ErrorCode::InternalError,
                        format!("idempotency lookup failed: {e}"),
                    ));
                }
            }
        }

        // 5. Resolve media.
        let request = match self.media_resolver.resolve(request, &snapshot).await {
            Ok(r) => r,
            Err(e) => {
                let _ = self
                    .job_store
                    .fail(
                        &job_id,
                        serde_json::json!({
                            "code": e.code.as_str(),
                            "message": e.message,
                        }),
                    )
                    .await;
                return Err(e);
            }
        };

        // 6. Look up provider.
        let provider_name = request.resolved_provider.clone().ok_or_else(|| {
            OrchestratorError::new(
                ErrorCode::InternalError,
                "Contextualizer did not resolve a provider.",
            )
        })?;
        let provider = self
            .directory
            .provider(&provider_name)
            .await
            .ok_or_else(|| {
                OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!("Provider `{provider_name}` is not registered."),
                )
            })?;

        // 7. Hand off.
        let outcome_request = request.clone();
        let outcome = match provider.onboard(request.clone()).await {
            Ok(o) => o,
            Err(e) => {
                let err = OrchestratorError::new(e.code(), e.message()).with_details(
                    serde_json::json!({ "provider": provider_name.as_str() }),
                );
                let _ = self
                    .job_store
                    .fail(
                        &job_id,
                        serde_json::json!({
                            "code": err.code.as_str(),
                            "message": err.message,
                        }),
                    )
                    .await;
                return Err(err);
            }
        };

        // 8. Post-onboard: job state + media reservation bookkeeping.
        match &outcome {
            ProviderOutcome::Sync(output) => {
                // Sync: the result is in-hand. Mark the job done
                // inline. No reservation is needed — media TTL of 24h
                // is sufficient.
                let _ = self.job_store.complete(&job_id, output.clone()).await;
            }
            ProviderOutcome::Async(_) | ProviderOutcome::Streaming { .. } => {
                // Async/streaming: transition to Running and reserve
                // every referenced media bound to this job. The job's
                // terminal transition will release them.
                let _ = self
                    .job_store
                    .update_state(&job_id, crate::domain::jobs::JobState::Running)
                    .await;
                for media_ref in &outcome_request.media.referenced {
                    let _ = self
                        .media_store
                        .reserve(
                            &media_ref.id,
                            MediaReservation {
                                job_id: Some(job_id.clone()),
                                reason: format!(
                                    "bound to {} job {}",
                                    outcome_request.action.dotted(),
                                    job_id
                                ),
                            },
                        )
                        .await;
                }
            }
        }

        // 9. Idempotency cache write.
        if let (Some(key), Some(fingerprint)) = (idem_key, idem_print) {
            match &outcome {
                ProviderOutcome::Sync(output) => {
                    let _ = self
                        .idempotency
                        .store(
                            key,
                            fingerprint,
                            CachedResponse::Sync {
                                output: output.clone(),
                            },
                        )
                        .await;
                }
                ProviderOutcome::Async(output) => {
                    if let Some(id_str) =
                        output.get(&keys::job::ID).and_then(|v| v.as_str())
                    {
                        let _ = self
                            .idempotency
                            .store(
                                key,
                                fingerprint,
                                CachedResponse::AsyncJob {
                                    job_id: JobId::from_string(id_str),
                                },
                            )
                            .await;
                    }
                }
                ProviderOutcome::Streaming { .. } => {
                    // Streams bypass the cache (ADR §Idempotency).
                }
            }
        }

        // 10. Demand ledger (passive).
        let model_label = outcome_request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_default();
        self.demand
            .record(DemandKey {
                primitive: outcome_request.action.primitive,
                provider: provider_name.as_str().to_string(),
                model: model_label,
                outcome: outcome_variant(&outcome).to_string(),
            })
            .await;

        Ok(DispatchResult::Fresh(outcome, outcome_request))
    }

    fn build_idempotency_pair(
        &self,
        request: &OrchestratorRequest,
    ) -> Option<(IdempotencyKey, ContentFingerprint)> {
        let header = request.constraints.idempotency_key.as_ref()?;
        let selectors_json = serde_json::to_value(&request.selectors).ok()?;
        let key = IdempotencyKey::from_header(header, &request.action.dotted());
        let fingerprint = ContentFingerprint::compute(&request.payload, &selectors_json);
        Some((key, fingerprint))
    }
}

fn outcome_variant(outcome: &ProviderOutcome) -> &'static str {
    match outcome {
        ProviderOutcome::Sync(_) => "sync",
        ProviderOutcome::Async(_) => "async",
        ProviderOutcome::Streaming { .. } => "stream",
    }
}

#[cfg(test)]
mod tests {
    //! Direct dispatcher unit tests (§Acceptance-4).
    //!
    //! These exercise [`Dispatcher::dispatch`] without the HTTP surface.
    //! They verify the dispatcher's *internal bookkeeping* — the things
    //! that are invisible from a 200/202/SSE envelope check:
    //!
    //! - a job record is pre-created **before** the provider is invoked
    //! - the demand ledger receives one record per dispatch with the
    //!   right key shape
    //! - sync outcomes complete the job and write to the idempotency cache
    //! - async outcomes transition the job to Running and reserve every
    //!   referenced media bound to that job id
    //! - streaming outcomes mark Running and bypass the idempotency cache
    //! - cache hits cancel the pre-created job (it must not linger as Queued)
    //! - content-fingerprint mismatches surface as `IdempotencyConflict`
    //!
    //! Each test builds a fresh Directory containing a single
    //! `MockProvider` whose script is set per-test.

    use super::*;

    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;
    use futures_util::stream::{self, BoxStream};
    use tempfile::TempDir;
    use tokio::sync::{watch, Mutex as TokioMutex};
    use tokio_util::sync::CancellationToken;
    use tracing::Span;

    use crate::domain::directory::Directory;
    use crate::domain::ids::{CorrelationId, ProviderName, RegistrationId, RequestId};
    use crate::domain::jobs::{JobFilter, JobState};
    use crate::domain::keys;
    use crate::domain::media::{MediaSource, MediaStore};
    use crate::domain::output::Output;
    use crate::domain::primitive::Primitive;
    use crate::domain::provider::{
        HonoredField, Provider, ProviderError, ProviderHealth, ProviderOutcome, ProviderState,
        ProviderStatePublisher, Registration, RegistrationStrategy,
    };
    use crate::domain::request::{Action, RawRequest};
    use crate::domain::selectors::{Constraints, Selectors};
    use crate::domain::vocabulary::VocabularyRegistry;
    use crate::services::contextualizer::Contextualizer;
    use crate::services::idempotency_store::InMemoryIdempotencyStore;
    use crate::services::job_store::DiskJobStore;
    use crate::services::media_resolver::MediaResolver;
    use crate::services::media_store::DiskMediaStore;
    use crate::services::recommendation::{DemandLedger, PinRegistry, RecommendationEngine};

    type Scripted = dyn Fn(OrchestratorRequest) -> Result<ProviderOutcome, ProviderError>
        + Send
        + Sync
        + 'static;

    /// Minimal in-test mock provider with a swappable scripted onboard.
    struct MockProvider {
        name: ProviderName,
        publisher: ProviderStatePublisher,
        script: TokioMutex<Arc<Scripted>>,
        invoked_at: TokioMutex<Option<chrono::DateTime<Utc>>>,
    }

    impl MockProvider {
        fn new(name: &str) -> Arc<Self> {
            let provider_name = ProviderName::new(name);
            let registration = Registration {
                id: RegistrationId::generate(),
                provider: provider_name.clone(),
                primitive: Primitive::TextChat,
                strategy: RegistrationStrategy::Bare,
                honored_fields: vec![HonoredField::new(keys::text::PROMPT_USER).required()],
                media_inputs: Vec::new(),
                media_outputs: Vec::new(),
            };
            let initial = ProviderState {
                health: ProviderHealth::Healthy,
                registrations: vec![registration],
                models: Vec::new(),
                performance_hints: Vec::new(),
            };
            let default: Arc<Scripted> =
                Arc::new(|_req: OrchestratorRequest| Ok(ProviderOutcome::Sync(Output::new())));
            Arc::new(Self {
                name: provider_name,
                publisher: ProviderStatePublisher::new(initial),
                script: TokioMutex::new(default),
                invoked_at: TokioMutex::new(None),
            })
        }

        async fn set_script<F>(&self, f: F)
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
            *self.invoked_at.lock().await = Some(Utc::now());
            let script = self.script.lock().await.clone();
            script(request)
        }
    }

    /// Bundle of fully-wired collaborators for one dispatcher test.
    struct Harness {
        _tmp: TempDir,
        dispatcher: Dispatcher,
        directory: Arc<Directory>,
        provider: Arc<MockProvider>,
        job_store: Arc<dyn JobStore>,
        idempotency_store: Arc<dyn IdempotencyStore>,
        demand: Arc<DemandLedger>,
        media_store: Arc<dyn MediaStore>,
    }

    async fn build_harness() -> Harness {
        let tmp = TempDir::new().expect("tmp");
        let data_dir = tmp.path().to_path_buf();

        let directory = Directory::new();
        let provider = MockProvider::new("mockchat");
        directory
            .register(provider.clone())
            .await
            .expect("register");
        directory.rebuild_snapshot().await;

        let media_store: Arc<dyn MediaStore> =
            DiskMediaStore::load(&data_dir).await.expect("media");
        let job_store: Arc<dyn JobStore> =
            DiskJobStore::load(&data_dir).await.expect("jobs");
        let idempotency_store: Arc<dyn IdempotencyStore> =
            Arc::new(InMemoryIdempotencyStore::new());

        let pins = Arc::new(PinRegistry::load(&data_dir).await);
        let demand = Arc::new(DemandLedger::new());
        let recommendation =
            RecommendationEngine::new(directory.clone(), pins, demand.clone());
        let resolver: Arc<
            dyn crate::services::contextualizer::RecommendationResolver,
        > = recommendation.clone();
        let vocabularies = VocabularyRegistry::build();
        let contextualizer = Arc::new(Contextualizer::new(vocabularies, Some(resolver)));
        let media_resolver = Arc::new(MediaResolver);

        let dispatcher = Dispatcher::new(
            directory.clone(),
            contextualizer,
            media_resolver,
            idempotency_store.clone(),
            demand.clone(),
            job_store.clone(),
            media_store.clone(),
        );

        Harness {
            _tmp: tmp,
            dispatcher,
            directory,
            provider,
            job_store,
            idempotency_store,
            demand,
            media_store,
        }
    }

    fn make_raw(prompt: &str, idempotency_key: Option<&str>) -> RawRequest {
        RawRequest {
            id: RequestId::generate(),
            correlation_id: CorrelationId::generate(),
            received_at: Utc::now(),
            action: Action::bare(Primitive::TextChat),
            payload: serde_json::json!({"text": {"prompt": {"user": prompt}}}),
            selectors: Selectors::default(),
            constraints: Constraints {
                idempotency_key: idempotency_key.map(str::to_string),
                ..Default::default()
            },
            cancel: CancellationToken::new(),
            span: Span::current(),
        }
    }

    // ── ProviderOutcome::Sync ─────────────────────────────────

    #[tokio::test]
    async fn sync_outcome_completes_job_and_writes_cache_and_records_demand() {
        let h = build_harness().await;
        h.provider
            .set_script(|_req| {
                let mut out = Output::new();
                out.set(&keys::text::RESPONSE, "ok");
                Ok(ProviderOutcome::Sync(out))
            })
            .await;

        let raw = make_raw("hi", Some("key-sync"));
        let result = h
            .dispatcher
            .dispatch(raw)
            .await
            .expect("dispatch must succeed");
        assert!(matches!(result, DispatchResult::Fresh(_, _)));

        // Job pre-created AND completed inline.
        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        assert_eq!(jobs.len(), 1, "exactly one job recorded");
        let job = &jobs[0];
        assert_eq!(job.state, JobState::Done, "sync must mark job Done inline");

        // Idempotency cache holds the sync output under the new key shape.
        let key = IdempotencyKey::from_header("key-sync", "text.chat");
        let record = h
            .idempotency_store
            .lookup(&key)
            .await
            .unwrap()
            .expect("cache write happened");
        assert!(matches!(record.response, CachedResponse::Sync { .. }));

        // Demand ledger received exactly one entry.
        let snap = h.demand.snapshot().await;
        assert_eq!(snap.values().sum::<u64>(), 1);
        let key = snap.keys().next().unwrap();
        assert_eq!(key.primitive, Primitive::TextChat);
        assert_eq!(key.provider, "mockchat");
        assert_eq!(key.outcome, "sync");
    }

    // ── ProviderOutcome::Async ────────────────────────────────

    #[tokio::test]
    async fn async_outcome_marks_running_and_reserves_referenced_media() {
        let h = build_harness().await;
        // First, upload a media entry and synthesize a request that
        // references it. Async outcomes reserve the bytes for the job.
        let entry = h
            .media_store
            .put(
                bytes::Bytes::from_static(b"PNG"),
                "image/png".to_string(),
                MediaSource::uploaded(),
            )
            .await
            .unwrap();

        // Re-register a provider that accepts image.analyze ById so the
        // contextualizer doesn't reject the media reference.
        let directory = Directory::new();
        let provider = {
            let provider_name = ProviderName::new("xfer");
            let registration = Registration {
                id: RegistrationId::generate(),
                provider: provider_name.clone(),
                primitive: Primitive::ImageAnalyze,
                strategy: RegistrationStrategy::Bare,
                honored_fields: vec![HonoredField::new(keys::image::SOURCE).required()],
                media_inputs: vec![crate::domain::provider::MediaInputSpec {
                    field: keys::image::SOURCE,
                    delivery: crate::domain::media::MediaDelivery::ById,
                    accepted_types: vec!["image/png".to_string()],
                    overlay: None,
                }],
                media_outputs: Vec::new(),
            };
            let initial = ProviderState {
                health: ProviderHealth::Healthy,
                registrations: vec![registration],
                models: Vec::new(),
                performance_hints: Vec::new(),
            };
            Arc::new(MockProvider {
                name: provider_name,
                publisher: ProviderStatePublisher::new(initial),
                script: TokioMutex::new(Arc::new(|_| {
                    let mut out = Output::new();
                    out.set(&keys::job::ID, "job-async-1");
                    out.set(&keys::job::STATUS, keys::job::values::STATUS_RUNNING);
                    Ok(ProviderOutcome::Async(out))
                })),
                invoked_at: TokioMutex::new(None),
            })
        };
        directory.register(provider).await.unwrap();
        directory.rebuild_snapshot().await;

        let pins = Arc::new(PinRegistry::load(h._tmp.path()).await);
        let demand = Arc::new(DemandLedger::new());
        let recommendation =
            RecommendationEngine::new(directory.clone(), pins, demand.clone());
        let resolver: Arc<
            dyn crate::services::contextualizer::RecommendationResolver,
        > = recommendation.clone();
        let contextualizer = Arc::new(Contextualizer::new(
            VocabularyRegistry::build(),
            Some(resolver),
        ));
        let dispatcher = Dispatcher::new(
            directory,
            contextualizer,
            Arc::new(MediaResolver),
            h.idempotency_store.clone(),
            demand.clone(),
            h.job_store.clone(),
            h.media_store.clone(),
        );

        let raw = RawRequest {
            id: RequestId::generate(),
            correlation_id: CorrelationId::generate(),
            received_at: Utc::now(),
            action: Action::bare(Primitive::ImageAnalyze),
            payload: serde_json::json!({
                "image": {"source": {"media_id": entry.id.as_str()}}
            }),
            selectors: Selectors::default(),
            constraints: Constraints::default(),
            cancel: CancellationToken::new(),
            span: Span::current(),
        };
        let _ = dispatcher.dispatch(raw).await.expect("dispatch ok");

        // Job state should be Running (not Done — that happens when
        // the worker reports completion).
        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        let job = jobs.first().expect("job recorded");
        assert_eq!(job.state, JobState::Running);

        // Media must be Reserved bound to this job id.
        let meta = h.media_store.get_metadata(&entry.id).await.unwrap();
        match meta.lifecycle {
            crate::domain::media::MediaLifecycle::Reserved { ref reservation, .. } => {
                assert_eq!(
                    reservation.job_id.as_ref().map(|j| j.as_str()),
                    Some(job.id.as_str()),
                    "reservation must bind to the dispatcher's job id"
                );
            }
            ref other => panic!("expected Reserved lifecycle, got {other:?}"),
        }

        // Demand ledger reflects the async outcome.
        let snap = demand.snapshot().await;
        assert_eq!(snap.values().sum::<u64>(), 1);
        assert_eq!(snap.keys().next().unwrap().outcome, "async");
    }

    // ── ProviderOutcome::Streaming ────────────────────────────

    #[tokio::test]
    async fn streaming_outcome_marks_running_and_skips_idempotency_cache() {
        let h = build_harness().await;
        h.provider
            .set_script(|_req| {
                let initial = Output::new();
                let chunk: Output = {
                    let mut o = Output::new();
                    o.set(&keys::text::RESPONSE, "x");
                    o
                };
                let stream: BoxStream<'static, Result<Output, ProviderError>> =
                    Box::pin(stream::iter(vec![Ok(chunk)]));
                Ok(ProviderOutcome::Streaming { initial, stream })
            })
            .await;

        let raw = make_raw("hi", Some("key-stream"));
        let result = h.dispatcher.dispatch(raw).await.expect("dispatch ok");
        assert!(matches!(result, DispatchResult::Fresh(_, _)));

        // Job moved to Running.
        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        let job = jobs.first().unwrap();
        assert_eq!(job.state, JobState::Running);

        // Idempotency cache MUST NOT have an entry — streaming bypasses.
        let key = IdempotencyKey::from_header("key-stream", "text.chat");
        assert!(
            h.idempotency_store.lookup(&key).await.unwrap().is_none(),
            "streaming outcomes must not be cached (§ADR Idempotency)"
        );

        // Demand ledger reflects the stream outcome.
        let snap = h.demand.snapshot().await;
        assert_eq!(snap.keys().next().unwrap().outcome, "stream");
    }

    // ── Cache hit cancels the pre-created job ─────────────────

    #[tokio::test]
    async fn idempotency_cache_hit_cancels_pre_created_job() {
        let h = build_harness().await;
        h.provider
            .set_script(|_req| {
                let mut out = Output::new();
                out.set(&keys::text::RESPONSE, "first");
                Ok(ProviderOutcome::Sync(out))
            })
            .await;

        // First call: warms the cache and completes inline.
        let _ = h
            .dispatcher
            .dispatch(make_raw("hi", Some("key-hit")))
            .await
            .unwrap();
        // Second call with the same key+content: must hit the cache.
        let result = h
            .dispatcher
            .dispatch(make_raw("hi", Some("key-hit")))
            .await
            .unwrap();
        assert!(matches!(result, DispatchResult::Cached(_, _)));

        // Two jobs: one Done (the original), one Cancelled (the
        // pre-created one for the cache hit). Neither must be Queued.
        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        assert_eq!(jobs.len(), 2);
        let states: Vec<JobState> = jobs.iter().map(|j| j.state).collect();
        assert!(states.contains(&JobState::Done));
        assert!(states.contains(&JobState::Cancelled));
        assert!(
            !states.contains(&JobState::Queued),
            "no job may linger as Queued after a cache hit"
        );
    }

    // ── Content fingerprint conflict ──────────────────────────

    #[tokio::test]
    async fn idempotency_fingerprint_mismatch_returns_conflict() {
        let h = build_harness().await;
        h.provider
            .set_script(|_req| Ok(ProviderOutcome::Sync(Output::new())))
            .await;

        // Warm the cache.
        let _ = h
            .dispatcher
            .dispatch(make_raw("first", Some("k-conflict")))
            .await
            .unwrap();

        // Same key, different content body.
        let err = h
            .dispatcher
            .dispatch(make_raw("second", Some("k-conflict")))
            .await
            .expect_err("must surface IdempotencyConflict");
        assert_eq!(err.code, ErrorCode::IdempotencyConflict);
        assert!(
            err.message.contains("Idempotency-Key"),
            "actionable message must name the offending header"
        );

        // The pre-created job for the conflicting call must be cancelled.
        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        assert!(
            jobs.iter().any(|j| j.state == JobState::Cancelled),
            "conflict path must cancel its pre-created job"
        );
    }

    // ── Job pre-creation timing ───────────────────────────────

    #[tokio::test]
    async fn job_is_pre_created_before_provider_invocation() {
        let h = build_harness().await;
        h.provider
            .set_script(|_req| Ok(ProviderOutcome::Sync(Output::new())))
            .await;

        let before = Utc::now();
        let _ = h
            .dispatcher
            .dispatch(make_raw("x", None))
            .await
            .unwrap();

        let jobs = h.job_store.list(JobFilter::default()).await.unwrap();
        let job = jobs.first().expect("job exists");
        let invoked = h
            .provider
            .invoked_at
            .lock()
            .await
            .expect("provider was invoked");
        assert!(
            job.created_at <= invoked,
            "job must be created before provider.onboard runs (created_at={}, invoked_at={})",
            job.created_at,
            invoked
        );
        assert!(job.created_at >= before);

        // Suppress warning about unused field on the harness.
        let _ = &h.directory;
    }
}
