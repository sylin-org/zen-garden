# Offering Trait Design Rationale

> Research artifact for ORCH-0013. Documents the design decisions behind
> the `Offering` trait contract based on external research and internal
> codebase analysis.

---

## Design Goal

Define the interface between the shared orchestration infrastructure and
each AI service type (Ollama, ComfyUI, whisper.cpp, etc.). The trait must:

1. Keep the mandatory surface minimal (research: 5-6 methods is the sweet spot)
2. Support backends with wildly different protocols (NDJSON, WebSocket, multipart, audio streaming)
3. Allow capability advertisement per offering type
4. Enable the domain layer to remain pure (zero I/O, zero async)
5. Be object-safe (`dyn Offering`) for runtime dispatch across heterogeneous types

---

## Method Inventory (8 methods)

### From ORCH-0013 ADR

```rust
pub trait Offering: Send + Sync {
    fn offering_type(&self) -> OfferingKind;
    fn capabilities(&self) -> &[Capability];
    fn discovery_config(&self) -> DiscoveryConfig;
    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>>;
    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>>;
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64>;
    fn proxy(&self, endpoint: &str, capability: Capability, request: ProxyRequest) -> BoxFuture<'_, Result<ProxyResponse>>;
    fn benchmark(&self, endpoint: &str, model: &str, capability: Capability) -> BoxFuture<'_, Result<BenchmarkSample>>;
    fn sync_resource(&self, resource: &str, from: &ServiceInstance, to: &ServiceInstance) -> BoxFuture<'_, Result<SyncProgress>>;
}
```

### Justification Per Method

| Method | Why It Exists | Can It Be Optional? | External Precedent |
|--------|--------------|--------------------|--------------------|
| `offering_type()` | Identity discriminator. Domain needs to know what kind of instance this is. | No — always required | Containerd `ID()`, Dapr `Features()` |
| `capabilities()` | Routing engine filters by capability before model matching. | No — without this, routing cannot work | Dapr `Features()`, Envoy filter factory |
| `discovery_config()` | Each offering discovers instances differently (port probe, topology filter, configured). | No — discovery task needs this | Containerd `Create()` opts |
| `probe()` | Health check. Each offering has a different health endpoint and expected response. | No — health checking is fundamental | Containerd `State()`, Dapr `Ping()` |
| `enumerate()` | List available models/resources. Each offering exposes this differently. | No — reconciliation and routing need model lists | Containerd `Tasks()` |
| `vram_estimate()` | Static VRAM estimate from model metadata. Not a live query. | Yes — some offerings (LibreTranslate, cloud) have no VRAM concept | Return `None` as default |
| `proxy()` | Forward a request to the backend in its native protocol. | No — this is the core function of the orchestrator | Envoy `onData()`, Dapr `Set()`/`Get()` |
| `benchmark()` | Fitness profiling with service-specific test payloads. | Could be optional — not all offerings need benchmarking initially | Default: return empty samples |
| `sync_resource()` | Replicate a model/resource across instances. | Could be optional — not all offerings support sync | Default: return `Failed("not supported")` |

### Why 8 Methods, Not 5

External research suggests 5-6 methods as the ideal mandatory surface. The
AI orchestrator has 8 because:

1. **`capabilities()` and `discovery_config()`** are metadata methods (no I/O, instant return). They correspond to Dapr's `Features()` and `Operations()` — capability advertisement that in other systems is a separate concern but here is folded into the trait for simplicity.

2. **`benchmark()` and `sync_resource()`** could be optional interfaces (Dapr pattern: type-assert for advanced capabilities). However:
   - In Rust, trait object downcasting requires `Any + 'static` and loses type safety
   - Default implementations (return empty/unsupported) are simpler and more idiomatic
   - Every offering will eventually need both — these are design requirements, not optional features

Following Envoy's pattern: **default implementations over optional interfaces**.

---

## Object Safety and BoxFuture

The trait uses `BoxFuture<'_, T>` (= `Pin<Box<dyn Future<Output = T> + Send + '_>>`)
for async methods because:

1. **Object safety**: `async fn` in traits is not object-safe (each impl generates a different Future type). The `OfferingRegistry` stores `Arc<dyn Offering>` for runtime dispatch.
2. **No async-trait**: The project removed `async-trait` in ARCH-0007. BoxFuture is the explicit replacement.
3. **Lifetime**: `'_` borrows from `&self`, allowing the future to reference the offering's HTTP client without cloning.

### Implementation Pattern

```rust
impl Offering for OllamaOffering {
    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            // Ollama-specific health check
            let resp = self.client.get(&format!("{endpoint}/")).send().await?;
            // ...
        })
    }
}
```

---

## Trait Support Types

### DiscoveryConfig

```rust
pub enum DiscoveryConfig {
    PortProbe { default_port: u16 },
    TopologyFilter { offering_name: String },
    Configured,
}
```

| Offering | Config | Rationale |
|----------|--------|-----------|
| Ollama | `TopologyFilter { "ollama" }` | Discovered via Moss topology + Tools API SSE |
| ComfyUI | `PortProbe { 8188 }` | Probe discovered stones for ComfyUI on known port |
| whisper.cpp | `PortProbe { 8080 }` | Same pattern |
| Speaches | `PortProbe { 8000 }` | Same pattern |
| OpenedAI Speech | `PortProbe { 8001 }` | Same pattern |
| Infinity | `PortProbe { 7997 }` | Same pattern |
| LibreTranslate | `PortProbe { 5000 }` | Same pattern |
| Cloud providers | `Configured` | No discovery — API key + endpoint from dashboard |

### ProbeResult

Contains version, capabilities, VRAM, and opaque metadata. The `metadata:
serde_json::Value` field follows Containerd's `Any` pattern — offering-specific
data that the domain layer never inspects but the dashboard can display.

### ProxyResponse / ProxyBody

```rust
pub enum ProxyBody {
    Complete(Vec<u8>),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>),
}
```

This two-variant enum handles all backend protocols:
- **Complete**: JSON responses (Infinity, LibreTranslate), image bytes (ComfyUI output), audio bytes (non-streaming TTS)
- **Stream**: NDJSON (Ollama), SSE progress (ComfyUI WebSocket relay adapted to HTTP), chunked audio (streaming TTS)

The proxy handler in `api/proxy.rs` dispatches based on the variant —
`Complete` becomes a single response, `Stream` becomes a chunked transfer.

---

## OfferingRegistry

```rust
pub struct OfferingRegistry {
    offerings: HashMap<OfferingKind, Arc<dyn Offering>>,
}
```

- Populated at startup (one instance per offering type)
- Immutable after init (no runtime registration/deregistration)
- Queried by routing engine to get the `Offering` impl for a given `OfferingKind`
- Dormant offerings (no instances discovered, no config) are still registered but their tasks are no-ops

### Validation at Registration (OC-1, OC-2)

```rust
impl OfferingRegistry {
    pub fn register(&mut self, offering: Arc<dyn Offering>) -> Result<()> {
        let kind = offering.offering_type();
        if self.offerings.contains_key(&kind) {
            bail!("Duplicate offering type: {:?}", kind);
        }
        if offering.capabilities().is_empty() {
            bail!("Offering {:?} declares no capabilities", kind);
        }
        self.offerings.insert(kind, offering);
        Ok(())
    }
}
```

---

## Domain Layer Isolation

The domain layer (`domain/`) never imports `catalog/` or `offerings/`.
It operates exclusively on:

- `ServiceInstance` (the generalized instance type)
- `Capability` (the unified capability enum)
- `ServiceModel` (model metadata)
- Primitive types (strings, numbers, booleans)

The `Offering` trait lives in `catalog/` because it defines I/O operations.
The task layer calls `offering.probe()`, `offering.enumerate()`, etc. and
feeds the results to domain functions as plain data.

```
offerings/ ──impl──> catalog/traits.rs
                         │
                    tasks/ calls trait methods
                         │
                    domain/ receives plain data
```

This mirrors the Ollama orchestrator's architecture where `OllamaClient`
(infra) feeds data to domain functions that operate on `OllamaInstance`
(types). The difference: instead of one client, there's a trait with
multiple implementations behind `Arc<dyn Offering>`.

---

## Comparison with ORCH-0012 ClusterAdapter

ORCH-0012 defined a `ClusterAdapter` trait for database orchestrators:

| Concern | ClusterAdapter | Offering |
|---------|---------------|----------|
| Purpose | Database cluster lifecycle | AI service orchestration |
| Methods | 5 (probe, bootstrap, add_member, remove_member, health_check) | 8 (above) |
| State model | Logical sets with membership | Instance registry with models |
| Associated types | `Instance`, `SetState`, `Action` | None (uses `ServiceInstance` directly) |
| Object safety | No (associated types) | Yes (`Arc<dyn Offering>`) |

The `Offering` trait chose object safety over associated types because:
- All offerings share the same instance type (`ServiceInstance`)
- Runtime dispatch via `Arc<dyn Offering>` is required (the routing engine doesn't know the offering type at compile time)
- Associated types would force generic parameters to propagate through the entire domain layer

ORCH-0012's `ClusterAdapter` uses associated types because database
instances are fundamentally different types (`MongoInstance` vs
`PostgresInstance`) with different fields. AI service instances share the
same shape (`ServiceInstance`) with the `kind` discriminator and opaque
`metadata` field handling the differences.
