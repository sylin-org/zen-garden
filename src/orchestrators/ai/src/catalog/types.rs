//! Types used in the [`Offering`](super::Offering) trait contract.
//!
//! These are the data structures that cross the boundary between the catalog
//! (I/O) layer and the domain (pure) layer. The domain never imports the trait
//! itself, but it does operate on these types.

use std::pin::Pin;

use bytes::Bytes;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

use crate::domain::types::Capability;

// ── Discovery ───────────────────────────────────────────────────────

/// How the orchestrator discovers instances of an offering type.
#[derive(Debug, Clone)]
pub enum DiscoveryConfig {
    /// Probe a well-known port on discovered stones.
    PortProbe { default_port: u16 },
    /// Filter Moss topology by offering name.
    TopologyFilter { offering_name: String },
    /// Manually configured endpoint (cloud providers, HuggingFace).
    Configured,
}

// ── Probe ───────────────────────────────────────────────────────────

/// Result of a successful health probe against a service instance.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    /// Service version string (e.g., "0.9.1" for Ollama).
    pub version: Option<String>,
    /// Capabilities confirmed by this specific instance.
    pub capabilities: Vec<Capability>,
    /// Real-time VRAM free bytes (ComfyUI provides this; Ollama does not).
    pub vram_free_bytes: Option<u64>,
    /// Offering-specific metadata (opaque to domain).
    pub metadata: serde_json::Value,
}

// ── Enumeration ─────────────────────────────────────────────────────

/// A model or resource available on a service instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceModel {
    /// Model identifier (e.g., "llama3.2:3b", "flux-dev.safetensors").
    pub name: String,
    /// Capabilities this specific model supports.
    pub capabilities: Vec<Capability>,
    /// VRAM consumption when loaded (bytes). `None` if unknown.
    pub vram_bytes: Option<u64>,
    /// Offering-specific model metadata (param count, quant level, etc.).
    pub metadata: serde_json::Value,
}

// ── Proxy ───────────────────────────────────────────────────────────

/// Inbound proxy request — the raw HTTP request from the client, normalized
/// by the proxy handler before dispatching to the offering.
#[derive(Debug)]
pub struct ProxyRequest {
    pub method: reqwest::Method,
    pub path: String,
    pub headers: reqwest::header::HeaderMap,
    pub body: Bytes,
}

/// Proxy response — the raw HTTP response from the offering instance.
///
/// The proxy handler forwards this directly to the client. Each offering
/// produces the correct content type: NDJSON stream for Ollama, image bytes
/// for ComfyUI, audio bytes for Speak/Transcribe, JSON for others.
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ProxyBody,
}

/// Proxy body — either a complete buffer or a byte stream.
pub enum ProxyBody {
    /// Complete response (JSON, image bytes, audio bytes).
    Complete(Bytes),
    /// Streaming response (Ollama NDJSON, SSE progress).
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, anyhow::Error>> + Send>>),
}

// Debug is not derivable for the Stream variant.
impl std::fmt::Debug for ProxyResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &match &self.body {
                ProxyBody::Complete(b) => format!("Complete({} bytes)", b.len()),
                ProxyBody::Stream(_) => "Stream(...)".to_owned(),
            })
            .finish()
    }
}

// ── Benchmark ───────────────────────────────────────────────────────

/// A single benchmark measurement for one capability on one instance.
///
/// Contains raw timing samples that the domain fitness module aggregates
/// into a [`Verdict`](crate::domain::types::Verdict).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSample {
    /// Model that was benchmarked.
    pub model: String,
    /// Capability tested.
    pub capability: Capability,
    /// Raw timing samples (one per test prompt/input).
    pub samples: Vec<Sample>,
}

// Re-export Sample from domain (canonical location) — catalog references
// it but does not own it. Domain must never import catalog.
pub use crate::domain::types::Sample;

// ── Resource Sync ───────────────────────────────────────────────────

/// Progress/completion of a resource sync operation.
#[derive(Debug, Clone, Serialize)]
pub enum SyncProgress {
    /// Sync completed successfully.
    Completed { bytes_transferred: u64 },
    /// Sync is in progress.
    InProgress {
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    },
    /// Sync failed.
    Failed { reason: String },
}
