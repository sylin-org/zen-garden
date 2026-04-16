---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-16
canonical: true
---

# COMPANION-0018: Three-Domain Device Architecture — Law of Instances

**Date**: 2026-04-16
**Status**: Accepted
**Supersedes**: [COMPANION-0012](COMPANION-0012-device-bus.md), [COMPANION-0015](COMPANION-0015-adapter-self-exit-on-stale-connection.md), and all device-bus iterations including the never-shipped COMPANION-0017 event-bus design.

## Context

Every iteration of the companion device bus — the original polling bus (0012), the adapter self-exit heuristic (0015), the event-driven rewrite (0017) — accreted fixes onto a monolithic "bus" object that entangled four unrelated concerns:

1. OS device discovery (udev/netlink)
2. Identity determination (does this device speak our protocol?)
3. Adapter lifecycle (spawn, dispose)
4. Byte-level serial I/O

The result was spot-fixes for PID reuse (0016), stale fds (0015), silent replug (this session), and probe-race retry (this session) — each patch addressed one symptom of the same architectural mistake.

## Decision

Three bounded contexts. Each answers one question. Each owns its state, its vocabulary, and its lifecycle.

### Domain 1 — `usb_devices` (SDK)

> *What USB serial devices does the OS report, and what state is each in?*

- **`UsbSerialDevice`** — entity. Owns its fd, its reader task, its state machine. Exposes `send(bytes)`, `lines()` (broadcast), `state_changes()` (watch), `dispose()`. Stable identity via USB serial number, sysfs devpath, or port path.
- **`UsbRegistry`** — aggregate. Owns `HashMap<DeviceId, Arc<UsbSerialDevice>>`. Subscribes to a `Monitor`. On OS add, opens the device asynchronously and emits `RegistryEvent::Appeared(Arc<UsbSerialDevice>)`. On OS remove, calls `device.dispose()` and emits `Disappeared(Arc<UsbSerialDevice>)`.
- **`Monitor`** — trait. `UdevMonitor` (Linux), `PollMonitor` (portable).

This domain knows nothing about firefly, identity probes, or adapters. It speaks only "USB serial device."

### Domain 2 — `firefly` (firefly crate)

> *Is this device a firefly, and if so, which kind?*

- **`Firefly`** — entity. Holds `Arc<UsbSerialDevice>` permanently (public field for ergonomic reach-through — see Law of Instances below). Carries `Identity { family, variant, version, capabilities, firmware_device_id }`. Exposes firefly-protocol vocabulary: `oled_health`, `oled_v2_dashboard`, `matrix_fill`, `tdisplay_json_push`, etc. — all `async`, each translates to bytes via `self.device.send(...)` and (when synchronous) awaits a response line via `self.device.lines()`.
- **`FireflyProbe`** — pure function. Given an `Arc<UsbSerialDevice>` in `Evaluating`, writes `I\n`, reads the next valid identity line, parses. Returns `Arc<Firefly>` or a rejection reason. The *only* code that consumes raw `lines()`/`send()` at the firefly boundary.
- **`FireflyOrchestrator`** — aggregate. Subscribes to `UsbRegistry`. On `Appeared(device)`: `device.begin_evaluation()`; probe; on success `device.accept(kind)` + spawn adapter; on failure `device.reject(reason)`. On `Disappeared(device)`: the device transitions to `Disposed`; adapter observes via `device.state_changes()` and exits; no orchestrator action needed.

### Domain 3 — `adapters` (existing SDK + firefly)

> *Drive this device.*

- The SDK's `Adapters` supervisor stays as-is (spawn_external, reap_id, exit events).
- Firefly adapters (`matrix`, `oled_v1`, `oled_v2`, `tdisplay`) are constructed with `Arc<Firefly>`. Their run loop watches shutdown + Pulse events + `firefly.device.state_changes()`. A `Disposed` transition exits the loop.

## The Law of Instances

1. **Pass instances, never ids.** If a caller needs to act on X, it holds `Arc<X>`, not a key. There is no lookup by id anywhere in the hot path; lookups happen once at creation, and the reference is the thing.
2. **Each layer exposes its own vocabulary.** `device.send(bytes)` is USB-domain vocabulary. `firefly.oled_health(state)` is firefly-domain vocabulary. `adapter.run(...)` is adapter-domain vocabulary. A layer's method must operate at that layer's abstraction level.
3. **Reach-through is fine when the vocabulary fits.** `firefly.device.send(bytes)` is legitimate: the caller is intentionally invoking USB-level vocabulary on this firefly's device. `firefly.device.fd.write(...)` would be a leak: `fd` is implementation, not vocabulary.
4. **References are permanent.** Once `Firefly` holds its `Arc<UsbSerialDevice>`, it holds it for the firefly's lifetime. Once an adapter holds its `Arc<Firefly>`, same. Teardown happens via state transitions propagating through subscribers, not by forced reference nulling.

## Disposal flow

```
OS remove event
 → UsbRegistry.remove(id)
   → Arc<UsbSerialDevice>::dispose()
     → state transitions Accepted | Rejected | * → Disposed
     → state_changes watcher publishes
     → fd closed internally
 → UsbRegistry drops its map entry (releases its Arc)

Subscribers observing state_changes:
  Reader task in UsbSerialDevice          — observes, exits
  Adapter's select! loop                  — observes, breaks, returns
     ↓
  Adapter task exits; drops Arc<Firefly>
     ↓
  Firefly's last ref released; Drop runs
     ↓
  Arc<UsbSerialDevice> last ref released
     ↓
  UsbSerialDevice::Drop — reader JoinHandle aborted
```

`dispose()` is idempotent. Any holder can call it (the reader task self-disposes on sustained EOF; the registry disposes on OS remove). First caller wins; state transition publishes once.

## Consequences

### Positive
- Kernel-speed event detection (udev netlink, single-digit ms).
- Rejection state persists; repeat udev ADDs for a known-rejected device produce no orchestrator work.
- Adapter code contains no reference to `serialport`, `udev`, or OS paths. The type `UsbSerialDevice` does not appear in adapter source.
- State transitions are explicit methods on the entity — `begin_evaluation`, `accept`, `reject`, `dispose` — each with documented invariants.
- `FireflyProbe` is the one place that knows the firefly identity protocol's wire format.
- Replug of the same physical device resolves to the same `DeviceId` (via USB serial number), so the registry sees "was Disposed, now Appeared again" — no stale-state confusion.

### Negative
- More types to implement than any single-file "bus" approach. Accepted: each type answers one question; none are incidental abstractions.
- Async boundary pushed into Firefly's command methods. Adapter handlers become `async`. Accepted: matches the nature of the work (await-a-response-line is intrinsically async).

### Migrations
Clean-slate rebuild. No compat layer, no shims. Old bus, old FireflyConnection, old FireflySerial, identity protocol, predicate DSL — all gone. The ADRs for those are superseded in full by this one.

## References

- `src/companion-sdk/src/usb_devices/` — domain 1
- `src/firefly/src/firefly.rs`, `probe.rs`, `orchestrator.rs` — domain 2
- `src/firefly/src/adapters/`, `src/companion-sdk/src/adapters/` — domain 3
