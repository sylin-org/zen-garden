---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-02
---

# PORT-0001: Universal Port Allocation and Topology Propagation

**Date**: 2026-03-02
**Status**: Accepted
**Applies to**: `moss` (Docker port allocation, topology chirps, service discovery)

## Context

The port allocation system relied on a `well-known-ports.yaml` catalog with
predefined remap ranges for ~13 common ports. When two offerings shared the
same default port (e.g., weaviate and searxng both use 8080) and the port had
no catalog entry, deployment failed with a generic error.

Additionally, `TopologyServiceEntry` — the struct broadcast in chirps — carried
no port information. Remote service discovery used `get_offering_port()` which
returned the manifest default, producing wrong connection URIs whenever a port
had been remapped.

Finally, `is_port_available()` used a TCP bind check, which missed stopped
Docker containers whose ports would conflict on restart.

## Decision

### 1. Universal increment-by-one fallback

When `resolve_port_conflict()` finds no catalog entry for a conflicting port,
it now tries `requested_port + 1` through `+100`, checking both TCP availability
and Docker container occupancy. The well-known-ports catalog remains the first
strategy for catalogued ports (Auto, Remap, Manual, Fail).

### 2. Docker port occupancy scan

`DockerManager::scan_port_occupancy()` lists ALL containers (`all: true`) and
builds a `HashMap<u16, String>` of host port to container name. This is called
once before port allocation and threaded through `check_and_remediate_ports()`
and `resolve_port_conflict()` to prevent conflicts with stopped containers.

### 3. Named port map in OfferingLocation

`OfferingLocation` gained a `port_map: HashMap<String, u16>` field that stores
only remapped ports (port name → actual host port). When all ports match
manifest defaults, the map is empty and omitted from serialization.

### 4. Port propagation in topology chirps

`TopologyServiceEntry` gained a `ports: HashMap<String, u16>` field, populated
from `OfferingLocation.port_map`. Remote service discovery checks this field
before falling back to manifest defaults.

## Properties

| Property | Value |
|----------|-------|
| Catalog precedence | Well-known-ports catalog strategies run first |
| Universal fallback range | `requested_port + 1` to `+100` |
| Occupancy source | All Docker containers (running + stopped) |
| port_map semantics | Only remapped ports; empty = all defaults |
| Chirp backward compat | `#[serde(default, skip_serializing_if)]` — older stones ignore unknown fields |
| Discovery fallback | If chirp has no ports, use manifest default (same as before) |

## Consequences

**Positive:**
- Any port conflict auto-resolves without catalog entries
- Remote `rake find` returns correct URIs for remapped services
- Stopped containers no longer cause silent deployment failures

**Negative:**
- Chirp payload grows slightly when ports are remapped (typically 0-2 entries)
- Occupancy scan adds one Docker API call per deployment

## Files Changed

- `src/moss/src/docker.rs` — `scan_port_occupancy()`, universal fallback, occupancy threading
- `src/common/src/types.rs` — `OfferingLocation.port_map`, `TopologyServiceEntry.ports`
- `src/moss/src/domain/offerings.rs` — `ports_vec_named()` helper
- `src/moss/src/tasks/job_executors.rs` — port_map construction at both deployment sites
- `src/moss/src/domain/service_discovery.rs` — chirp-aware port resolution
