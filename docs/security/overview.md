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

**Pond** - Optional security layer for encrypted Stone-to-Stone communication using P2P shared-secret model.

**Philosophy**: "Set your stones, make sure everything is working, fill the pond."

Users start without Pond (frictionless), then enable security when ready:
```bash
garden-rake place keystone
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

**Keystone** - Encrypted file containing Pond CA keypair (shared secret). All pond members hold the complete keypair (P2P trust model).

**Cornerstone** - Stone that initialized the pond (ran `place keystone`). Ceremonial role—not a control point. Any pond member can invite new stones.

**Encrypted UDP** - All pond traffic uses XChaCha20-Poly1305 encryption. Strong, fast, modern cryptography (same primitives as WireGuard).

**TOTP Codes** - Time-based one-time passwords for Stone admission. 6 characters, 5-minute window, validated locally (never transmitted).

---

## What Pond Protects

### ✅ Protection Provided

**1. Encryption** - Protect traffic from network sniffing
```
Without Pond: UDP announcements plaintext, traffic visible
With Pond: XChaCha20-Poly1305 encryption for all inter-Stone communication
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

**6. Insider Attack** - If trusted admin goes rogue:
```
Risk: Full access to all systems (shared-secret model)
Mitigation: This is the trust model. Use Drain if trust is broken.
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
3. Enable Pond when ready: `garden-rake place keystone`
4. Existing services continue (zero downtime)
5. All pond traffic encrypted automatically

---

## Related Documentation

- **[Pond Setup Guide](pond-setup.md)** - Enable Pond, join Stones
- **[Pond Protocol Specification](../specs/POND-0001-protocol.md)** - Complete protocol design
- **[Threat Analysis](threat-analysis.md)** - Detailed attack scenarios

---

**Last Updated**: 2026-01-26
