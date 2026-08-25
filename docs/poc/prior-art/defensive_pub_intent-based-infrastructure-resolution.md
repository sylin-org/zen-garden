# Defensive Publication: Intent-Based Infrastructure Resolution with Pluggable Provider Layers

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field of Invention**: Distributed systems service discovery and infrastructure connection management
**Keywords**: intent URI, service discovery, infrastructure resolution, pluggable providers, connection management, circuit breaker, capability provisioning, zero-configuration

---

## 1. Problem Statement

Applications that depend on external infrastructure services — databases, AI inference engines, vector stores, object storage — must today specify precisely *where* those services run. Connection strings, hostnames, ports, and protocol details are embedded in configuration files, environment variables, or deployment manifests. This creates a direct coupling between the application's code (or its configuration artifacts) and the infrastructure topology.

The consequences of this coupling are:

1. **Configuration fragility.** When infrastructure migrates between nodes — due to failover, scaling, or operator decision — every consuming application must have its configuration updated. In a multi-service deployment with N consumers and M infrastructure services, the configuration surface grows as N x M.

2. **No capability abstraction.** An application cannot express "I need a MongoDB-compatible database" without also specifying which specific instance, at which address. The *what* (capability need) is conflated with the *where* (network location).

3. **No autonomous recovery.** If a service endpoint becomes unavailable, the application has no built-in mechanism to discover an alternative provider. Orchestrators like Kubernetes offer endpoint watches for pods behind a Service abstraction, but these are limited to a single cluster namespace and a single resolution strategy.

4. **No progressive provisioning.** When an application requires infrastructure capabilities that do not yet exist (e.g., a specific AI model not yet loaded), there is no standard mechanism to express that need and have the platform provision it autonomously.

Existing systems address fragments of this problem:

- **Container orchestrators** (Kubernetes, Docker Compose) provide DNS-based service names, but the application still specifies a service name that maps 1:1 to a deployment. There is no pluggable resolution chain, no capability query, and no cross-cluster discovery.
- **Service meshes** (Istio, Linkerd) handle routing and failover at the network layer but do not operate at the application intent layer — they route traffic to named services, not to capability requests.
- **Configuration-based discovery** (.NET Aspire, Spring Cloud Config) externalizes connection strings but still requires an operator to populate them. The application declares a configuration key, not an infrastructure intent.

The disclosed system solves this by introducing an intent-based resolution pipeline where applications declare *what* they need using a structured URI scheme, and a layered chain of pluggable providers resolves, discovers, provisions, connects, and maintains the connection — without the application specifying or knowing the infrastructure topology.

---

## 2. Prior Art Summary

### 2.1 DNS-SD (RFC 6763)

DNS Service Discovery uses DNS records (SRV, TXT, PTR) to advertise services by type (e.g., `_http._tcp`). A client queries for a service type and receives one or more host:port pairs.

**Limitations relative to the disclosed system:**
- Resolution is a single strategy (DNS query). There is no fallback chain, no pluggable provider mechanism, and no domain-specific health probing.
- Service types are IANA-registered strings, not extensible intent URIs. There is no mechanism for sub-capability requirements (e.g., "MongoDB with oplog enabled" or "Ollama with llama2 loaded").
- Discovery is passive — the client queries and receives results. There is no wishful provisioning where the system creates missing capabilities in response to the query.
- No connection lifecycle management. After resolution, the client is responsible for detecting failures and re-resolving.

### 2.2 HashiCorp Consul

Consul provides service registration, health checking, and DNS/HTTP-based service lookup. Services register with a name and optional tags. Clients query by service name and receive healthy instances.

**Limitations relative to the disclosed system:**
- Single resolver. Consul is itself the sole resolution authority. There is no ordered chain of fallback providers (environment variables, container detection, topology cache, multicast discovery, localhost).
- Tags are static metadata attached at registration time. They do not support dynamic capability queries with AND semantics across capability types (e.g., "has model:llama2 AND model:nomic-embed-text").
- No intent URI scheme. The client specifies a service name, not a structured intent. There is no mechanism for a URI scheme to dispatch to different resolution pipelines.
- No reference-as-intent pattern. Adding a library does not automatically register a service discovery binding.
- No wishful provisioning. If a requested capability does not exist, the query fails; Consul does not instruct the infrastructure to create it.

### 2.3 Kubernetes Services

Kubernetes Services provide a stable DNS name (e.g., `my-service.my-namespace.svc.cluster.local`) that resolves to a set of pod IP addresses. Endpoint watches notify clients when the set of backing pods changes.

**Limitations relative to the disclosed system:**
- DNS alias, not intent abstraction. The Service name is a deployment-specific identifier chosen by the operator, not a capability declaration by the consumer.
- Single-cluster scope. Cross-cluster discovery requires additional infrastructure (federation, service mesh).
- No pluggable provider chain. Resolution always goes through kube-dns/CoreDNS. There is no mechanism for the application to fall through from Kubernetes DNS to environment variables to multicast LAN discovery.
- No capability-predicate queries. A client cannot request "a service that has capability model:llama2"; it can only request a named Service.
- No circuit breaker with SSE-driven endpoint migration. Endpoint watches update the pod set, but the application must implement its own connection lifecycle management.

### 2.4 .NET Aspire

Aspire provides service discovery through configuration keys. An application references a service by a logical name (e.g., `"mongodb"`), and Aspire resolves it through configuration or the AppHost's orchestration model.

**Limitations relative to the disclosed system:**
- Configuration-key resolution, not intent URI. The application uses a string key mapped in configuration, not a structured URI with offering, instance, and capability fields.
- Single resolution strategy per environment. In development, the AppHost provides endpoints. In production, the operator must configure them. There is no autonomous multi-strategy fallback chain.
- Partial reference-as-intent. Aspire service defaults register via the hosting model, but the pattern is limited to the Aspire AppHost context and does not extend to autonomous discovery in standalone or containerized deployments.
- No capability queries or wishful provisioning.
- No SSE-driven circuit breaker for live connection maintenance.

### 2.5 Dapr (Distributed Application Runtime)

Dapr provides a sidecar that abstracts infrastructure components (state stores, pub/sub, bindings). Applications call the sidecar's HTTP/gRPC API with a component name, and the sidecar routes to the configured backend.

**Limitations relative to the disclosed system:**
- Sidecar-mediated, not in-process resolution. All traffic flows through the sidecar, adding latency and requiring sidecar lifecycle management.
- Component names are configured by the operator, not derived from intent URIs. There is no structured grammar for expressing offering + instance + capabilities.
- No layered fallback. Each component has a single configured backend. If it fails, the sidecar does not autonomously discover alternatives through environment detection, multicast, or topology caches.
- No reference-as-intent. Adding a Dapr SDK does not automatically declare what infrastructure the application needs.
- No capability-predicate queries. A component either exists in the configuration or it does not.

### 2.6 JNDI (Java Naming and Directory Interface)

JNDI provides a naming context for looking up resources (DataSources, JMS queues) by name in Java EE environments. The application server pre-configures the bindings.

**Limitations relative to the disclosed system:**
- Static naming context. Resources are bound by an administrator before the application starts. There is no autonomous discovery.
- No fallback chain. If the named resource is not bound, the lookup fails. There is no mechanism to fall through alternative providers.
- No capability queries. The name is an opaque string, not a structured intent with capability selectors.
- No live connection maintenance. JNDI returns a reference at lookup time; it does not subscribe to infrastructure changes or migrate connections.
- No wishful provisioning. Missing resources cause lookup failures, not provisioning actions.

---

## 3. Detailed Description of the Invention

### 3.1 Intent Handler URI Scheme

The disclosed system introduces a URI scheme where the scheme name is a dispatch key to a resolution pipeline rather than a network protocol identifier.

**Grammar:**

```
intent-uri     = handler "://" offering [ ":" instance ] [ "?" query ]
handler        = 1*( ALPHA / DIGIT / "-" )
offering       = 1*( ALPHA / DIGIT / "-" )
instance       = 1*( ALPHA / DIGIT / "-" / "." )
query          = param *( "&" param )
param          = "cap" "=" capability *( "," capability )
capability     = cap-type ":" cap-item
cap-type       = 1*( ALPHA / DIGIT / "-" )
cap-item       = 1*( ALPHA / DIGIT / "-" / "." / ":" )
```

**Examples:**

```
zen-garden://mongodb                          → resolve MongoDB, any instance
zen-garden://mongodb:prod                     → resolve MongoDB, prod instance
zen-garden://ollama?cap=model:llama2          → resolve Ollama with llama2 model loaded
zen-garden://ollama?cap=model:llama2,model:nomic-embed-text → AND semantics
```

The scheme (`zen-garden` in the reference implementation) identifies which resolution pipeline to invoke. A different framework could register `my-cloud://` with an entirely different set of resolution providers. The dispatch mechanism — scheme identifies pipeline — is the invention, not the particular scheme name.

**Scheme-to-Pipeline Dispatch Mechanism:**

The runtime maintains a handler registry — a map from scheme strings to resolution pipeline instances. Registration occurs at host builder startup:

1. The host builder scans loaded assemblies for types implementing a handler registration interface (e.g., `IIntentHandlerRegistration`).
2. Each registration contributes a `(scheme_name, pipeline_factory)` tuple.
3. The registry stores these in a `Dictionary<string, IResolutionPipeline>` (or language equivalent: `HashMap<String, Box<dyn ResolutionPipeline>>` in Rust).
4. At resolution time, the URI's scheme is extracted and used as a lookup key in the registry. If the scheme is not registered, the URI is treated as a conventional connection string (passthrough).
5. The registry is populated at startup and is immutable during the application's lifetime. Hot-reloading is explicitly not required — pipeline composition changes require a restart. This is a deliberate design choice: resolution pipelines are structural, not dynamic.

An alternative embodiment supports lazy registration where the first parse of an unrecognized scheme triggers assembly scanning for a matching handler, enabling plugin-style extensibility without upfront registration of all handlers.

**Parsing algorithm:**

1. Extract scheme prefix; reject if not a registered handler.
2. Strip scheme prefix to obtain payload.
3. Split payload at `?` to separate target from query string.
4. Split target at `:` to separate offering from optional instance.
5. URI-decode both offering and instance. Normalize to lowercase.
6. Parse query string: extract all `cap=` parameters. Split comma-separated values. Normalize to lowercase. Deduplicate preserving insertion order.
7. Construct an immutable intent record containing: offering (required string), instance (optional string), capabilities (ordered list of strings).

**Programmatic construction:**

```
ForOffering(offering: "mongodb", instance: "prod", capabilities: ["model:llama2"])
  → ZenGardenConnectionIntent { Offering: "mongodb", Instance: "prod", Capabilities: ["model:llama2"] }
```

**Selector conversion:**

The intent converts to an offering selector string (`"mongodb"` or `"mongodb:prod"`) used by downstream APIs. This selector is the lookup key, not the full intent — capabilities travel as a separate predicate on the query.

### 3.2 Reference-As-Intent (Zero-Config Binding)

The disclosed system implements a pattern where adding a library dependency to a project automatically registers an infrastructure intent binding, with no explicit configuration required.

**Mechanism:**

1. A connector library (e.g., a MongoDB connector package) includes an auto-registrar class implementing a known bootstrap interface (`IKoanAutoRegistrar`).

2. The framework's host builder scans loaded assemblies at startup and invokes each auto-registrar's `Initialize` method.

3. The auto-registrar registers an offering binding into the dependency injection container:

```
interface IZenGardenOfferingBinding {
    AdapterId: string   // e.g., "mongo"
    Offering: string    // e.g., "mongodb"
}
```

4. The binding maps the adapter's local identifier to the canonical offering name used in intent resolution.

5. Multiple bindings can map to the same offering (e.g., both `"mongo"` and `"mongodb"` map to offering `"mongodb"`).

**Result:** The act of adding a package reference to a project is simultaneously the declaration that the application needs that infrastructure capability. No `appsettings.json` entry, no environment variable, no deployment manifest annotation is required for the basic intent to be resolvable.

**Differentiation from Spring Boot auto-configuration and similar auto-registration systems:**

Spring Boot `@ConditionalOnClass` and similar mechanisms (Quarkus extensions, Micronaut auto-config) register *implementation beans* — a `DataSource`, a `MongoClient` — into a DI container. The auto-configured bean is a ready-to-use service client. The developer still must configure the connection target (via `application.properties` or environment variables). The auto-registration is of the *implementation*, not the *intent*.

In the disclosed system, the auto-registrar does not register a client, a connection, or an implementation. It registers an *offering binding* — a mapping from an adapter identifier to a canonical offering name. This binding carries no connection details, no endpoint, no credentials. It feeds into the resolution pipeline (Section 3.3) which autonomously discovers and connects. The binding says "this application needs MongoDB" — not "here is a configured MongoDB client." The resolution of that need is entirely deferred to the multi-strategy discovery chain.

This distinction is mechanically verifiable: the registered type (`IZenGardenOfferingBinding`) has exactly two string fields (`AdapterId`, `Offering`) and no connection-related state. It is a declaration of need, not a provision of service.

**Example binding implementation:**

```
class MongoZenGardenOfferingBinding : IZenGardenOfferingBinding {
    AdapterId = "mongo"
    Offering  = "mongodb"
}

class MongoDbZenGardenOfferingBinding : IZenGardenOfferingBinding {
    AdapterId = "mongodb"
    Offering  = "mongodb"
}
```

Both are registered via `TryAddEnumerable` so they coexist without conflict and are automatically discovered by the resolution pipeline.

### 3.3 Multi-Strategy Autonomous Discovery

Each infrastructure domain implements a discovery adapter. The base class (`ServiceDiscoveryAdapterBase`) orchestrates a priority-ordered fallback chain. The adapter provides domain-specific health validation.

**Fallback chain (7 tiers, evaluated in priority order):**

| Priority | Strategy | Source |
|----------|----------|--------|
| 0 | Service-specific environment variables | `MONGO_URLS`, `OLLAMA_HOST`, etc. |
| 1 | Explicit configuration | `appsettings.json`, user secrets |
| 2 | Container instance detection | Docker networking: service hostname + port from `KoanServiceAttribute` |
| 2 (alt) | Local instance | `localhost:{default_port}` when not containerized |
| 1 (override) | Aspire service discovery | AppHost-provided endpoint (highest priority in Aspire mode) |
| 4 | Zen Garden topology API | SSE tools stream with cursor-based resume |
| 5 | File-based topology cache | `garden-stones.json` on shared mount |
| 6 | Multicast/broadcast LAN discovery | UDP multicast group 239.255.42.99:7184 |
| 7 | Localhost fallback | `localhost:{default_port}` as last resort |

**Algorithm:**

1. Read the adapter's `KoanServiceAttribute` to obtain default host, port, scheme, local host, and local port.
2. Build a list of `DiscoveryCandidate` records. Each record contains:
   - `Url`: the fully-qualified endpoint URL (e.g., `mongodb://192.168.1.50:27017/mydb`)
   - `Method`: a human-readable label identifying the discovery strategy (e.g., `"environment"`, `"container"`, `"zen-garden-topology"`, `"multicast"`, `"localhost"`)
   - `Priority`: an integer where 0 is highest priority
   - `IsContainerLocal`: boolean indicating whether this candidate was discovered via container networking (used for diagnostic logging)
   - `RawEndpoint`: the unmodified endpoint before service-specific connection parameters were applied (used for deduplication — two strategies may produce the same endpoint)
3. Add environment variable candidates (priority 0).
4. Add explicit configuration candidates (priority 1).
5. If running inside a container: add container-instance candidate (priority 2) using the attribute's host and port; add local fallback (priority 3) using the attribute's local host and local port. If running standalone: add local candidate (priority 2).
6. If orchestration mode is Aspire AppHost: add Aspire service discovery candidate at priority 1 (overrides container/local).
7. Apply service-specific connection parameters (database name, credentials) to each candidate URL.
8. Sort candidates by priority (ascending = highest priority first).
9. For each candidate in order: validate health using the adapter's domain-specific probe. If healthy, return the candidate. If unhealthy, continue to next.
10. If all candidates fail, return a failure result.

**Domain-specific health probes:**

Each adapter overrides `ValidateServiceHealth` with a protocol-appropriate check:

- **MongoDB**: Attempts a `ping` command against the MongoDB wire protocol.
- **Weaviate**: Sends an HTTP GET to `/v1/.well-known/ready`.
- **Ollama**: Sends an HTTP GET to `/api/tags` (model list endpoint). The Ollama adapter additionally uses parallel probing with a 750ms timeout across multiple candidates rather than sequential evaluation.
- **HTTP services**: Generic HTTP GET to `/health`.
- **TCP services**: TCP connect with timeout.

**Priority short-circuit:** When a high-priority candidate passes health validation, lower-priority candidates are never evaluated. This avoids unnecessary network probes.

**Parallel probing variant:** For latency-sensitive domains (e.g., AI inference), the discovery adapter supports a parallel probing mode where multiple candidates at the same priority level are health-checked concurrently. The first candidate to pass health validation within a timeout window (e.g., 750ms) is selected. If no candidate passes within the window, the adapter falls through to the next priority tier. This parallel-within-tier, sequential-across-tiers strategy combines low-latency discovery with deterministic priority ordering. The choice between sequential and parallel evaluation is a per-adapter configuration, not a global setting.

**Negative and disjunctive capability selectors:** The capability query syntax supports three forms:

- AND (default): `?cap=model:llama2,model:nomic-embed-text` — tool must satisfy all selectors.
- OR: `?cap=model:llama2|model:mistral` — tool must satisfy at least one selector within a pipe-delimited group.
- NOT: `?cap=!model:deprecated-v1` — tool must NOT have the specified capability.
- Combined: `?cap=model:llama2|model:mistral,!model:deprecated-v1` — groups are AND-combined; within each group, pipe is OR; exclamation prefix is NOT.

The grammar extension:

```
param          = "cap" "=" cap-group *( "," cap-group )
cap-group      = [ "!" ] capability *( "|" capability )
```

This covers the full Boolean predicate space (AND of ORs with optional negation per group — effectively CNF).

### 3.4 Unified Garden Registry

The platform maintains a single write-through cache on each node holding all infrastructure tool entries from all known sources.

**Data model:**

```
RegistryEntry {
    tool:       GardenTool      // the tool data (TOOLS-0002 contract)
    version:    u64             // per-entry monotonic version, incremented on each upsert
    origin:     EntryOrigin     // who wrote this entry
    expires_at: Option<Instant> // TTL expiry (Gateway entries only)
}

EntryOrigin = Local | Gateway | Announced { stone_id: String }

RegistryKey = "{stone_id}:{fqid}:{category}"
```

**Origin tracking:**

Each entry records its origin, which determines its lifecycle owner:

- `Local`: Projected from this node's own offerings and storage volumes. The local reconciliation process owns the lifecycle — it adds entries when offerings are deployed and removes them when offerings are removed.
- `Gateway`: Written directly by an orchestrator via HTTP PUT. TTL-managed — the entry expires if not refreshed within the lease period (default 60 seconds). If the orchestrator crashes, its entries self-evict.
- `Announced`: Received from a remote node via UDP beacon. Beacon reconciliation owns the lifecycle — a full-snapshot beacon from a remote node causes removal of any previously-announced entries from that node that are absent from the snapshot.

**Write-through semantics:**

Every mutation goes through the `upsert` method, which:

1. Computes the registry key from stone_id, fqid, and category.
2. Checks if an existing entry with the same key has equivalent content and the same origin. If so, silently refreshes the TTL (for Gateway entries) without generating a delta event.
3. If content differs or the entry is new: increments the version counter, inserts the entry, and appends a `ToolDelta` to the history ring buffer.
4. Returns the delta (if any) so the caller can broadcast it to SSE subscribers.

**TTL-based eviction:**

A periodic reaper task scans all entries. Any entry with `expires_at < now` is removed and a `Remove` delta is generated. Only `Gateway`-origin entries carry an expiry. This ensures that orchestrator registrations self-heal after crashes without requiring explicit de-registration.

**Capability-predicate queries:**

The registry supports queries with AND semantics across capability selectors:

```
?capability=model:llama2,model:nomic-embed-text
```

Each selector is a `(cap_type, item)` pair. A tool matches only if it reports *all* requested capabilities. The `has_capability` method checks the tool's capability list.

### 3.5 Cursor-Based SSE Delta Streaming with Replay on Reconnect

The registry maintains a monotonically increasing cursor and a bounded history ring buffer (default 4096 entries).

**Initial connection:**

1. Client connects to the SSE endpoint.
2. Server reads the registry under a read lock.
3. Server emits a `tools.snapshot` event containing the current cursor and the full set of tools matching the client's query filter.
4. If the client provided a `since` cursor (via query parameter or `Last-Event-ID` header), the server also emits replay deltas — all deltas with cursor > since that match the filter.

**Live streaming:**

After the initial snapshot and replay, the server subscribes to a broadcast channel. Each delta event is:

1. Checked against the snapshot cursor — deltas already covered by the snapshot are suppressed.
2. Filtered against the client's query predicate.
3. Serialized as an SSE event with the delta's cursor as the `id` field.

**Reconnection:**

When a client reconnects (network interruption, process restart), it sends `Last-Event-ID` with the cursor of the last received event. The server:

1. Checks if the cursor falls within the history buffer.
2. If yes: emits a fresh snapshot plus replay deltas since the cursor. The client merges these into its local state without data loss.
3. If no (cursor too old, history evicted): emits a full snapshot. The client replaces its local state.

**Heartbeat:**

A periodic heartbeat event (default 15 seconds) keeps the connection alive and provides the current cursor for monitoring.

**Event types:**

- `tools.snapshot`: Full state snapshot.
- `tools.upsert`: A tool was added or updated.
- `tools.remove`: A tool was removed.
- `tools.heartbeat`: Connection keepalive with cursor.

### 3.6 Wishful Capability Provisioning

When an application needs capabilities that do not yet exist in the infrastructure, it can issue a "wish" — a declarative request that the platform provision the missing capabilities.

**Algorithm:**

1. The client calls `Wish(offering, capabilities)` where `offering` is a string (e.g., `"ollama"`) and `capabilities` is a list of required capabilities (e.g., `["model:llama2", "model:nomic-embed-text"]`).

2. The system resolves the current tool snapshot for the offering from the local registry mirror.

3. The system evaluates which requested capabilities are already satisfied by comparing the capability list from the tool snapshot against the requested capabilities.

4. For each missing capability, the system issues a provisioning request to the platform (e.g., an HTTP POST to the Moss API requesting that the model be pulled to the offering).

5. The wish is tracked as a stateful record:

```
CapabilityWish {
    RequestId:         string
    ToolFqid:          string
    OfferingSelector:  string
    Requested:         string[]    // all requested capabilities
    Satisfied:         string[]    // capabilities already present
    Missing:           string[]    // capabilities not yet present
    IsFulfilled:       bool
    Status:            string      // "requested" | "in_progress" | "partial" | "fulfilled" | "failed"
    CreatedAt:         timestamp
    UpdatedAt:         timestamp
}
```

6. Progress events are published to subscribers as the platform provisions capabilities:

```
CapabilityProgressEvent {
    Kind:        Requested | InProgress | PartiallyFulfilled | Fulfilled | Failed
    Wish:        CapabilityWish     // current state
    Previous:    CapabilityWish?    // previous state (for diffing)
    CurrentTool: ToolSnapshot?      // latest tool state
    Timestamp:   timestamp
}
```

7. As the SSE stream delivers tool updates showing new capabilities appearing on the offering, the wish is re-evaluated. When all requested capabilities are present, the wish transitions to `Fulfilled`.

**Idempotency and deduplication:**

If a client issues a wish for an offering + capabilities combination that is already tracked by an active (non-terminal) wish, the system returns the existing wish record rather than creating a duplicate. Deduplication is keyed on `(OfferingSelector, sorted(Requested))`. This prevents redundant provisioning requests when multiple components in the same application independently wish for the same capabilities. Terminal wishes (Fulfilled, Failed) are excluded from deduplication — a new wish for the same capabilities after a failure creates a fresh tracking record.

**Timeout and failure semantics:**

- Each wish carries a configurable timeout (default: 5 minutes). If the wish has not reached `Fulfilled` within the timeout, it transitions to `Failed` with a `Timeout` reason.
- If the platform daemon is unreachable when provisioning requests are issued, the wish transitions to `Failed` with a `PlatformUnreachable` reason. The client may retry by issuing a new wish.
- If provisioning partially succeeds (some capabilities appear, others do not), the wish remains in `PartiallyFulfilled` until either the remaining capabilities appear or the timeout expires.
- The client can cancel an in-progress wish, which transitions it to `Failed` with a `Cancelled` reason and stops further provisioning attempts.
- A wish in `Failed` state is terminal. The client must issue a new wish to retry. This prevents zombie wishes from consuming resources indefinitely.

**Fallback-on-partial mode:** The client may optionally request `AcceptPartial = true`, in which case a `PartiallyFulfilled` wish is treated as success — the client proceeds with whatever capabilities were provisioned. This enables graceful degradation (e.g., an application that prefers two AI models but can function with one).

**Dry-run mode:** The system supports a `DryRun` option that evaluates current vs. requested capabilities without issuing provisioning requests. This allows the application to check feasibility before committing.

### 3.7 Live Connection Maintenance via SSE-Driven Circuit Breaker

The disclosed system provides a generic connection manager (`GardenAwareEndpointManager<TConnection>`) that subscribes to SSE availability events and maintains active connections transparently to the application.

**Type parameter:** `TConnection` is the connection type specific to each infrastructure domain — a MongoDB client, an HTTP client for Ollama, an S3 client. The manager is generic over this type.

**Construction:**

The manager is initialized with:
- A `ZenGardenSubscription` predicate identifying which tool to watch.
- A `connectionFactory: (string endpoint) -> TConnection` delegate that creates a new connection from an endpoint URL.
- An optional `disposeConnection` delegate for cleanup.
- An optional initial endpoint for non-SSE bootstrap.

**Circuit breaker states:**

```
CircuitState = Closed | Open | HalfOpen

Closed:   Primary endpoint is healthy. All operations use the current connection.
Open:     Primary is unavailable. Operations return null (caller uses fallback or fails).
HalfOpen: SSE reported the endpoint is ready. Next operation attempts the connection;
          success transitions to Closed, failure transitions back to Open.
```

**Event handling:**

The manager subscribes to the `ZenGardenClient`'s availability event stream. Events are:

- `Online` / `Changed`: The tool's endpoint is available or has changed.
  1. Extract the new endpoint URL from the tool snapshot (preference order: connection URIs, hostname + port, stone endpoint).
  2. If the endpoint differs from the current one: dispose the old connection, invoke the connection factory with the new endpoint.
  3. If circuit was `Open`: transition to `HalfOpen`.
  4. Invoke the `OnEndpointChanged` event for any listeners.

- `Offline`: The tool is no longer available.
  1. Transition circuit to `Open`.

**Transparent migration:**

When infrastructure moves between nodes (e.g., a MongoDB container is migrated from node A to node B), the sequence is:

1. Node A reports the tool as offline via beacon.
2. The registry removes the tool entry and emits a `Remove` delta.
3. The SSE stream delivers an `Offline` event to the client. Circuit opens.
4. Node B deploys the tool and reports it via beacon.
5. The registry upserts the tool entry with node B's endpoint.
6. The SSE stream delivers an `Online` event with the new endpoint.
7. The manager creates a new connection via the factory. Circuit enters `HalfOpen`.
8. The application's next operation succeeds. Circuit closes.

The application code never observes this migration. It calls `GetConnection()` and receives either a valid connection or `null` (if circuit is open, prompting graceful degradation).

**Failure reporting:**

The application can proactively report transport failures via `ReportFailure()`, which immediately opens the circuit without waiting for SSE propagation. This handles cases where the application detects a failure before the platform does (e.g., a 503 response from an S3 gateway).

**Configurable circuit breaker parameters:**

- `HalfOpenProbeInterval`: How long to wait in HalfOpen before the next connection attempt (default: 5 seconds). If the first operation after entering HalfOpen fails, the manager waits this interval before allowing the next attempt, with exponential backoff (2x per failure, capped at 60 seconds).
- `MaxOpenDuration`: Maximum time to remain in Open state before forcibly attempting a probe (default: 120 seconds). This prevents indefinite Open state when SSE events are delayed.
- `ConsecutiveFailuresToOpen`: Number of consecutive `ReportFailure()` calls required to open the circuit when it is currently Closed (default: 1). Setting this higher provides tolerance for transient failures.

**Multi-endpoint load balancing variant:**

When the registry reports multiple endpoints for the same tool (e.g., the same offering deployed on multiple nodes), the manager supports a multi-endpoint mode:

1. The manager maintains a connection pool — one connection per endpoint, each created via the connection factory.
2. Requests are distributed across healthy connections using round-robin, random, or least-connections strategies (configurable).
3. Each connection has its own independent circuit breaker state. A single endpoint's failure does not affect other connections.
4. When an endpoint goes offline (SSE `Offline` event), its connection is disposed and removed from the pool. When a new endpoint appears (SSE `Online` event), a connection is created and added.
5. `GetConnection()` returns any healthy connection from the pool, or null if all circuits are open.

The single-endpoint mode (default) and multi-endpoint mode share the same SSE subscription and circuit breaker state machine. The difference is cardinality: single mode manages one `(endpoint, connection)` pair; multi mode manages N pairs.

### 3.8 Shared Topology Directory

The disclosed system uses a dual-file shared directory to enable infrastructure discovery without HTTP dependency, particularly for cold container startup.

**Directory location:** A well-known path on the host filesystem (e.g., `/var/lib/zen-garden/topology/` on Linux, `{ProgramData}\zen-garden\topology\` on Windows).

**Dual-file ownership:**

| File | Writer | Reader | Content |
|------|--------|--------|---------|
| `garden-topology.json` | Platform daemon (Moss) | Client applications, containers | Full topology: all nodes, their endpoints, health status |
| `garden-stones.json` | Client applications | Platform daemon, other clients | Stone roster: discovered nodes with timestamps, TTL-based eviction (7-day TTL) |

Each file has exactly one writer category and multiple readers. This eliminates write contention without requiring file locking.

**Container auto-injection:**

When the platform daemon deploys a managed container, it automatically adds a bind mount:

```
host: {topology_dir} → container: /app/cache/zen-garden/
```

This mount is injected transparently for all managed containers, regardless of their type. The container can read topology files immediately at startup without making any HTTP requests.

**Cold container startup sequence:**

1. Container starts. No HTTP connectivity to the platform yet.
2. Application reads `garden-stones.json` from `/app/cache/zen-garden/`.
3. Application discovers the platform daemon's endpoint from the topology file.
4. Application establishes SSE connection to the tools stream.
5. Application receives live updates from this point forward.

Steps 2-3 happen in-process with no network I/O. This eliminates the bootstrap dependency cycle where a container needs to discover the platform in order to discover anything else.

**File schemas:**

`garden-topology.json` (written by platform daemon):

```json
{
  "version": 1,
  "cursor": 48291,
  "updated_at": "2026-03-24T12:00:00Z",
  "stones": [
    {
      "id": "stone-abc123",
      "name": "stone-crystal-forest",
      "endpoint": "http://192.168.1.50:7185",
      "health": "healthy",
      "capabilities": { "gpu": "RTX 4090", "vram_mb": 24576 },
      "tools": [
        { "fqid": "ollama", "category": "orchestrator", "endpoints": ["http://192.168.1.50:21434"] }
      ]
    }
  ]
}
```

`garden-stones.json` (written by client applications):

```json
{
  "version": 1,
  "entries": [
    {
      "stone_id": "stone-abc123",
      "name": "stone-crystal-forest",
      "endpoint": "http://192.168.1.50:7185",
      "discovered_at": "2026-03-24T12:00:00Z",
      "last_seen": "2026-03-24T14:30:00Z",
      "source": "multicast"
    }
  ]
}
```

Both schemas are intentionally flat JSON with no nesting beyond one level. The `version` field enables forward-compatible schema evolution. Entries in `garden-stones.json` are evicted when `last_seen` exceeds the TTL (default 7 days).

**Persistence semantics:**

- The platform daemon writes `garden-topology.json` with debounced persistence: a 500ms dirty-write delay (coalesces rapid changes) plus a 30-second periodic flush (catches stragglers).
- Client applications write `garden-stones.json` using atomic rename: write to a temporary file, then rename to the target path. This prevents partial reads.
- Entries in `garden-stones.json` carry timestamps and are evicted after a configurable TTL (default 7 days) to prevent stale data accumulation.

---

## 4. Claims-Style Disclosure

The following numbered statements describe the disclosed system's novel aspects. These are published to establish prior art and prevent future patent claims on these techniques.

**Disclosure 1 — Intent Handler URI Scheme.** A method for addressing infrastructure services comprising: a URI whose scheme component identifies a resolution pipeline rather than a network protocol; the URI authority component encoding a desired infrastructure offering; an optional instance selector separated by a colon within the authority; optional capability requirements encoded as query parameters with AND semantics; wherein the scheme is a dispatch key to an ordered sequence of resolution providers, and any framework may register its own scheme to invoke its own provider chain.

**Disclosure 2 — Reference-As-Intent Binding.** A method for automatic infrastructure intent declaration comprising: a software library that, upon inclusion as a project dependency, registers a binding between a local adapter identifier and a canonical infrastructure offering name via an auto-registrar pattern; wherein the framework's host builder discovers and invokes all auto-registrars at startup; wherein no explicit configuration file entry, environment variable, or deployment manifest annotation is required for the intent to be resolvable; and wherein multiple adapter identifiers may map to the same canonical offering.

**Disclosure 3 — Multi-Strategy Autonomous Discovery with Domain-Specific Probes.** A method for resolving infrastructure endpoints comprising: an ordered fallback chain of at least five resolution strategies (explicit configuration, environment variables, container host detection, platform topology API, file-based cache, multicast LAN discovery, localhost); each candidate validated by a domain-specific health probe (database wire-protocol ping, HTTP health endpoint, TCP connect); priority short-circuit where a healthy high-priority candidate suppresses evaluation of lower-priority strategies; an abstract base class that encapsulates the fallback chain while allowing each infrastructure domain to provide its own health validation and connection parameter normalization; and an optional parallel probing mode where candidates within the same priority tier are health-checked concurrently with a configurable timeout window, the first passing candidate being selected.

**Disclosure 4 — Unified Registry with Origin Tracking and TTL Eviction.** A write-through cache for infrastructure service entries comprising: entries from heterogeneous sources each tagged with an origin (Local, Gateway, Announced); a composite registry key of (node identifier, fully-qualified tool identifier, category); per-entry monotonic versioning; TTL-based expiration for lease-registered entries such that orchestrator crashes cause automatic entry eviction without explicit deregistration; and batch reconciliation where a full-snapshot beacon from a remote node causes removal of previously-announced entries absent from the snapshot.

**Disclosure 5 — Cursor-Based SSE Streaming with Delta Replay.** A server-sent event streaming protocol for infrastructure registry changes comprising: a monotonically increasing cursor assigned to each mutation; a bounded history ring buffer; an initial snapshot event on connection containing the current cursor and all matching tool entries; replay of delta events between a client-provided resume cursor and the current state; the resume cursor transmitted via the SSE `Last-Event-ID` header; and heartbeat events carrying the current cursor for monitoring; wherein clients that reconnect within the history window receive only the missed deltas rather than a full state transfer.

**Disclosure 6 — Wishful Capability Provisioning.** A method for declarative infrastructure provisioning comprising: receiving a wish specifying an offering and a set of required capabilities; evaluating which capabilities are currently satisfied by querying the tool registry; issuing provisioning requests to the platform for each missing capability; tracking wish state through a progression of states (Requested, InProgress, PartiallyFulfilled, Fulfilled, Failed); publishing progress events to subscribers as capabilities are provisioned; and re-evaluating the wish against the SSE tool stream as the infrastructure state changes, transitioning to Fulfilled when all requested capabilities appear.

**Disclosure 7 — Live Connection Maintenance via SSE-Driven Circuit Breaker.** A generic connection manager parameterized over a connection type comprising: subscription to an SSE availability event stream filtered by a tool predicate; a three-state circuit breaker (Closed, Open, HalfOpen) driven by availability events rather than failure counters; automatic endpoint migration when the tool's endpoint changes — disposing the old connection and invoking a connection factory with the new endpoint; transition from Open to HalfOpen on an Online/Changed event and from HalfOpen to Closed on successful application operation; and proactive failure reporting by the application that opens the circuit without waiting for SSE propagation; wherein the application code is unaware of endpoint changes and observes only a stable connection interface.

**Disclosure 8 — Shared Topology Directory with Dual-File Ownership.** A file-based topology cache comprising: a well-known directory on the host filesystem; two files with distinct ownership — one written exclusively by the platform daemon and one written exclusively by client applications; automatic bind-mount injection into all managed containers at a well-known container path; debounced persistence with dirty-write delay and periodic flush; atomic-rename writes to prevent partial reads; timestamp-based entry eviction with configurable TTL; enabling cold container startup without HTTP dependency by reading the topology file before establishing network connectivity.

**Disclosure 9 — Scheme-to-Pipeline Dispatch Registry.** A runtime dispatch mechanism comprising: a registry mapping URI scheme strings to resolution pipeline instances; population of the registry at application startup via assembly scanning for handler registration types; immutable registry after startup (pipeline composition is structural, not dynamic); passthrough behavior for unrecognized schemes (treated as conventional connection strings); and an alternative embodiment with lazy registration where first-parse of an unrecognized scheme triggers on-demand scanning for matching handler types.

**Disclosure 10 — Combined System Operating Across Heterogeneous Infrastructure Domains.** A system combining Disclosures 1 through 9 wherein the same intent resolution pipeline, discovery mechanism, registry, SSE streaming protocol, capability provisioning, and connection maintenance operate uniformly across heterogeneous infrastructure domains including but not limited to: document databases (MongoDB), vector databases (Weaviate), AI inference engines (Ollama), object storage (S3-compatible), and service registries; with each domain providing only a domain-specific health probe and connection parameter normalizer while inheriting the full resolution, discovery, streaming, and connection lifecycle infrastructure.

**Disclosure 11 — Capability-Predicate Queries Across Tool Categories.** A query mechanism for infrastructure registries comprising: capability selectors as typed key-value pairs (e.g., `model:llama2`); AND semantics where a tool must satisfy all selectors to match; OR semantics within pipe-delimited selector groups; NOT semantics via exclamation prefix on individual selectors; the combination forming a CNF-style Boolean predicate over capabilities; selectors operating across tool categories (orchestrators, offerings, storage) in a single unified registry; and the same selector syntax usable in both REST API queries, SSE stream filters, and intent URI query parameters.

---

## 5. Implementation Evidence

The following files in the two reference implementations contain the disclosed mechanisms.

### Zen Garden (Rust platform)

| Mechanism | File |
|-----------|------|
| Unified Garden Registry | `src/moss/src/domain/garden_registry.rs` |
| SSE Tools Stream API | `src/moss/src/api/v1/tools.rs` |
| Tool data contracts | `src/common/` — `GardenTool`, `ToolDelta`, `ToolDeltaKind`, `ToolsBeacon`, `CapabilitySelector` |
| Multicast discovery | `src/moss/src/infra/communications/p2p.rs` |

### Koan Framework (.NET application framework)

| Mechanism | File |
|-----------|------|
| Intent Handler URI | `src/Koan.ZenGarden.Core/ZenGardenConnectionIntent.cs` |
| Offering Binding interface | `src/Koan.ZenGarden.Core/IZenGardenOfferingBinding.cs` |
| Service Discovery Adapter | `src/Koan.Core/Orchestration/Abstractions/IServiceDiscoveryAdapter.cs` |
| Multi-Strategy Discovery Base | `src/Koan.Core/Orchestration/ServiceDiscoveryAdapterBase.cs` |
| ZenGarden Client (registry mirror, SSE, wish) | `src/Koan.ZenGarden/ZenGardenClient.cs` |
| Client interface | `src/Koan.ZenGarden/IZenGardenClient.cs` |
| Circuit Breaker Endpoint Manager | `src/Koan.ZenGarden/GardenAwareEndpointManager.cs` |
| Capability Wish record | `src/Koan.ZenGarden/ZenGardenCapabilityWish.cs` |
| Capability Progress Event | `src/Koan.ZenGarden/ZenGardenCapabilityProgressEvent.cs` |
| Auto-Registrar (ZenGarden module) | `src/Koan.ZenGarden/Initialization/KoanAutoRegistrar.cs` |
| MongoDB Offering Binding | `src/Connectors/Data/Mongo/Initialization/MongoZenGardenOfferingBinding.cs` |
| MongoDB Auto-Registrar (reference-as-intent) | `src/Connectors/Data/Mongo/Initialization/KoanAutoRegistrar.cs` |
| Ollama Offering Binding | `src/Connectors/AI/Ollama/Initialization/OllamaZenGardenOfferingBinding.cs` |
| Weaviate Offering Binding | `src/Connectors/Data/Vector/Weaviate/Initialization/WeaviateZenGardenOfferingBinding.cs` |

### Key Interfaces and Methods

- `ZenGardenConnectionIntent.TryParse(string?, out ZenGardenConnectionIntent?)` — Parses intent URI.
- `ZenGardenConnectionIntent.ForOffering(string, string?, IEnumerable<string>?)` — Constructs intent programmatically.
- `ZenGardenConnectionIntent.ToOfferingSelector()` — Converts to offering selector string.
- `IZenGardenOfferingBinding.AdapterId` / `.Offering` — Maps adapter to offering.
- `ServiceDiscoveryAdapterBase.Discover(DiscoveryContext, CancellationToken)` — Runs fallback chain.
- `ServiceDiscoveryAdapterBase.ValidateServiceHealth(string, DiscoveryContext, CancellationToken)` — Domain-specific probe (abstract).
- `ServiceDiscoveryAdapterBase.BuildDiscoveryCandidates(KoanServiceAttribute, DiscoveryContext)` — Builds priority-ordered candidate list.
- `GardenRegistryInner.upsert(GardenTool, EntryOrigin)` — Write-through with delta generation.
- `GardenRegistryInner.snapshot(ToolQuery)` — Filtered registry snapshot.
- `GardenRegistryInner.deltas_since(u64, ToolQuery)` — Delta replay since cursor.
- `GardenRegistryInner.apply_remote_beacon(ToolsBeacon)` — Merge remote beacon with reconciliation.
- `ZenGardenClient.WishAsync(string, IReadOnlyList<string>, ...)` — Wishful capability provisioning.
- `ZenGardenClient.Subscribe(ZenGardenSubscription, handler, options)` — SSE availability subscription.
- `GardenAwareEndpointManager<TConnection>.GetConnection()` — Returns current connection or null.
- `GardenAwareEndpointManager<TConnection>.ReportFailure()` — Opens circuit proactively.
- `GardenAwareEndpointManager<TConnection>.ReportSuccess()` — Closes circuit after HalfOpen probe.

---

## 6. Publication Notice

This document constitutes a defensive publication under the doctrine of voluntary prior art disclosure. The inventor publishes the technical details of the disclosed system to establish prior art as of the disclosure date (2026-03-24), for the purpose of preventing future patent claims on the described techniques by any party.

The described inventions are hereby dedicated to the public domain for the purpose of prior art. Any person may use, implement, modify, and distribute implementations of the described techniques without restriction or obligation to the inventor.

This publication does not constitute a waiver of any existing rights in the described implementations (source code, trademarks, trade dress), which remain subject to their respective licenses.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) Abstraction gap: scheme-to-pipeline dispatch mechanism unspecified — a competitor could patent the dispatch registry itself. (2) No parallel resolution variant described — only sequential fallback. (3) No negative/disjunctive capability selectors — competitor could patent Boolean capability expressions. (4) Topology file schemas unspecified — competitor could patent a specific schema. (5) Reference-As-Intent insufficiently differentiated from Spring Boot auto-configuration.
**Author revision:** Added "Scheme-to-Pipeline Dispatch Mechanism" subsection with full registry description (startup scanning, immutable map, lazy registration variant). Added "Parallel probing variant" with parallel-within-tier semantics. Added "Negative and disjunctive capability selectors" with formal grammar extension (AND/OR/NOT in CNF). Added complete JSON schemas for both topology files. Added explicit differentiation from Spring Boot auto-config showing the mechanical distinction (offering binding vs. implementation bean). Added Disclosure 9 (Dispatch Registry) and updated Disclosure 11 (Capability Predicates) to cover OR/NOT.

### Pass 2
**Antagonist:** (1) Wishful provisioning lacks timeout, failure transition conditions, and partial-acceptance semantics. (2) Circuit breaker has no configurable thresholds, probe intervals, or exponential backoff. (3) No multi-endpoint load balancing variant — only single-endpoint management described.
**Author revision:** Added "Timeout and failure semantics" to wishful provisioning (5-minute default timeout, PlatformUnreachable, Cancelled reasons, terminal state semantics). Added "Fallback-on-partial mode" (AcceptPartial flag). Added "Configurable circuit breaker parameters" (HalfOpenProbeInterval with exponential backoff, MaxOpenDuration, ConsecutiveFailuresToOpen). Added "Multi-endpoint load balancing variant" with connection pool, per-endpoint circuit breakers, and distribution strategies.

### Pass 3
**Antagonist:** (1) Wish idempotency unaddressed — concurrent duplicate wishes could cause redundant provisioning. (2) DiscoveryCandidate record structure incomplete.
**Author revision:** Added "Idempotency and deduplication" section keyed on (OfferingSelector, sorted(Requested)). Expanded DiscoveryCandidate record to include all five fields (Url, Method, Priority, IsContainerLocal, RawEndpoint).

### Pass 4
**Antagonist:** No further substantive objections. Security/authentication on SSE streams is an operational concern, not a patentable mechanism gap.

### Final Status
CLEARED — Antagonist found no further weaknesses. Safe to publish.
