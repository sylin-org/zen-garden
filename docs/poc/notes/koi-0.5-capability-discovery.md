# koi 0.5.0 consumable interface — capability discovery (for zen to react to)

> Working note (untracked stash). Built 2026-06-22 from an 8-cluster catalog→accuracy-check
> workflow over koi 0.5.0 (`../koi`). Every signature was read from code and adversarially
> re-confirmed; corrections from the verify pass are folded in. This is a *reference* — the
> shape of koi's matured interface and how zen reacts to it, not a refactor plan
> (that's [koi-0.5-leverage-refactor-plan.md](koi-0.5-leverage-refactor-plan.md)).

## The two things you named, confirmed in code

**1. "koi can sign payloads"** — yes. `CertmeshHandle::sign(&[u8]) -> Envelope` /
`verify(&Envelope) -> Assurance`, mode-transparent: a passthrough (freshness-stamped, `sig: None`)
when Open, a real ES256-signed-with-carry-cert envelope when secured. Plus `seal`/`open` (the same,
wrapped in a future-encryption-ready type that is passthrough today).

**2. "koi emits an event when the mode changes insecure→secured so we can react"** — yes.
`KoiEvent::PostureChanged { from: Posture, to: Posture }` on the broadcast stream, **and** the
cleaner `watch_posture() -> watch::Receiver<Posture>` if you hold the core. The degrade direction
(secured→Open, e.g. silent cert loss) fires **as loudly as** the upgrade.

## The single most useful correction the verify pass produced

The clean **`watch_posture()`** channel *is* reachable from an embedded consumer — through
`handle.certmesh()?.core()?.watch_posture()`. (`CertmeshHandle::core() -> Arc<CertmeshCore>`,
koi-embedded/src/handle.rs:812, Embedded-only.) The catalog first claimed it wasn't; it is. This
matters for zen because **zen already holds the core** (`state.discovery.koi().certmesh()`), so zen
gets the coalesced, always-has-a-current-value watch — not just the lossy broadcast.

---

## Posture: the mode oracle (koi-common/src/posture.rs)

- `Posture { signed: bool, encrypted: bool }` — wire-stable serde `{"signed":_,"encrypted":_}`.
  `OPEN = {false,false}`. **`encrypted` is hardcoded `false` in 0.5.0** (seal/open are passthrough);
  treat `signed` as the only live bit. `is_secure() == signed`.
- `PostureLevel { Open, Authenticated, Confidential }` — `Ord` (so `level >= Authenticated`),
  `as_wire()`/`from_wire()` snake_case (the mDNS TXT contract), `From<Posture>`. **Confidential is a
  reachable type but never produced today.**
- Read it (sync): `CertmeshCore::posture() -> Posture` (core_identity.rs:56) — a cheap disk check
  ("do I have an identity", *not* "is it fresh"). Handle: `certmesh()?.posture() -> Result<Posture>`
  (handle.rs:839), **embedded-only** (Remote → `DisabledCapability`).
- Watch it: `CertmeshCore::watch_posture() -> watch::Receiver<Posture>` (core_setup.rs:125) —
  coalesced (fires only on real change), seeded from disk (a new subscriber reads current
  immediately).

### Recipe — react to mode change → switch protocol (your stated goal)

There are two levels, pick by *what* zen needs to change:

**A. If the only thing to change is the HTTP transport** → don't watch at all. Hand koi your router:
```rust
handle.serve(router, addr, cancel)?;        // plain↔mTLS, live-flip, same port, no dropped conns
// or, for identity + posture-stamped mDNS announce + serve in one call:
handle.participate(router, addr, "_zen._tcp", cancel).await?;
```
koi runs its own `watch_posture()` loop internally and flips for you.

**B. If zen must change application-level behavior on the flip** (re-stamp the UDP-7184 announce,
gate features, update lantern), react explicitly — and because zen holds the core, prefer the watch:
```rust
let core = state.discovery.koi().certmesh()?.core()?;     // Arc<CertmeshCore>
let mut rx = core.watch_posture();
let mut last = *rx.borrow_and_update();                    // seed baseline — do NOT treat as a transition
loop {
    rx.changed().await?;
    let now = *rx.borrow_and_update();
    on_posture_flip(last, now);                            // last.signed=false→now.signed=true = upgrade
    last = now;
}
```
The broadcast alternative (`handle.subscribe()`/`events()` + match `KoiEvent::PostureChanged{from,to}`)
works too, but is **lossy** (capacity 256; a `Lagged` drops events) and has **no held value** — you
must seed with one `posture()` call and re-read `posture()` after a `Lagged` to resync.

### Two reactive gotchas that will bite

- **Locked-CA-at-boot does not fire a transition.** If a node boots with a *locked* CA, posture is
  already `signed:true`; `koi certmesh unlock` does **not** emit a change (the leaf was already on
  disk). koi's own supervisor works around this with a 5s retry timer — if zen reacts only to the
  event, add a timer for this case.
- **Posture answers "have an identity", not "is it valid".** Near-expiry never fires a
  PostureChanged — the degrade fires only *after* identity is already lost. For "expires soon" poll
  `diagnose()` / `local_identity().renewal` on a timer.

---

## Payload signing — Envelope / Assurance / Sealed

Types live in **koi-common** (`koi_common::envelope::{Envelope, Sig, SigAlg, Freshness, Assurance,
RejectReason}`, `koi_common::sealed::{Sealed, Opened, Confidentiality}`). The **logic** lives in
**koi-certmesh** (`koi_certmesh::envelope::{build_envelope, verify_envelope, decode_payload}`,
`koi_certmesh::sealed::{seal_passthrough, open_sealed}`). **Neither Envelope nor Assurance is in the
koi-embedded prelude** (only `Confidentiality/Opened/Sealed` are) — import them from `koi_common`.

| Primitive | Signature / location | Open | Secured |
|---|---|---|---|
| `CertmeshHandle::sign` | `async (&[u8]) -> Result<Envelope>` handle.rs:867 | `sig: None` passthrough | ES256 + carry-cert |
| `CertmeshHandle::verify` | `async (&Envelope) -> Result<Assurance>` handle.rs:876 | `Anonymous{freshness}` | `Authenticated{cn}` / `Rejected{reason}` |
| `CertmeshHandle::seal`/`open` | handle.rs:889/898 | passthrough, `Confidentiality::None` | signed-but-still-not-encrypted; `open` is **fail-closed** on tamper (Err, never bytes) |
| `koi_certmesh::envelope::build_envelope` | **sync** `(Option<(&key_pem,&cert_pem)>, bytes, nonce, ts) -> Envelope` envelope.rs:59 | — | the no-handle, deterministic path |
| `koi_certmesh::envelope::verify_envelope` | **sync** `(&Envelope, Option<&ca_pem>, &[revoked_fp], now) -> Assurance` envelope.rs:97 | — | the no-handle path |

- **The one trust door:** `Assurance::identity() -> Option<&str>` (envelope.rs:112) — `Some(cn)` **iff
  Authenticated AND Fresh**. Gate authz on `identity().is_some()`, **never** on `!is_rejected()`
  (that leaks `Anonymous`/`Stale` as trusted — the API is shaped to make the wrong check obviously
  wrong).
- **`verify` returns `Ok(Assurance)` for *every* verdict** including `Rejected`; the `Err` is reserved
  for the disabled-capability (Remote) case, not for verification failure.
- **What `verify_envelope` actually enforces** (envelope.rs): version+alg gate (alg-confusion closed —
  the in-band `Sig.alg` never picks a codepath) → CA-chain (leaf chains to pinned CA → else
  `UnknownSigner`) → expiry → revocation (fingerprint) → ES256 over `canonical_bytes`. **CN is read
  authoritatively from the cert**, never a claimed field.
- **Freshness ≠ replay protection.** `FRESHNESS_WINDOW_SECS = 300` (±300s). The `nonce` exists for
  per-message uniqueness but **koi keeps no nonce/seen cache** — a captured envelope replays within
  300s. If zen needs true replay defense, it builds its own seen-nonce cache.
- **3 `RejectReason` variants never fire** in 0.5.0: `NoSignature`, `ClockSkew`, `NameMismatch`. An
  out-of-window timestamp yields `Authenticated{Stale}` (then refused by `identity()`), **not**
  `ClockSkew`. Match exhaustively, but know these are dead.
- **`seal`/`open` are passthrough today** (`Confidentiality::None`; one-time "signed but NOT
  encrypted" warn). Coding against them now buys a zero-change upgrade when the group-key rung lands.
  Cross-impl note: `Confidentiality` has two wire strings — serde `"group_key"`/`"none"` vs
  `as_wire()` `"groupkey"`/`"passthrough"` (the `/v1/status` contract uses `as_wire()`).
- **Cross-language contract:** the signed bytes are
  `"koi-envelope-v1\n{v}\n{payload_b64}\n{nonce_b64}\n{ts}"` (`ENVELOPE_DOMAIN_V1`, envelope.rs:26),
  pinned by `docs/reference/vectors/trust-vectors.json` and specced in
  `docs/reference/trust-protocol.md` — this is what a C# Koan reproduces.

---

## The full event menu — `KoiEvent` (koi-embedded/src/events.rs)

Subscribe: `handle.subscribe() -> broadcast::Receiver<KoiEvent>` (handle.rs:211) or
`handle.events() -> BroadcastStream<KoiEvent>` (handle.rs:105). Capacity **256**, lossy
(`Lagged(n)` → warn-and-continue, never breaks the stream — matches zen code-standards §13).
**`KoiEvent` derives only `Debug, Clone` — NOT `Serialize`**, so zen must map to its own DTO before
re-emitting over SSE. **Remote mode emits nothing** (the mapper tasks never spawn) — the whole event
surface is effectively embedded-only.

| Event | Payload | Fires when | Capability needed |
|---|---|---|---|
| `PostureChanged` | `{from: Posture, to: Posture}` | this node's Open↔Authenticated flip (both directions) | certmesh |
| `CertmeshMemberJoined` | `{hostname, fingerprint}` | **CA-side**: a member enrolls | certmesh + CA role |
| `CertmeshMemberRevoked` | `{hostname}` | **CA-side**: a member is revoked | certmesh + CA role |
| `CertmeshDestroyed` | (unit) | CA/mesh torn down (pairs with a degrade `PostureChanged`) | certmesh |
| `MdnsResolved` | `ServiceRecord` (TXT carries peer posture/fp/expiry) | a peer fully resolves | mdns + active browse |
| `MdnsRemoved` | `{name, service_type}` | a peer expires/goodbyes | mdns + active browse |
| `MdnsFound` | `ServiceRecord` | **only on a meta-query** (all-types discovery) — *not* a normal browse | mdns |
| `HealthChanged` | `{name, status: Up/Down/Unknown}` | a registered check transitions | health + a check |
| `DnsEntryUpdated`/`Removed` | `{name, ip}` / `{name}` | a static DNS entry changes | dns |
| `ProxyEntryUpdated`/`Removed` | `{entry}` / `{name}` | a proxy entry changes | proxy |
| `RuntimeInstanceStarted`/`Stopped` | `{name, backend}` / `{name}` | container start/stop | runtime adapter |

Event-surface gaps worth knowing: **no startup/ready/shutdown event** (read `bound_http_port()`
after `start()`; shutdown is a method); **no cert-renewal/expiry-warning event** (poll `diagnose()`);
runtime `Updated`/`BackendDisconnected`/`BackendReconnected` are **dropped** (dashboard-SSE only);
`RuntimeInstanceStopped` discards the runtime `id`; mDNS events need an **active browse pump** (just
subscribing on an idle node yields nothing). There is also a write side:
`MdnsHandle::emit_event(KoiEvent)` (handle.rs:549) lets the host inject its own events into the same
stream.

---

## Adaptive serving + posture-keyed client

- **`KoiHandle::serve(router, addr, cancel) -> Result<JoinHandle>`** (handle.rs:132, sync, spawns the
  supervisor) — same-port plain↔mTLS, **never rebinds the socket**, flips live, refuses
  TLS-to-Open and plaintext-to-secure (secure-by-default, never silent downgrade).
  **Gotcha: it swallows bind failure** into a log; the `JoinHandle` resolves anyway. For bind-error
  visibility call the underlying `koi_embedded::serve_adaptive(core, router, addr, cancel).await?`
  (serve.rs:40, returns `io::Result`).
- **`KoiHandle::participate(router, addr, service_type, cancel)`** (handle.rs:170, async) —
  `ensure_identity` + posture-stamped mDNS announce (re-stamped on every flip) + `serve`, one call.
  *(Both `serve` and `participate` are on `KoiHandle`, **not** `CertmeshHandle` — the catalog first
  misattributed them.)*
- **`CertmeshHandle::client_for(&Peer) -> Result<PeerClient>`** (handle.rs:926) — picks plain-HTTP for
  an Open peer, mTLS (our leaf + pinned CA) for a secure peer; errors loudly on "peer needs auth but
  we're Open" and "anchors to a different mesh". `PeerClient`: `is_secure()/host()/port()/get(path)/
  post_json(path,body)` returning `(u16, String)`. **Limited: GET + JSON-POST only, body buffered to
  4 MiB, no PUT/DELETE/PATCH/HEAD, no custom headers, no streaming/SSE.** zen's SSE/large-transfer
  paths can't use it — they keep their own client.
- **Inbound caller identity for zen's own listener:** `koi_certmesh::http::ClientCn(pub String)` is
  injected as an axum `Extension` on a secure mTLS connection. Handler pattern that runs in both
  postures: take `client_cn: Option<Extension<ClientCn>>` — `Some(cn)` on mTLS, `None` on plaintext.
- **`CertmeshCore::require_auth(router) -> Router`** (core_identity.rs:291, via `core()`) — wrap your
  write routes once: **no-op while Open**, **401 once secured** (gates on the *presence* of any mesh
  CN, not an allowlist — add your own CN/role check for finer authz). Note the consequence: a
  `require_auth`'d route becomes **inter-node-only after the flip** (browsers have no client cert →
  401). The same-port secure path is **mandatory mTLS, not browser-safe**; a browser-reachable HTTPS
  UI on a secure stone is the separate ACME server-auth listener (koi-serve's trust_plane), not in
  this surface.
- Low-level building blocks if zen ever terminates its own TLS:
  `koi_certmesh::mtls::{build_server_config(cert,key,ca), build_client_config(...), serve(router,
  listener,config,cancel), extract_cn(der), get(...), post_json(...)}`.

---

## Identity & enrollment lifecycle (koi-certmesh; reach via `certmesh()?.core()?`)

`CertmeshHandle` re-exposes only `posture/local_identity/ensure_identity/diagnose/sign/verify/seal/
open/client_for`. **Everything else is core-only** via `certmesh()?.core()?`.

- `ensure_identity() -> Option<Identity>` (core_identity.rs:124) — idempotent enroll-if-CA /
  renew-if-member. **Does not do first-join.** Swallows errors to `None` (call the role-specific
  method for a loud result). Run once at startup + on a timer unless `certmesh_background(true)`.
- `local_identity() -> Option<Identity>` (core_identity.rs:76) — pure read of this node's
  `Identity { hostname(=CN), cert_pem, key_pem, ca_cert_pem, ca_fingerprint, renewal: RenewalHealth }`.
  **`Identity` is not `Serialize`** (redacted Debug); `RenewalHealth` *is*.
- `diagnose() -> TrustDiagnosis` (core_identity.rs:228) — trust-doctor: named checks with
  status/detail/remedy, `is_red()`/`exit_code()`. Fully serde — re-exposable over zen's API; remedies
  reference `koi certmesh …` CLI (zen may re-map).
- `certmesh_status() -> CertmeshStatus` (core_identity.rs:34) — **zen already uses this**
  (pond.rs:374): roster/membership + `policy: CertPolicy` (the 90/30/14 cadence — read it, don't
  hardcode).
- First-join is a **3-call compose** (no single `join`): `prepare_member_csr` → (remote enroll with an
  invite token) → `install_member_cert(.., ca_endpoint, ca_fingerprint, ..)`. **Pass `ca_endpoint` +
  `ca_fingerprint`** or the node won't write `member.json` and won't auto-renew. `mint_invite` (CA
  side) yields a single-use, hostname-bound, fingerprint-pinned token `"<hex>.<ca_fp>"`.
- **Cert path (matches the Phase-1 caveat in the refactor plan):** leaf lives at
  `{data_dir}/certs/{OS-hostname}/{cert,key,ca,fullchain}.pem`, keyed on `hostname::get()` —
  the same value used as CN/SAN. **If a stone's display name ≠ its OS hostname, the leaf dir is keyed
  by the OS hostname.** (CA/roster/member.json/invites live under `{data_dir}/certmesh/`.) This is
  exactly why the chirp-signing migration must confirm `stone.name == hostname` before delegating to
  `handle.sign()`.

---

## Builder / handle composition (koi-embedded/src/lib.rs) — how zen wires it

`Builder::new()` defaults: **mdns ON, dns ON, http OFF**, health/certmesh/proxy/udp/runtime OFF,
`service_mode = Auto`. zen's actual config (run.rs:566-595): `service_mode(EmbeddedOnly)` +
`http(true)` + `certmesh(true)` + `udp(true)` + `mdns(true)` + `dashboard(true)` +
`mdns_browser(true)`, and explicitly **off**: `dns_enabled(false)`, `health(false)`, `proxy(false)`.

- **`ServiceMode::EmbeddedOnly` is load-bearing** — it guarantees the embedded backend, hence the
  full trust/serve/posture surface. In `Remote`/`Auto`-attached mode, `serve`/`participate`/`udp`/
  every `CertmeshHandle` trust op returns `DisabledCapability`.
- **Fail-closed:** `announce_http(true)` without `http_token(..)` → `KoiError::InsecureConfig` at
  `start()` (before any socket binds). Loopback default needs no token. Validation is at `start()`,
  **not** `build()` (build never errs today).
- **Capability gates:** each `handle.<cap>()` returns `Err(DisabledCapability)` if not selected.
- New-in-0.5.0 knobs: `orchestrator(bool)` (auto-wire koi's own mDNS/DNS/health/proxy from discovered
  containers — needs runtime), `certmesh_background(bool)` (role-driven renewal + trust-bundle pull;
  **auto-denies enrollment requests** under embedded — interactive enroll must be consumer-driven).
- `start() -> KoiHandle` builds via the **same `koi-compose` root** the daemon and Windows service
  use (no `/v1` drift), with `fail_fast=true` (a failed capability is an `Err`, not a silently-dropped
  capability).

---

## Discovery & peer trust-legibility (koi-common/src/peer.rs, koi-embedded mdns handle)

- `MdnsHandle::discover(service_type) -> Vec<Peer>` (handle.rs:475) / `discover_for(type, window)` —
  each `Peer` carries the peer's advertised **posture + fingerprint + expiry** so a client can pick
  http-vs-mTLS **before** dialing. (Advertised = untrusted hint; adjudicate with `client_for`/
  `verify`.)
- `Peer`: fields `posture`, `fp`, `cn`, `expires_at`; methods `addr() -> Option<(String,u16)>`,
  `level() -> PostureLevel`, `is_secure() -> bool`, `expires_in(now)`. Re-exported `koi_embedded::Peer`.
- `koi_common::peer::stamp(&mut txt, posture, ca_fp, expires_at)` (peer.rs:128) — **if zen wants
  koi-compatible posture-stamped TXT on its own UDP-7184/mDNS announce, this is the exact writer.**
  Read side: TXT keys `TXT_FP`/`TXT_POSTURE`/`TXT_EXPIRES`.
- `MdnsHandle`: `browse(type) -> KoiBrowseHandle` (`recv() -> Option<MdnsEvent>`, the **remote-mode**
  reactive path), `register(RegisterPayload) -> RegistrationResult`, `resolve(name) -> ServiceRecord`,
  `subscribe() -> broadcast::Receiver<MdnsEvent>` (embedded-only; raw per-domain stream),
  `LeasePolicy { Permanent, Session, Heartbeat }`.

---

## Published cross-platform contract (docs/reference/)

- **`trust-protocol.md`** + **`vectors/trust-vectors.json`** — the authoritative, versioned envelope/
  trust wire spec + conformance vectors (what a C# Koan implements to). `envelope-encryption.md`
  (exists) is about **CA private-key** encryption, *not* payload sign/verify — don't conflate.
- Ports: `DEFAULT_CA_MTLS_PORT = 5642`, HTTP `5641` (koi's own; zen overrides koi's adapter to
  `KOI_HTTP = 7183`). DAT/`http_token` is `x-koi-token` header auth on koi's HTTP adapter (orthogonal
  to mTLS).
- Cards (`docs/reference/cards/`) carry a `validation: verified|drafted` status each.

---

## What this changes for the refactor plan

Two refinements to [koi-0.5-leverage-refactor-plan.md](koi-0.5-leverage-refactor-plan.md) Phase 1:

1. zen should react to posture via **`certmesh()?.core()?.watch_posture()`** (clean, coalesced,
   held-value) rather than the lossy broadcast — zen already holds the core.
2. The chirp-signing delegation's identity-resolution check is **confirmed and located**: koi's leaf
   is keyed on `hostname::get()` at `{data_dir}/certs/{OS-hostname}/`; verify `stone.name == OS
   hostname` (or stop assuming it) before `handle.sign()` uses koi's identity. Use the **sync**
   `koi_certmesh::envelope::{build_envelope, verify_envelope}` to avoid the async-in-sync-closure
   bridge in the p2p hooks.
