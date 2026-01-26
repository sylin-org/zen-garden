---
status: Accepted
date: 2026-01-26
---

# SECURITY-0004: Tier 2 (Deep Pond) Deferred Until Demand Exists

## Status

**Accepted** - Tier 2 documented but implementation deferred indefinitely.

## Context

During security review of POND-0001 protocol specification, we evaluated whether Tier 2 (Deep Pond) provides sufficient value to justify implementation effort.

### Original Tier 2 Design

Tier 2 was designed for "enterprise" environments with:
- Hierarchical CA (only cornerstone holds private key)
- Short-lived certificates (1 hour TTL, 30-minute auto-renewal)
- Individual stone revocation
- mTLS per-connection mutual authentication
- Resurrection protocol for offline stones with expired certificates

### The Question

**Does Tier 2 solve a real problem for real users?**

## Analysis

### Who Would Use Tier 2?

| Potential User | Stone Count | Reality |
|----------------|-------------|---------|
| Home lab | 2-10 | Tier 1 is sufficient |
| Small business | 10-30 | Tier 1 + VPN is sufficient |
| Enterprise | 50+ | Would use Kubernetes, Nomad, or commercial solutions |

**Finding:** Tier 2 exists in a market gap that may not have customers.

### Complexity vs. Security Gain

| Feature | Complexity | Security Gain |
|---------|------------|---------------|
| 1-hour certificate TTL | High (resurrection protocol, offline handling) | Marginal (attacker with cert also has identity key) |
| Cornerstone-only signing | Medium (single point of failure) | Low (enterprises want HSM backing anyway) |
| Individual revocation | Medium | Low for <10 stones (re-baptize takes 30 seconds) |
| mTLS per-connection | High | Low (UDP already encrypted with same keys) |

### What Tier 1 Already Provides

Tier 1 is not "weak security." It provides:

- **XChaCha20-Poly1305 encryption** (same as WireGuard)
- **Ed25519 signatures** (same as SSH, Signal)
- **Replay protection** (nonce tracking)
- **Forward secrecy** (ephemeral X25519)
- **Authenticated membership** (TOTP invitation)

For home labs, this blocks 95%+ of realistic threats:
- Network sniffing → Encrypted
- Unauthorized devices → Needs TOTP invitation
- Replay attacks → Nonce rejected
- Compromised captures → Forward secrecy

### Honest Assessment

**Tier 2 is over-engineered for the actual user base.**

The short-lived certificate model (1-hour TTL) creates:
- Constant certificate churn
- Resurrection protocol complexity
- More failure modes
- Marginal security improvement

If an attacker has a valid certificate, they already compromised a stone. The stone's identity key is also compromised. They can resurrection-rejoin. Short TTL buys a ~1 hour detection window at best.

Most real-world enterprise PKI uses 24-hour to 90-day certificates. 1-hour is overkill.

## Decision

**Defer Tier 2 implementation until real demand exists.**

### Immediate Actions

1. ✅ Document Tier 2 design in POND-0001 (preserve knowledge)
2. ✅ Mark Tier 2 sections as `FUTURE` (not for v1.0)
3. ✅ Focus implementation on Tier 1 (ship what's valuable)
4. ✅ Update roadmap to remove Tier 2 timeline

### Future Triggers

Revisit Tier 2 if:
- User requests with specific requirements (not hypothetical)
- Enterprise adoption blocked on missing features
- Compliance framework explicitly requires hierarchical CA

### What We Preserve

The POND-0001 specification documents:
- Tier 2 architecture (for future reference)
- Certificate model design
- Resurrection protocol
- Revocation mechanisms

This knowledge is not lost—just not implemented.

## Consequences

### Positive

✅ **Ship faster**: Focus on Tier 1 implementation  
✅ **Less code to maintain**: No resurrection protocol, no certificate renewal  
✅ **Simpler operations**: No cornerstone single-point-of-failure  
✅ **Honest product**: Solve real problems, not hypothetical ones  
✅ **Preserve option value**: Design documented, can implement later

### Negative

❌ **No enterprise tier at launch**: May limit adoption in regulated environments  
❌ **Documentation describes unimplemented features**: Could confuse readers

### Mitigations

- Clearly mark Tier 2 as `FUTURE` in all documentation
- Tier 1 + external hardening (VPN, firewall) acceptable for enterprise interim
- Collect user feedback to guide if/when Tier 2 is needed

## Alternatives Considered

### Alternative 1: Implement Tier 2 as Designed

**Why not:** High effort, uncertain demand, over-engineered for likely users.

### Alternative 2: Simplify Tier 2 to "Tier 1 + Restricted Invites"

**Approach:** Keep shared secret model, but only cornerstone can invite.

**Consideration:** This could be a reasonable middle ground. May revisit.

**Why deferred:** Even this simpler version requires additional code paths. Wait for demand.

### Alternative 3: Remove Tier 2 from Documentation Entirely

**Why not:** Preserving the design has value. Future contributors can reference it.

## References

- [POND-0001-protocol.md](../specs/POND-0001-protocol.md) - Protocol specification
- [SECURITY-0001-pond-tiers.md](SECURITY-0001-pond-tiers.md) - Original two-tier decision
- [security.md](../specs/security.md) - Security specification

## Review

**Reviewed by:** Security analysis session 2026-01-26  
**Conclusion:** Tier 1 provides real security value. Tier 2 solves theoretical problems for users who would choose different tools.
