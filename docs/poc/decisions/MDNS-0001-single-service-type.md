---
status: Accepted
date: 2026-01-15
---

# MDNS-0001: Single Service Type for All Stones

## Status

**Accepted** - Implemented in Moss mDNS announcement

## Context

mDNS (Multicast DNS) allows services to announce themselves on local networks. Common pattern: each service type gets own mDNS identifier (e.g., `_http._tcp`, `_ssh._tcp`, `_printer._tcp`).

**Question:** Should Zen Garden announce each service under its own mDNS type?

**Options:**
1. **Service-specific types:** `_mongodb._tcp.local.`, `_redis._tcp.local.`, `_postgresql._tcp.local.`
2. **Single Stone type:** `_koan-stone._tcp.local.` with service metadata in TXT records

**Constraints:**
- Discovery must be fast (<1 second)
- Stones may offer multiple services (MongoDB + Redis on same device)
- Backward compatibility required (changing service type breaks discovery)

**Source:** [discovery.md § mDNS Service Types](../specs/discovery.md#mdns-service-types), [connection-strings.md § mDNS Service Announcement](../reference/connection-strings.md#mdns-service-announcement)

## Decision

Use **single service type** `_koan-stone._tcp.local.` for ALL Stones, differentiate services via TXT records.

**Announcement structure:**
```
Service Type: _koan-stone._tcp.local.
Instance Name: stone-01
TXT Records:
  offering=mongodb
  instance=            (empty for default instance)
  port=27017
  protocols=mongodb    (comma-separated list of supported protocols)
  protocol_default=mongodb
  admission=communal   (communal or dedicated)
  version=0.2.202601181256
  health=healthy
  fingerprint=abc123   (if Pond active)
```

**Multiple services on same Stone:**
```
Service Type: _koan-stone._tcp.local.
Instance Name: stone-01
TXT Records:
  offerings=mongodb,redis    # Comma-separated list
  mongodb_port=27017
  mongodb_protocols=mongodb
  mongodb_admission=communal
  redis_port=6379
  redis_protocols=redis
  redis_admission=communal
  mongodb_health=healthy
  redis_health=degraded
```

**Multi-instance offerings:**
```
Service Type: _koan-stone._tcp.local.
Instance Name: stone-01-mongodb-staging
TXT Records:
  offering=mongodb
  instance=staging
  port=27018
  protocols=mongodb
  admission=dedicated
  health=healthy
```

## Consequences

### Positive

✅ **Single query discovers all:** Rake queries `_koan-stone._tcp.local.` once, gets entire garden topology  
✅ **Efficient discovery:** One mDNS query vs N queries (one per service type)  
✅ **Future-proof:** New services added without protocol changes  
✅ **Consistent naming:** All Stones identifiable as "Zen Garden infrastructure"  
✅ **Backward compatible:** Service type never changes (stability guarantee)

### Negative

❌ **Non-standard:** Most mDNS services use service-specific types (`_mongodb._tcp`)  
❌ **TXT record parsing:** Clients must parse TXT records (not just service type)  
❌ **Discovery tools:** Generic mDNS browsers (avahi-browse) show "koan-stone" not "mongodb"  
❌ **Multiple services complexity:** TXT record format more complex

### Risks

**Risk:** TXT record size limits (typically 255 bytes per record)  
**Mitigation:** Stone with 20+ services splits into multiple TXT records (DNS-SD supports multi-record TXT)

**Risk:** Clients filter by service type, miss Zen Garden services  
**Mitigation:** Documentation clearly states to query `_koan-stone._tcp.local.`, not individual service types

## Alternatives Considered

### Alternative 1: Service-Specific Types

**Approach:** `_mongodb._tcp.local.`, `_redis._tcp.local.`, etc.

**Why not:**
- **Discovery performance:** N queries to find N service types (slow)
- **Service addition:** Every new service requires new mDNS type (protocol change)
- **Stone identity lost:** No way to know "mongodb on stone-01" vs "mongodb on stone-02" without additional queries

**Example problem:**
```bash
# Operator wants to see ALL Stones
# Must query EVERY service type individually:
avahi-browse _mongodb._tcp.local.
avahi-browse _redis._tcp.local.
avahi-browse _postgresql._tcp.local.
avahi-browse _rabbitmq._tcp.local.
# ... 20+ queries for comprehensive discovery
```

### Alternative 2: Hybrid Approach (Both Types)

**Approach:** Announce both `_koan-stone._tcp.local.` AND `_mongodb._tcp.local.`

**Why not:**
- **Broadcast traffic:** Doubles announcement overhead (every service announced twice)
- **Consistency challenges:** Two sources of truth, risk of divergence
- **No benefit:** Clients querying `_mongodb._tcp` don't know about Zen Garden semantics (connection strings, offerings)

### Alternative 3: Dynamic Service Types

**Approach:** Stone announces `_koan-stone-mongodb._tcp.local.` (service-specific Stone type)

**Why not:**
- **Type explosion:** 20 services = 20 different types to maintain
- **Discovery complexity:** Still requires N queries (doesn't solve performance problem)
- **Backward compatibility nightmare:** Changing service changes type (breaks discovery)

## Implementation Notes

### mDNS Announcement (Moss)

```rust
// Moss announces on startup
let service_type = "_koan-stone._tcp.local.";
let instance_name = config.stone_name; // "stone-01"
let port = 7185; // Moss HTTP API

let txt_records = vec![
    format!("offering={}", offering_name), // "mongodb"
    format!("port={}", service_port),      // "27017"
    format!("version={}", MOSS_VERSION),   // "0.2.202601181256"
    format!("health={}", health_status),   // "healthy"
];

mdns::announce(service_type, instance_name, port, txt_records)?;
```

### Discovery (Rake)

```rust
// Rake discovers all Stones
let stones = mdns::browse("_koan-stone._tcp.local.", timeout)?;

for stone in stones {
    // Parse TXT records
    let offering = stone.txt_get("offering")?; // "mongodb"
    let port = stone.txt_get("port")?.parse::<u16>()?; // 27017
    let health = stone.txt_get("health")?; // "healthy"
    
    // Build endpoint: mongodb://stone-01:27017
    let endpoint = format!("{}://{}:{}", offering, stone.hostname, port);
}
```

### TXT Record Format (Stable)

**Core fields (never removed):**
- `offering=<type>` (mongodb, redis, postgresql) - singular for backward compat
- `instance=<name>` (staging, prod, empty) - multi-instance support
- `port=<number>` (deprecated v1.0, but kept) - primary service port
- `version=<semver>` (0.2.202601181256) - Moss version
- `health=<status>` (healthy, degraded, unavailable) - current health

**Additive fields (v1.0+):**
- `offerings=<csv>` (mongodb,redis) - multiple services (replaces singular)
- `protocols=<csv>` (mongodb,storage) - supported wire protocols
- `protocol_default=<name>` (mongodb) - default protocol for offering
- `admission=<policy>` (communal,dedicated) - access policy
- `endpoints=<uri-list>` (tcp:27017,http:8080) - multiple protocols
- `fingerprint=<sha256>` (abc123) - Pond certificate hash
- `capabilities=<flags>` (snapshot,backup) - feature flags

**Protocol vs Offering distinction:**
- `protocols` = wire formats (s3, mongodb, redis, storage)
- `offering` = software name (minio, mongodb, redis)
- Resolution can be by protocol (`zen-garden:s3//`) or offering (`zen-garden:minio`)

**Multiple services example:**
```
offerings=mongodb,redis
mongodb_port=27017
mongodb_protocols=mongodb
mongodb_admission=communal
redis_port=6379
redis_protocols=redis
redis_admission=communal
mongodb_health=healthy
redis_health=degraded
```

## References

- **Discovery spec:** [Discovery Protocol](../specs/discovery.md)
- **Discovery transport:** [Discovery Transport](../specs/discovery-transport.md)
- **Components:** [Components](../reference/components.md)

## Standards Compliance

**DNS-SD (RFC 6763):** ✅ Compliant (service instance name, service type, TXT records)  
**mDNS (RFC 6762):** ✅ Compliant (multicast query/response, conflict resolution)  
**Deviation:** Service type `_koan-stone` not registered with IANA (not required for .local domain)

## Versioning

**Service type stability:** `_koan-stone._tcp.local.` NEVER changes (backward compatibility guarantee)

**TXT record evolution:**
- v0.1: Core fields (`offering`, `port`, `version`, `health`)
- v0.2: Add `fingerprint` (Pond support)
- v0.3: Add `capabilities` (feature flags)
- v1.0: Add `endpoints`, deprecate `port` (but keep for compat)
- v3.0: Remove deprecated `port` (2 major versions after deprecation)

**Migration:** Old Rake ignores unknown TXT fields (forward compat). New Rake understands old fields (backward compat).
