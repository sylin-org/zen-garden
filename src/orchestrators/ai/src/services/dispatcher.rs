//! The Dispatcher — a coordinator that owns the request pipeline
//! from raw caller intent through provider handoff.
//!
//! Responsibilities (ADR §Pipeline + §Job model):
//! - Pre-create a `Job` record before calling `onboard`.
//! - Assemble the `ExecutionContext` (media store, job sink, cancel,
//!   span).
//! - Run the contextualizer (which queries `CapabilityDirectory`).
//! - Consult the idempotency cache.
//! - Run the media resolver.
//! - Look up the provider handle by resolved name in
//!   [`crate::services::provider_registry::ProviderRegistry`].
//! - Call `onboard`.
//! - Reserve referenced media for Async/Streaming outcomes (bound
//!   to the job id).
//! - Persist a synchronous result in the idempotency cache or, for
//!   async outcomes, the job reference.
//!
//! No per-primitive logic; no per-provider branching.
//!
//! # ORCH-0030 R2 M3 changes
//!
//! - The dispatcher takes
//!   [`crate::services::directory_subscriber::CapabilityDirectory`]
//!   and [`crate::services::provider_registry::ProviderRegistry`]
//!   instead of the legacy `Directory` aggregate.
//! - The demand ledger is gone (the recommendation engine that
//!   consumed it is gone).
//! - `resolved_model` is gone from the request type — model
//!   resolution is adapter-local.

use std::sync::Arc;

use serde_json::Value;

use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::idempotency::{
    CachedResponse, ContentFingerprint, IdempotencyKey, IdempotencyRecord, IdempotencyStore,
};
use crate::domain::ids::JobId;
use crate::domain::jobs::{JobCategory, JobSink, JobStore};
use crate::domain::keys;
use crate::domain::media::{MediaReservation, SharedMediaStore};
use crate::domain::provider::{ProviderOutcome, ProviderResult};
use crate::domain::persisted_request::{
    ErrorSnapshot, PersistedRequest, RequestMedia, RequestMeta, SelectorsSnapshot,
};
use crate::domain::request::{ExecutionContext, OrchestratorRequest, RawRequest};
use crate::services::contextualizer::Contextualizer;
use crate::services::directory_subscriber::CapabilityDirectory;
use crate::services::media_resolver::MediaResolver;
use crate::services::provider_registry::ProviderRegistry;
use crate::services::request_store::DiskRequestStore;

/// The outcome returned to HTTP handlers.
pub enum DispatchResult {
    Fresh(ProviderResult, OrchestratorRequest),
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
    capability_directory: Arc<CapabilityDirectory>,
    provider_registry: Arc<ProviderRegistry>,
    contextualizer: Arc<Contextualizer>,
    media_resolver: Arc<MediaResolver>,
    idempotency: Arc<dyn IdempotencyStore>,
    job_store: Arc<dyn JobStore>,
    media_store: SharedMediaStore,
    request_store: Arc<DiskRequestStore>,
}

impl Dispatcher {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capability_directory: Arc<CapabilityDirectory>,
        provider_registry: Arc<ProviderRegistry>,
        contextualizer: Arc<Contextualizer>,
        media_resolver: Arc<MediaResolver>,
        idempotency: Arc<dyn IdempotencyStore>,
        job_store: Arc<dyn JobStore>,
        media_store: SharedMediaStore,
        request_store: Arc<DiskRequestStore>,
    ) -> Self {
        Self {
            capability_directory,
            provider_registry,
            contextualizer,
            media_resolver,
            idempotency,
            job_store,
            media_store,
            request_store,
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

        // Capture the raw payload before contextualization (ORCH-0033).
        // The contextualizer normalizes aliases (e.g. text.body →
        // text.prompt.user) which mutates request.payload. We store the
        // original so the dashboard can repopulate by simple key matching
        // against the catalog field descriptors.
        let raw_input_snapshot = raw.payload.clone();

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
            context,
        };

        // 3. Contextualize.
        let request = match self
            .contextualizer
            .resolve(request, &self.capability_directory)
            .await
        {
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
        let request = match self
            .media_resolver
            .resolve(request, &self.capability_directory)
            .await
        {
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

        // 5b. Persist request record (ORCH-0033) — input snapshot.
        //     The request is fully contextualized at this point:
        //     payload normalized, provider resolved, media resolved.
        let media_inputs: Vec<RequestMedia> = request
            .media
            .referenced
            .iter()
            .map(|mr| RequestMedia {
                media_id: mr.id.as_str().to_string(),
                field: mr.field.as_str().to_string(),
                content_type: mr.content_type.clone(),
            })
            .collect();

        let selectors_snapshot = SelectorsSnapshot {
            provider: request
                .selectors
                .provider
                .as_ref()
                .map(|p| p.as_str().to_string()),
            model: request
                .selectors
                .model
                .as_ref()
                .map(|s| s.to_string()),
            variant: request
                .selectors
                .variant
                .as_ref()
                .map(|s| s.to_string()),
        };

        // Extract lineage.parent from the raw input (ORCH-0036).
        let parent_id = raw_input_snapshot
            .pointer("/lineage/parent")
            .and_then(|v| v.as_str())
            .map(String::from);

        let persisted = PersistedRequest::new_running(
            request.id.clone(),
            request.correlation_id.as_ref().to_string(),
            request.action.dotted().to_string(),
            raw_input_snapshot,
            selectors_snapshot,
            media_inputs,
            request
                .resolved_provider
                .as_ref()
                .map(|p| p.as_str().to_string()),
            Some(job_id.clone()),
            parent_id,
        );
        // Non-blocking: persist failure should not fail the dispatch.
        if let Err(e) = self.request_store.create(persisted).await {
            tracing::warn!(request_id = %request.id, error = %e, "failed to persist request record");
        }

        // 6. Look up provider handle in the registry.
        let provider_name = request.resolved_provider.clone().ok_or_else(|| {
            OrchestratorError::new(
                ErrorCode::InternalError,
                "Contextualizer did not resolve a provider.",
            )
        })?;
        let provider = self
            .provider_registry
            .get(&provider_name)
            .await
            .ok_or_else(|| {
                OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!("Provider `{provider_name}` is not registered."),
                )
            })?;

        // 7. Hand off.
        let outcome_request = request.clone();
        let provider_result = match provider.onboard(request.clone()).await {
            Ok(r) => r,
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
                // ORCH-0033: mark request as failed.
                let _ = self
                    .request_store
                    .fail(
                        request.id.as_str(),
                        ErrorSnapshot {
                            code: err.code.as_str().to_string(),
                            message: err.message.clone(),
                            details: if err.details.is_null() {
                                None
                            } else {
                                Some(err.details.clone())
                            },
                        },
                    )
                    .await;
                return Err(err);
            }
        };

        // 8. Post-onboard: job state + media reservation bookkeeping.
        let provider_meta = &provider_result.meta;
        match &provider_result.outcome {
            ProviderOutcome::Sync(output) => {
                // Sync: the result is in-hand. Mark the job done
                // inline, and pin any input media referenced by this
                // request. Pinning on success promotes the media from
                // its 24h transient TTL to a 30-day reservation bound
                // to the completed job — this keeps the request log's
                // historical references resolvable, otherwise the
                // sweeper would strand them as dangling pointers the
                // moment the TTL lapses.
                let _ = self.job_store.complete(&job_id, output.clone()).await;

                for media_ref in &outcome_request.media.referenced {
                    let _ = self
                        .media_store
                        .reserve(
                            &media_ref.id,
                            MediaReservation {
                                job_id: Some(job_id.clone()),
                                reason: format!(
                                    "bound to completed {} request {}",
                                    outcome_request.action.dotted(),
                                    outcome_request.id
                                ),
                            },
                        )
                        .await;
                }

                // ORCH-0033 + ORCH-0034: mark request as succeeded
                // with output and provider resolution metadata.
                let output_value = output.to_nested();
                let media_outputs = extract_media_outputs(&output_value);
                let latency = (chrono::Utc::now() - outcome_request.received_at)
                    .num_milliseconds()
                    .unsigned_abs();
                let _ = self
                    .request_store
                    .complete(
                        outcome_request.id.as_str(),
                        output_value,
                        media_outputs,
                        RequestMeta {
                            provider: outcome_request
                                .resolved_provider
                                .as_ref()
                                .map(|p| p.as_str().to_string()),
                            model: provider_meta.model.clone(),
                            stone: provider_meta.stone.clone(),
                            latency_ms: Some(latency),
                            tokens_in: provider_meta.tokens_in,
                            tokens_out: provider_meta.tokens_out,
                            summary: provider_meta.summary.clone(),
                        },
                    )
                    .await;
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
            match &provider_result.outcome {
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

        Ok(DispatchResult::Fresh(provider_result, outcome_request))
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


// ── ORCH-0033 helpers ────────────────────────────────────────

/// Scan a nested output Value for media_id references, returning
/// structured `RequestMedia` entries. Looks for any object with a
/// `media_id` key or string values that look like media references
/// at known output paths (image.data, audio.data, etc.).
fn extract_media_outputs(output: &serde_json::Value) -> Vec<RequestMedia> {
    let mut results = Vec::new();
    walk_for_media_ids("", output, &mut results);
    results
}

fn walk_for_media_ids(prefix: &str, value: &serde_json::Value, out: &mut Vec<RequestMedia>) {
    match value {
        serde_json::Value::Object(map) => {
            // Direct media_id reference
            if let Some(mid) = map.get("media_id").and_then(|v| v.as_str()) {
                let content_type = map
                    .get("content_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                out.push(RequestMedia {
                    media_id: mid.to_string(),
                    field: prefix.to_string(),
                    content_type,
                });
                return;
            }
            // Recurse into nested objects
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk_for_media_ids(&path, val, out);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let path = format!("{prefix}[{i}]");
                walk_for_media_ids(&path, val, out);
            }
        }
        _ => {}
    }
}
