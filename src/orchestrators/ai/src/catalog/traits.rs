//! The [`Offering`] trait — core abstraction for AI service types.
//!
//! Each offering implementation encapsulates all service-specific protocol
//! knowledge: HTTP endpoints, response shapes, streaming formats, model
//! management commands. The orchestrator's domain layer never sees these
//! details — it operates on [`ServiceInstance`] and [`Capability`] exclusively.
//!
//! Methods that perform I/O return [`BoxFuture`] rather than using `async fn`
//! so that the trait is object-safe (`dyn Offering`). The project removed
//! `async-trait` in ARCH-0007; boxed futures are the explicit replacement
//! for dyn-compatible async methods.

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;

use super::types::{
    BenchmarkSample, DiscoveryConfig, ProbeResult, ProxyRequest, ProxyResponse, ServiceModel,
    SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// A boxed, `Send`, lifetime-scoped future — the dyn-safe replacement for
/// `async fn` in trait methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A type of AI service the orchestrator can discover, probe, and route to.
///
/// Implementing this trait is the sole requirement for adding a new AI service
/// type to the orchestrator. The domain layer, tasks, and API handlers
/// dispatch through [`OfferingRegistry`](super::OfferingRegistry) without
/// knowledge of the concrete offering type.
pub trait Offering: Send + Sync {
    /// Unique type identifier for this offering.
    fn offering_type(&self) -> OfferingKind;

    /// AI capabilities this offering type can provide.
    fn capabilities(&self) -> &[Capability];

    /// How the orchestrator discovers instances of this offering type.
    fn discovery_config(&self) -> DiscoveryConfig;

    /// Probe an endpoint for liveness.
    ///
    /// Called by the health check task at regular intervals. Returns service
    /// metadata if healthy, or an error if the probe fails.
    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>>;

    /// Enumerate available models/resources on a live instance.
    ///
    /// Called by the reconciliation task to detect drift (models
    /// appeared/disappeared, VRAM changes).
    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>>;

    /// Estimate VRAM consumption for a model on this offering.
    ///
    /// Static estimate from model metadata — not a live query. Real-time VRAM
    /// data comes from [`probe`](Self::probe) and is cached in
    /// [`ServiceInstance::vram`].
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64>;

    /// Forward a capability request to the instance's native API.
    ///
    /// The orchestrator calls this after the routing engine selects a target.
    /// The offering translates the generic [`ProxyRequest`] into the
    /// service-specific protocol and returns a [`ProxyResponse`] that the
    /// proxy handler forwards to the client.
    fn proxy(
        &self,
        endpoint: &str,
        capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>>;

    /// Benchmark a specific model's capability on an instance.
    ///
    /// Returns raw timing samples that the domain fitness module aggregates
    /// into a verdict. Each offering defines its own test payloads (prompts
    /// for Ollama, workflows for ComfyUI, audio clips for Speaches).
    fn benchmark(
        &self,
        endpoint: &str,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>>;

    /// Sync a resource from one instance to another.
    ///
    /// - **Ollama:** calls `POST /api/pull` on the target instance.
    /// - **ComfyUI:** transfers checkpoint via Moss storage bank.
    /// - **Cloud providers:** no-op (cloud manages its own resources).
    fn sync_resource(
        &self,
        resource: &str,
        from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>>;
}
