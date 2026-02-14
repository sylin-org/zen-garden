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
- Encryption (mTLS for all authenticated Stone-to-Stone communication)
- Admission control (TOTP-based enrollment — control which Stones can join)
- Authentication (verify Stone identity via ECDSA P-256 certificates)

**Security model**: CA-based mTLS via koi-certmesh. The cornerstone holds the CA private key and acts as the trust anchor. Stones receive individual certificates. The cornerstone controls enrollment and revocation.

---

## Prerequisites

### Before Creating Pond

✅ **All Stones operational** - Services running, discovery working  
✅ **Network connectivity** - All Stones can reach each other  
✅ **Time synchronization** - NTP configured (±10 min tolerance)  
✅ **Backups ready** - Important data backed up before security changes

### Strong Passphrase Required

Pond security depends on Keystone encryption. Use a strong passphrase:

The passphrase encrypts the CA private key using AES-256-GCM with an Argon2id-derived key. If someone steals the keystone file, they still need the passphrase.

```bash
# Initialize pond with passphrase
garden-rake pond init --passphrase "my-secure-passphrase" --profile just-me
```

**Requirements:**
- Minimum: Use a passphrase that's hard to guess
- Recommended: 20+ characters or multi-word passphrases
- Pattern: `word1-word2-word3-number` (e.g., `compass-twilight-harvest-82`)

**Learn more:** [XKCD 936](https://xkcd.com/936/)

Store passphrases securely (1Password, Bitwarden, etc.). **Never plain text.**

---

## Create a Pond

### Step 1: Choose Initial Stone (Cornerstone)

Select a stable Stone to be the cornerstone (CA holder):

```bash
# On your chosen Stone (e.g., stone-01)
garden-rake pond init --passphrase "my-secure-passphrase" --profile just-me

✓ CA created (ECDSA P-256)
✓ Cornerstone: stone-01
✓ Keystone sealed

CA Fingerprint: AB:CD:EF:01:23:45:67:89...
TOTP URI: otpauth://totp/certmesh:stone-01?secret=BASE32&issuer=certmesh&algorithm=SHA1&digits=6&period=30

Next steps:
  1. Verify status: garden-rake pond status
  2. Open enrollment: garden-rake pond invite --passphrase "..."
```

**Trust profiles:**
- `just-me` (default) — Solo operator, auto-approve enrollment
- `my-team` — Small trusted team
- `my-organization` — Requires explicit approval for each enrollment

**What happened**:
- Generated a CA keypair (ECDSA P-256)
- Encrypted the CA private key with the passphrase (AES-256-GCM, Argon2id KDF)
- Designated this stone as the cornerstone
- Generated a TOTP secret for enrollment

### Step 2: Verify Pond Status

```bash
garden-rake pond status

Pond Status: Active
Cornerstone: stone-01
Profile: JustMe
Enrollment: Closed
Stones: 1 (primary, active)
CA Fingerprint: AB:CD:EF:01:23:45:67:89...
```

---

## Join Stones to Pond

### Step 3: Open Enrollment

On the cornerstone, open enrollment and get a TOTP URI:

```bash
# On the cornerstone (stone-01)
garden-rake pond invite --passphrase "my-secure-passphrase"

Enrollment open for 30 minutes.

TOTP URI: otpauth://totp/certmesh:stone-01?secret=BASE32&issuer=certmesh&algorithm=SHA1&digits=6&period=30
Expires at: 14:57:00 UTC

Share this URI with stones that need to join.
Generate a 6-digit code from the URI to use on each joining stone.
```

**Security model**: The TOTP URI encodes a shared secret. Any authenticator app (Google Authenticator, Authy, etc.) can generate valid 6-digit codes from it. Codes rotate every 30 seconds.

### Step 4: Join from New Stone

On the Stone you want to join:

```bash
# On stone-05
garden-rake pond join --code 123456

Validating code...
✓ Code accepted
✓ Certificate issued (ECDSA P-256)
✓ Joined pond

Stone: stone-05
Cornerstone: stone-01
CA Fingerprint: AB:CD:EF:01:23:45:67:89...
```

**What happened**:
- stone-05 generated a TOTP code from the shared URI
- Sent the code to the cornerstone via HTTP POST
- Cornerstone validated the code and issued a certificate
- stone-05 received its certificate and the CA public certificate
- stone-05 is now an authenticated pond member

---

## Verify Pond Status

### Check All Stones

```bash
garden-rake pond status

Pond Status: Active
Cornerstone: stone-01
Profile: JustMe
Enrollment: Closed
CA Fingerprint: AB:CD:EF:01:23:45:67:89...

Stones:
  stone-01 [primary] (active)
  stone-02 [member] (active)
  stone-03 [member] (active)
  stone-04 [member] (active)
```

---

## Troubleshooting

### Join Failed: Invalid Code

```
Error: INVALID_AUTH — wrong or expired TOTP code

Possible causes:
  1. Code expired (30-second rotation period)
  2. Clock drift between Stones
  3. Typo in entered code
  4. Enrollment window closed

Solutions:
  1. Generate a fresh code from the TOTP URI
  2. Check time sync: systemctl status systemd-timesyncd
  3. Re-open enrollment: garden-rake pond invite --passphrase "..."
```

### Keystone Passphrase Forgotten

```
Problem: Lost Keystone passphrase, cannot unlock CA after restart

Recovery:
  1. No recovery possible (CA key is encrypted, no backdoor)
  2. Options:
     a) If a promoted standby stone has the CA key, use that stone
     b) Drain the pond and create a new one (re-invite all Stones)

Prevention:
  - Store passphrase in password manager
  - Promote a standby CA: garden-rake pond promote --passphrase "..."
```

### CA Locked After Restart

```bash
# After Moss restarts, the CA key is locked
garden-rake pond status

Pond Status: Locked
Cornerstone: stone-01

# Unlock with passphrase
garden-rake pond unlock --passphrase "my-secure-passphrase"

✓ CA key unlocked
```

### Pond Compromise (Emergency)

If you suspect a stone is compromised:

```bash
# Option 1: Revoke a single stone
garden-rake pond untrust stone-05

✓ Certificate revoked for stone-05
```

```bash
# Option 2: Drain the entire pond (nuclear option)
garden-rake pond remove

✓ CA destroyed
✓ All certificates invalidated
✓ Pond dissolved

All stones are now in open (unauthenticated) mode.
To secure again: garden-rake pond init --passphrase "..."
```

**Individual revocation:** Unlike the old shared-secret model, the CA-based model supports revoking individual stones without draining the entire pond.

---

## Common Commands

```bash
# Status
garden-rake pond status                         # Pond status and membership

# Initialize
garden-rake pond init --passphrase "..." --profile just-me  # Create pond

# Enrollment
garden-rake pond invite --passphrase "..."      # Open enrollment, get TOTP URI
garden-rake pond join --code 123456             # Join with TOTP code

# CA Lifecycle
garden-rake pond unlock --passphrase "..."      # Unlock CA after restart
garden-rake pond promote --passphrase "..."     # Promote to standby CA

# Revocation
garden-rake pond untrust stone-02               # Revoke a stone's certificate
garden-rake pond remove                         # Drain pond (destroy CA)

# CA Certificate
curl http://stone:7185/api/v1/pond/ca.pem       # Download CA public cert
```

---

## Related Documentation

- **[Security Overview](overview.md)** - Threat model and security philosophy
- **[Pond Protocol Specification](../specs/POND-0001-protocol.md)** - Original protocol design (superseded by certmesh implementation)
- **[Koi Embedded Integration](../proposals/koi-embedded-integration.md)** - Implementation proposal and verification

---

**Last Updated**: 2026-02-16
