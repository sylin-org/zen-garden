# Koi wishlist — strategically valuable surfaces for consumers to react to

> Paste to a koi-repo agent. Drafted 2026-06-22 from the zen-garden side, grounded in a code-level
> capability discovery of koi 0.5.0 (see the gap citations — they are real `file:line`, not guesses).
> **This is a vision + a prioritized set of opportunities, NOT a spec.** You own koi's internals,
> crate layout, exact APIs, and the trust model; treat everything here as *yours to design or decline*.
> The deliverable is whatever subset is strategically worth it, as koi-side primitives consumers
> (zen-garden's moss/rake, and the C# sibling Koan) adopt afterward. Greenfield, pre-1.0: no shims.

## Status — 2026-06-22: koi delivered 1.3 + 3.1 + 4.1 (commits b2cf976, a3f315a on koi `dev`)

- **3.1 DELIVERED** — `CertmeshCore::tls_client_config_for(peer) -> Option<rustls::ClientConfig>`
  (None=Open⇒plain, Some=secure⇒mTLS; shares `resolve_tls_config` with `select_client`) +
  `CertmeshHandle::reqwest_client_for(peer) -> reqwest::Client` (the ergonomic dual of `client_for`,
  carrying koi's transport policy while the consumer drives the full request surface — every verb,
  headers, SSE, large bodies). Single rustls in the lockfile makes the `use_preconfigured_tls` downcast safe.
- **4.1 DELIVERED** — `CertmeshCore::require_auth_with(router, policy: Fn(&str, &Request) -> bool)`
  (Open⇒no-op, secure+no-CN⇒401, secure+CN⇒policy; false⇒403). New `CertmeshError::Forbidden` →
  `ScopeViolation` (403). `require_auth` stays the zero-config default; K2-clean.
- **1.3 DELIVERED** — remote `KoiClient` now token-aware (`Builder::service_token()` wins, else the
  breadcrumb token is adopted when `endpoints_match`, never leaked to a foreign host) → unblocks every
  DAT-gated remote call; `CertmeshHandle::posture()` is now async and works in Remote mode (queries
  `/v1/certmesh/posture`). No wire change (the endpoint was already in trust-protocol.md §7).
- **5.4 DECLINED (correctly)** — CN==OS-hostname is a load-bearing machine-binding invariant; the
  supported lever is a separate `data_dir` per identity. Would need an ADR.
- Gate: fmt + clippy -D warnings + `cargo test --workspace --locked` green (+10 tests).
- **zen impact verified safe (additive):** `certmesh_err` has a `_=>` catch-all (pond.rs:246) so the
  new `Forbidden` variant compiles (currently → 500; add an explicit 403 arm when zen adopts
  `require_auth_with`); zen never calls `CertmeshHandle::posture()` (uses `certmesh_status()`); zen
  uses neither `client_for` nor `reqwest_client_for` yet → clean adopt surface. These commits do NOT
  change the pre-existing P0 (`koi-truststore`→`os-truststore`) that still gates zen's build.

Remaining open: Tier 1.1/1.2 (serde event DTO + events/posture SSE), Tier 2 (cert-lifecycle +
trust-bundle events), Tier 5 sharp edges.

## The framing (so this stays on koi's side of the line)

koi and zen are **distinct identities**. Everything below is about koi **exposing its own plane more
completely** — the trust/identity/discovery/posture state that is unambiguously koi's — so consumers
can *react* to it and *interoperate* with it. None of it asks koi to absorb a consumer's
responsibilities (container control, L7 routing, offering/host health — those stay zen's). The
through-line: **koi already computes this state internally; the wish is to make it observable and
actionable from outside the embedded process.**

What's already excellent and should NOT be redone: `sign/verify/seal/open` + the `Envelope`/
`Assurance` contract + conformance vectors; `posture()`/`watch_posture()`; `serve_adaptive`/
`participate` (the same-port live flip); `client_for`; `testkit::{open,secured}`. The wishlist is the
delta around these.

---

## Tier 1 — Keystone: make the reactive + trust surface real beyond the embedded process

Today the **entire event/posture/identity surface is embedded-only**. In `ServiceMode::Remote`/Auto,
`subscribe()`/`events()` return a live-but-permanently-empty stream (koi-embedded/src/lib.rs:325,334
return `new_remote` before any mapper spawns), and `CertmeshHandle::posture()/local_identity()/
diagnose()` all return `DisabledCapability` in Remote (handle.rs:839-917; the docstring itself says
posture-over-the-wire "arrives in a later ADR-020 phase"). That single boundary is the highest-value
thing to move — it's what unlocks rake, Koan, and browsers, not just an in-process Rust host.

**1.1 — A serde-able event/posture wire DTO.** `KoiEvent` derives only `Debug, Clone`
(koi-embedded/src/events.rs:6) — not `Serialize` — so every consumer must hand-map 14 variants
before it can cross a wire. A versioned, `Serialize`/`ToSchema` `KoiEventWire` (or making the
underlying domain events the wire format, as several already are) is the **enabling primitive** for
everything else in this tier. *Strategic value:* one contract instead of N hand-mappings; it becomes
the cross-platform event shape Koan reads.

**1.2 — An events + posture SSE endpoint on koi's HTTP adapter (DAT-gated).** Expose the unified
stream (or a curated subset) over `GET /v1/events` and the current posture over `GET /v1/certmesh/
posture`, both behind the existing `x-koi-token`. *This is the "we can totally expose events" you
named.* *Strategic value:* **any** consumer reacts to koi's state without embedding it — rake can
show live trust state, a browser dashboard can react, Koan can subscribe over HTTP. It turns the
reactive surface from an in-process Rust-only feature into a stack-wide contract. (The dashboard SSE
already forwards events internally — this is exposing that as a stable, documented consumer
endpoint.)

**1.3 — Posture readable + watchable by a remote handle.** With 1.2 in place, let
`CertmeshHandle::posture()` (and ideally a `watch`-like SSE) work in Remote mode instead of
`DisabledCapability`. *Strategic value:* a remote rake/Koan client learns a stone's degree (open vs
authenticated) to decide http-vs-mTLS *before* dialing — the client-side half of the dual-mode
contract.

---

## Tier 2 — Emit the trust-lifecycle events koi already knows but doesn't broadcast

koi computes all of this internally; it just isn't an event a consumer can react to.

**2.1 — Certificate lifecycle events:** `CertRenewed{expires_at}`, `CertExpiringSoon{days_left}`,
`CertRenewalFailed{reason, consecutive_failures}`. Today there is **no** renewal/expiry event — the
only identity signal is `PostureChanged` *degrade*, which fires **after** identity is already lost; to
catch "expires soon" a consumer must poll `diagnose()`/`local_identity().renewal` on a timer, and
`RenewalHealth` has no attempt telemetry (last-attempt/failure-streak are "a later increment" per
koi-certmesh/src/lib.rs:176). *Strategic value:* zen/lantern surface renewal health and alert
*before* a stone silently falls back to Open — the difference between proactive and post-mortem.

**2.2 — Trust-bundle / roster-change event:** emit when `pull_trust_bundle` updates policy or the
revocation set (today `BundleOutcome` is only a return value, koi-certmesh/src/core_renewal.rs:156).
*Strategic value:* a member reacts immediately to "I was revoked" or "the 90/30/14 policy changed"
instead of discovering it on the next poll. Pairs with the CA-side `CertmeshMemberJoined/Revoked` that
already exist.

**2.3 — Node lifecycle events:** `Ready` (HTTP adapter bound, cores up) and `ShuttingDown`. Today
readiness is observed out-of-band (`bound_http_port()` after `start()`) and shutdown is a method, not
an event. *Strategic value:* a consumer sequences its own boot/drain off koi's cleanly. (Lower weight
than 2.1/2.2.)

---

## Tier 3 — Client-side mode-transparency parity (the biggest "adopt" opportunity for zen)

`serve_adaptive` gave the **server** side beautiful plain↔mTLS transparency. The **client** side —
`PeerClient` from `client_for` — is the dual, but it's currently too thin to route real traffic
through: **GET + JSON-POST only, body buffered to 4 MiB, no PUT/DELETE/PATCH/HEAD, no custom headers,
no streaming/SSE** (koi-certmesh/src/client.rs:31-115). zen's inter-stone surface is exactly the
opposite shape — it's REST (PUT/DELETE on storage, env, banks), SSE (storage/log/replication
streams), and large transfers (snapshot/object reads). So today zen *cannot* route its highest-volume
calls through koi's mode-transparency and falls back to its own client, losing the very benefit.

**3.1 — A fuller posture-keyed client:** either extend `PeerClient` to the full verb set + headers +
a streaming/SSE response mode, **or** expose a way to obtain a *configured* transport (e.g. a
posture-selected `reqwest::Client` / a tower service / the rustls `ClientConfig` already built by
`client_for`) that the consumer drives with its own request builder. *Strategic value (high):* this
is what lets zen run **all** inter-stone traffic — not just trivial GETs — through one mode-transparent
client, the client-side keystone of the dual-mode vision. You decide the shape; the requirement is
"REST + SSE + large bodies, posture-keyed."

---

## Tier 4 — Authorization expressiveness

**4.1 — A CN/role policy hook on `require_auth`.** Today `require_auth` is binary: no-op while Open,
401 once secured, gating on the **presence of any mesh CN** — not a specific allowlist or role
(koi-certmesh/src/http.rs:1120; the policy hook is documented as "planned"). *Strategic value:* zen's
write-authz story (the pond "only these stones may write" requirement) needs "which CN/role," not
just "any member." A `require_auth_with(policy: impl Fn(&ClientCn, &Request) -> Decision)` (or
role-based variant reading the roster) lets zen express fleet authz without re-implementing the
middleware and re-deriving roles. Keep the zero-config "any member" as the default.

---

## Tier 5 — Smaller sharp edges (close if cheap; each is a real gap)

- **5.1 Surface `serve()` bind failure.** `KoiHandle::serve` swallows the bind `io::Error` into a log;
  the returned `JoinHandle` resolves anyway (handle.rs:132) — a consumer can't tell its listener never
  came up. A `try_serve` returning the bind `Result`, or a `ListenerFailed` event, fixes it.
  (`serve_adaptive` already returns `io::Result` — just plumb it through the handle.)
- **5.2 A direct `handle.on_posture()` watch.** Reaching the clean coalesced watch currently means
  `certmesh()?.core()?.watch_posture()`. A first-class `on_posture() -> watch::Receiver<Posture>` on
  the handle (the `on_{event}()` pattern) is the ergonomic match. Minor vs 1.2, but cheap.
- **5.3 A serde projection of `Identity`.** `Identity` isn't `Serialize` (redacted Debug only,
  koi-certmesh/src/lib.rs:142), so a consumer re-exposing "who am I" hand-maps it. A key-redacting
  `IdentityInfo` DTO (hostname, ca_fingerprint, renewal) would be re-exposable verbatim.
- **5.4 Optional cert-dir override.** The leaf is keyed strictly on `hostname::get()` at
  `{data_dir}/certs/{OS-hostname}/` (koi-certmesh/src/certmesh_paths.rs:119, lib.rs:455). A consumer
  whose node name diverges from the OS hostname has no override (only `data_dir` is injectable). An
  explicit identity-name parameter would decouple koi's identity from the OS hostname. *(This one is
  borderline — it may be intentional that CN == OS hostname; flag it, your call.)*
- **5.5 Prune the 3 dead `RejectReason` variants** (`NoSignature`, `ClockSkew`, `NameMismatch`) the
  verifier never produces, or wire them — today an out-of-window timestamp returns
  `Authenticated{Stale}`, never `ClockSkew`, which is surprising for a cross-impl reader.

---

## Deliberately NOT on this list (respecting the boundaries)

- **A replay/nonce cache for chirp payloads.** koi's `Envelope` is freshness-only (±300s, no seen-
  nonce cache). True replay defense on the UDP-7184 gossip is arguably **zen's** to build, because
  STACK-0001 D4 keeps that mesh ZG-internal. If you'd rather offer a reusable opt-in nonce-guard as a
  koi primitive, that's welcome — but it's a genuine fork, not an obvious ask. Flagging, not
  requesting.
- **Anything that makes koi own a consumer's plane** (container lifecycle, L7 routing, offering
  health). Out of scope by the distinct-identities rule.

## Cross-platform note (for Koan)

Tier 1 (the serde event DTO + the SSE/posture HTTP surface) and the existing envelope/posture wire
contract together are what let **Koan** react to and interoperate with a koi stone over HTTP without a
Rust embed. If any of Tier 1 lands, please extend `docs/reference/trust-protocol.md` + the conformance
vectors to cover the event/posture wire shape so Koan implements to a spec, not to koi's binary.

## Priority, if you only do a few

1. **1.1 + 1.2** (serde event DTO + the events/posture SSE) — the keystone; unlocks every other
   consumer and the whole "react to koi" story you proposed.
2. **3.1** (fuller posture-keyed client) — the highest-value *adopt* for zen; without it the
   client-side dual-mode is unreachable for zen's real traffic.
3. **2.1** (cert lifecycle events) — proactive trust health.
4. **4.1** (CN/role authz hook) — unblocks zen's pond write-authz cleanly.
