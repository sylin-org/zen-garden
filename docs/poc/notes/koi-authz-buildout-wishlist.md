# Koi wishlist — clear-signed authz plane + renewal buildout (round 2)

> **STATUS: DELIVERED in koi 0.7.0 (ADR-022, tag v0.7.0).** M1 `Assurance::identity_for(env, expected)`,
> N1 `Rejected { reason, signer_cn }`, N3 pub leaf parsers, N4 `RenewResponse.policy` + N5 doc-steer
> all landed. zen migrated 0.6.0→0.7.0 and adopted M1 (request-binding door at `/renew`, replacing the
> hand-rolled bind-check) + N1 (by-name welcome-back) — commit `a1bce117`, green. N2 (single-use nonce
> cache) was not taken; zen will build per-nonce single-use for destructive ops in its Stage 4
> enforcement. Kept below for the rationale.

> Context for the koi maintainer. Written 2026-06-24 against koi **0.6.0** (path-dep dogfooding),
> while Zen Garden builds its dual-mode authorization plane on the ADR-020 **envelope** primitive
> and a **zen-native renewal** loop over the same clear+signed channel.
>
> **Headline: there is no hard blocker.** koi 0.6.0 has everything the plane needs. This round is
> "make the secure path the easy path" + observability — every item is optional. The previous
> wishlist (`koi-envelope-authz-wishlist.md`, delivered in 0.6.0) covered the API surface; this one
> is the *ergonomics and footgun* findings from actually wiring it up.

## What we built on (works as-is — please don't "fix")

- **`KoiHandle::sign` / `CertmeshCore::sign` → `Envelope`** — the in-process, mode-transparent signer
  is exactly right. Open posture → unsigned freshness stamp; Authenticated → ES256 with carried leaf.
  rake holds no key: it asks its local Moss to sign over loopback. 🙏
- **`CertmeshCore::verify(&Envelope) → Assurance`** + `Assurance::identity()` as the one trust door.
- **`CertmeshCore::renew_member(authenticated_cn, csr)`** (core_renewal.rs:245) — transport-agnostic
  CA-side renewal taking a *pre-authenticated* CN. Pairs perfectly with envelope auth: we
  `verify → identity() → renew_member(cn, csr)`. SAN-pinning + roster checks inside. Ideal.
- **`CertmeshCore::local_identity() → Identity { renewal: RenewalHealth }`** (core_identity.rs:76,
  lib.rs:208) — this is the one we want to call out. `renewal.{expires_at, expires_in_days,
  renew_overdue, expired}` is **cert-derived** and works for a member that never armed `member.json`
  (the policy falls back to the local roster/default — only `next_renewal_at` depends on the
  threshold; the expiry facts do not). This neatly removed our need to either arm `member.json`
  (which would imply koi's mTLS pull-renewal — which our `EmbeddedOnly` cornerstone doesn't serve)
  or parse the leaf ourselves. **Thank you for putting `renewal` on `Identity`.**
- **Events** — `PostureChanged` + `CertRenewed`/`CertExpiringSoon`/`CertRenewalFailed` on `.events()`
  let us route trust lifecycle into our own bus without a polling loop.

## Must-have (safety-by-default — not a blocker, but the highest-value ask)

### M1 — Fold the request-binding check into verification: `Assurance::identity_for(expected)`

`verify()` deliberately attests the **signer**, decoupled from the payload — correct, since koi can't
know a consumer's canonicalization. But that leaves a **silent-impersonation footgun**: the obvious
code

```rust
if core.verify(&env).await.identity().is_some() { authorize(request) }   // VULNERABLE
```

authorizes a *captured* envelope replayed against a *different* request. We hit the sharp edge
directly building `/pond/renew`: an attacker who captured **any** signed envelope from member-A could
POST it to `/renew` with their **own** CSR and obtain a CA-signed cert for A's identity bound to a key
they hold — full impersonation — unless the verifier *also* checks that the envelope's payload equals
the canonical bytes of *this* request (which embed a hash of the body, i.e. the CSR). We do that
check by hand today (decode `env.payload`, compare to our `canonical_request_bytes`).

The ask keeps koi payload-agnostic but makes the safe path the default — the consumer supplies the
bytes it expected to be signed:

```rust
impl Assurance {
    /// `Some(cn)` iff Authenticated + Fresh **and** the envelope's signed payload
    /// equals `expected`. The only safe door for request authorization.
    pub fn identity_for(&self, env: &Envelope, expected: &[u8]) -> Option<&str>;
}
// or, symmetric with sign:  core.verify_bound(&env, expected) -> Assurance
```

This matches koi's existing "one identity door / no footgun" philosophy (envelope.rs §13) — it just
extends it to the request-binding dimension, which is the one a consumer is most likely to get wrong
and where the failure is silent. **If only one item lands, this is it.**

## Nice-to-have

### N1 — `RejectReason` should carry the attempted signer's CN (when the leaf parses)

`Assurance::Rejected { reason }` drops identity. For audit and a *warm* welcome-back we want to name
the stone: "stone-mossy-brook's identity expired — rejoin." Today an Expired/Revoked envelope still
carries a parseable leaf (CN readable) but the verdict discards it.

```rust
Assurance::Rejected { reason: RejectReason, signer_cn: Option<String> }
```
`signer_cn` = `Some` when the carried leaf parsed (Expired/Revoked/known-but-stale), `None` for
Malformed/UnknownSigner-that-won't-parse. Lets the CA log who, and lets a "rejoin" prompt greet the
stone by name.

### N2 — Opt-in bounded single-use (nonce) cache for replay defence

koi is explicit that it keeps **no** seen-nonce cache — replay within the ±300s window is the
consumer's problem. Every envelope consumer that has destructive operations must therefore build the
same bounded, time-windowed `seen-nonce` set. A koi-provided opt-in would standardize it and stop
each consumer reinventing (and under-building) it:

```rust
// A bounded cache keyed on (signer_cn, env.nonce), evicting entries older than the
// freshness window. `verify_single_use` = verify() + reject a nonce seen before.
core.verify_single_use(&env).await -> Assurance   // Rejected { reason: Replayed } on reuse
```
We'll build this in Zen for destructive ops regardless; offering it in koi (where the freshness
window already lives) would be the natural home.

### N3 — Make the stateless leaf parsers public

`leaf_not_after_utc(pem)` (lib.rs:522) and CN extraction (`mtls::extract_cn`) are crate-private.
`local_identity().renewal` covers our node's *own* leaf, but a consumer occasionally needs to read an
*arbitrary* leaf (a peer's cert from discovery, a cert pasted by an operator) without a full verify.
Two small `pub` free functions would cover it:

```rust
pub fn leaf_not_after_utc(cert_pem: &str) -> Option<DateTime<Utc>>;
pub fn leaf_cn(cert_pem: &str) -> Option<String>;
```

### N4 — Propagate the CA's cert policy to members so `RenewalHealth.next_renewal_at` is accurate

For a member without `member.json`, `local_policy()` falls back to a default `renew_threshold_days`,
so `renewal.next_renewal_at` / `renew_overdue` can disagree with the CA's actual policy (the
expiry/`expired` facts are correct; only the *threshold-derived* fields drift). If the enroll/renew
response carried the CA's `CertPolicy` (or just `renew_threshold_days`), a member could compute an
accurate renewal schedule. Minor — we can use our own threshold — but it would make the reactive
"renews in N days" line authoritative.

### N5 — Doc note: `member_cert_expiry()` vs `local_identity().renewal`

`member_cert_expiry()` (core_renewal.rs:77) is `member.json`-gated, so it returns `None` for a
consumer that intentionally doesn't arm `member.json` (us). We reached for it first and were briefly
misled. Either a one-line doc steer ("for own-leaf expiry independent of member state, use
`local_identity().renewal`") or having `member_cert_expiry()` fall back to the local leaf would save
the next consumer the detour.

## Explicit non-asks (so they aren't "fixed")

- **koi's mTLS pull-renewal loop** (`renew_self_if_due`, `.certmesh_background(true)`): we don't use
  it. Our `EmbeddedOnly` cornerstone serves no mTLS `:5642` authority, and we drive renewal over our
  own clear+signed `/api/v1/pond/renew`. `local_identity().renewal` + `renew_member` are the only
  renewal surfaces we need.
- **Confidentiality / `seal`**: the sign-only `Posture { encrypted: false }` is fine; we want
  authenticity, not secrecy, on the LAN plane.
