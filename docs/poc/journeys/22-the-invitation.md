# The Invitation

*A friend wants to join your garden. You extend trust carefully.*

---

## The Story

Your friend has been watching you use Zen Garden. They finally bought a mini PC for their home office—across town, different network. They want their machine to participate in your garden.

But your garden has a pond. New Stones don't just join. They need an invitation.

---

### The Request

Your friend messages you: "I've got Moss installed. When I run `garden-rake observe`, it sees your garden but says I need an invitation."

Their screen shows:

```
Discovering garden...

⚠ Pond detected: "home-garden"
  This garden requires an invitation to join.

  To join, ask an existing member to run:
    garden-rake invite <your-stone-name>

  Then run:
    garden-rake join pond <code>

No services available (not a pond member).
```

They can see the pond exists. They can't see inside.

---

### Creating the Invitation

On your Stone, you generate an invitation:

```bash
garden-rake invite stone-willow-branch
```

```
Generating invitation for stone-willow-branch...

╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║  INVITATION CODE (valid for 5 minutes):                         ║
║                                                                  ║
║                      M3K7X2                                      ║
║                                                                  ║
║  On stone-willow-branch, run:                                    ║
║    garden-rake join pond M3K7X2                                  ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

You text your friend: "M3K7X2"

Six characters. Valid for five minutes. That's all they need.

---

### Joining

Your friend types the code:

```bash
garden-rake join pond M3K7X2
```

```
Joining pond "home-garden"...

  Validating invitation code... valid
  Requesting membership... accepted
  Receiving credentials... done
  Storing Keystone... done
  Activating encryption... done

✓ Welcome to the pond!

stone-willow-branch is now a member of "home-garden".
All communications are now encrypted.

Available services:
  mongodb     on stone-amber-ridge     192.168.1.42:27017
  redis       on stone-coral-reef      192.168.1.58:6379
  grafana     on stone-bronze-canyon   192.168.1.73:3000
```

They're in. Their Stone can now see all your services, and the garden sees theirs.

---

### Checking Membership

You verify the new member:

```bash
garden-rake pond status
```

```
Pond Status

  State: ACTIVE
  Keystone: stone-amber-ridge
  Created: 2026-03-15T10:30:00Z

  Members (4):
    ● stone-amber-ridge    Founder    Online    Certificate valid (52m remaining)
    ● stone-coral-reef     Member     Online    Certificate valid (52m remaining)
    ● stone-bronze-canyon  Member     Online    Certificate valid (52m remaining)
    ● stone-willow-branch  Member     Online    Certificate valid (58m remaining)

  Security:
    ✓ All communications encrypted (XChaCha20-Poly1305)
    ✓ mTLS certificates active (1-hour lifetime, auto-renewed)
    ✓ No pending invitations
    ✓ No revoked Stones
```

Four Stones. Your friend's machine shows as online, certificate freshly issued.

---

### Weeks Later

Your friend's Stone has been running great. Their home automation service is available from your place. Your media server is available from theirs.

Then they mention they're selling the mini PC.

Time to revoke access.

```bash
garden-rake pond revoke stone-willow-branch
```

```
Revoking stone-willow-branch from pond...

  This will:
    • Invalidate their certificate immediately
    • Remove them from the membership list
    • Block future connection attempts
    • NOT notify the Stone (they may see connection failures)

  The Stone will need a new invitation to rejoin.

Proceed? [y/N] y

  Broadcasting revocation... done
  Updating membership... done

✓ stone-willow-branch has been revoked

They will lose access when their current certificate expires (within 1 hour),
or immediately if they attempt any new connections.
```

---

Your friend's Stone now sees:

```
⚠ Connection to garden lost

  Your membership has been revoked.
  Certificate: INVALID (revoked by pond authority)

  Contact a garden member for a new invitation if this was unexpected.
```

Access removed. When they sell the PC, the new owner won't have your garden's secrets.

---

## What Just Happened

### The Invitation Flow

TOTP-based invitations provide a human-transferable authentication mechanism:

```
Inviter (your Stone)                    Joiner (friend's Stone)
       │                                        │
       ├─ Generate TOTP code                    │
       │   Input: Inviter private key           │
       │          + Joiner Stone name           │
       │          + Current time (5-min window) │
       │   Output: 6-char alphanumeric code     │
       │                                        │
       │   "M3K7X2" ─── (text message) ────────►│
       │                                        │
       │◄────── Join request ──────────────────┤
       │        (encrypted to pond public key)  │
       │        Contains: Stone name            │
       │                  Stone public key      │
       │                  Submitted TOTP        │
       │                                        │
       ├─ Validate TOTP                         │
       │   • Recompute expected code            │
       │   • Compare with submitted             │
       │   • Check time window                  │
       │                                        │
       ├─ If valid:                             │
       │   • Add to membership list             │
       │   • Issue certificate                  │
       │   • Send credentials ─────────────────►├─ Store credentials
       │     (encrypted to joiner's public key) │   • Keystone reference
       │                                        │   • Certificate
       │                                        │   • Private key
       │                                        │
       └─ Announce new member                   └─ Activate encryption
          (to all existing members)                (begin secure comms)
```

The TOTP code never crosses the network—it's transferred via text, voice, or in-person. This provides an out-of-band verification that the person requesting membership is actually who they claim to be.

### What the Joiner Receives

When accepted, the new member gets:

```
Credentials received:
├── pond_id: "a1b2c3..."              # Pond identifier
├── keystone_public: "ed25519:..."   # Pond's public key (for verification)
├── member_cert:                     # Their mTLS certificate
│   ├── subject: "stone-willow-branch"
│   ├── issuer: "home-garden-pond"
│   ├── valid_from: 2026-03-22T14:30:00Z
│   ├── valid_until: 2026-03-22T15:30:00Z
│   └── signature: (signed by Keystone)
├── member_private_key: "..."        # Private key for the certificate
└── encryption_key: "..."            # Symmetric key for UDP traffic
```

The certificate is short-lived (1 hour) and automatically renewed. This limits the damage if credentials are compromised.

### Permission Levels

Members have roles that determine what they can do:

| Role | Discover | Deploy | Invite | Revoke | Drain |
|------|----------|--------|--------|--------|-------|
| Founder | ✓ | ✓ | ✓ | ✓ | ✓ |
| Admin | ✓ | ✓ | ✓ | ✓ | ✗ |
| Member | ✓ | ✓ | ✓ | ✗ | ✗ |
| Observer | ✓ | ✗ | ✗ | ✗ | ✗ |

By default, new members join as "Member"—they can discover services, deploy offerings, and invite others, but can't revoke existing members or drain the pond.

You can specify a role when inviting:

```bash
# Invite as observer (can only see services, not deploy)
garden-rake invite stone-guest --role observer

# Invite as admin (can revoke others, but not drain pond)
garden-rake invite stone-trusted --role admin
```

### The Revocation Process

When you revoke a member:

1. **Immediate broadcast**: All Stones learn about the revocation
2. **Certificate invalidation**: The revoked Stone's cert is added to a short CRL (Certificate Revocation List)
3. **Connection rejection**: Any new connection attempts are rejected
4. **Natural expiration**: Even if broadcast fails, the cert expires within 1 hour

```
Revocation timeline:

t=0      You run 'garden-rake pond revoke'
         │
t=0+50ms Revocation broadcast reaches all Stones
         │
t=0+100ms Existing connections see no change
         (current session keys still valid)
         │
t=???    Revoked Stone tries new connection
         → Rejected immediately (cert revoked)
         │
t≤1hr    Even if revocation didn't propagate,
         certificate expires naturally
```

The 1-hour certificate lifetime means revocation is guaranteed to take effect within an hour, even if the revoked Stone was offline during the broadcast.

### Pending Invitations

You can see outstanding invitations:

```bash
garden-rake pond invitations
```

```
Pending Invitations:

  Code      Target Stone          Expires In    Invited By
  ────────────────────────────────────────────────────────
  M3K7X2    stone-willow-branch   2m 15s        stone-amber-ridge
  P9Q4R1    stone-unknown         4m 30s        stone-coral-reef

To cancel an invitation:
  garden-rake pond cancel-invitation M3K7X2
```

Invitations expire automatically after 5 minutes. You can also cancel them explicitly.

### Cross-Network Joining

Your friend is on a different network. How did the invitation work?

The join request travels through the Lantern registry if one exists:

```
Friend's Network                     Your Network
      │                                    │
stone-willow-branch                  stone-amber-ridge
      │                                    │
      ├─ Cannot reach 192.168.1.42         │
      │   directly (different subnet)      │
      │                                    │
      └─► Lantern (public IP) ────────────►│
          • Routes join request            │
          • Encrypted end-to-end           │
          • Lantern cannot read content    │
```

If no Lantern is configured, the joining Stone needs direct network access to your garden—same subnet or VPN.

---

## Security Considerations

### What Invitations Protect Against

- **Unauthorized discovery**: Can't see garden services without membership
- **Network sniffing**: All traffic encrypted after joining
- **Rogue Stones**: Can't join without human-verified code
- **Stolen devices**: Can be revoked, cert expires naturally

### What Invitations Don't Protect Against

- **Compromised inviter**: A malicious member can invite anyone
- **Code interception**: If someone sees/hears the TOTP, they have 5 minutes
- **Physical access**: Someone with the device has the stored credentials
- **Social engineering**: "Hey, can you invite my new Stone?"

The pond provides "friends and family" level security. It's appropriate for home labs and trusted groups. It's not designed for adversarial environments.

---

## Commands From This Journey

```bash
# Generate invitation for a specific Stone
garden-rake invite stone-name

# Generate invitation with specific role
garden-rake invite stone-name --role observer
garden-rake invite stone-name --role admin

# Join a pond with invitation code
garden-rake join pond M3K7X2

# Check pond membership
garden-rake pond status

# List pending invitations
garden-rake pond invitations

# Cancel a pending invitation
garden-rake pond cancel-invitation M3K7X2

# Revoke a member
garden-rake pond revoke stone-name

# Revoke without confirmation prompt
garden-rake pond revoke stone-name --force

# View revocation list
garden-rake pond revoked
```

---

*Zen Garden Documentation — Journeys*
