---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-15
canonical: true
---

# COMPANION-0015: Adapter Self-Exit on Stale Serial Connection

**Date**: 2026-04-15
**Status**: Accepted
**Depends on**: [COMPANION-0012](COMPANION-0012-device-bus.md) (bus model and adapter lifecycle)

## Context

The device bus (COMPANION-0012) decides device lifecycle via the USB enumerator. A port disappearing from `serialport::available_ports()` fires a `Detached` event; the bus reaps the owning adapter and a subsequent reattach triggers the identity-probe + claim dance.

This works when the kernel reports the detach. It does not work when a USB serial device is unplugged and replugged quickly enough that the kernel re-assigns the same node name (`/dev/ttyUSB0`) without the enumerator's scan interval ever observing the gap. Symptoms seen on stone-silent-cascade (2026-04-15):

- Device claimed at `T+0`.
- At `T+49s`, every `D` frame times out.
- No `device detached` signal from the enumerator — `/dev/ttyUSB0` is still present.
- `lsof` confirms the firefly process holds the original fd.
- A fresh fd opened by a direct probe gets the full `I` response: the firmware is alive and correct.

The held fd is a stale kernel tty. Reads return `Ok(0)` or time out; writes are silently discarded. The adapter loop keeps ingesting events and issuing commands into the void.

`FireflyConnection::with_device` explicitly does *not* auto-disconnect on a single timeout. That policy is correct (COMPANION-0012: a slow frame is not an unplug; early revisions caused false self-exits). What the policy misses is the difference between *occasional* timeout and *sustained* silence: the latter is indistinguishable from a dead fd and cannot be resolved by continuing to write into it.

## Decision

The adapter self-exits when its serial connection has produced no successful I/O for a bounded window despite sustained attempts. The bus treats this as `AdapterExitReason::SelfExit` and re-runs the full identity dance (open, probe, claim) against the cached port descriptor, which produces a fresh fd.

### Failure tracking lives in `FireflyConnection`

`FireflyConnection` already wraps every command in `with_device`. It is the single choke point through which every adapter talks to the device, so it is the right place to observe health.

```rust
pub struct FireflyConnection {
    serial: Mutex<Option<FireflySerial>>,
    device_type: Mutex<FireflyDeviceType>,
    preferred_port: Option<String>,

    // New — connection health
    consecutive_failures: AtomicU32,
    last_success: Mutex<Instant>,
}
```

`with_device` updates these on each call:

- `Ok(_)` → `consecutive_failures = 0`, `last_success = now()`.
- `Err(_)` → `consecutive_failures += 1`.

### `is_lost()` is the single health predicate

```rust
const LOST_FAILURE_THRESHOLD: u32 = 5;
const LOST_DURATION: Duration = Duration::from_secs(15);

pub fn is_lost(&self) -> bool {
    self.consecutive_failures.load(Relaxed) >= LOST_FAILURE_THRESHOLD
        && self.last_success.lock().ok()
            .map(|t| t.elapsed() >= LOST_DURATION)
            .unwrap_or(false)
}
```

Both conditions must hold. The failure-count gate keeps a single slow frame from tripping the exit. The elapsed-time gate keeps an adapter that has had at least *some* successful I/O in the last 15s from exiting just because of a burst of timeouts.

### Adapters check in their main loop

Every adapter's `run` already drives a `tokio::select!` with `shutdown.cancelled()` and `events.recv()`. A third branch — a periodic health tick — checks `connection.is_lost()` and breaks.

```rust
let mut health = tokio::time::interval(Duration::from_secs(5));
health.tick().await; // consume immediate tick

loop {
    tokio::select! {
        _ = shutdown.cancelled() => break,
        _ = health.tick() => {
            if connection.is_lost() {
                tracing::warn!(
                    port = %connection.port_name(),
                    "connection appears lost — self-exiting for re-identification"
                );
                break;
            }
        }
        maybe = events.recv() => match maybe {
            Some(event) => handle_event(&event, ...).await,
            None => break,
        },
    }
}
```

Returning from `run` closes the task; the supervisor publishes `AdapterExited { reason: SelfExit }`; `DeviceBus::handle_adapter_exit` (runtime.rs:411) already dispatches this — it calls `reap_id` and then `handle_attach` on the cached port descriptor. `open_usb_serial` gets a fresh fd.

## Consequences

### Positive

- Silent-replug scenarios self-heal within ≤ 20s (5s health tick + 15s loss window) with no user action.
- Policy stays in the connection layer; adapters get a single predicate to check.
- The bus design is unchanged — this is a completion of the existing `SelfExit` path, not a new concept.
- No false exits from single laggy frames: the dual-gate predicate only fires when *both* thresholds are crossed.

### Negative

- Up to ≈20s of dark-display time during a silent replug before the adapter re-identifies. Acceptable; the firmware side also takes seconds to reboot, and an OLED device with no live feed is visibly degraded anyway.
- Adapters that go genuinely idle (no commands issued for 15s) would not trip the predicate by accident because no *failures* accumulate while the adapter is idle. A newly-replugged dead adapter only trips once it actually attempts I/O.

### Risks

- An extremely-slow firmware that legitimately takes >15s per frame *and* fails ≥5 in a row would be declared lost. This would cause a false re-identification, which is harmless (fresh fd, same firmware, re-claim).

## Alternatives considered

- **Enumerator probes every tick**: periodically re-run the identity protocol on owned ports to detect stale fd. Rejected — opens a second reader on a port the adapter is actively using, and identity probes are expensive on ESP8266 (full reset + boot delay).
- **Rely on `Ok(0)` detection in the serial read loop**: `serial.rs` already loops on `Ok(0)` up to `MAX_ZERO_READS`. The actual symptom here is a sequence of timeouts, not `Ok(0)`, so this wouldn't trip. Adding aggressive "any timeout = disconnect" brings back the false-exit-on-slow-frame bug explicitly noted in the code.
- **Stable-identity enumeration** (USB sysfs path or device_id-based tracking): bigger refactor, doesn't solve the stale-fd problem — the adapter would still need to exit-and-rejoin because the old fd can't be revived.

## References

- [COMPANION-0012](COMPANION-0012-device-bus.md) — the bus model this completes
- `src/companion-sdk/src/bus/runtime.rs` — `handle_adapter_exit` at line 411
- `src/firefly/src/serial.rs` — `with_device` at line 697
