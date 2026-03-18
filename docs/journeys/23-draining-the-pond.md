# Draining the Pond

*Sometimes you need to start fresh. The pond can be emptied.*

---

## The Story

Your garden has grown. Eight Stones across two locations. The pond has been active for two years.

But three of those Stones were sold. One was stolen. You revoked them all, but the paranoia lingers. Those devices had the pond's encryption keys. Even revoked, someone with enough skill might extract something.

Time to drain the pond and fill it fresh.

---

### Assessing the Situation

You check the current state:

```bash
garden-rake pond status --verbose
```

```
Pond Status

  State: ACTIVE
  Keystone: stone-amber-ridge
  Created: 2024-01-15T10:30:00Z (2 years ago)

  Members (5 active, 4 revoked):
    ACTIVE:
      ● stone-amber-ridge    Founder    Online    Certificate valid (42m)
      ● stone-coral-reef     Admin      Online    Certificate valid (42m)
      ● stone-bronze-canyon  Member     Online    Certificate valid (42m)
      ● stone-silver-creek   Member     Online    Certificate valid (42m)
      ● stone-golden-peak    Member     Offline   Last seen: 2 days ago

    REVOKED:
      ○ stone-willow-branch  (revoked 2025-06-15, device sold)
      ○ stone-iron-gate      (revoked 2025-09-22, device sold)
      ○ stone-cedar-ridge    (revoked 2025-11-30, device stolen)
      ○ stone-maple-grove    (revoked 2026-01-05, device sold)

  Security:
    ✓ All communications encrypted
    ✓ mTLS certificates active
    ⚠ 4 revoked members (credentials may exist on those devices)

  Keystone Age: 2 years, 2 months
    Consider key rotation if any revoked devices were compromised.
```

Four revoked devices. One stolen. The Keystone is two years old.

---

### The Decision

You have two options:

**Option 1: Key Rotation** — Generate new encryption keys, re-issue certificates to current members. Old keys become invalid. Faster, less disruptive.

**Option 2: Full Drain** — Destroy the pond entirely. Start fresh. All members must be re-invited. More thorough, more work.

Given the stolen device, you choose the full drain.

```bash
garden-rake pond drain
```

```
╔══════════════════════════════════════════════════════════════════╗
║                     DRAIN POND                                    ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  You are about to DESTROY the pond "home-garden".                ║
║                                                                  ║
║  This will:                                                      ║
║    • Delete the Keystone (master secret)                         ║
║    • Invalidate ALL member credentials                          ║
║    • Stop ALL encrypted communications                           ║
║    • Remove ALL membership records                               ║
║                                                                  ║
║  After draining:                                                 ║
║    • Garden returns to unencrypted mode                          ║
║    • Services remain accessible (unprotected)                    ║
║    • You can create a new pond with 'garden-rake pond init'      ║
║                                                                  ║
║  This action requires your Keystone passphrase.                  ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝

Enter Keystone passphrase: ************

Draining pond...

  Notifying all members of drain...
    → stone-coral-reef: notified
    → stone-bronze-canyon: notified
    → stone-silver-creek: notified
    → stone-golden-peak: offline (will see drain on reconnect)

  Deleting Keystone... done
  Clearing membership records... done
  Clearing revocation list... done
  Resetting to discovery mode... done

✓ Pond drained

Your garden is now unencrypted. Services are discoverable by anyone on the network.
To restore security, run: garden-rake pond init
```

The pond is gone. Your garden is naked again.

---

### The Other Stones

On stone-coral-reef, the notification appeared:

```
⚠ POND DRAINED

  The pond "home-garden" has been drained by stone-amber-ridge.
  Your credentials are no longer valid.

  The garden has returned to unencrypted mode.
  Services are now discoverable without authentication.

  If this was unexpected, contact your garden administrator.
```

All the other online Stones received the same message. They automatically switched back to unencrypted mode.

---

### Refilling

You immediately create a new pond:

```bash
garden-rake pond init
```

```
╔══════════════════════════════════════════════════════════════════╗
║                     POND INITIALIZATION                          ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  Creating a new Security Pond for your garden.                   ║
║                                                                  ║
║  Existing Stones will be baptized into the new pond.             ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝

Generate passphrase automatically? [Y/n] y

Your passphrase (write this down):

    mountain-crystal-harbor-47

Confirm you've recorded it by typing the last word: harbor

Creating Keystone...
  Generating Ed25519 keypair... done
  Encrypting with Argon2id + AES-256-GCM... done
  Saving to /var/lib/zen-garden/keystone.enc... done

✓ Keystone created

Baptizing existing Stones...

  Sending credentials to stone-coral-reef... ✓
  Sending credentials to stone-bronze-canyon... ✓
  Sending credentials to stone-silver-creek... ✓
  stone-golden-peak: offline (will need invitation when online)

✓ Pond initialized with 4 Stones

⚠ Note: stone-golden-peak was offline during baptism.
  When it comes online, invite it with:
    garden-rake invite stone-golden-peak
```

New keys. New passphrase. Clean slate.

---

### The Offline Stone

Two days later, stone-golden-peak comes back online:

```
Reconnecting to garden...

⚠ Pond membership invalid

  Your credentials for "home-garden" are no longer valid.
  The pond may have been drained and recreated.

  Contact a garden member for a new invitation.
```

You send them an invitation:

```bash
garden-rake invite stone-golden-peak
```

They rejoin with the code, and the garden is whole again—but with fresh credentials that the sold and stolen devices don't have.

---

## What Just Happened

### Drain vs Rotation

The pond offers two ways to invalidate old credentials:

| Action | Key Rotation | Full Drain |
|--------|--------------|------------|
| New encryption keys | ✓ | ✓ |
| New certificates | ✓ | ✓ |
| Same Keystone | ✓ | ✗ |
| Same passphrase | ✓ | ✗ |
| Members stay joined | ✓ | ✗ |
| Disruption | Minimal | Significant |
| Re-invitation needed | No | Yes |

**Key rotation** is appropriate when:
- You want periodic security refresh
- No devices were physically compromised
- You trust current membership

**Full drain** is appropriate when:
- Devices were stolen or physically compromised
- You suspect the Keystone itself might be exposed
- You want to verify membership from scratch

### The Drain Protocol

```
Draining Stone                     Other Stones
      │                                 │
      ├─ Verify passphrase              │
      │  (prove Keystone ownership)     │
      │                                 │
      ├─ Broadcast DRAIN message ──────►│ Receive drain notification
      │  (signed by Keystone)           │
      │                                 │ Verify signature
      │                                 │
      │                                 │ Clear stored credentials
      │                                 │
      │                                 │ Switch to unencrypted mode
      │                                 │
      ├─ Securely delete Keystone       │
      │  (multiple overwrite passes)    │
      │                                 │
      ├─ Clear membership database      │
      │                                 │
      └─ Reset to discovery mode        │
```

The drain message is signed by the Keystone, proving the drain was authorized. Stones verify this signature before clearing their credentials.

### Key Rotation (Alternative)

If you'd chosen rotation instead:

```bash
garden-rake pond rotate-keys
```

```
Rotating pond encryption keys...

  Generating new encryption keys... done
  Generating new certificate authority... done

  Re-issuing certificates:
    → stone-coral-reef: issued
    → stone-bronze-canyon: issued
    → stone-silver-creek: issued
    → stone-golden-peak: offline (will get new cert on reconnect)

  Broadcasting key update... done
  Revoking old keys... done

✓ Keys rotated

All active members have new credentials.
Old encryption keys are now invalid.
```

Rotation keeps the Keystone but generates new derived keys. It's faster and doesn't require re-invitations, but the Keystone itself (and its passphrase) remain the same.

### What the Stolen Device Has

After drain, what can someone with the stolen device access?

**Before drain:**
```
Stolen device has:
├── Pond ID
├── Old member certificate (revoked, but on device)
├── Old private key
├── Old encryption key
└── Keystone public key

Can they:
├── Decrypt old captured traffic? YES (if they captured it)
├── Join the pond? NO (certificate revoked)
├── Impersonate the stone? NO (cert invalid)
└── Read new traffic? NO (new encryption keys)
```

**After drain:**
```
Stolen device still has:
├── Old credentials (now meaningless)
└── Old encryption key (pond no longer exists)

Can they:
├── Decrypt old captured traffic? YES (same as before)
├── Join the new pond? NO (completely new credentials)
├── Do anything useful? NO
```

The drain doesn't protect against historical traffic capture—if someone recorded your encrypted traffic before, they could still decrypt it with the old keys. But it completely prevents future access.

### Passphrase Security

The passphrase is critical:

- **Required for**: Draining the pond, exporting Keystone
- **NOT required for**: Day-to-day operations, inviting members
- **Never stored**: Only in your memory (or written down safely)

If you lose the passphrase:
- You can still use the garden normally
- You CANNOT drain the pond gracefully
- You CANNOT export the Keystone for backup
- You'd have to delete the Keystone file and recreate (same effect as drain)

### Scheduled Rotation

For security hygiene, schedule regular key rotation:

```bash
# Check key age
garden-rake pond status

# Rotate every 6 months (example policy)
garden-rake pond rotate-keys
```

Rotation is low-overhead and keeps credentials fresh. It's good practice even if you haven't had any compromises.

---

## Commands From This Journey

```bash
# Full drain (destroys pond, requires passphrase)
garden-rake pond drain

# Key rotation (keeps pond, refreshes credentials)
garden-rake pond rotate-keys

# Check pond age and status
garden-rake pond status --verbose

# See revoked members
garden-rake pond revoked

# Create new pond after drain
garden-rake pond init

# Invite offline Stone after pond recreation
garden-rake invite stone-name

# Export Keystone for backup (requires passphrase)
garden-rake pond export-keystone --to /secure/location/

# Import Keystone (for Keystone migration)
garden-rake pond import-keystone --from /backup/keystone.enc
```

---

*Zen Garden Documentation — Journeys*
