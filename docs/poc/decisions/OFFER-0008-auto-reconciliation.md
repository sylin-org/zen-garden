---
audience: developer
doc_type: decision
status: accepted
---

# OFFER-0008: Auto-Reconciliation of Missing Managed Containers

**Date**: 2026-04-03
**Status**: Accepted

---

## Context

After a Docker wipe (container prune, full reset, or engine reinstall), Moss retains offering registry entries for containers that no longer exist. The health monitor polls every 30 seconds and logs a WARN for each missing container, producing sustained log spam with no self-recovery:

```
WARN Failed to get offering status, marking as offline offering=kokoro
  Caused by: Docker responded with status code 404: No such container: zen-offering-kokoro
```

Moss prepares managed containers so persistent data (model weights, databases, configs) lives on host bind mounts, not inside containers. Containers are ephemeral shells around a manifest spec + config patches. Recreating them is idempotent and safe.

The existing `rebuild_missing_container()` function (called by `service_lifecycle::start()`) has a critical port-loss bug: it re-derives the container spec from the manifest (ignoring stored port mappings) and discards the resolved ports returned by `install_service()`. This means any port remappings from the original install are silently lost on rebuild.

---

## Decision

### 1. Auto-reconciliation in the health monitor

The health monitor detects missing containers for **managed offerings only** (not adopted or borrowed) and queues background reconciliation jobs with bounded concurrency.

**Detection**: When `get_service_status()` returns an error for a managed offering, the health monitor calls `zen_container_exists()` to distinguish "container stopped" from "container missing." Only the latter triggers reconciliation.

**State gate**: The offering is marked `Installing` before reconciliation begins. The health monitor already skips offerings in `Installing` state, preventing double-trigger.

**Desired state preservation**: If the offering was `Stopped` before the container disappeared, reconciliation rebuilds the container but does not start it. The pre-reconciliation status is captured and used as the target state on completion.

### 2. Port-preserving reconciliation

A new `reconcile_offering()` function replaces `rebuild_missing_container()`:

1. Reads the stored offering's `location.port`, `location.port_map`, and `location.agnostic_port`
2. Builds the container spec from manifest + config patches (existing `build_spec_from_manifest`)
3. **Replaces spec ports with stored resolved ports** (not manifest defaults)
4. Passes the spec through `install_service()` (which still runs port scanning)
5. If stored ports are free, they bind as-is (scanning finds no conflict)
6. If a stored port is occupied, scanning remaps it (service still comes up)
7. Compares returned actual ports against stored ports
8. **Updates the offering registry only when ports actually changed**

Priority ordering: service running > perfect port preservation. A port remap is acceptable; a service that doesn't come back is not.

### 3. Bounded concurrency and backoff

After a Docker wipe, all managed offerings are missing simultaneously. Reconciliation uses a `tokio::sync::Semaphore` (permits: 2) to limit concurrent rebuilds. Image pulls are I/O-bound and benefit from some overlap, but too many concurrent container creations cause port scanning contention.

Per-offering exponential backoff (30s, 60s, 120s, 240s, 480s) with a hard cap at 5 attempts. After exhaustion, the offering is marked `Degraded` and skipped until operator intervention or daemon restart (which resets the tracker).

Backoff state is tracked in-memory (`HashMap<String, ReconciliationTracker>`) — not persisted. A daemon restart is a natural retry-counter reset.

### 4. In-flight guard

A `HashSet<String>` of currently-reconciling offering names prevents the existing TOPO-0002 remediation loop and the new auto-reconciliation from targeting the same offering concurrently.

### 5. Pre-existing port-loss bugs fixed

The same port-loss pattern exists in three places:

| Call site | Bug | Fix |
|-----------|-----|-----|
| `rebuild_missing_container()` | Discards resolved ports from `install_service()` | Replaced by `reconcile_offering()` |
| `service_lifecycle::start()` | Calls `rebuild_missing_container()`, never updates port | Uses `reconcile_offering()`, writes back ports |
| `await_container_removed()` | Treats all Docker errors as 404 | Discriminates 404 from transient errors |

### 6. Volume path validation

Config patches accept arbitrary host paths for bind mounts. These are persisted and replayed on every reconciliation. A path-prefix validation check is added to `patch_config_v1` to reject host paths outside `data_dir()` and `shared_data_dir()`, and to reject path traversal sequences.

---

## Consequences

**Positive:**
- Moss self-heals after Docker wipe without operator intervention
- Log spam eliminated (log once on detection, once on outcome)
- Port mappings preserved across container rebuilds
- Pre-existing port-loss bugs in `start()` path fixed
- Config patch volume paths validated against path traversal

**Negative:**
- Additional Docker API call (`zen_container_exists`) per missing offering per health cycle
- Semaphore and backoff tracker add in-memory state to the health monitor
- Offerings that were intentionally removed via Docker (not through Moss) will be automatically resurrected — operators should use `rake remove` instead

**Neutral:**
- `rebuild_missing_container()` is replaced, not extended. The single caller (`start()`) is updated.
- The TOPO-0002 remediation loop in the health monitor is unchanged but now shares the in-flight guard.

---

## Recovery from Degraded

After 5 consecutive failed reconciliation attempts, the offering is marked `Degraded` and auto-reconciliation stops. Three recovery paths exist:

1. **`rake start <offering>`** — The `service_lifecycle::start()` path does not check the backoff tracker. It calls `reconcile_offering()` directly. If the root cause is fixed (image available, Docker healthy), this succeeds and sets the offering to `Running`.

2. **`rake remove <offering>` + `rake plant <offering>`** — Full removal clears the registry entry and the in-memory backoff tracker is pruned. Re-planting starts fresh.

3. **Daemon restart** — The backoff tracker is in-memory only. A Moss restart resets all trackers to zero attempts. A `Degraded` offering will get 5 new attempts automatically.

---

## Race Prevention

The health monitor's in-flight guard (`HashSet<String>`) prevents double-reconciliation within the health monitor. The `service_lifecycle::start()` path checks if the offering is in `Installing` status (set by the health monitor before spawning) and returns an error asking the caller to retry, preventing concurrent `reconcile_offering()` calls for the same offering.

---

## Event Emission

Successful auto-reconciliation emits `OfferingEvent::started()` and uses `auto_chirp=true` on the final status update. This ensures SSE consumers, companion notifications, and orchestrator routing tables learn about the reconciled offering immediately.

---

## References

- OFFER-0002: Container namespace collision (zen-offering-* naming)
- OFFER-0005: Offering modes (managed/adopted/borrowed distinction)
- PORT-0001: Named port map (port_map field semantics)
- TOPO-0002: Shared topology directory (mount remediation in health monitor)
