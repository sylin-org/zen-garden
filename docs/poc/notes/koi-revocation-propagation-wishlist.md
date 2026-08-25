# koi wishlist — revocation propagation to members

**Context:** zen-garden adopts a **binary "membership = enforcement"** model (SECURITY-0005,
in progress): a stone verifies signed koi envelopes and rejects non-member control-plane mutations
*iff* it holds a pond identity; an open stone is permissive. Membership becomes the **sole** trust gate.

**Problem this exposes (pre-existing koi behavior, now load-bearing):** koi `verify`
(`crates/koi-certmesh/src/core_identity.rs`) honors revocation only via the **local roster's** Revoked
fingerprints (`revoked_fingerprints()`), and koi's own doc states a pure member's roster is empty —
*"revocation there is eventual-consistent; the CA chain remains the hard gate."* So when zen untrusts a
stone (`core.revoke_member` on the cornerstone), **only the cornerstone rejects it**. Every other member
keeps trusting the revoked stone's envelopes (its leaf still chains to the CA), and the revoked stone keeps
both enforcing and signing. With membership as the sole gate, untrust must become effective fleet-wide.

The operator chose **"propagate revocation to members"** as the fix (2026-06-24).

## What zen needs from koi (minimal surface)

zen already moves trust state member↔cornerstone over its **clear-signed plane** (the `/pond/renew`
pattern: member signs an envelope, cornerstone verifies via `identity_for`). Revocation propagation reuses
that — so koi does **not** need its own transport; it needs only **export** (CA side) and **ingest**
(member side):

1. **CA-side export** — a public accessor for the authoritative revoked set on a CA node. Today
   `revoked_fingerprints()` is private `async`. Wish:
   ```rust
   /// The revoked leaf fingerprints in this node's roster (CA node holds the full set).
   pub async fn revoked_fingerprints(&self) -> Vec<String>;
   // or richer, if hostnames/timestamps are useful to surface:
   pub async fn revocations(&self) -> Vec<RevokedMember>;
   ```
   Lets the cornerstone serve the current list.

2. **Member-side ingest** — a way for a member (empty roster) to record externally-learned revoked
   fingerprints so its local `verify` rejects them. Wish:
   ```rust
   /// Replace this node's known-revoked set with the authoritative list learned from the CA.
   /// Idempotent; full-replace (not additive) so un-revocation is also reflected. Populates the
   /// roster's revocation list so `verify` -> `revoked_fingerprints()` honors them on a member too.
   pub async fn set_revoked(&self, fingerprints: &[String]) -> Result<(), CertmeshError>;
   ```
   Full-replace (authoritative) is preferred over additive so a corrected/un-revoked entry also clears.

**Authenticity is handled by zen, not koi:** the member fetches the list over the clear-signed plane and
**verifies the cornerstone's envelope signature** (via the CA anchor it already trusts) before calling
`set_revoked`. koi only needs the two plain accessors above — no signing/transport logic.

## zen side (no koi dependency beyond the two APIs)

- **New endpoint** `GET /api/v1/pond/revocations` on the cornerstone → returns `revoked_fingerprints()`,
  response **signed** (clear-signed plane, like `/pond/renew`). It's a read, but signed so the member can
  authenticate it.
- **Pull on the existing `CertRenewalTask` tick** (`tasks/task_defs/cert_renewal.rs`: 60s post-boot, then
  hourly): each member, after its renewal check, pulls `/pond/revocations` from the cornerstone, verifies
  the envelope, and calls `core.set_revoked(...)`. Eventual-consistency is bounded by the tick interval
  (hourly) — acceptable for a homelab pond; a chirp-triggered immediate pull can be a later refinement.
- Bootstrap allowlist: `/api/v1/pond/revocations` is under `/api/v1/pond/*` → already in the enforce
  bypass (a member without a fresh identity can still pull), consistent with `/pond/renew`.

## Open questions for koi

- Does koi already expose an equivalent of either accessor (roster import/export, or a CRL surface)?
  Check before adding — don't reinvent.
- Should `set_revoked` also refuse/clear the node's **own** identity if it finds its own fingerprint in
  the list (self-revocation awareness)? That would make a revoked stone stand itself down on next pull —
  a useful complement (though not a defense against a hostile stone that simply doesn't pull).

## Not in scope

- Wiping the revoked stone's on-disk leaf remotely (operator chose propagation, not the drain-style option).
- A real-time push/CRL distribution-point — the renewal-tick pull is the dogfood-minimal mechanism.
