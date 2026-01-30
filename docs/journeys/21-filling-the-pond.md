# Filling the Pond

*Your garden works. Now you want it secure.*

---

## The Story

Your garden has been running for months. Three Stones, a dozen services, everything talking over UDP broadcasts. It works beautifully.

But anyone on your network can see those broadcasts. Anyone could pretend to be a Stone. Your home lab is trusted—but maybe you want a little more assurance.

Time to fill the pond.

---

```bash
garden-rake place keystone
```

```
╔══════════════════════════════════════════════════════════════════╗
║                     POND INITIALIZATION                          ║
╠══════════════════════════════════════════════════════════════════╣
║                                                                  ║
║  You're about to create a Security Pond for your garden.        ║
║                                                                  ║
║  This will:                                                      ║
║    • Generate cryptographic keys for this garden                 ║
║    • Encrypt all Stone-to-Stone communication                   ║
║    • Require invitation codes for new Stones to join            ║
║                                                                  ║
║  You'll need a passphrase to protect the Keystone.              ║
║  This passphrase unlocks the garden's master keys.              ║
║                                                                  ║
║  IMPORTANT: If you lose this passphrase, you'll need to         ║
║  drain the pond and start fresh. Write it down safely.          ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝

Generate passphrase automatically? [Y/n] y

Your passphrase (write this down):

    compass-twilight-harvest-82

Confirm you've recorded it by typing the last word: harvest

Creating Keystone...
  Generating Ed25519 keypair... done
  Encrypting with Argon2id + AES-256-GCM... done
  Saving to /var/lib/zen-garden/keystone.enc... done

✓ Keystone created

Now baptizing existing Stones into the pond...

  Sending credentials to stone-coral-reef... ✓
  Sending credentials to stone-bronze-canyon... ✓

✓ Pond initialized with 3 Stones

Your garden is now encrypted. New Stones will need an invitation to join.
```

The pond is filled. Your garden now speaks in encrypted whispers.

---

You check the status:

```bash
garden-rake pond status
```

```
Pond Status

  State: ACTIVE
  Keystone: stone-amber-ridge
  Created: 2026-03-15T10:30:00Z

  Members (3):
    ● stone-amber-ridge    Founder    Online    Certificate valid (58m remaining)
    ● stone-coral-reef     Member     Online    Certificate valid (58m remaining)
    ● stone-bronze-canyon  Member     Online    Certificate valid (58m remaining)

  Security:
    ✓ All communications encrypted (XChaCha20-Poly1305)
    ✓ mTLS certificates active (1-hour lifetime, auto-renewed)
    ✓ No pending invitations
    ✓ No revoked Stones
```

Three Stones, all encrypted, all verified. The UDP broadcasts that were once plaintext now look like random noise to anyone listening.

---

A week later, you acquire a new mini PC. You install Zen Garden and try to join:

```bash
# On the new Stone
garden-rake observe
```

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

The new Stone can see that a pond exists, but it can't see what's inside. It needs an invitation.

---

On your existing Stone:

```bash
garden-rake invite stone-silver-creek
```

```
Generating invitation for stone-silver-creek...

╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║  INVITATION CODE (valid for 5 minutes):                         ║
║                                                                  ║
║                      K7X9M2                                      ║
║                                                                  ║
║  On stone-silver-creek, run:                                     ║
║    garden-rake join pond K7X9M2                                  ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
```

You read the code to your new Stone (or walk over and type it):

```bash
# On stone-silver-creek
garden-rake join pond K7X9M2
```

```
Joining pond "home-garden"...

  Sending join request... done
  Receiving credentials... done
  Storing Keystone... done
  Activating encryption... done

✓ Welcome to the pond!

stone-silver-creek is now a member of "home-garden".
All communications are now encrypted.
```

The new Stone is in. It received the pond's cryptographic credentials and can now participate in encrypted conversations.

---

## What Just Happened

### The Keystone

The Keystone is your garden's master secret. It's an Ed25519 keypair that:

- Signs all authentication tokens
- Derives encryption keys for UDP traffic
- Validates membership credentials

The Keystone is stored encrypted on the founding Stone:

```
/var/lib/zen-garden/keystone.enc
  ├── Ed25519 private key (encrypted)
  ├── Ed25519 public key
  └── Encrypted with: AES-256-GCM
      └── Key derived via: Argon2id (64MB memory, 3 iterations)
```

The passphrase never leaves your head (or your piece of paper). It's not stored anywhere.

### Baptism

When you create a pond with existing Stones, they're "baptized"—they receive the Keystone credentials directly:

```
Founder Stone                     Other Stones
     │                                 │
     ├─ Generate Keystone              │
     │                                 │
     ├─ For each known Stone:          │
     │   └─ Encrypt credentials ──────►├─ Receive credentials
     │      (using Stone's public key) │   └─ Store Keystone
     │                                 │   └─ Activate encryption
     │                                 │
     └─ Pond active                    └─ Pond member
```

Baptism is direct—the founder sends credentials to Stones it already knows. No codes needed because trust is already established through the existing topology.

### TOTP Invitations

New Stones joining later use TOTP (Time-based One-Time Password) codes:

```
Inviter                          Joiner
   │                                │
   ├─ Generate TOTP code            │
   │   (from Stone identity +       │
   │    current time)               │
   │                                │
   │   K7X9M2 ─── (human reads) ───►├─ Enter code
   │                                │
   │◄── Join request ──────────────┤
   │    (encrypted to pond)         │
   │                                │
   ├─ Validate TOTP                 │
   │                                │
   ├─ Send credentials ────────────►├─ Store credentials
   │   (encrypted to joiner)        │
   │                                │
   └─ Announce new member           └─ Activate encryption
```

The TOTP code is derived from:
- The inviter's private key
- The joiner's name
- The current time (5-minute window)

It's a 6-character code (like `K7X9M2`) that must be physically transferred—read aloud, typed in person, sent via trusted channel. The code never crosses the network.

This is similar to Bluetooth pairing: simple, familiar, secure enough for home use.

### What Gets Encrypted

Once the pond is active:

**Before (plaintext):**
```json
{
  "type": "stone_chirp",
  "data": {
    "stone_name": "stone-amber-ridge",
    "services": ["mongodb", "redis"]
  }
}
```

**After (encrypted):**
```json
{
  "pond_id": "a1b2c3...",
  "sender_id": "019c3a2b...",
  "nonce": "base64...",
  "ciphertext": "base64...",
  "signature": "base64..."
}
```

The message content is encrypted with XChaCha20-Poly1305. The signature proves the sender is a valid pond member. Anyone intercepting the traffic sees random bytes.

### Certificate Lifecycle

Each Stone gets short-lived mTLS certificates:

- **Lifetime**: 1 hour
- **Renewal**: Every 30 minutes (automatic)
- **Validation**: Certificate Common Name must match Stone ID

If a Stone is compromised, you revoke it. The certificate expires within an hour, and the Stone can't get a new one.

---

## Security Boundaries

### What the Pond Protects

- **Network sniffing**: Traffic is encrypted
- **Unauthorized joins**: Requires TOTP code
- **Replay attacks**: Nonce tracking prevents reuse
- **Device impersonation**: Certificates bound to Stone ID

### What the Pond Does NOT Protect

- **Physical access**: If someone has the Keystone file and passphrase, they're in
- **Compromised Stones**: Malware on a Stone has the keys
- **Nation-state adversaries**: This is home lab security, not defense-grade

The pond is "good enough" security for trusted home networks. It's not designed for hostile environments or compliance requirements.

---

## The Pond Philosophy

Security in Zen Garden follows a principle: **fill when ready**.

You don't need encryption to deploy MongoDB. You don't need certificates to discover services. The garden works perfectly without a pond.

But when you're ready—when the garden is stable and you want that extra layer—you fill the pond. Security is opt-in, not mandatory.

This is deliberate. Debugging mTLS handshakes when a container won't start is miserable. Get the system working first, then add security.

---

## Commands From This Journey

```bash
# Create a pond (generates Keystone, baptizes existing Stones)
garden-rake place keystone

# Create with auto-generated passphrase
garden-rake place keystone --auto-passphrase

# Check pond status
garden-rake pond status

# Invite a new Stone
garden-rake invite stone-silver-creek

# Join a pond (on the new Stone)
garden-rake join pond K7X9M2

# View security status across all Stones
garden-rake status --security --all
```

---

*Zen Garden Documentation — Journeys*
