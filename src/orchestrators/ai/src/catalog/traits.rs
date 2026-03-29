//! The Offering trait — core abstraction between shared infrastructure
//! and service-specific adapters.
//!
//! See `docs/research/04-offering-trait-design.md` for design rationale.

use std::pin::Pin;

use anyhow::Result;
use bytes::Bytes;
use futures_util::Stream;

use axum::http;

use std::any::Any;

use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// Boxed future for object-safe async methods.
/// The project removed `async-trait` in ARCH-0007; boxed futures are
/// the explicit replacement for dyn-compatible async methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// A type of AI service the orchestrator can discover, probe, and route to.
///
/// Each offering implementation encapsulates all service-specific protocol
/// knowledge: HTTP endpoints, response shapes, streaming formats, model
/// management commands. The orchestrator's domain layer never sees these
/// details — it operates on `ServiceInstance` and `Capability` exclusively.
pub trait Offering: Send + Sync + 'static {
    /// Unique type identifier.
    fn offering_type(&self) -> OfferingKind;

    /// Downcast support for accessing concrete offering types.
    fn as_any(&self) -> &dyn Any;

    /// AI capabilities this offering type can provide.
    fn capabilities(&self) -> &[Capability];

    /// How to discover instances (port probe, topology filter, configured).
    fn discovery_config(&self) -> DiscoveryConfig;

    /// Probe an endpoint for liveness. Returns service metadata if healthy.
    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>>;

    /// Enumerate available models/resources on a live instance.
    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>>;

    /// Estimate VRAM consumption for a model on this offering.
    /// Static estimate from model metadata — not a live query.
    /// Returns `None` if VRAM is not applicable (CPU-only, cloud).
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        let _ = model;
        None
    }

    /// Forward a capability request to the instance's native API.
    fn proxy(
        &self,
        endpoint: &str,
        capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>>;

    /// Benchmark a specific model's capability on an instance.
    /// Default: returns empty samples (offering does not support benchmarking yet).
    fn benchmark(
        &self,
        endpoint: &str,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let _ = (endpoint, model, capability);
        Box::pin(async move { Ok(BenchmarkSample { samples: vec![], capability }) })
    }

    /// Sync a resource from one instance to another.
    /// Default: not supported.
    fn sync_resource(
        &self,
        resource: &str,
        from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        let _ = (resource, from, to);
        Box::pin(async {
            Ok(SyncProgress::Failed {
                reason: "sync not supported for this offering".to_string(),
            })
        })
    }
}

// ── Trait Support Types ─────────────────────────────────────────

/// How the orchestrator discovers instances of this offering type.
#[derive(Debug, Clone)]
pub enum DiscoveryConfig {
    /// Probe a well-known port on discovered stones.
    PortProbe { default_port: u16 },
    /// Filter Moss topology by offering name.
    TopologyFilter { offering_name: String },
    /// Manually configured endpoint (cloud providers, HuggingFace).
    Configured,
}

/// Result of a successful health probe.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Service version string.
    pub version: Option<String>,
    /// Capabilities confirmed by this specific instance.
    pub capabilities: Vec<Capability>,
    /// Real-time VRAM free bytes (ComfyUI provides this; Ollama does not).
    pub vram_free_bytes: Option<u64>,
    /// Offering-specific metadata (opaque to domain).
    pub metadata: serde_json::Value,
}

/// A model or resource available on a service instance.
#[derive(Debug, Clone)]
pub struct ServiceModel {
    /// Model identifier (e.g., "llama3.2:3b", "flux-dev.safetensors").
    pub name: String,
    /// Capabilities this specific model supports.
    pub capabilities: Vec<Capability>,
    /// Specialization tags derived from model name/metadata.
    pub specializations: Vec<String>,
    /// VRAM consumption when loaded (bytes). None if unknown.
    pub vram_bytes: Option<u64>,
    /// Offering-specific model metadata.
    pub metadata: serde_json::Value,
}

/// Incoming proxy request — the raw HTTP request from the client.
pub struct ProxyRequest {
    pub method: http::Method,
    pub path: String,
    pub headers: http::HeaderMap,
    pub body: ProxyBody,
}

/// Proxy response — the raw HTTP response from the offering instance.
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ProxyBody,
}

/// Proxy body — either a complete buffer or a byte stream.
pub enum ProxyBody {
    /// Complete response (JSON, image bytes, audio bytes).
    Complete(Vec<u8>),
    /// Streaming response (Ollama NDJSON, SSE progress, chunked audio).
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>),
}

/// A single benchmark measurement for one capability on one instance.
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

use serde::{Deserialize, Serialize};

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
