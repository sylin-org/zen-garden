# Koi agent prompt — graduated dual-mode security for the stack (discovery + transport + identity)

> Paste to a koi-repo agent. Drafted by the zen-garden side 2026-06-15 from an architecture realignment
> with the operator. **This is a vision + a set of locked decisions + open questions — NOT a spec to
> follow literally.** You have far more context on koi's internals (certmesh, mdns, udp, crypto, runtime)
> than the author; treat crate layout, exact APIs, the signing scheme, and the integration as **yours to
> design**. The deliverable is a koi-side ADR + the phase-one primitives, with consumers (zen-garden's
> moss/rake, and the C# sibling Koan) adopting afterward. Greenfield, pre-1.0: no shims, no deprecation.

## The vision: security is a dial, not a switch

The stack must operate in **graduated degrees of security**, opt-in, additive, with a **fully-open
default**. Nobody pays a security tax they didn't ask for; "more secure" is a deliberate step up.

| Degree | Transport | Discovery + requests | Stops | Status |
|---|---|---|---|---|
| **Open** (default) | HTTP | unsigned | nothing — zero-config homelab | the baseline experience, must stay zero-ceremony |
| **Authenticated** (pond on) | **HTTPS on the same port** | **signed** (+ freshness) | spoof / tamper / inject / redirect | **phase one — build this** |
| **Confidential** (pond + encryption) | HTTPS | signed **then encrypted** (group key) | + passive recon / topology mapping | **explicitly deferred** — design for it, don't build it |

Each rung is a strict superset of the one below. The default open experience is sacred: a brand-new
stone with no pond is plain HTTP + unsigned UDP and Just Works with no setup.

## Locked decisions (constraints — honor these)

1. **The auth primitive is SIGNING, not required-mTLS.** In pond mode the HTTP port serves **HTTPS**
   (server TLS — encryption + server identity), and **sender authentication is an application-layer
   signature** keyed on the pond identity and verified against the roster/CA. Rationale: the browser
   "portrait page" must load over HTTPS in pond mode, and **required client-cert mTLS breaks browsers**.
   Signing is uniform across HTTP requests AND UDP AND every client platform; TLS is just the encryption
   layer. (Note: koi recently shipped `koi_certmesh::mtls` with a *required* `WebPkiClientVerifier` — that
   primitive is likely NOT the path here, or at most an optional-encryption transport. Reconcile its fate
   as part of this — keep / repurpose / remove — your call.)

2. **Same-port protocol flip — no second listener.** Pond enable flips the existing port from HTTP to
   HTTPS (a mode switch on pond enable/disable), rather than standing up a parallel TLS port. (zen
   currently runs a separate :7183 mTLS listener alongside :7185 — that split goes away; the one port
   changes protocol with pond state. zen owns its listener; you own the contract + the TLS material.)

3. **Signed UDP with freshness, in a versioned, encryption-ready format.** Signatures alone are
   replayable; bind a timestamp/nonce (and ideally the sender's current cert serial) into the signed
   payload so stale/replayed messages are rejected. Version the message header so the **encryption rung
   can later wrap the already-signed payload without changing the signing scheme or the join door.**

4. **The join door is the one unauthenticated affordance.** A non-member's unsigned discovery is ignored
   by pond peers (no reply); its only bootstrap is a special "request to join" broadcast that **only
   cornerstones answer**. This is the entire unauthenticated attack surface — design it for replay/flood
   resistance. It must be constant across all degrees.

5. **Signed ≠ encrypted by default.** A non-member can *see* signed pond traffic (names, IPs, services,
   topology) but cannot interfere. Confidentiality is the separate, deferred encryption rung — not part
   of phase one. (See "deferred" below for why.)

6. **Store the degree as real booleans, not a profile abstraction.** The stored truth is `signed?` and
   `encrypted?` (open = neither; pond = signed; private = signed+encrypted). Named tiers
   ("open / secured / private") are **UX labels over the booleans** — the same pattern as the
   just-me/my-team presets you just flattened to. **Do not reintroduce a `TrustProfile`-style indirection
   — you deleted that in the certmesh diet for good reason; this must follow the same discipline.**

7. **The degree is a pond-level property, uniform across members, announced at join.** Peers must agree on
   the scheme to interoperate (a signing-only stone can't read an encrypted pond). So the degree is fixed
   at pond creation, inherited by members, and **announced as part of the pond's identity** (alongside the
   CA fingerprint) so clients learn it at discovery/join. A stone does not pick its own degree.

8. **Additive layers, not divergent code paths.** HTTPS layers over HTTP on the same port; signing wraps
   the message; encryption wraps the signed message. One pipeline with layers toggled — if it becomes
   three separate modes with their own branches, it's wrong.

## This is a cross-platform contract — publish it

The dual-mode model is a contract every client speaks, not a koi-internal detail: **"learn the pond's
degree at discovery/join → speak it (http/https + sign), or fail clearly."** Consumers are zen-garden's
**moss** (server side: flip the port, verify inbound signatures, expose caller identity to handlers) and
**rake**, **and the C# sibling Koan** (client side: discover degree, sign outbound, use HTTPS). So the
message formats, the degree announcement, the signing scheme, and the client expectations need to be a
**published koi contract** (versioned), implementable in Rust and C# alike — not buried in koi's binary.

> **Canon tension to resolve (flag, don't silently decide):** zen's stack canon (STACK-0001) currently
> says *"the UDP-7184 garden mesh is ZG-internal, never a cross-project contract."* But "Koan uses signed
> UDP" makes discovery cross-project. Either signed-UDP discovery becomes a koi-published contract (and
> STACK-0001 is amended), **or** Koan talks only the HTTP(S) API and signed UDP stays ZG-internal. This
> is an architecture-owner decision — surface it; recommend a direction; let the operator ratify.

## Open questions (your agency — design + recommend; escalate the strategic ones)

- **Browser mutations.** Reads over HTTPS are clean. The portrait page's *admin* actions have no signing
  key in the browser. Options: dashboard goes **read-only with mutations via rake** (cleanest), a browser
  **session** bootstrapped from operator identity, or browser client-cert import (heavy). Recommend one.
- **The signing scheme itself** — what fields are signed, which key (per-stone cert key? a derived pond
  key?), the verification path against the roster, key rotation on membership change. Yours to design.
- **Same-port flip mechanics** — listener rebind vs ALPN/peek; how a stone transitions live on pond
  enable/disable/drain. Define the contract; zen implements its listener to it.
- **`koi_certmesh::mtls` fate** — keep as optional encryption transport, fold into this, or remove.

## Deliverable (suggested shape — adapt freely)

1. A koi ADR establishing the graduated dual-mode model: the degrees, the additive-layer principle, the
   signing-as-primitive decision, the degree-as-booleans discipline, the join door, the deferred
   encryption rung, and the published-contract boundary (with the STACK-0001 reconciliation recommended).
2. The **phase-one** koi primitives, as library APIs consumers can use: signing/verification keyed on the
   pond identity (HTTP + UDP), the dual-mode discovery behavior (signed exchange + silence to outsiders +
   the join door), the degree announcement, and whatever moss needs to flip its port to HTTPS and read
   the authenticated caller identity in handlers. Crate layout + exact signatures are yours.
3. Encryption rung: **design the format/headers to admit it; do not build group-key management now.**
   Document it as the next rung.

## Constraints

- Pre-1.0 / greenfield: move and rebuild cleanly, no compat shims.
- Keep the **flatten-to-booleans** discipline (no profile object).
- The **open default must remain zero-config**; the join door must stay simple.
- koi gate green: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`.
- Name the consumers (zen moss/rake, Koan) only as adopters; this prompt does not change their repos.

## For the zen side (context, not your task)

Once this lands, zen replaces its dual-listener (:7185 + :7183) with the same-port flip, verifies inbound
signatures + reads caller identity via the koi API, and rake/Koan sign outbound + choose http/https by
degree. zen's in-flight prompt 05 (per-endpoint mTLS gating) is **subsumed** by this model and will be
re-scoped to "adopt the koi dual-mode primitive." Plans on the zen side:
`security-rake-mtls-plan.md`, `koi-mtls-library-prompt.md` (the now-superseded narrower step).
