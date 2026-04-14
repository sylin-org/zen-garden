---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0012: Device Bus — Plug-and-Play Hardware Discovery for Companions

**Date**: 2026-04-14
**Status**: Accepted — **implementation pending**
**Depends on**: [COMPANION-0001](COMPANION-0001-companion-integration-epic.md), [COMPANION-0007](COMPANION-0007-adapters.md)
**Pairs with**: [FIREFLY-0004](FIREFLY-0004-device-protocol.md) (consumes the bus)

## Context

COMPANION-0001 closed with five working adapters across cricket and firefly. Within a week of validation on real stones, the firefly adapters surfaced a structural bug:

- Three factories (`OledV1Factory`, `OledV2Factory`, `TDisplayFactory`) each scan USB on every 5-second discovery tick.
- Each factory probes every CH340-VID port by opening the serial port, sending `I`, parsing the response, and closing.
- Once a factory spawns an adapter, the adapter holds the port open. The next tick's probe fails with "access denied."
- Per-factory `claimed` caches were added as a hotfix to stop reap/respawn churn.

The hotfix works but masks the real architectural gap: **discovery is per-adapter when it should be system-wide.** A single physical device gets opened up to three times per tick, each open triggering an ESP32 auto-reset, racing the firmware boot against our 2-second stabilization window. The T-Display device on stone-coral-prairie consistently times out the first probe — it boots after the OLED v1 factory's probe expires, the v2 factory's probe expires, and only the third (TDisplay) probe might succeed if timing aligns. Often it doesn't.

The pattern that solves this — and many adjacent problems — is the OS plug-and-play model: a central bus owns physical-resource enumeration, adapters declare interest in resource classes, the bus probes each new device exactly once and offers it to interested adapters by descriptor.

## Decision

Introduce a `DeviceBus` in `garden-companion-sdk` as a peer of `Adapters`. The bus owns physical-resource discovery; adapters declare interest by data; ownership is explicit and event-driven.

### The shape

```
ResourceClass            ←  what kind of physical thing (USB serial, BT, network)
   │
   ▼
DeviceBus enumerates  →   emits Attached { device }
                          emits Detached { device_handle }
   │
   ▼
IdentityProtocol         ←  registered per ecosystem (firefly, …);
                            given an open port, returns Option<Identification>
   │
   ▼
Identification           ←  structured descriptor (JSON-shaped) parsed from
                            the device. Carries device_id (provenance proof),
                            family / variant / version / capabilities.
   │
   ▼
AdapterRegistration      ←  { name, interest: predicate, build: fn }
                            pure data + builder. No probe code.
   │
   ▼
DeviceBus                →  evaluates predicates against descriptor;
                            picks highest-specificity match;
                            invokes build(open_port, descriptor)
                            spawns adapter under Adapters supervisor
```

### Resource classes (Phase 1)

```rust
pub enum ResourceClass {
    UsbSerial { vid: Option<u16>, pid: Option<u16> },
    // Future: Bluetooth, NetworkMdns, Gpio, …
}
```

The bus owns one enumerator per `ResourceClass`. `UsbSerial` enumerator wraps `serialport::available_ports()` + a per-port lifecycle tracker that emits `Attached` / `Detached` deltas across ticks.

### Identity protocols

An `IdentityProtocol` is the bridge between "an opened device" and "a parsed descriptor."

```rust
pub trait IdentityProtocol: Send + Sync {
    fn ecosystem(&self) -> &'static str;  // "firefly", "future-pico", …
    fn identify(&self, port: &mut OpenedDevice) -> Option<Identification>;
}
```

The firefly identity protocol (defined in FIREFLY-0004) waits ≤3 s for an unsolicited HELLO frame; if none arrives, sends `I` once; parses the JSON response into an `Identification`.

The bus tries registered identity protocols sequentially. First one to return `Some` wins; the descriptor flows downstream. All `None` → device classified as foreign, emit `unclaimed.foreign` telemetry, drop into per-port backoff.

### The descriptor shape

```rust
pub struct Identification {
    pub ecosystem: &'static str,           // from the protocol that parsed it
    pub device_id: String,                 // provenance proof — see FIREFLY-0004
    pub hardware_id: Option<String>,       // forensic
    pub fields: serde_json::Value,         // the full parsed descriptor
}
```

Adapter interest predicates evaluate against `fields`. Examples:

```rust
Predicate::all_of([
    Pred::eq("family", "firefly"),
    Pred::eq("variant", "oled"),
    Pred::version_caret("version", "0.2.0"),  // semver ^0.2.0
    Pred::eq("processor", "esp8266"),
    Pred::has_capability("dashboard"),
])
```

### Sequential claim, specificity-ordered

When a device's descriptor matches multiple registrations, **specificity wins** — the registration with more matched predicates ranks first. Ties broken by registration order. This eliminates the "OLED v1's loose `contains("firefly-oled")` accidentally claims an OLED v2" failure mode by construction: v2's predicate matches more fields, scores higher, claims first.

The bus invokes claim candidates **sequentially, never in parallel**. Reason: the open port is a single resource. The first candidate that returns `Claim` receives the open port handle and spawns; the rest are not contacted. This also stops compounding ESP32 resets — the port opens once for the identity probe, stays open through claim, hands off to the adapter.

### Identity caching

`device_id` is the cache key (FIREFLY-0004 specifies its provenance). Cache shape:

```rust
HashMap<String /* device_id */, String /* adapter_class */>
```

On `Attached`: look up `device_id` in cache. If present, route the descriptor to the cached adapter class FIRST (skip the dance). If that class still claims, fast-path. If it returns `Pass`, invalidate the cache entry and run the full sequential dance.

The cache is a **hint, not a bypass**. It survives daemon restarts (persisted to `{state_dir}/device-bus-cache.json`). It does not skip predicate evaluation — that runs every time so a "same device, different firmware" scenario (NVS preserved, firmware changed) is detected and re-routed.

### Per-port backoff

Each port has independent failure-counting state:

```rust
PortState {
    last_attempt: Instant,
    consecutive_failures: u32,
    next_eligible: Instant,
}
```

Backoff schedule on probe failure: 5 s → 30 s → 2 min → 5 min (capped). Reset on success or on `Detached`. Prevents busy-loop on broken hardware, dying flash, wrong firmware. The stone's logs stay readable.

### Unclaimed telemetry

Three failure modes, three event kinds emitted to `Pulse`:

| Event kind | Trigger | Payload |
|---|---|---|
| `core.companion.device.unprovisioned` | Identity protocol parsed descriptor but `device_id` absent or malformed | `{ port, ecosystem, raw_descriptor }` |
| `core.companion.device.unclaimed` | Descriptor parsed and well-formed but no adapter predicate matched | `{ port, descriptor }` |
| `core.companion.device.foreign` | All identity protocols returned `None` | `{ port, vid, pid, product }` |

Operators see explicit signals via `garden-rake companions devices` — no more "is the daemon alive?" guessing when a freshly plugged device sits silent.

### Supervisor relaxation

The 2-second grace window in `Adapters::with_grace_window` is a polling-era hack: "don't reap if maybe the next tick will see the device again." With the bus, `Detached` is explicit and instant. Bus-spawned adapters are reaped immediately on `Detached`, no grace window. Singleton adapters (cricket-style) keep the grace window for their existing semantics.

Implementation: `Adapters` gains a `spawn_with_lifecycle` API where the caller (the bus) can signal "this adapter's lifecycle is externally tracked — skip the grace check."

### MockBus for tests

`testing::MockBus` pairs with `MockTransport` and `RecordingAdapter` to give integration tests full control over discovery flow:

```rust
let bus = MockBus::new();
let harness = TestHarness::new("scenario")
    .with_bus(bus.clone())
    .with_adapter_registration(MyAdapterReg)
    .start().await;

bus.attach(FakeDevice::new("01938abc-…", json!({…})));
tokio::time::sleep(Duration::from_millis(100)).await;
assert_eq!(harness.adapters().active_count(), 1);

bus.detach("01938abc-…");
// adapter exits within bounded join window
```

Closes a real coverage gap from Book IX (which had `MockTransport` but couldn't simulate physical hardware events).

## Implementation plan

**Chapter 1** — Bus core + UsbSerial enumerator + IdentityProtocol trait + Identification type. No adapter integration yet; lib-only with unit tests for enumerator delta computation, sequential identity protocol invocation, descriptor parsing.

**Chapter 2** — AdapterRegistration + predicate engine + claim mechanics. Specificity scoring, sequential claim, open-port handoff. MockBus + integration tests covering attach → identify → claim → spawn → detach → reap.

**Chapter 3** — Cache (in-memory + persisted), per-port backoff, unclaimed telemetry. MockBus tests for cache hit-and-miss paths, backoff schedule, telemetry shape.

**Chapter 4** — Firefly migration. Replace the four firefly factories with adapter registrations. Delete `claimed` caches (the hotfix scaffolding). MockBus integration test exercising the four registrations + a sample descriptor for each variant. Live hardware validation gate before close.

Ships green to `dev` per chapter. Cricket is unaffected at every chapter boundary — it remains a singleton-style factory under the existing `Adapters` API.

## Out of scope (deferred)

| Item | Deferred |
|---|---|
| Resource classes beyond UsbSerial (Bluetooth, mDNS, GPIO) | Add when first consumer needs one — bus contract supports it |
| Garden-wide replication of the device-id cache | When multi-stone hot-move becomes common; today operator handles it manually |
| Cryptographic device-id signing (HMAC of GUID) | Not needed for local-environment trust model |
| `--strict-roster` enforcement (descriptor accepted only if device_id is in known roster) | Phase 2 of FIREFLY-0004's roster sync work |
| Hot-reload of adapter registrations | Standard "restart the daemon" suffices |

## References

- [COMPANION-0001 §Postmortem](COMPANION-0001-companion-integration-epic.md#postmortem) — flagged the per-factory probe issue as a post-epic refactor opportunity
- [COMPANION-0007](COMPANION-0007-adapters.md) — Adapters supervisor that the bus integrates with
- [FIREFLY-0004](FIREFLY-0004-device-protocol.md) — first identity protocol consumer; defines the descriptor schema
- [Linux udev](https://www.man7.org/linux/man-pages/man7/udev.7.html) — pattern source for the bus model
