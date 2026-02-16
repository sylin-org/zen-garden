# KOI-0001: Phase 0 Prerequisite for Offering Orchestration

**Canonical Location:** [`koi` repo → docs/proposals/KOI-0001-embedded-http-and-udp-bridging.md](https://github.com/your-org/koi/blob/dev/docs/proposals/KOI-0001-embedded-http-and-udp-bridging.md)  
**Status:** Draft  
**Depended On By:** ORCH-0001, ORCH-0002, ORCH-0003

---

## Summary

KOI-0001 adds two capabilities to `koi-embedded`:

1. **HTTP self-hosting** — koi-embedded spawns its own axum listener on `:5641`, exposing the full Koi HTTP API (mDNS, DNS, certmesh, health, proxy). Activates an existing dead `http_enabled` config field.

2. **UDP bridging** (`koi-udp`) — A new domain crate that bridges host UDP sockets into HTTP/SSE, allowing containers to receive/send datagrams through the host's network stack.

## Why This Blocks ORCH

Containerized orchestrators (ORCH-0002 AI Router, ORCH-0003 DB Choreographer) need:

- UDP mesh access (chirps, beacons on port 7184) → `koi-udp` `/v1/udp/*`
- DNS registration (e.g., `ollama.lan` takeover) → `/v1/dns/*`
- mDNS discovery (find Ollama instances) → `/v1/mdns/*`
- TLS proxy (certmesh certificates) → `/v1/proxy/*`

None of these are reachable from Docker bridge networking today.

## What Zen Garden Needs to Do (Phase 0c)

After KOI-0001 Phase 0a + 0b are done in the `koi` repo, Moss needs these changes:

### 1. Enable Koi HTTP + UDP in builder (`src/moss/src/bootstrap/run.rs`)

```rust
let koi = koi_embedded::Builder::new()
    .data_dir(koi_data_dir)
    .service_mode(koi_embedded::ServiceMode::EmbeddedOnly)
    .mdns(true)
    .dns_enabled(true)      // ← was false
    .dns_auto_start(true)   // ← NEW
    .certmesh(true)
    .http(true)             // ← NEW: self-host HTTP on :5641
    .udp(true)              // ← NEW: enable UDP bridging
    .build()?;
```

### 2. Docker `extra_hosts` (`src/moss/src/docker.rs`)

```rust
extra_hosts: Some(vec![
    "host.docker.internal:host-gateway".to_string(),
]),
```

### 3. Environment variable injection (`src/moss/src/docker.rs` or `job_executors.rs`)

```rust
env.push(format!("KOI_ENDPOINT=http://host.docker.internal:{}", koi_port));       // 5641
env.push(format!("GARDEN_STONE_ENDPOINT=http://host.docker.internal:{}", moss_port)); // 7185
env.push(format!("GARDEN_OFFERING_NAME={}", name));
```

### 4. Container DNS (optional, high value)

```rust
dns: Some(vec![stone_ip.to_string()]),  // containers resolve .lan names via Koi DNS
```

### 5. Update `tools/koi/tool.json`

Set `retired: false`, update description.

---

## Effort

| Phase | Repo | Effort |
|-------|------|--------|
| 0a: HTTP self-hosting | `koi` | ~1-2 days |
| 0b: koi-udp crate | `koi` | ~3-5 days |
| 0c: Moss container wiring | `zen-garden` | ~1 day |

See the full proposal in the `koi` repo for complete API surface, types, security analysis, and testing plan.
