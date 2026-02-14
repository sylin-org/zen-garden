# Koi Embedded Integration

**Status:** Approved  
**Priority:** High  
**Created:** 2026-02-14  
**Authors:** Leo Botinelly  
**Supersedes:** POND-0001 (implementation approach — Pond concept and vocabulary preserved)  
**Related:** [POND-0001](../specs/POND-0001-protocol.md), [SECURITY-0001](../decisions/SECURITY-0001-pond-tiers.md), [LANTERN-0003](../decisions/LANTERN-0003-mdns-service-discovery.md), [pond-totp-admission](pond-totp-admission.md)

---

## Abstract

Embed [Koi](https://github.com/sylin-org/koi) as an in-process library (`koi-embedded`) in Zen Garden's core services, replacing the current Koi HTTP sidecar for mDNS and delivering four new infrastructure capabilities: **certificate mesh** (implementing Pond security), **local DNS**, **TLS reverse proxy**, and **health monitoring**.

This eliminates ~2,400 lines of platform-conditional HTTP/SSE client code, delivers the Pond security vision without from-scratch crypto engineering, and unlocks friendly HTTPS URLs for all garden services.

---

## Motivation

### Current State

1. **mDNS is platform-split.** Moss and Lantern maintain two independent mDNS implementations — `mdns_sd::ServiceDaemon` on Linux and `KoiClient` HTTP on Windows — with `#[cfg]` conditionals across ~1,500 LOC. The Windows path requires an external Koi sidecar process, SSE stream parsing, heartbeat loops, reconnection logic, and TTL-based dedup caches.

2. **Pond security is fully unimplemented.** Every API endpoint returns `501 NOT_IMPLEMENTED` with "Phase 3b" messages. The [POND-0001 specification](../specs/POND-0001-protocol.md) defines a complete protocol (Ed25519 keys, XChaCha20-Poly1305 encryption, BLAKE3 hashing, TOTP invitation, certificate lifecycle) — representing months of security-critical cryptographic engineering that has not started.

3. **Services are reachable only by IP:port.** After deploying `zen-offering-grafana`, users access it via `http://192.168.1.10:3000`. There is no name resolution, no TLS, and no friendly URLs. The [LANTERN-0003](../decisions/LANTERN-0003-mdns-service-discovery.md) decision anticipated a Tier 3 with reverse proxy (Caddy/Traefik), but this hasn't been built.

4. **Dependency versions were misaligned.** Koi uses axum 0.8, reqwest 0.12, thiserror 2, mdns-sd 0.17, and tower-http 0.6. Zen Garden used older versions of all five. **This blocker has been resolved** — all workspace dependencies were upgraded and the entire workspace compiles with zero errors and zero warnings.

### What Koi Provides

Koi is a local network infrastructure toolkit with five capabilities:

| Capability | What it does |
|---|---|
| **mDNS** | Service discovery — browse, register, subscribe, with leased and permanent registrations |
| **DNS** | Local resolver — static entries, certmesh SANs, and mDNS aliases merged into a `.lan` zone |
| **Certmesh** | Private CA — ECDSA P-256, TOTP/FIDO2 enrollment, auto-renewal, revocation, audit log |
| **Proxy** | TLS reverse proxy — certmesh-issued certificates, hot-reload on renewal |
| **Health** | Endpoint monitoring — HTTP, TCP, and process checks with transition history |

The `koi-embedded` crate bundles all five for in-process use via a typed Builder/Handle API with push-based events over broadcast channels.

---

## Design

### Pond Vocabulary Preserved

The Pond metaphor and zen vocabulary are preserved. Certmesh is the implementation engine; Pond remains the user-facing concept.

| Pond concept | Certmesh implementation | CLI command |
|---|---|---|
| **Keystone** | CA keypair (encrypted at rest) | `garden-rake place keystone` |
| **Cornerstone** | Primary CA member | (the stone that creates the pond) |
| **Backup keystone** | Standby CA (`certmesh promote`) | `garden-rake pond promote <stone>` |
| **Fill the pond** | `certmesh join` — stone enrolls, gets cert | `garden-rake pond join <code>` |
| **Pond active** | Mesh has members, CA is unlocked | `garden-rake pond status` |
| **Pond locked** | CA locked after reboot | `garden-rake pond unlock` |
| **Invite** | TOTP code / FIDO2 enrollment auth | `garden-rake pond invite` |
| **Drain the pond** | `certmesh destroy` | `garden-rake drain pond` |
| **Untrust** | `certmesh revoke` | `garden-rake pond untrust <stone>` |

### Certificate Model

After enrollment, each stone holds:

```
/var/lib/koi/certs/<hostname>/
  cert.pem        # This stone's identity certificate (ECDSA P-256)
  key.pem         # This stone's private key
  ca.pem          # CA public certificate
  fullchain.pem   # cert + CA chain
```

The CA public certificate is also installed in the **system trust store** at creation/join time. This means:

- `reqwest` with `rustls-tls` validates stone-to-stone HTTPS automatically
- Browsers on enrolled machines show a green lock for all garden HTTPS URLs
- No `--insecure` flags, no manual trust configuration

Certificates have 30-day lifetimes with automatic renewal handled by certmesh.

### Dual-Port Architecture

| Port | Protocol | When | Purpose |
|---|---|---|---|
| **7185** | HTTP | Always | Discovery, health checks, pond join requests, public status |
| **7186** | HTTPS | When pond active | All authenticated stone-to-stone communication |

After a stone joins the pond, Moss binds HTTPS on **7186** using its certmesh-issued certificate. The mDNS TXT record advertises the pond state:

```
pond=active
http_port=7185
https_port=7186
```

**Security boundary:** When pond is active, sensitive API endpoints (service management, configuration, data access) are served only on :7186. The HTTP port (:7185) becomes a "lobby" — serving health checks, public status, and pond join requests only.

### Join Flow

```
New Stone (no cert)                      Keystone Stone (CA holder)
───────────────────                      ────────────────────────────

1. mDNS browse: discovers _moss._tcp
   Reads TXT: pond=active, http_port=7185

2. HTTP POST :7185/api/v1/pond/join      ← plain HTTP (join is unauthenticated transport)
   { "code": "123456" }                  Validates TOTP / FIDO2

3. Certmesh issues certificate:
   - Generates keypair locally (CSR-based when supported)
   - CA signs with SANs: [hostname, hostname.local]
   - Returns: cert.pem, key.pem, ca.pem

4. Stone installs CA in system trust store
   Moss binds HTTPS on :7186
   Announces pond=active in mDNS TXT

5. All pond members now reachable via HTTPS  ← trust established
```

The security model is sound: TOTP/FIDO2 proves human authorization (same as ACME/Let's Encrypt). The cert material returned from the keystone is useless without the locally-generated private key.

### Chirp Signing

Once in a pond, UDP chirps are **signed** using each stone's private key. Receivers verify the signature against the CA — rejecting chirps from non-pond members.

Signing addresses the real threat: **impersonation** (a rogue device announcing itself as a stone). Chirp contents (IP, name, health status) are not secrets — the same information is visible via mDNS. Full encryption is unnecessary and would add symmetric key management complexity.

```rust
// Sender
let chirp = build_chirp();
let signature = sign_with_stone_key(&chirp, &stone_private_key);
udp_broadcast(&chirp, &signature);

// Receiver
let valid = verify_signature(&chirp, &signature, &ca_public_cert);
if !valid { drop(chirp); }  // Reject non-pond chirps
```

### DNS Registration for Docker Services

When Moss deploys a container, it registers a DNS entry:

```rust
// On container start
koi_handle.dns()?.add_entry(DnsEntry {
    name: format!("{}.lan", offering_name),  // e.g., "grafana.lan"
    ip: stone_ip.to_string(),
    ttl: None,
})?;

// On container stop/remove
koi_handle.dns()?.remove_entry(&format!("{}.lan", offering_name))?;
```

DNS entries are tied to offering lifecycle — no orphaned records.

Koi DNS merges three sources automatically:
1. **Static entries** — Moss-registered service names
2. **Certmesh SANs** — enrolled stone hostnames
3. **mDNS aliases** — discovered services

Anything outside the `.lan` zone is forwarded to the system upstream resolver.

### TLS Proxy for Friendly URLs

For each deployed offering, Moss creates a TLS-terminating proxy with a certmesh-issued service certificate:

```rust
// 1. Request service certificate with custom SANs
koi_handle.certmesh()?.add_alias_sans(&stone_hostname, &[
    format!("{}.lan", offering_name),
]);

// 2. Create proxy entry
koi_handle.proxy()?.upsert(ProxyEntry {
    name: offering_name.clone(),
    listen_port: assigned_port,  // e.g., 8443
    backend: format!("http://127.0.0.1:{}", container_port),
    allow_remote: false,
})?;

// 3. Register DNS
koi_handle.dns()?.add_entry(DnsEntry {
    name: format!("{}.lan", offering_name),
    ip: stone_ip.to_string(),
    ttl: None,
})?;
```

Result: `https://grafana.lan:8443` — trusted TLS, friendly name, no browser warnings on enrolled machines.

Certmesh already supports custom SANs via `JoinRequest.sans` and `add_alias_sans()` — no Koi changes needed for this path.

### Lantern Dual-Mode

| Scenario | Behavior |
|---|---|
| No pond | HTTP only (current behavior, zero friction) |
| Pond active, enrolled browser | HTTPS — green lock, CA trusted |
| Pond active, non-enrolled browser | HTTP for UI access + banner offering CA download |

Lantern serves its web UI over HTTP always (it's a read-mostly dashboard). The HTTPS port is used for Lantern↔Moss API communication when both are in the pond. Non-enrolled browsers (phone, tablet, guest laptop) can download the CA cert from `http://<stone>:7185/api/v1/pond/ca.pem` for manual trust installation.

### Rake Integration

**mDNS:** Rake uses `koi-embedded` with mDNS-only for stone discovery, replacing its direct `mdns-sd` dependency with the unified stack.

```rust
let koi = koi_embedded::Builder::new()
    .mdns(true)
    .dns_enabled(false).health(false).certmesh(false).proxy(false)
    .build()?;
```

**Client enrollment:** When a user runs Rake on a non-stone workstation and the target stone is in a pond:

```bash
garden-rake pond enroll
```

This performs a lightweight certmesh join — gets a **client certificate** (not a stone identity), installs the CA in the trust store. Rake stores its cert in `~/.config/zen-garden/certs/`. The cert CN is `rake-<hostname>` to distinguish from stone identities.

---

## Koi Enhancements Required

Two enhancements are needed in the Koi project to fully support this integration:

### 1. Service Certificate Issuance

**Status:** Partially exists — `add_alias_sans()` covers the SAN injection case. For dedicated service certs (not bound to a member hostname), a new API is needed.

```rust
// New API on CertmeshCore
pub fn issue_service_cert(&self, request: ServiceCertRequest) -> Result<ServiceCert>;
pub fn revoke_service_cert(&self, name: &str) -> Result<()>;

pub struct ServiceCertRequest {
    pub name: String,           // e.g., "grafana"
    pub sans: Vec<String>,      // e.g., ["grafana.lan", "grafana.stone-01.lan"]
}

pub struct ServiceCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_path: PathBuf,
}
```

Service certs are signed by the same CA but don't create roster members. All mesh members automatically trust them. Cert lifecycle is managed by the application (Moss creates on deploy, revokes on remove).

### 2. Client Enrollment

**Status:** New capability needed.

Lighter than member join — issues a client certificate without adding a roster member. Suitable for CLI tools (Rake) and third-party API consumers.

```rust
// New API on CertmeshCore
pub fn enroll_client(&self, request: ClientEnrollRequest) -> Result<ClientEnrollResponse>;

pub struct ClientEnrollRequest {
    pub client_name: String,    // e.g., "rake-workstation"
    pub auth: AuthCredential,   // TOTP code or FIDO2
}
```

Client certs are trusted for TLS but don't participate in the mesh (no renewal, no roster presence, no voting). They can be revoked individually.

---

## Implementation Phases

### Phase 1: Embed Koi + Unified mDNS

**Scope:** Add `koi-embedded` as a dependency. Replace platform-split mDNS code with unified `handle.mdns()` calls in Moss and Lantern.

**Changes — Zen Garden:**

| File | Action |
|---|---|
| `Cargo.toml` (workspace) | Add `koi-embedded` dependency (path = `../../koi/crates/koi-embedded`) |
| `src/common/Cargo.toml` | Add `koi-embedded` dependency |
| `src/moss/Cargo.toml` | Add `koi-embedded`, remove direct `mdns-sd` dependency |
| `src/lantern/Cargo.toml` | Add `koi-embedded`, remove direct `mdns-sd` dependency |
| `src/common/src/infra/koi_client.rs` | **Delete** — HTTP client, SSE parsing, dedup cache (~900 LOC) |
| `src/moss/src/mdns.rs` | Replace dual `#[cfg]` implementations (~350 LOC) with single `handle.mdns()` path |
| `src/lantern/src/tasks/discovery.rs` | Replace dual `#[cfg]` implementations with `handle.mdns()` |
| `src/moss/src/app_state.rs` | Add `KoiHandle` to `AppState` |

**Before (Windows mDNS in Moss — current):**
```rust
#[cfg(target_os = "windows")]
pub struct MdnsHandle {
    koi: Option<Arc<KoiClient>>,        // HTTP sidecar client
    registration_id: RwLock<Option<String>>,
    heartbeat_cancel: CancellationToken,
}
```

**After (unified — both platforms):**
```rust
pub struct MdnsHandle {
    koi: Arc<KoiHandle>,                // In-process handle
    registration_id: RwLock<Option<String>>,
}
```

**Estimated LOC delta:** −1,500 (delete platform-split code and HTTP client), +200 (Koi initialization and unified mDNS calls). Net: **−1,300 LOC**.

**Changes — Koi:** None expected. Phase 1 uses existing mDNS API surface.

**Verification:**
- [ ] Moss announces `_moss._tcp` via embedded Koi on both Windows and Linux
- [ ] Lantern discovers stones via embedded Koi on both platforms
- [ ] Heartbeat/lease management handled by Koi internally (no manual loop)
- [ ] KoiClient HTTP code fully removed
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --workspace` passes

---

### Phase 2: Pond Security via Certmesh

**Scope:** Implement the Pond security layer by delegating to certmesh. Rewire all Pond API stubs. Add HTTPS binding to Moss.

**Changes — Zen Garden:**

| File | Action |
|---|---|
| `src/moss/src/api/v1/pond.rs` | Rewire all handlers from `501 NOT_IMPLEMENTED` to `handle.certmesh()` calls |
| `src/moss/src/main.rs` | Add conditional HTTPS binding on :7186 when pond is active |
| `src/moss/src/bootstrap/router.rs` | Split routes: public (HTTP-only) vs authenticated (HTTPS-only) |
| `src/moss/src/mdns.rs` | Add `pond=active`, `https_port=7186` to mDNS TXT records |
| `src/moss/src/api/v1/pond.rs` | Add `GET /api/v1/pond/ca.pem` endpoint for CA download |
| `src/common/src/constants/mod.rs` | Add `MOSS_HTTPS` port constant (7186) |
| `src/common/src/types.rs` | Align `PondConfig` fields with certmesh state |
| `src/moss/src/infra/listeners/chirp.rs` | Add signature to outbound chirps when pond active |
| `src/moss/src/infra/listeners/chirp.rs` | Verify signature on inbound chirps when pond active |
| `src/rake/src/commands/management/pond.rs` | Rewire all subcommands to call Moss API (which delegates to certmesh) |
| `src/moss/Cargo.toml` | Add `axum-server` with `tls-rustls` feature |

**API mapping:**

| Pond endpoint | Current | After |
|---|---|---|
| `POST /api/v1/pond/init` | 501 stub | `handle.certmesh().create()` → interactive wizard or `--flags` |
| `GET /api/v1/pond/status` | Implemented (basic) | `handle.certmesh().status()` → real mesh status |
| `POST /api/v1/pond/invite` | 501 stub | Generate TOTP code from certmesh enrollment auth |
| `POST /api/v1/pond/join` | 501 stub | `handle.certmesh().enroll(request)` |
| `DELETE /api/v1/pond` | 501 stub | `handle.certmesh().destroy()` |
| `DELETE /api/v1/pond/stones/:name` | 501 stub | `handle.certmesh().revoke_member(name)` |
| `POST /api/v1/pond/unlock` | New | `handle.certmesh().unlock(passphrase)` |
| `POST /api/v1/pond/promote` | New | `handle.certmesh().promote(passphrase)` |
| `GET /api/v1/pond/ca.pem` | New | Serve CA public cert for client trust installation |

**Moss HTTPS binding:**

```rust
// In Moss startup, after Koi initialization
if let Ok(certmesh) = koi_handle.certmesh() {
    if let Some(cert_path) = certmesh_cert_path(&stone_name) {
        let rustls_config = RustlsConfig::from_pem_file(
            cert_path.join("fullchain.pem"),
            cert_path.join("key.pem"),
        ).await?;

        // Spawn HTTPS listener alongside HTTP
        tokio::spawn(
            axum_server::bind_rustls(https_addr, rustls_config)
                .serve(authenticated_router.into_make_service())
        );
        tracing::info!(port = MOSS_HTTPS, "HTTPS listener active (pond)");
    }
}
```

**Changes — Koi:** None expected. Phase 2 uses existing certmesh API. The `enroll()`, `unlock()`, `revoke_member()`, `destroy()` methods are all implemented.

**Verification:**
- [ ] `garden-rake place keystone` creates a certmesh CA with trust profile wizard
- [ ] `garden-rake pond invite` generates TOTP code / shows QR
- [ ] `garden-rake pond join <code>` on a second stone succeeds over HTTP :7185
- [ ] Joined stone binds HTTPS on :7186 with a valid certmesh-issued cert
- [ ] `reqwest` on an enrolled stone validates HTTPS to other enrolled stones
- [ ] `garden-rake pond status` shows real membership from certmesh roster
- [ ] `garden-rake pond untrust <stone>` revokes via certmesh
- [ ] `garden-rake drain pond` destroys CA and clears all certs
- [ ] Chirps from non-pond stones are rejected when pond is active
- [ ] Chirps from pond stones are verified and accepted
- [ ] CA unlock prompt after Moss restart

---

### Phase 3: DNS + TLS Proxy for Services

**Scope:** Register DNS entries and TLS proxy routes when offerings are deployed. Enable friendly HTTPS URLs for all managed services.

**Changes — Zen Garden:**

| File | Action |
|---|---|
| `src/moss/src/tasks/job_executors.rs` | After container deploy: register DNS entry + proxy entry |
| `src/moss/src/api/v1/services.rs` | On `delete_service`: remove DNS entry + proxy entry |
| `src/moss/src/domain/adoption.rs` | On adoption: register DNS for adopted containers |
| `src/moss/src/app_state.rs` | Track DNS/proxy registrations tied to offerings |
| `src/common/src/types.rs` | Add `OfferingNetwork { dns_name, proxy_port, https_url }` to `Offering` |
| `src/moss/src/api/v1/services.rs` | Include HTTPS URL in service response when available |
| `src/moss/src/api/v1/offerings.rs` | Include HTTPS URL in offering response when available |

**DNS registration in offering lifecycle:**

```rust
// In install_service_task, after successful container start:
if let Ok(dns) = state.koi_handle.dns() {
    let dns_name = format!("{}.lan", offering_name);
    let _ = dns.add_entry(koi_config::state::DnsEntry {
        name: dns_name.clone(),
        ip: state.stone_ip.to_string(),
        ttl: None,
    });
    tracing::info!(offering = %offering_name, dns = %dns_name, "DNS entry registered");
}
```

**TLS proxy in offering lifecycle (when pond active):**

```rust
// After DNS registration, if certmesh is active:
if let Ok(proxy) = state.koi_handle.proxy() {
    let proxy_port = allocate_proxy_port()?;  // From port catalog or dynamic
    let _ = proxy.upsert(koi_proxy::ProxyEntry {
        name: offering_name.clone(),
        listen_port: proxy_port,
        backend: format!("http://127.0.0.1:{}", container_port),
        allow_remote: false,
    });
    tracing::info!(
        offering = %offering_name,
        url = %format!("https://{}.lan:{}", offering_name, proxy_port),
        "TLS proxy registered"
    );
}
```

**Changes — Koi:**

| Enhancement | Description |
|---|---|
| Service certificate issuance | `issue_service_cert(name, sans)` on CertmeshCore — issues cert without roster entry |
| Service certificate revocation | `revoke_service_cert(name)` — revokes on offering removal |

**Verification:**
- [ ] Deploying an offering registers `<name>.lan` in Koi DNS
- [ ] Removing an offering removes the DNS entry
- [ ] DNS lookup from any machine using Koi resolver returns correct IP
- [ ] When pond active, proxy entry is created with certmesh-issued cert
- [ ] `https://<name>.lan:<port>` works from any enrolled machine
- [ ] Service cert is revoked when offering is removed
- [ ] `garden-rake observe` / Lantern UI shows HTTPS URLs when available

---

### Phase 4: Rake Client Enrollment + Lantern HTTPS

**Scope:** Enable Rake to enroll as a client for HTTPS access. Add optional HTTPS to Lantern.

**Changes — Zen Garden:**

| File | Action |
|---|---|
| `src/rake/Cargo.toml` | Add `koi-embedded` (mDNS + certmesh client) |
| `src/rake/src/commands/management/pond.rs` | Add `PondActionType::Enroll` — client enrollment flow |
| `src/rake/src/client.rs` | Load client cert from `~/.config/zen-garden/certs/` for HTTPS |
| `src/rake/src/discovery.rs` | Replace `mdns-sd` with embedded Koi mDNS |
| `src/lantern/src/main.rs` | Optional HTTPS binding when stone is in pond |
| `src/lantern/src/tasks/discovery.rs` | Prefer HTTPS for Lantern→Moss API calls when pond active |

**Rake enrollment flow:**
```bash
$ garden-rake pond enroll

Discovering pond on local network...
Found pond: "home-garden" on stone-bronze-canyon
Authenticate to join (TOTP code or tap security key): 123456
✓ Enrolled as client: rake-workstation
  Certificate: ~/.config/zen-garden/certs/rake-workstation/cert.pem
  CA installed in system trust store
  All future commands to pond stones will use HTTPS
```

**Changes — Koi:**

| Enhancement | Description |
|---|---|
| Client enrollment API | `enroll_client(name, auth)` — issues client cert without roster membership |
| Client cert revocation | Revokable via existing `revoke_member()` or new `revoke_client()` |

**Verification:**
- [ ] `garden-rake pond enroll` succeeds with TOTP on a non-stone workstation
- [ ] Subsequent `garden-rake` commands to pond stones use HTTPS automatically
- [ ] Rake on an already-enrolled stone skips enrollment (CA already trusted)
- [ ] Lantern serves HTTPS for Lantern→Moss API communication
- [ ] Lantern keeps HTTP for browser UI access

---

## Dependency Changes

### Already Completed (2026-02-14)

These upgrades align zen-garden with Koi's dependency versions:

| Dependency | Before | After |
|---|---|---|
| `axum` | 0.7 | **0.8** |
| `reqwest` | 0.11 | **0.12** |
| `thiserror` | 1.0 | **2** |
| `mdns-sd` | 0.11 | **0.17** |
| `tower-http` | 0.5 | **0.6** |
| `terminal_size` | 0.3 | **0.4** |

All 8 workspace crates compile with zero errors and zero warnings. All tests pass.

### Phase 1 Additions

```toml
# In workspace Cargo.toml
[workspace.dependencies]
koi-embedded = { path = "../../koi/crates/koi-embedded" }

# In src/moss/Cargo.toml
koi-embedded = { workspace = true }

# In src/lantern/Cargo.toml
koi-embedded = { workspace = true }
```

### Phase 2 Additions

```toml
# In src/moss/Cargo.toml
axum-server = { version = "0.8", features = ["tls-rustls"] }
```

### Phase 4 Additions

```toml
# In src/rake/Cargo.toml
koi-embedded = { workspace = true }
```

### Docker Build Impact

The Linux build Dockerfile (`Dockerfile.linux-x64`) needs one addition for the `keyring` crate (transitive via koi-certmesh → koi-crypto):

```dockerfile
RUN apt-get update && apt-get install -y \
    musl-tools \
    file \
    libasound2-dev \
    libudev-dev \
    pkg-config \
    libsecret-1-dev \    # NEW: required by keyring crate for certmesh
    libdbus-1-dev \      # NEW: required by keyring/libsecret on Linux
    && rm -rf /var/lib/apt/lists/*
```

---

## Code Deletion Summary

Files and code removed across all phases:

| File | LOC removed | Reason |
|---|---|---|
| `src/common/src/infra/koi_client.rs` | ~900 | HTTP/SSE client, dedup cache, browse fallback — replaced by in-process handle |
| `src/moss/src/mdns.rs` (Windows half) | ~350 | `#[cfg(windows)]` path — unified with embedded mDNS |
| `src/lantern/src/tasks/discovery.rs` (Windows half) | ~100 | `#[cfg(windows)]` SSE discovery — unified |
| `src/moss/src/api/v1/pond.rs` (stubs) | ~140 | 501 stubs — replaced with real certmesh delegation |
| Direct `mdns-sd` dependencies | — | Removed from moss, lantern, rake, common Cargo.toml |

**Estimated net deletion: ~1,500 LOC**

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `keyring` crate fails on headless Linux | Medium | Medium | Test in Docker build early; fall back to file-based secret storage if needed |
| Binary size increase (~3-4 MB) | Certain | Low | Acceptable — Moss is already a heavy binary; all new deps are used |
| Koi embedded API changes | Low | Medium | We maintain both repos; pin to path dependency |
| Port 53 conflict with systemd-resolved | Medium | Low | Default to non-standard DNS port (15353); document resolver config |
| CA unlock friction on reboot | Medium | Medium | "Just Me" profile auto-unlocks via keyring; prompt in Lantern UI for other profiles |

---

## Decision Record

This proposal supersedes the **implementation approach** of POND-0001 while preserving its security model and vocabulary. Key differences:

| POND-0001 (original) | This proposal |
|---|---|
| From-scratch crypto (Ed25519, XChaCha20, BLAKE3) | Delegate to certmesh (ECDSA P-256, Argon2id, AES-256-GCM) |
| Shared secret — all stones hold CA keypair | Centralized CA with per-stone certificates (more secure) |
| Custom UDP encryption layer | Chirp signing with stone certificates |
| Months of security engineering | Incremental integration over phases |
| Windows-only Koi sidecar for mDNS | Unified embedded Koi on all platforms |
| No DNS, no proxy, no HTTPS service URLs | Full stack: DNS + proxy + HTTPS |

The security guarantees are equivalent or stronger. The attack Pond defended against — impersonation, eavesdropping, unauthorized admission — are all addressed by certmesh's mTLS model. The centralized CA (vs shared secret) is strictly more secure: a compromised stone doesn't hold the CA private key.
