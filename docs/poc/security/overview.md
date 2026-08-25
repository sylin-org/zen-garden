# Security Overview

**Threat models, security guarantees, and when to use Pond**

**Purpose**: Understand Zen Garden's security model and what it protects  
**Audience**: Security, Visitor, Operator

---

## Contents

- [Security Philosophy](#security-philosophy)
- [Threat Model](#threat-model)
- [Pond Security](#pond-security)
- [What Pond Protects](#what-pond-protects)
- [What Pond Does NOT Protect](#what-pond-does-not-protect)
- [When to Use Pond](#when-to-use-pond)

---

## Security Philosophy

**Pragmatic, not paranoid** - Security should protect against realistic threats, not theoretical ones. Home labs face different risks than enterprises.

**Visible, not hidden** - Users should understand security status at a glance. No security through obscurity.

**Recoverable, not fragile** - When issues occur, clear remediation paths. No unrecoverable states.

**Frictionless by default** - Zero configuration for common use cases. Security shouldn't require a PhD.

### Design Principle

> "Security that requires reading documentation is security that won't be used. Make it visible, understandable, and recoverable."

Every security feature includes:
1. **Prevention** (Technical) - Cryptographic/protocol-level security
2. **Detection** (Monitoring) - Visual feedback when issues occur
3. **Recovery** (User-Facing) - Clear steps to fix problems

---

## Threat Model

### Home Lab Reality

**Environment**:
- Solo admin or small trusted team
- 2-10 Stones on local network
- Physical security assumed (home/office)
- Trusted users (family, colleagues)

**Realistic Threats**:

| Threat           | Likelihood | Impact | Mitigation Priority              |
| ---------------- | ---------- | ------ | -------------------------------- |
| User mistakes    | HIGH       | Medium | **P0** - Safety nets, rollback   |
| Network sniffing | MEDIUM     | Medium | **P0** - Encryption              |
| Physical theft   | LOW        | High   | **P1** - Passphrase + encryption |
| Malware on Stone | LOW        | High   | **P1** - Isolation, monitoring   |
| Insider attack   | VERY LOW   | High   | **P2** - Audit logs              |
| Nation-state     | NEGLIGIBLE | N/A    | Not addressed                    |

**Accepted Risks**:
- Single admin can break things (trust model)
- Keystone extractable with physical access + weak passphrase
- Network partition may cause temporary inconsistency
- Time manipulation possible if router compromised

**Philosophy**: Protect against accidents and common attacks, not nation-states.

---

## Pond Security

**Pond** — Optional security layer for encrypted Stone-to-Stone communication using CA-based mTLS (ECDSA P-256 certificates via koi-certmesh).

**Philosophy**: "Set your stones, make sure everything is working, fill the pond."

Users start without Pond (frictionless), then enable security when ready:
```bash
garden-rake pond init --passphrase "my-secure-pass" --profile just-me
```

### Bluetooth Pairing Model

Inspired by Bluetooth device pairing - familiar UX, proven security model.

```
Bluetooth Pairing          →  Pond Join
─────────────────────────────────────────
1. Put device in pairing mode  →  garden-rake invite stone
2. Shows 6-digit code          →  Displays TOTP code locally
3. Type code on other device   →  Type code on new Stone
4. Devices paired              →  Stone joined pond
```

**Security team rating**: 9.5/10

### Components

**Keystone** — Encrypted CA private key stored on the cornerstone. Protected by passphrase encryption (AES-256-GCM via Argon2id KDF). Only the cornerstone (and promoted standby stones) hold the CA private key.

**Cornerstone** — Stone that initialized the pond (ran `pond init`). Holds the CA private key and acts as the certificate authority — the trust anchor for the pond.

**mTLS** — All inter-Stone communication uses mutual TLS with ECDSA P-256 certificates issued by the pond CA. When pond is active, Moss binds HTTPS on port 7183.

**TOTP Codes** — Time-based one-time passwords for Stone admission. 6 digits, 30-second period (SHA1), configurable enrollment window (default 30 minutes).

---

## What Pond Protects

### ✅ Protection Provided

**1. Encryption** — Protect traffic from network sniffing
```
Without Pond: HTTP API plaintext, traffic visible
With Pond: mTLS (ECDSA P-256) encryption for all inter-Stone communication
```

**2. Admission Control** - Control which Stones can join Garden
```
Without Pond: Any device on network can join automatically
With Pond: Administrator approves each Stone (TOTP code)
```

**3. Authentication** - Verify Stone identity (prevent rogue devices)
```
Without Pond: Any device can announce "I offer MongoDB"
With Pond: Only Stones with valid pond credentials trusted
```

**4. Outsider Detection** - Identify non-member stones
```
Stones without pond credentials receive "join_required" signal
Clear indication of which stones need invitation
```

**5. Visual Security Status** - Understand security posture at a glance
```bash
garden-rake status --security

Pond Status: Active
Cornerstone: stone-01
Stones: 4 joined, 0 pending
Last Audit: 2 events (last 24h) - view with 'audit show'
```

---

## What Pond Does NOT Protect

### ❌ Not Protected

**1. Physical Access** - If attacker has physical access to any pond stone:
```
Risk: Extract Keystone file, brute-force weak passphrase
Mitigation: Strong passphrase (20+ chars), Argon2id key derivation
```

**2. Time Manipulation** - If attacker controls network time (NTP):
```
Risk: TOTP replay attacks (valid codes reused)
Mitigation: Used codes tracking, timestamp validation
```

**3. Malware on Stone** - If attacker compromises Stone OS:
```
Risk: Extract private key, impersonate stone
Mitigation: OS hardening, container isolation
```

**4. Network Partition** - If network splits (split-brain):
```
Risk: Temporary inconsistency
Mitigation: Read-only mode during partition
```

**5. Nation-State Attacks** - Advanced persistent threats (APT):
```
Scope: Out of scope (home lab focus)
Reality: Home labs are not APT targets
```

**6. Insider Attack** — If trusted admin goes rogue:
```
Risk: Full access to all systems (CA model — cornerstone controls trust)
Mitigation: Cornerstone passphrase required for key operations. Use Drain if trust is broken.
```

---

## When to Use Pond

### Use Pond When...

✅ **Running production workloads** - Sensitive data, uptime requirements  
✅ **On untrusted networks** - Public WiFi, shared office networks  
✅ **Rogue device risk** - Open network where unknown devices can join

### Don't Need Pond When...

❌ **Home lab experimentation** - Learning, testing, non-sensitive data  
❌ **Strong physical security** - Locked office, trusted users only  
❌ **Same local network** - All devices on home LAN with firewall  
❌ **Solo administrator** - You control all Stones, trust yourself  
❌ **Non-sensitive data** - Test databases, development environments

### Migration Path

**Start simple, add security when needed**:
1. Deploy Stones without Pond (get familiar)
2. Validate discovery and services working
3. Enable Pond when ready: `garden-rake pond init --passphrase "my-pass"`
4. Existing services continue (zero downtime)
5. All pond traffic encrypted automatically

---

## Related Documentation

- **[Pond Setup Guide](pond-setup.md)** - Enable Pond, join Stones
- **[Pond Protocol Specification](../specs/POND-0001-protocol.md)** - Complete protocol design
- **[Threat Analysis](threat-analysis.md)** - Detailed attack scenarios

---

**Last Updated**: 2026-02-16
