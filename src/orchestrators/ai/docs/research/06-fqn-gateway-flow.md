# FQN Gateway Registration Flow

> Research artifact for ORCH-0013. Documents how the AI orchestrator
> registers as a handler for each offering type, so that garden clients
> can discover and route through it.

---

## The Flow

```
Orchestrator Boot
    │
    ├─ 1. Register mDNS via Koi (/v1/mdns/announce)
    │     name: "ZenGarden orchestrator: AI"
    │     port: 7190 (dashboard)
    │     → receives mdns_id for heartbeating
    │
    ├─ 2. Wait for tended stone (discovery task finds one)
    │
    ├─ 3. Resolve host identity via Koi (/v1/host)
    │     → hostname: "stone-azure-pool.local"
    │     → ip: "192.168.1.166"
    │     (Critical for Docker: Koi returns real LAN IP)
    │
    ├─ 4. For EACH active offering in registry:
    │     PUT /api/v1/garden/gateway/{offering_name}
    │     Body:
    │       fqn: "{offering}::orchestrator"
    │       hostname: from Koi
    │       ip: from Koi
    │       port: offering's proxy port (21434 for ollama, etc.)
    │       handler_for: ["{offering_name}"]
    │       protocol: "http"
    │       uri_template: "http://{host}:{port}"
    │       source: "zen-garden.ai.orchestrator"
    │
    │     Moss does:
    │       → Validates handler_for contains path offering
    │       → Parses FQN (OfferingFqn::parse)
    │       → Creates GardenTool with ServiceInfo
    │       → Resolves URIs from template (IP + hostname variants)
    │       → Stores in tool registry with 60s TTL
    │       → Broadcasts to topology subscribers
    │       → Returns {lease_id: "gw-{offering}", ttl_seconds: 60}
    │
    ├─ 5. Every 30s: Heartbeat
    │     → koi.heartbeat(mdns_id)
    │     → For each offering: PUT (idempotent, refreshes TTL)
    │     → On stone change: deregister old, register new
    │
    └─ 6. On shutdown:
          → For each offering: DELETE /api/v1/garden/gateway/{offering}
          → koi.unregister(mdns_id)
```

## What Each Adapter Must Provide for Gateway Registration

The gateway registration iterates `state.registry.kinds()`. Each adapter
that registers in the `OfferingRegistry` automatically gets gateway
registration. The adapter needs:

1. **`offering_type() -> OfferingKind`** — determines the offering name
   used in the gateway path and `handler_for` field
2. **`OfferingKind::proxy_port()`** — determines the port in the
   `uri_template`. Offerings without a proxy port (cloud) are skipped.
3. **`OfferingKind::as_str()`** — the string used in FQN construction
   and topology matching

## Per-Offering Gateway Registrations

| Offering | Gateway Path | FQN | Proxy Port | URI Template |
|----------|-------------|-----|-----------|--------------|
| ollama | `/gateway/ollama` | `ollama::orchestrator` | 21434 | `http://{host}:21434` |
| comfyui | `/gateway/comfyui` | `comfyui::orchestrator` | 21435 | `http://{host}:21435` |
| speaches | `/gateway/speaches` | `speaches::orchestrator` | 21436 | `http://{host}:21436` |
| openedai-speech | `/gateway/openedai-speech` | `openedai-speech::orchestrator` | 21437 | `http://{host}:21437` |
| infinity | `/gateway/infinity` | `infinity::orchestrator` | 21438 | `http://{host}:21438` |
| libretranslate | `/gateway/libretranslate` | `libretranslate::orchestrator` | 21439 | `http://{host}:21439` |

Cloud providers (OpenAI, Anthropic, etc.) have no proxy port and are
NOT registered as gateways — they are internal routing targets only.

## How Clients Discover the Orchestrator

1. Client calls Moss: `GET /api/v1/garden/services?q=ollama`
2. Moss returns `GardenTool` entries from its registry
3. One entry is the orchestrator's gateway registration:
   - `fqid: "ollama::orchestrator"`
   - `service.uris: ["http://192.168.1.166:21434", "http://stone-azure-pool.local:21434"]`
4. Client connects to the URI and speaks native Ollama protocol
5. The orchestrator routes the request to the best Ollama instance

## Key Invariant

The `handler_for` field MUST contain the same offering name used in
the PUT path. Moss validates this:

```
PUT /api/v1/garden/gateway/ollama
Body: { handler_for: ["ollama"], ... }  ← must match path
```

If they don't match, Moss returns 400 Bad Request.

## Demand-Driven Registration

The AI orchestrator only registers gateways for offerings that have
adapters in the `OfferingRegistry`. An empty registry = no gateway
registrations = invisible to clients. As adapters are added (Block 3+),
gateway registration happens automatically on the next heartbeat cycle.
