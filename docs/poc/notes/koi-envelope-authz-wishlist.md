# Koi wishlist — envelope authz plane + embedded renewal (from Zen Garden)

> **STATUS: DELIVERED in koi 0.6.0 (2026-06-24).** Tier 1 `renew_member` (core_renewal.rs:245),
> Tier 2 `member_cert_expiry()` pub (core_renewal.rs:77) + top-level `KoiHandle::sign`/`verify`
> (handle.rs:297/305). zen re-verified green. Kept for the historical ask + rationale.

> Context for the koi maintainer. Written 2026-06-23 against koi 0.5.1 (path-dep dogfooding).
> Zen Garden is adopting the ADR-020 **envelope** plane as its dual-mode authorization layer
> and needs a small number of surfaces to use it from an **`ServiceMode::EmbeddedOnly`** consumer
> that runs koi *in-process* and never serves koi's own mTLS/HTTP listeners. None of these are
> blocking for the happy path — they remove the last places where we'd otherwise have to either
> run koi's standalone server stack or re-implement koi's security invariants.

## What Zen is building (so the asks make sense)

- **Clear, signed, per-request authorization.** No mTLS for auth, no user identity — the actor is
  the **stone's leaf CN** (pure infrastructure). In an *open garden* anyone may call any Moss
  unsigned; in a *pond* a request is honored only if it carries a valid envelope signature, else
  it's allowed for public commands or met with a graceful "rejoin" prompt.
- **Moss is the in-process signer.** rake holds no key and no certmesh dep: it asks its **local**
  Moss (loopback) to sign each request via `KoiHandle::sign`; the private key never leaves
  koi-embedded. Target Moss authorizes with `CertmeshCore::verify`.
- **Zen-native renewal over that same clear+signed channel** — because EmbeddedOnly doesn't serve
  koi's mTLS `:5642` renewal listener, we can't use koi's built-in pull loop as-is.

## What already works great (no change wanted — listed so it isn't "fixed")

- `KoiHandle::sign(bytes) -> Envelope` (handle.rs:926) → `CertmeshCore::sign` (core_identity.rs:149):
  per-request signing that returns an **unsigned freshness-stamped** envelope in Open posture and a
  real ES256 envelope in Authenticated — the consumer can't tell and doesn't have to. This is
  exactly the in-process signer we want. 🙏
- `CertmeshCore::verify(&Envelope) -> Assurance` (core_identity.rs:168) gathers pinned CA + revoked
  set + now internally; `Assurance::identity()` as the single trust door is perfect.
- `koi_common::envelope::*` wire types living in the **light** crate (a non-certmesh consumer can
  hold the types). Good split.
- Member-side custody: `prepare_member_csr` / `install_member_cert` (core_member.rs) are
  transport-agnostic and reusable. CA-side enroll: `core.enroll` (CSR-only) + `csr::sign_csr`.
- Events: `PostureChanged` + cert-lifecycle `KoiEvent`s are delivered to the embedded `.events()`
  callback; `spawn_posture_watcher` fires whenever `certmesh` is enabled (lib.rs:622), not gated on
  `certmesh_background` — so we get posture reactivity for free. `posture()` / `watch_posture()` /
  `certmesh_status()` cover introspection.

## Asks

### Tier 1 — needed for zen-native renewal (the one real gap)

**1. Transport-agnostic CA-side member renewal.**
Today the renewal authorization logic (member must be active + non-revoked; SANs pinned to the
enrollment record — "a renewal CSR cannot expand them"; policy lifetime; sign; roster update;
audit) is **inlined inside the mTLS `renew_handler`** (`koi-certmesh/src/http.rs:902`) and gated on
the TLS `ClientCn`. An embedded consumer that authenticates the caller over its *own* transport
can't reuse it without (a) standing up koi's mTLS listener, or (b) re-implementing koi's renewal
invariants — both bad.

Please expose it as a `CertmeshCore` method, e.g.:

```rust
/// Sign a rotate-key renewal for an ALREADY-AUTHENTICATED member.
/// The caller is responsible for authenticating `authenticated_cn` (mTLS CN today;
/// an envelope identity for embedded consumers). This method re-applies every CA-side
/// invariant — active + non-revoked, SANs pinned to the enrollment record, policy
/// lifetime — signs the CSR, updates the roster, audits, and emits CertRenewed.
pub async fn renew_member(
    &self,
    authenticated_cn: &str,
    csr_pem: &str,
) -> Result<protocol::RenewResponse, CertmeshError>;
```

Then koi's own `renew_handler` becomes a thin wrapper (`ClientCn` → `renew_member`), and zen can
offer renewal over the envelope plane by calling the same method after `verify().identity()`.

### Tier 2 — small ergonomics

**2. Public local-leaf expiry.** `cert_days_left_if_member()` is private
(`core_renewal.rs:74`). Expose it (or `member_cert_expiry() -> Option<DateTime<Utc>>`) so an
embedded consumer can drive its own renewal timer and a "next renewal in N days" status line
without re-parsing cert files.

**3. `KoiHandle::verify` symmetric with `KoiHandle::sign`.** `CertmeshCore::verify` exists;
surfacing it on the embedded handle (like `handle.sign`) saves the `handle.certmesh()?.core()?`
dance on the hot verify path. Trivial.

### Tier 3 — optional / forward-looking (only if cheap)

**4. Member-side renewal over an injected transport.** Complementary to #1: let
`renew_self_if_due` drive its POST through a consumer-supplied transport instead of the hard-coded
mTLS-to-`ca_mtls_authority()` (`core_renewal.rs:125`), so an embedded member could reuse koi's whole
rotate loop over a non-mTLS channel. #1 is the priority; this is the symmetric convenience.

## Non-asks (contract confirmations, not changes)

- `sign(bytes)` signs arbitrary caller-chosen bytes — we'll sign canonical `method + path + body`
  per request so a captured signature can't be lifted onto another op. Good as-is.
- koi keeps **no seen-nonce cache** (envelope.rs doc): replay defense within the ±300s window is the
  consumer's responsibility. Understood; we'll add a server-side single-use check for destructive
  ops if we decide we need it. No koi change wanted.
