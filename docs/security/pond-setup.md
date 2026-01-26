# Pond Setup Guide

**Create and join Stones to a Pond for encrypted communication**

**Purpose**: Step-by-step guide to enable Pond security  
**Audience**: Operator

---

## Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Create a Pond](#create-a-pond)
- [Join Stones to Pond](#join-stones-to-pond)
- [Verify Pond Status](#verify-pond-status)
- [Troubleshooting](#troubleshooting)

---

## Overview

**Pond** enables encrypted communication between Stones. Start without Pond (frictionless), enable when ready.

**Philosophy**: "Set your stones, make sure everything is working, fill the pond."

**What Pond provides**:
- Encryption (protect traffic from network sniffing)
- Admission control (control which Stones can join)
- Authentication (verify Stone identity via shared secret)

**Security model**: P2P shared-secret. All pond members hold the complete keypair. Any member can invite new stones. Cornerstone is ceremonial (the stone that initialized the pond), not a control point.

---

## Prerequisites

### Before Creating Pond

✅ **All Stones operational** - Services running, discovery working  
✅ **Network connectivity** - All Stones can reach each other  
✅ **Time synchronization** - NTP configured (±10 min tolerance)  
✅ **Backups ready** - Important data backed up before security changes

### Strong Passphrase Required

Pond security depends on Keystone encryption. Use a strong passphrase:

**Zen Garden offers three generation methods:**
1. **Keyboard mashing** (default) - Fun & secure entropy collection
2. **Auto-generated** - XKCD-style 4-word passphrases
3. **Manual entry** - For password manager users

```bash
# Interactive generation (recommended)
garden-rake place keystone

How would you like to create your passphrase?
1. Let me mash the keyboard! 🎹 (fun & secure)
2. Generate one for me (quick & easy)
3. I'll type my own (advanced)

Choice [1]: 1

# Mash your keyboard for ~3 seconds
# System generates: forest-lantern-compass-71

# Or use standalone generator
garden-rake generate-passphrase

Output:
  Passphrase: autumn-laptop-database-71
  Entropy: 52 bits (strong)
  Memorization: "Autumn laptop database, room 71"
```

**Requirements:**
- Minimum: 40 bits entropy (validated automatically)
- Recommended: 52+ bits (4 XKCD words + number)
- Pattern: `word1-word2-word3-number` (e.g., `compass-twilight-harvest-82`)

**Learn more:** [XKCD 936](https://xkcd.com/936/)

Store passphrases securely (1Password, Bitwarden, etc.). **Never plain text.**

---

## Create a Pond

### Step 1: Choose Initial Stone (Cornerstone)

Select any Stone to initialize the pond:

```bash
# On your chosen Stone (e.g., stone-01)
garden-rake place keystone

Initializing Pond...

How would you like to create your passphrase?
1. Let me mash the keyboard! (fun & secure)
2. Generate one for me (quick & easy)
3. I'll type my own (advanced)

Choice [1]: 1

Mash your keyboard randomly... GO!
████████████████████ 100%

✓ Collected 248 bits of entropy

Generated passphrase: forest-lantern-compass-71
Memorization: "A forest with lanterns and compass #71"

Use this passphrase? [Y/n]: y

✓ Pond created (hardware-backed via TPM 2.0)
✓ Cornerstone: stone-01
✓ Keystone sealed in TPM
✓ CA keypair generated (shared secret model)

Next steps:
  1. Verify status: garden-rake status --security
  2. Invite other Stones: garden-rake invite <stone-name>
```

**What happened**:
- Generated Pond CA keypair (Ed25519)
- **Auto-detected TPM** and sealed Keystone in hardware (or encrypted with passphrase if no TPM)
- Enabled encrypted UDP mode for all pond traffic

**Protection tier shown:** System automatically uses best available:
- `hardware-backed via TPM 2.0` - Sealed in physical security module
- `hypervisor-backed via KVM` - vTPM in virtual machine
- `software-backed` - Passphrase encryption (fallback)

**Learn more:** [Keystone Protection Tiers](../decisions/SECURITY-0003-keystone-protection-tiers.md)

### Step 2: Verify Pond Status

```bash
garden-rake status --security

Pond Status: Active
Cornerstone: stone-01
Role: Founder (ceremonial)
Joined Stones: 1
Pending Joins: 0
Last Audit: 1 event (pond initialized)
```

---

## Join Stones to Pond

### Option A: Baptism (During Pond Creation)

When creating a pond, optionally auto-invite all existing stones:

```bash
garden-rake place keystone --auto-invite

✓ Pond created
✓ Sending baptism to 3 stones...
  ✓ stone-02 baptized
  ✓ stone-03 baptized
  ✓ stone-04 baptized

Pond ready with 4 members.
```

**What happened**:
- Cornerstone read topology cache (known stones)
- Sent encrypted credentials to each stone individually (unicast)
- Each stone verified and stored credentials
- All stones now in encrypted mode

### Option B: Individual Invitation (Later)

Any pond member can invite new stones:

```bash
# On any pond member (e.g., stone-01 or stone-02)
garden-rake invite stone-05

Invitation ready for: stone-05

TOTP Code: KP7X9M
Valid for: 5 minutes
Expires at: 14:35:00 UTC

Display this code to the administrator adding stone-05.
```

**Security model**: TOTP code displayed locally on both Stones (never transmitted over network). Inspired by Bluetooth pairing - familiar UX, proven security.

### Step 4: Join from New Stone

On the Stone you want to join:

```bash
# On stone-05
garden-rake join pond

Discovering pond members...
✓ Found pond member: stone-01

Requesting join...

Enter TOTP code: KP7X9M

Validating...
✓ Code valid
✓ Credentials received
✓ Joined pond

Pond Status: Active
Joined as: Member
Encryption: XChaCha20-Poly1305
```

**What happened**:
- stone-05 discovered an existing pond member via mDNS
- Generated join request encrypted with Pond CA public key
- Administrator verified matching TOTP code
- Received complete pond credentials (shared secret)
- stone-05 now authenticated in Pond

---

## Verify Pond Status

### Check All Stones

```bash
garden-rake status --security --all

stone-01 (Cornerstone):
  Role: Founder
  Status: Healthy

stone-02:
  Role: Member
  Status: Healthy

stone-03:
  Role: Member
  Status: Healthy

stone-04:
  Role: Member
  Status: Healthy

Summary:
  Pond: Active
  Stones: 4 joined, 0 pending
  Encryption: XChaCha20-Poly1305
```

### View Audit Log

```bash
garden-rake audit show --last 10

2026-01-18 14:30:15 | stone-01 | pond_initialized
2026-01-18 14:32:48 | stone-02 | stone_joined | invited_by=stone-01
2026-01-18 14:35:12 | stone-03 | stone_joined | invited_by=stone-01
2026-01-18 14:38:05 | stone-04 | stone_joined | invited_by=stone-02
```

---

## Troubleshooting

### Join Failed: Invalid Code

```
Error: TOTP code validation failed

Possible causes:
  1. Code expired (5-minute window)
  2. Clock drift between Stones (±10 min tolerance)
  3. Typo in entered code
  4. Code already used (one-time use)

Solutions:
  1. Request new code: garden-rake invite <stone> (on any pond member)
  2. Check time sync: garden-rake check-time
  3. Verify NTP: systemctl status systemd-timesyncd
```

### Keystone Passphrase Forgotten

```
Problem: Lost Keystone passphrase, cannot unlock keystone

Recovery:
  1. No recovery possible (Keystone is encrypted, no backdoor)
  2. Options:
     a) If you have another pond member with unlocked keystone, pond continues working
     b) Create new Pond (requires draining old pond, re-inviting all Stones)

Prevention:
  - Store passphrase in password manager
  - Backup Keystone file: /var/lib/zen-garden/keystone.enc
  - Test passphrase: garden-rake verify-keystone
```

### Outsider Detected

```bash
# A stone without pond credentials is detected
garden-rake status

⚠️  Outsider detected: stone-new (192.168.1.50)
    This stone is sending plaintext messages.

Actions:
  1. If legitimate: garden-rake invite stone-new
  2. If unauthorized: Check your network for rogue devices
```

### Pond Compromise (Emergency)

If you suspect a stone is compromised and has leaked the shared secret:

```bash
# Drain the entire pond (nuclear option)
garden-rake drain pond --yes-i-am-sure

⚠️  This will destroy the pond and revert all stones to open mode.
    All stones will need to be re-invited to a new pond.

Draining pond...
✓ Drain signal sent to 4 stones
✓ Local credentials destroyed
✓ Pond dissolved

All stones are now in open (unencrypted) mode.
To secure again: garden-rake place keystone
```

**Why drain?** With shared-secret model, if one stone is compromised, the attacker has the complete keypair. Individual revocation is not possible. Drain resets everything.

---

## Common Commands

```bash
# Status
garden-rake status --security              # Local Stone security status
garden-rake status --security --all        # All Stones in Garden

# Join/Leave
garden-rake join pond                      # Join existing Pond
garden-rake drain pond                     # Drain Pond (emergency reset)

# Invitation (any pond member)
garden-rake invite <stone>                 # Generate TOTP for Stone join

# Keystone
garden-rake verify-keystone                # Test Keystone passphrase

# Audit
garden-rake audit show                     # View audit log
garden-rake audit --filter stone_joined    # Filter by event type

# Troubleshooting
garden-rake check-time                     # Verify time sync
garden-rake diagnose pond                  # Pond health check
```

---

## Related Documentation

- **[Security Overview](overview.md)** - Threat model and security philosophy
- **[Pond Protocol Specification](../specs/POND-0001-protocol.md)** - Complete protocol design
- **[Keystone Protection Tiers](../decisions/SECURITY-0003-keystone-protection-tiers.md)** - TPM/vTPM/passphrase protection

---

**Last Updated**: 2026-01-26

---

**Last Updated**: 2026-01-18
