# Rake Client Enrollment

**Status:** Proposed
**Priority:** High
**Created:** 2026-02-15
**Authors:** Leo Botinelly
**Related:** [pond-ceremony-engine](pond-ceremony-engine.md), [pond-totp-admission](pond-totp-admission.md), [koi-embedded-integration](koi-embedded-integration.md)

---

## Abstract

Enable Rake running on machines without Moss to enroll as a **client** in a pond's trust mesh. Today, pond security covers stone-to-stone mTLS but leaves Rake→Moss traffic unencrypted over plain HTTP. This proposal defines a flow where Rake discovers the cornerstone via mDNS, authenticates via TOTP, receives a certificate bundle, and installs it locally — in the same directory Moss expects, so a future Moss installation on the same machine inherits the enrollment automatically.

---

## Motivation

### Current State

Rake is architecturally a dumb HTTP client:

- All endpoint resolution produces `http://` URLs (`client.rs:39,46,81`)
- `reqwest::Client` has no custom TLS configuration — no root certificates, no client identity
- The join flow (`pond.rs:352`) sends `POST /api/v1/pond/join { "code": code }` and displays the result — Rake receives no certificates
- Port 7183 (HTTPS/mTLS) is exclusively stone-to-stone via `StoneClient`

The pond's mTLS mesh protects stone-to-stone traffic, but the Rake→Moss channel is unprotected. On a network where the pond exists specifically to secure communications, this is a gap.

### Target State

A Rake on any machine (with or without Moss) can:

1. Detect that a tended stone is in a pond
2. Discover the cornerstone directly via mDNS
3. Authenticate and receive a certificate bundle
4. Use mTLS for all subsequent connections to pond stones
5. Leave certificates in place so a future Moss installation inherits them

---

## Design

### Trigger: Pond Awareness in `/health`

The `/health` endpoint (always available on HTTP, port 7185) gains an optional `pond` field:

```json
{
  "status": "healthy",
  "version": "0.2.0.202602151643",
  "pond": "pond-quiet-meadow",
  "components": { ... }
}
```

When present, Rake knows the stone is in a pond. When absent, no pond exists.

**Implementation:** `DaemonHealthStatus` (`common/src/types.rs:323`) adds:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub pond: Option<String>,
```

The health handler reads `state.pond_active` (an `AtomicBool` — zero-cost) and `state.pond.name()`. Old Rakes ignore the unknown field (no `deny_unknown_fields` on the struct).

### Step 1: Rake Detects Pond Membership

When Rake tends a stone, it already calls `GET /health` in 7 code paths (`dispatch.rs:232`, `tend.rs:57,105,206,245`, `status.rs:42`, `client.rs:178`). After a successful health check, Rake inspects the `pond` field:

```
Stone "stone-crystal-forest" is in pond "pond-quiet-meadow".
This machine is not enrolled. To join:

  garden-rake pond join
```

In the initial implementation, this is a **soft warning** — Rake still operates over HTTP. Once Moss supports HTTPS-only mode (mTLS enforcement), unenrolled clients won't be able to connect at all.

### Step 2: Cornerstone Discovery via mDNS

Rake discovers the cornerstone by browsing `_certmesh._tcp.local.` via mDNS.

**Why not `/api/v1/pond/status`:** This endpoint will move behind HTTPS once the pond is active. An unenrolled client cannot reach it. Chicken-and-egg.

**Why mDNS:** The cornerstone already has `CertmeshCore::ca_announcement()` (`lib.rs:341-359`) producing service data with TXT records (`role`, `fingerprint`, `profile`). This data just needs to be registered as an mDNS service. The standalone `koi` binary does this; Moss currently does not.

#### Cornerstone mDNS Registration

When Moss initializes a pond (or boots as a cornerstone), it registers a second mDNS service alongside `_moss._tcp`:

```rust
// In mdns.rs, after pond init/unlock
let ca_info = core.ca_announcement(http_port);
koi_handle.mdns().register(RegisterPayload {
    name: ca_info.name,                    // "koi-ca-stone-crystal-forest"
    service_type: "_certmesh._tcp".into(), // koi-certmesh::CERTMESH_SERVICE_TYPE
    port: ca_info.port,                    // 7185 (HTTP lobby port)
    ip: Some(current_ip),
    txt: {
        "role": "primary",
        "fingerprint": ca_info.fingerprint,
        "profile": ca_info.profile,
        "auth": "totp",                    // NEW: auth method for enrollment
    },
});
```

The `auth` TXT property is new — added to `ca_announcement()` so Rake knows what credential type to prompt for without any HTTP call.

#### Rake-Side Discovery

Rake adds `discover_certmesh_ca()` in `discovery.rs`:

```rust
pub fn discover_certmesh_ca(timeout: Duration) -> Result<Option<CornerstoneInfo>> {
    // Browse _certmesh._tcp.local. using mdns_sd (already a dependency)
    // Extract: IP, port, fingerprint, auth method from TXT records
    // Return CornerstoneInfo { endpoint, fingerprint, auth_method }
}
```

**Windows consideration:** Rake's mDNS browse on Windows is currently stubbed (`discovery.rs:412`). The `mdns_sd` crate works on Windows — the stub exists because Windows Moss doesn't *announce* via mDNS, not because Windows can't *browse*. A Linux cornerstone announcing `_certmesh._tcp` is discoverable from a Windows Rake browser. The stub should be replaced with a real browse for `_certmesh._tcp` even on Windows.

### Step 3: Admin Privilege Check

Certificate installation requires elevation:

| Platform | CA Trust Store | Command | Privilege |
|----------|---------------|---------|-----------|
| Linux | System certs | `update-ca-certificates` | root |
| macOS | System Keychain | `security add-trusted-cert` | admin |
| Windows | Root cert store | `certutil -addstore Root` | admin |

Additionally, the cert file directory (`{data_dir}/koi/certs/`) may require elevation to create on machines without Moss.

`garden-rake pond join` checks for elevation early:

```
CA certificate installation requires administrator privileges.
Re-run this command in an elevated prompt.
```

One elevation, both operations (cert directory creation + trust store installation).

### Step 4: Authentication and Certificate Issuance

#### New Endpoint: `POST /api/v1/pond/enroll-client`

This endpoint lives on the cornerstone's **HTTP lobby** (port 7185, `configure_public()`). It is reachable without TLS because the caller is not yet enrolled.

```rust
/// POST /api/v1/pond/enroll-client — Client enrollment (no stone state mutation)
///
/// Issues a certificate for a non-Moss client (Rake on a workstation).
/// Only works on the cornerstone (the stone with the active CA).
/// Auth-gated by TOTP/FIDO2 code in the request body.
pub async fn pond_enroll_client_v1(
    State(state): State<AppState>,
    Json(payload): Json<ClientEnrollRequest>,
) -> Result<Json<ApiResponse<ClientEnrollResponse>>, (StatusCode, Json<ApiErrorResponse>)>
```

**Why a separate endpoint from `/api/v1/pond/join`:**

The current `join` handler (`pond.rs:548-566`) conflates certificate issuance with stone lifecycle:

- `local_enrollment()` (`pond.rs:571`) defaults to `state.stone_name` as the hostname
- `proxy_enrollment()` (`pond.rs:620`) hardcodes `"hostname": state.stone_name`
- Both call `notify_enrollment_changed()` which flips `pond_active`, starts HTTPS, emits `PondEvent`

These side effects are for stone enrollment, not client enrollment. The client endpoint calls `CertmeshCore::enroll()` directly — same crypto, same auth verification — without any stone state mutation.

#### Request / Response

```rust
struct ClientEnrollRequest {
    hostname: String,       // Rake machine's hostname
    code: String,           // TOTP code
    sans: Vec<String>,      // Optional: IPs, .local aliases
}

struct ClientEnrollResponse {
    ca_cert: String,        // PEM — for trust store + ca.pem
    service_cert: String,   // PEM — for mTLS identity
    service_key: String,    // PEM — for mTLS identity
    ca_fingerprint: String, // For verification
    hostname: String,       // Enrolled hostname (echoed back)
    cert_expires: String,   // ISO 8601
}
```

#### Handler Logic

```rust
async fn pond_enroll_client_v1(state, payload) {
    let core = get_certmesh_core(&state)?;  // Fails on non-cornerstones

    let join_req = JoinRequest {
        hostname: payload.hostname,
        auth: AuthResponse::Totp { code: payload.code },
        sans: payload.sans,
    };

    let resp = core.enroll(&join_req).await.map_err(certmesh_err)?;

    // Create roster entry with Client role
    // No notify_enrollment_changed() — no stone state mutation

    Ok(Json(ApiResponse::new(ClientEnrollResponse {
        ca_cert: resp.ca_cert,
        service_cert: resp.service_cert,
        service_key: resp.service_key,
        ca_fingerprint: resp.ca_fingerprint,
        hostname: resp.hostname,
        cert_expires: resp.cert_expires.unwrap_or_default(),
    })))
}
```

Non-cornerstone stones return 409 Conflict: `"This stone is not the CA. Discover the cornerstone via _certmesh._tcp mDNS."`.

### Step 5: Rake Receives and Installs Certificates

After receiving the response, Rake performs two operations:

#### 5a. Write Certificate Files

Writes to `{data_dir}/koi/certs/{hostname}/` — the **same path** Moss reads from:

| File | Content | Permissions |
|------|---------|-------------|
| `cert.pem` | Service certificate | 0644 |
| `key.pem` | Service private key | 0600 |
| `ca.pem` | CA public certificate | 0644 |
| `fullchain.pem` | cert.pem + ca.pem | 0644 |

This matches `write_enrollment_certs()` in `pond.rs:831` exactly.

#### Why This Path Matters

Moss's boot sequence (`run.rs:385-396`) already auto-detects pre-existing enrollment certs:

```rust
// Enrolled member fallback: check for enrollment certs on disk
if !pond_active.load(Ordering::Relaxed) {
    let certs_dir = PathBuf::from(data_dir()).join("koi").join("certs").join(&stone_name);
    if certs_dir.join("cert.pem").exists() && certs_dir.join("key.pem").exists() {
        pond_active.store(true, Ordering::Relaxed);
        pond_state.seed_enrolled(true);
        tracing::info!("Pond active — enrolled member with certs from previous enrollment");
    }
}
```

If Moss is later installed on the same machine:

1. It finds cert files at `{data_dir}/koi/certs/{hostname}/` — sets `pond_active = true`
2. `activate_pond_security()` starts HTTPS using the existing certs
3. The machine is already in the cornerstone's roster
4. The CA cert is already in the system trust store
5. The stone is immediately operational as a pond member — no re-enrollment needed

#### 5b. Install CA in System Trust Store

```rust
koi_truststore::install_ca_cert(&ca_cert_pem, "zen-garden-pond")?;
```

`koi-truststore` already handles all three platforms (`linux.rs`, `windows.rs`, `darwin.rs`). After installation, browsers trust HTTPS connections to any Moss stone in the pond.

#### 5c. Store Enrollment Metadata

Write a `.pond-enrollment.json` alongside the certs:

```json
{
  "pond_name": "pond-quiet-meadow",
  "cornerstone": "stone-crystal-forest",
  "ca_fingerprint": "abc123...",
  "enrolled_at": "2026-02-15T22:35:01Z",
  "cert_expires": "2026-03-17T22:35:01Z",
  "role": "client"
}
```

Rake reads this on subsequent runs to configure its `reqwest::Client` with the CA cert and client identity.

### Step 6: Rake Uses mTLS for Future Connections

After enrollment, Rake's HTTP client is configured:

```rust
let client = reqwest::Client::builder()
    .add_root_certificate(ca_cert)      // trust the pond CA
    .identity(client_identity)           // present client cert for mTLS
    .build()?;
```

Endpoint resolution prefers `https://stone:7183` when certs are available, falling back to `http://stone:7185` when they are not.

---

## Roster: Client vs Stone

A client enrollment creates a roster entry with a new `MemberRole::Client` variant. This distinguishes workstations from stones for:

- **Topology views:** Clients don't appear in garden topology (they're not stones)
- **Renewal policy:** Clients may have different cert lifetimes
- **Health tracking:** A client that hasn't heartbeated isn't "down" — it's a workstation that's turned off
- **Operator visibility:** "3 stones + 2 clients enrolled" instead of "5 members"

Revocation works identically — `DELETE /api/v1/pond/stones/:name` revokes a client the same as a stone.

---

## Complete Flow

```
1. Rake tends stone-crystal-forest
   → GET http://stone-crystal-forest:7185/health
   → { "status": "healthy", "pond": "pond-quiet-meadow", ... }
   → "This stone is in pond 'pond-quiet-meadow'. You are not enrolled."
   → "Run: garden-rake pond join"

2. garden-rake pond join
   a. Check admin privileges → bail if not elevated
   b. Browse mDNS _certmesh._tcp.local. (3s timeout)
      → "koi-ca-stone-crystal-forest" at 192.168.1.10:7185
      → TXT: role=primary, auth=totp, fingerprint=abc123
   c. Prompt: "Enter the 6-digit code from your authenticator app:"
   d. User enters code

3. POST http://192.168.1.10:7185/api/v1/pond/enroll-client
   { "hostname": "workstation-01", "code": "847291", "sans": ["workstation-01.local"] }

4. Cornerstone processes:
   → CertmeshCore::enroll() verifies TOTP, issues cert
   → Roster entry with role=Client
   → Returns cert bundle (ca_cert, service_cert, service_key)

5. Rake installs:
   a. Creates {data_dir}/koi/certs/workstation-01/
   b. Writes cert.pem, key.pem (0600), ca.pem, fullchain.pem
   c. koi_truststore::install_ca_cert(&ca_cert, "zen-garden-pond")
   d. Writes .pond-enrollment.json
   → "Enrolled in pond 'pond-quiet-meadow'. HTTPS connections enabled."

6. Future Rake commands use mTLS:
   → GET https://stone-crystal-forest:7183/api/v1/stone/services
```

---

## Scope

### In Scope

| Component | Change |
|-----------|--------|
| `DaemonHealthStatus` | Add optional `pond` field |
| Health handler (`health.rs`) | Read `pond_active` + pond name |
| Moss mDNS (`mdns.rs`) | Register `_certmesh._tcp` for cornerstone |
| `ca_announcement()` (`koi-certmesh`) | Add `auth` method to TXT records |
| Moss lobby router | Mount `POST /api/v1/pond/enroll-client` |
| Moss pond handler | New `enroll_client_v1()` — thin wrapper around `CertmeshCore::enroll()` |
| Koi roster | New `MemberRole::Client` variant |
| Rake discovery (`discovery.rs`) | New `discover_certmesh_ca()` — browse `_certmesh._tcp` |
| Rake Windows mDNS | Replace `_certmesh._tcp` browse stub with real browse |
| Rake pond command | New `pond join` flow for client enrollment |
| Rake cert writer | New module — write cert bundle to data_dir |
| Rake HTTP client | Configure `reqwest` with CA cert + client identity |

### Out of Scope (Future Work)

- **FIDO2 client enrollment:** Requires browser/WebAuthn. TOTP is sufficient for CLI-based enrollment.
- **Certificate renewal for clients:** Rake-triggered renewal when certs approach expiry. Requires a renewal endpoint on the cornerstone.
- **Multiple pond support:** A workstation enrolled in multiple ponds. Requires per-pond cert storage keyed by CA fingerprint.
- **HTTPS-only enforcement:** Moss disabling the HTTP port entirely once all clients are enrolled.
- **Token Adapter abstraction:** Generalizing TOTP/FIDO2 behind a pluggable interface (separate proposal).

---

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| mDNS discovery, not HTTP | `/api/v1/pond/status` moves behind HTTPS — chicken-and-egg |
| Separate `/enroll-client` endpoint | `/pond/join` conflates cert issuance with stone lifecycle (starts HTTPS, emits events) |
| Same cert directory as Moss | Future Moss installation inherits enrollment — boot sequence already detects it (`run.rs:385-396`) |
| `MemberRole::Client` roster variant | Distinguishes workstations from stones for topology, health, and renewal |
| Admin check before enrollment | CA trust store installation requires elevation on all platforms |
| `pond` field in `/health` | Zero-cost trigger (atomic bool read) on an endpoint Rake already calls |
| Soft warning initially | Rake still works over HTTP after detecting pond — no breaking change |
