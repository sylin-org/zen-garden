//! Provider trait — the single abstraction for all AI service providers.
//!
//! Every provider (Ollama, OpenAI, Anthropic, Google, Infinity, OpenedAI Speech,
//! LibreTranslate) implements this one trait. It covers lifecycle (probe, enumerate)
//! and inference (infer, stream, embed, speak, transcribe).
//!
//! No separate Offering/InferenceAdapter split. One trait, one registry, one path.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

use super::inference::{
    BoxStream, EmbedRequest, EmbedResponse, InferenceChunk, InferenceRequest, InferenceResponse,
    SpeechRequest, SpeechResponse, TranscribeRequest, TranscribeResponse,
};

/// Boxed future for object-safe async methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

// ── Provider Context ────────────────────────────────────────────

/// Everything a provider needs for a single operation.
///
/// Built by the caller (unified API, cloud_sync, provider_test) from
/// routing decisions and cloud config. Providers are stateless — all
/// per-request state lives here.
#[derive(Debug, Clone)]
pub struct ProviderContext {
    /// Target endpoint URL.
    pub endpoint: String,
    /// Model name (set for inference, `None` for probe/enumerate).
    pub model: Option<String>,
    /// API key (set for cloud providers, `None` for local).
    pub api_key: Option<String>,
}

// ── Provider Trait ──────────────────────────────────────────────

/// The single abstraction for all AI service providers.
///
/// Each provider implements only the methods it supports. Default impls
/// return "not supported" for inference methods.
pub trait Provider: Send + Sync + 'static {
    /// Provider type identifier.
    fn kind(&self) -> OfferingKind;

    /// AI capabilities this provider can serve.
    fn capabilities(&self) -> &[Capability];

    /// How to discover instances of this provider.
    fn discovery(&self) -> DiscoveryConfig;

    // ── Lifecycle ───────────────────────────────────────────────

    /// Probe an endpoint for liveness.
    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>>;

    /// Enumerate available models/resources on a live instance.
    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>>;

    // ── Inference (defaults return "not supported") ─────────────

    /// Non-streaming chat inference.
    fn infer(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let _ = (ctx, req);
        Box::pin(async { anyhow::bail!("chat inference not supported") })
    }

    /// Streaming chat inference.
    fn infer_stream(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let _ = (ctx, req);
        Box::pin(async { anyhow::bail!("streaming inference not supported") })
    }

    /// Text embedding.
    fn embed(
        &self,
        ctx: &ProviderContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let _ = (ctx, req);
        Box::pin(async { anyhow::bail!("embeddings not supported") })
    }

    /// Text-to-speech.
    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let _ = (ctx, req);
        Box::pin(async { anyhow::bail!("text-to-speech not supported") })
    }

    /// Speech-to-text.
    fn transcribe(
        &self,
        ctx: &ProviderContext,
        req: TranscribeRequest,
    ) -> BoxFuture<'_, Result<TranscribeResponse>> {
        let _ = (ctx, req);
        Box::pin(async { anyhow::bail!("transcription not supported") })
    }

    // ── Optional ────────────────────────────────────────────────

    /// Estimate VRAM consumption for a model. `None` if not applicable.
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        let _ = model;
        None
    }

    /// Benchmark a model's capability on an instance.
    fn benchmark(
        &self,
        ctx: &ProviderContext,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let _ = (ctx, model, capability);
        Box::pin(async move {
            Ok(BenchmarkSample {
                samples: vec![],
                capability,
            })
        })
    }

    /// Sync a resource from one instance to another.
    fn sync_resource(
        &self,
        resource: &str,
        from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        let _ = (resource, from, to);
        Box::pin(async {
            Ok(SyncProgress::Failed {
                reason: "sync not supported".to_string(),
            })
        })
    }
}

// ── Provider Registry ───────────────────────────────────────────

/// Single registry for all providers. Replaces OfferingRegistry + AdapterRegistry.
pub struct ProviderRegistry {
    providers: HashMap<OfferingKind, Arc<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        let kind = provider.kind();
        self.providers.insert(kind, provider);
    }

    pub fn get(&self, kind: OfferingKind) -> Option<&Arc<dyn Provider>> {
        self.providers.get(&kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = OfferingKind> + '_ {
        self.providers.keys().copied()
    }

    pub fn all(&self) -> impl Iterator<Item = &Arc<dyn Provider>> {
        self.providers.values()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

// ── Support Types ───────────────────────────────────────────────

/// How the orchestrator discovers instances of this provider.
#[derive(Debug, Clone)]
pub enum DiscoveryConfig {
    /// Filter Moss topology by offering name.
    TopologyFilter { offering_name: String },
    /// Manually configured endpoint (cloud providers).
    Configured,
}

/// Result of a successful health probe.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub version: Option<String>,
    pub capabilities: Vec<Capability>,
    pub vram_free_bytes: Option<u64>,
    pub metadata: serde_json::Value,
}

/// A model or resource available on a service instance.
#[derive(Debug, Clone)]
pub struct ServiceModel {
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub specializations: Vec<String>,
    pub vram_bytes: Option<u64>,
    pub metadata: serde_json::Value,
}

/// A single benchmark measurement.
#[derive(Debug, Clone)]
pub struct BenchmarkSample {
    pub samples: Vec<Sample>,
    pub capability: Capability,
}

/// One timing sample from a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub cold_start_ms: u64,
    pub tokens_per_second: f64,
    pub total_duration_ms: u64,
}

/// Progress/completion of a resource sync operation.
#[derive(Debug, Clone)]
pub enum SyncProgress {
    Completed { bytes_transferred: u64 },
    InProgress {
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    },
    Failed { reason: String },
}
