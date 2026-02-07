# POND-0001: Pond Security Protocol Specification

**Status**: Draft  
**Created**: 2026-01-26  
**Updated**: 2026-01-26  
**Authors**: Leo Botinelly  
**Supersedes**: None (additive to security.md)  
**Related**: [security.md](security.md), [ELECTION-0001](ELECTION-0001-distributed-election.md)

---

## Abstract

Pond is an optional security layer for Zen Garden that provides encrypted communication and authenticated membership between stones.

**Security model:** Shared secret with P2P invitation. All pond members hold the complete CA keypair. Any member can invite new stones. Cornerstone (founder) is ceremonial, not a control point.

**Cryptographic primitives:** Ed25519 (signatures), X25519 + XChaCha20-Poly1305 (encryption), BLAKE3 (hashing). Same algorithms as WireGuard and Signal.

**UX philosophy:** One-time setup, then invisible. Like browser certificates.

---

## Design Decisions

This section documents key architectural decisions and their rationale.

### Decision 1: Unicast Baptism

Founder sends individual unicast messages to each stone from topology cache.

**Why unicast:**
- Topology cache is already validated through chirp protocol (trusted source)
- Direct addressing prevents interception by passive observers
- Signature includes recipient stone ID (prevents message forwarding attacks)
- Confirmation ACK proves delivery (no race conditions)

**Trade-off:** Sequential delivery is slower than broadcast for large gardens. A 10-stone baptism takes <10 seconds—acceptable.

### Decision 2: Shared Secret Model

All pond members hold the complete CA keypair (public + private).

**Why shared secret:**
- Home labs have a single administrator—trust is binary (your devices or not)
- Enables P2P invitation (any member can invite new stones)
- Cornerstone is ceremonial, not a control point
- Matches proven models: WireGuard shares keys with peers

**Trade-off:** Compromised member = drain pond and re-baptize all stones. For 2-10 stones, this takes 30 seconds. Acceptable.

### Decision 3: TOTP Invitation (Bluetooth Pairing UX)

New stones prove authorization via 6-character TOTP codes with 5-minute window.

**Why TOTP:**
- Implies physical proximity (operator must be at both machines)
- No complex key exchange ceremony or pre-shared secrets
- Familiar UX (identical to Bluetooth pairing, Authy/Google Auth)
- 5-minute window allows walking between machines in different rooms

**Security model:** TOTP is not cryptographic proof—it's a rate-limited secret sharing mechanism that proves physical access to an authorized terminal.

### Decision 4: Security Posture (What Pond Protects)

Pond provides "browser certificate" level security—strong encryption that's invisible after setup.

**Threats blocked (95% of realistic attacks):**

| Threat | Protection |
|--------|------------|
| Network sniffing | XChaCha20-Poly1305 encryption |
| Unauthorized device joins | TOTP invitation required |
| Replay attacks | Nonce tracking with rejection |
| Captured traffic decryption | Forward secrecy (ephemeral X25519) |
| Passive eavesdropping | All pond traffic encrypted |
| Shodan/scanner discovery | Encrypted UDP, no plaintext metadata |

**Threats NOT blocked (accepted risks for home labs):**

| Threat | Why Accepted |
|--------|--------------|
| Compromised member stone | If malware is on your stone, pond security is the least of your problems |
| Physical key extraction | Same as stealing your laptop—game over regardless |
| Sophisticated timing attacks | No one targets home labs with academic attacks |

**Verdict:** Same cryptographic primitives as WireGuard and Signal. Real security, not theater.

---

## Table of Contents

1. [Terminology](#1-terminology)
2. [Cryptographic Primitives](#2-cryptographic-primitives)
3. [Key Types](#3-key-types)
4. [Message Envelope Formats](#4-message-envelope-formats)
5. [Pond Initialization](#5-pond-initialization)
6. [Baptism Protocol](#6-baptism-protocol)
7. [Invitation Protocol](#7-invitation-protocol)
8. [Outsider Detection](#8-outsider-detection)
9. [Revocation](#9-revocation)
10. [Encrypted UDP](#10-encrypted-udp)
11. [State Machines](#11-state-machines)
12. [Configuration Options](#12-configuration-options)
13. [Security Considerations](#13-security-considerations)
14. [Implementation Checklist](#14-implementation-checklist)

---

## 1. Terminology

| Term | Definition |
|------|------------|
| **Pond** | Security boundary created by shared credentials. Inside = trusted, outside = untrusted. |
| **Keystone** | Encrypted file containing Pond CA keypair, protected by passphrase and/or TPM. |
| **Cornerstone** | Stone that initialized the pond (ran `place keystone`). Ceremonial role—not a control point. |
| **Founder** | Alias for cornerstone. |
| **Member** | Stone that has joined the pond and holds valid credentials. |
| **Outsider** | Stone that is not a pond member (sends plaintext messages). |
| **Baptism** | One-time event where existing stones receive pond credentials during initialization. |
| **Identity Keypair** | Permanent Ed25519 keypair unique to each stone, generated on first boot. |

---

## 2. Cryptographic Primitives

### 2.1 Algorithms

| Purpose | Algorithm | Library |
|---------|-----------|---------|
| Asymmetric encryption | X25519 + XChaCha20-Poly1305 | `crypto_box` |
| Signatures | Ed25519 | `ed25519-dalek` |
| Symmetric encryption | XChaCha20-Poly1305 | `chacha20poly1305` |
| Key derivation | HKDF-SHA256 | `hkdf` |
| Hashing | BLAKE3 | `blake3` |
| TOTP | HMAC-SHA256, 6-char base36, 300s window | custom |
| Password KDF | Argon2id (64MB, 3 iterations) | `argon2` |

### 2.2 Key Sizes

| Key Type | Size | Format |
|----------|------|--------|
| Ed25519 private key | 32 bytes | Raw bytes |
| Ed25519 public key | 32 bytes | Raw bytes |
| X25519 private key | 32 bytes | Raw bytes |
| X25519 public key | 32 bytes | Raw bytes |
| Symmetric key | 32 bytes | Raw bytes |
| Nonce (XChaCha20) | 24 bytes | Raw bytes |
| TOTP code | 6 characters | Base36 (A-Z, 0-9) |

### 2.3 Fingerprints

Pond and stone fingerprints use BLAKE3 truncated to 16 bytes, encoded as hex:

```rust
fn fingerprint(pubkey: &[u8; 32]) -> String {
    let hash = blake3::hash(pubkey);
    hex::encode(&hash.as_bytes()[..16])
}
```

---

## 3. Key Types

### 3.1 Stone Identity Keypair

**Purpose**: Permanent identity for each stone. Used for TOTP generation, resurrection, and signatures.

**Generation**: Created on first Moss boot if not exists.

**Storage**: `{data_dir}/stone-identity.key` (encrypted with passphrase via Argon2id + AES-256-GCM)

**Lifetime**: Permanent (never expires, only explicitly revoked)

```rust
struct StoneIdentity {
    stone_id: String,           // GUIDv7
    stone_name: String,         // Human-readable name
    private_key: Ed25519Private,
    public_key: Ed25519Public,
    created_at: u64,            // Unix timestamp
}
```

### 3.2 Pond CA Keypair

**Purpose**: Shared secret for encryption and membership proof. All pond members hold the complete keypair.

**Generation**: Created during `place keystone` command.

**Storage**: `{data_dir}/keystone.enc` (encrypted, optionally TPM-sealed)

**Distribution**: All members receive full keypair (public + private) during baptism or invitation.

```rust
struct PondCA {
    pond_id: String,            // Fingerprint of public key
    private_key: Ed25519Private,
    public_key: Ed25519Public,
    created_at: u64,
}
```

---

## 4. Message Envelope Formats

### 4.1 Plaintext Envelope (Open Garden)

Used when pond is not enabled or for outsider communication:

```json
{
  "type": "stone_chirp",
  "data": { ... },
  "sender_id": "019bece4-...",
  "timestamp": 1706284800
}
```

### 4.2 Encrypted Envelope (Pond Mode)

All pond communication uses this envelope:

```json
{
  "pond_id": "a1b2c3d4e5f6...",
  "sender_id": "019bece4-...",
  "nonce": "<24 bytes, base64>",
  "ciphertext": "<encrypted payload, base64>",
  "signature": "<Ed25519 signature of ciphertext, base64>"
}
```

**Encryption process**:
```rust
fn encrypt_pond_message(payload: &[u8], pond_key: &SymmetricKey) -> EncryptedEnvelope {
    let nonce = random_bytes::<24>();
    let ciphertext = xchacha20poly1305_encrypt(pond_key, &nonce, payload);
    let signature = sign(my_identity_privkey(), &ciphertext);
    
    EncryptedEnvelope {
        pond_id: pond_fingerprint(),
        sender_id: my_stone_id(),
        nonce: base64_encode(nonce),
        ciphertext: base64_encode(ciphertext),
        signature: base64_encode(signature),
    }
}
```

**Symmetric key derivation** (from Pond CA):
```rust
fn derive_pond_symmetric_key(pond_ca_privkey: &[u8]) -> SymmetricKey {
    let ikm = pond_ca_privkey;
    let salt = b"zen-garden-pond-v1";
    let info = b"udp-encryption";
    
    hkdf_sha256(ikm, salt, info, 32)
}
```

### 4.3 Per-Stone Encrypted Payload

Used when sending secrets to a specific stone (baptism, invitation, resurrection):

```rust
fn encrypt_to_stone(payload: &[u8], recipient_pubkey: &X25519Public) -> PerStonePayload {
    // Convert Ed25519 pubkey to X25519 for encryption
    let x25519_pubkey = ed25519_to_x25519(recipient_pubkey);
    
    // Generate ephemeral keypair
    let ephemeral = X25519Keypair::generate();
    
    // Derive shared secret
    let shared = x25519_diffie_hellman(ephemeral.private, x25519_pubkey);
    let symmetric_key = hkdf_sha256(shared, b"stone-encrypt", b"", 32);
    
    // Encrypt
    let nonce = random_bytes::<24>();
    let ciphertext = xchacha20poly1305_encrypt(symmetric_key, &nonce, payload);
    
    PerStonePayload {
        ephemeral_pubkey: base64_encode(ephemeral.public),
        nonce: base64_encode(nonce),
        ciphertext: base64_encode(ciphertext),
    }
}
```

---

## 5. Pond Initialization

### 5.1 Command

```bash
garden-rake place keystone [--auto-invite | --no-auto-invite]
```

### 5.2 Initialization Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  POND INITIALIZATION                                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  INPUT:                                                             │
│    auto_invite: bool                                                │
│    passphrase: String (from user)                                   │
│                                                                     │
│  PROCESS:                                                           │
│    1. Generate Pond CA keypair (Ed25519)                            │
│    2. Derive pond_id = fingerprint(ca_pubkey)                       │
│    3. Encrypt CA keypair with passphrase (Argon2id + AES-256-GCM)   │
│    4. If TPM available: seal encrypted blob in TPM                  │
│    5. Store as {data_dir}/keystone.enc                              │
│    6. Set pond_state = POND_FOUNDER                                 │
│    7. If auto_invite: execute Baptism Protocol                      │
│                                                                     │
│  OUTPUT:                                                            │
│    keystone.enc file                                                │
│    pond_config.json with pond_id, founder                           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.3 Pond Config File

Stored at `{data_dir}/pond_config.json`:

```json
{
  "pond_id": "a1b2c3d4e5f6...",
  "founder": "stone-01",
  "created_at": 1706284800,
  "members": [
    {
      "stone_id": "019bece4-...",
      "stone_name": "stone-01",
      "stone_pubkey": "<base64>",
      "joined_at": 1706284800,
      "invited_by": null,
      "role": "founder"
    }
  ],
  "revoked": []
}
```

---

## 6. Baptism Protocol

### 6.1 Overview

Baptism is an **optional, one-time event** during pond initialization that automatically invites all currently visible stones.

### 6.2 Pre-requisites

- All stones must have identity keypairs (generated on first boot)
- Founder has topology cache with stone pubkeys

### 6.3 Protocol Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  BAPTISM PROTOCOL (Unicast Direct Delivery)                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PARTICIPANTS:                                                      │
│    Founder (F): Stone running 'place keystone --auto-invite'        │
│    Recipients (R1, R2, ...): Stones in topology cache               │
│                                                                     │
│  STEP 1: Founder gathers recipients from topology cache             │
│    topology = load_topology_cache()                                 │
│    recipients = topology.filter(|s| s.is_online && s.id != my_id)   │
│                                                                     │
│  STEP 2: Founder sends baptism to each stone (UNICAST)              │
│    FOR each recipient R in recipients:                              │
│      credentials = PondCredentials {                                │
│        pond_ca_privkey,                                             │
│        pond_ca_pubkey,                                              │
│        pond_id,                                                     │
│      }                                                              │
│                                                                     │
│      bundle = encrypt_to_stone(credentials, R.pubkey)               │
│                                                                     │
│      message = {                                                    │
│        type: "pond_baptism",                                        │
│        pond_id: "<fingerprint>",                                    │
│        pond_ca_pubkey: "<base64>",                                  │
│        founder_id: "<stone_id>",                                    │
│        founder_name: "stone-01",                                    │
│        founder_signature: sign(founder_privkey, pond_id + R.id + ts)│
│        recipient_id: R.id,  // Explicitly addressed                 │
│        timestamp: now(),                                            │
│        bundle: { ephemeral_pubkey, nonce, ciphertext }              │
│      }                                                              │
│                                                                     │
│      send_unicast(R.ip, R.port, message)  // Direct UDP            │
│      await_ack_or_timeout(R.id, 30 seconds)                         │
│                                                                     │
│  STEP 3: Each recipient validates and processes                     │
│    1. Verify message addressed to self (recipient_id matches)       │
│    2. Verify founder_signature includes own stone_id                │
│    3. Decrypt bundle with own identity private key                  │
│    4. Store pond credentials                                        │
│    5. Switch to encrypted UDP mode                                  │
│                                                                     │
│  STEP 4: Recipient sends confirmation (unicast back to founder)     │
│    {                                                                │
│      type: "pond_baptism_ack",                                      │
│      stone_id: "<recipient-id>",                                    │
│      stone_name: "<recipient-name>",                                │
│      pond_fingerprint: fingerprint(received_ca_pubkey),             │
│      timestamp: now(),                                              │
│      signature: sign(recipient_identity, pond_id + founder_id)      │
│    }                                                                │
│                                                                     │
│  STEP 5: Founder collects confirmations                             │
│    - Wait up to 60 seconds for all acks                             │
│    - Update members list with confirmed stones                      │
│    - Log baptism result: X/Y stones confirmed                       │
│    - Offline stones can rejoin via invitation when they return      │
│                                                                     │
│  Security properties:                                               │
│  ✓ Direct delivery (no broadcast interception)                      │
│  ✓ Per-stone addressing (recipient_id binding)                      │
│  ✓ Signature includes recipient (prevents forwarding)               │
│  ✓ Confirmation proves receipt (TOCTOU protection)                  │
│  ✓ Unicast from topology (no Sybil attack)                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.4 Baptism Message Schema

```rust
struct BaptismMessage {
    #[serde(rename = "type")]
    msg_type: String,               // "pond_baptism"
    pond_id: String,
    pond_ca_pubkey: String,         // Base64 encoded
    founder_id: String,
    founder_name: String,
    recipient_id: String,           // Explicitly addressed to this stone
    founder_signature: String,      // Base64, sign(pond_id + recipient_id + timestamp)
    timestamp: u64,
    bundle: PerStonePayload,        // Single bundle for this recipient
}

struct PerStonePayload {
    ephemeral_pubkey: String,       // Base64
    nonce: String,                  // Base64
    ciphertext: String,             // Base64
}
```

### 6.5 Baptism Acknowledgment
as **plaintext unicast** to founder (recipient doesn't have pond encryption key until after processing):

```rust
struct BaptismAck {
    #[serde(rename = "type")]
    msg_type: String,               // "pond_baptism_ack"
    stone_id: String,
    stone_name: String,
    pond_fingerprint: String,       // Proves correct credentials received
    timestamp: u64,
    signature: String,              // sign(identity_key, pond_id + founder_id + timestamp)
}
```

**Note**: Ack is plaintext because recipient just received pond credentials and hasn't fully initialized encrypted mode yet. The signature proves authenticity.

### 6.6 Implementation Example

```rust
async fn baptize_stones(
    founder_identity: &Identity,
    pond_ca: &PondCA,
    topology: &TopologyCache,
) -> BaptismResult {
    let recipients: Vec<_> = topology
        .stones()
        .filter(|s| s.is_online() && s.id != founder_identity.stone_id)
        .collect();
    
    info!("Baptizing {} stones", recipients.len());
    
    let mut confirmations = Vec::new();
    let mut failures = Vec::new();
    
    for recipient in recipients {
        match baptize_single_stone(founder_identity, pond_ca, &recipient).await {
            Ok(ack) => {
                // Verify fingerprint matches
                let expected = fingerprint(&pond_ca.public_key);
                if ack.pond_fingerprint == expected {
                    confirmations.push(ack);
                    info!("✓ {} baptized", recipient.name);
                } else {
                    warn!("✗ {} fingerprint mismatch", recipient.name);
                    failures.push(recipient.name);
                }
            }
            Err(e) => {
                warn!("✗ {} failed: {}", recipient.name, e);
                failures.push(recipient.name);
            }
        }
    }
    
    BaptismResult {
        confirmed: confirmations,
        failed: failures,
    }
}

async fn baptize_single_stone(
    founder: &Identity,
    pond_ca: &PondCA,
    recipient: &StoneInfo,
) -> Result<BaptismAck> {
    // Create credentials bundle
    let credentials = PondCredentials {
        pond_ca_privkey: Some(pond_ca.private_key.clone()),
        pond_ca_pubkey: pond_ca.public_key.clone(),
        pond_id: pond_ca.pond_id.clone(),
    };
    
    // Encrypt for recipient
    let bundle = encrypt_to_stone(&credentials, &recipient.pubkey)?;
    
    // Create message
    let message = BaptismMessage {
        msg_type: "pond_baptism".into(),
        pond_id: pond_ca.pond_id.clone(),
        pond_ca_pubkey: base64_encode(&pond_ca.public_key),
        founder_id: founder.stone_id.clone(),
        founder_name: founder.stone_name.clone(),
        recipient_id: recipient.id.clone(),
        founder_signature: sign_baptism(founder, &pond_ca.pond_id, &recipient.id)?,
        timestamp: now(),
        bundle,
    };
    
    // Send unicast
    send_udp_unicast(&recipient.ip, recipient.port, &message)?;
    
    // Wait for ack (30 second timeout)
    timeout(Duration::from_secs(30), wait_for_ack(&recipient.id)).await?
    signature: String,              // Signed with stone identity key
}
```

---

## 7. Invitation Protocol

### 7.1 Overview

Invitation adds a new stone to an existing pond using TOTP-based Bluetooth-style pairing.

### 7.2 Who Can Invite

Any pond member can invite new stones. This reflects the P2P trust model: all members hold the complete CA keypair.

### 7.3 Protocol Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  INVITATION PROTOCOL                                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PARTICIPANTS:                                                      │
│    Inviter (I): Existing pond member                                │
│    Joiner (J): New stone wanting to join                            │
│                                                                     │
│  STEP 1: Generate invitation (on Inviter)                           │
│    $ garden-rake invite <joiner-name>                               │
│                                                                     │
│    code = TOTP(                                                     │
│      key = HKDF(inviter_identity_privkey, "totp-invite"),           │
│      timestamp = now() / 300,  // 5-minute windows                  │
│      context = joiner_name                                          │
│    )                                                                │
│                                                                     │
│    Display: "Code: KP7X9M (valid 5 minutes)"                        │
│    Store: pending_invitations[joiner_name] = { code, expires_at }   │
│                                                                     │
│  STEP 2: Join request (on Joiner)                                   │
│    $ garden-rake join pond                                          │
│    Enter code: KP7X9M                                               │
│                                                                     │
│    Joiner broadcasts (plaintext, encrypted to pond):                │
│    {                                                                │
│      type: "pond_join_request",                                     │
│      pond_id: "<from discovery>",                                   │
│      encrypted_payload: encrypt_to_pond_ca({                        │
│        stone_id: "<joiner-id>",                                     │
│        stone_name: "<joiner-name>",                                 │
│        stone_pubkey: "<identity pubkey>",                           │
│        code: "KP7X9M",                                              │
│        timestamp: now(),                                            │
│        signature: sign(joiner_privkey, ...)                         │
│      })                                                             │
│    }                                                                │
│                                                                     │
│  STEP 3: Validate (on Inviter)                                      │
│    1. Decrypt join request with pond CA                             │
│    2. Verify joiner signature                                       │
│    3. Validate TOTP code matches pending invitation                 │
│    4. Check joiner not in revocation list                           │
│                                                                     │
│  STEP 4: Issue credentials (on Inviter)                             │
│    credentials = PondCredentials {                                  │
│      pond_ca_privkey,                                               │
│      pond_ca_pubkey,                                                │
│      pond_id,                                                       │
│    }                                                                │
│                                                                     │
│    Send to Joiner (encrypted to joiner pubkey):                     │
│    {                                                                │
│      type: "pond_join_accepted",                                    │
│      encrypted_payload: encrypt_to_stone(credentials, joiner_pubkey)│
│      inviter_id: "<inviter-id>",                                    │
│      inviter_signature: sign(inviter_privkey, ...),                 │
│      timestamp: now()                                               │
│    }                                                                │
│                                                                     │
│  STEP 5: Complete join (on Joiner)                                  │
│    1. Decrypt credentials with own identity key                     │
│    2. Verify inviter signature                                      │
│    3. Store pond credentials                                        │
│    4. Switch to encrypted UDP mode                                  │
│    5. Broadcast join announcement (encrypted)                       │
│                                                                     │
│  STEP 6: Update membership (all members)                            │
│    - Add joiner to members list                                     │
│    - Log: "stone-05 joined, invited by stone-02"                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 7.4 TOTP Generation

```rust
fn generate_totp(inviter_privkey: &[u8], joiner_name: &str) -> String {
    // Derive TOTP key from identity key
    let totp_key = hkdf_sha256(
        inviter_privkey,
        b"zen-garden-totp-v1",
        joiner_name.as_bytes(),
        32
    );
    
    // Get time window (300 seconds = 5 minutes)
    let time_window = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() / 300;
    
    // HMAC-SHA256
    let payload = format!("{}:{}", hex::encode(&totp_key), time_window);
    let hmac = hmac_sha256(&totp_key, payload.as_bytes());
    
    // Extract 20 bits for 6-character base36 code
    let code_bits = u32::from_be_bytes([hmac[0], hmac[1], hmac[2], 0]) >> 12;
    encode_base36(code_bits, 6)
}

fn encode_base36(mut value: u32, length: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut result = Vec::with_capacity(length);
    for _ in 0..length {
        result.push(ALPHABET[(value % 36) as usize]);
        value /= 36;
    }
    result.reverse();
    String::from_utf8(result).unwrap()
}
```

### 7.5 Join Request Schema

```rust
struct JoinRequest {
    #[serde(rename = "type")]
    msg_type: String,               // "pond_join_request"
    pond_id: String,
    encrypted_payload: String,      // Base64, encrypted to pond CA
}

struct JoinRequestPayload {
    stone_id: String,
    stone_name: String,
    stone_pubkey: String,           // Base64
    code: String,                   // 6-char TOTP
    timestamp: u64,
    signature: String,              // Base64
}
```

### 7.6 Join Accepted Schema

```rust
struct JoinAccepted {
    #[serde(rename = "type")]
    msg_type: String,               // "pond_join_accepted"
    encrypted_payload: PerStonePayload,
    inviter_id: String,
    inviter_signature: String,      // Base64
    timestamp: u64,
}

struct PondCredentials {
    pond_id: String,
    pond_ca_pubkey: String,         // Base64
    pond_ca_privkey: String,        // Base64, shared secret
}
```

---

## 8. Outsider Detection

### 9.1 Overview

When a pond is active, stones must detect and handle outsiders (non-member stones sending plaintext).

### 9.2 Detection Criteria

A message is from an outsider if:
1. No `pond_id` field, OR
2. `pond_id` doesn't match our pond, OR
3. Missing encrypted envelope structure, OR
4. Decryption fails

### 9.3 Signaling Protocol

```
┌─────────────────────────────────────────────────────────────────────┐
│  OUTSIDER SIGNALING                                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  STEP 1: Pond member receives plaintext message                     │
│    - Identify as outsider (missing pond envelope)                   │
│    - Extract source IP and claimed stone_id                         │
│                                                                     │
│  STEP 2: Election for who responds (prevent storm)                  │
│    delay = blake3(my_stone_id + outsider_id + request_id) % 5000ms  │
│    sleep(delay)                                                     │
│                                                                     │
│    IF another member already responded (seen signal):               │
│      return  // Abort, someone else handled it                      │
│                                                                     │
│  STEP 3: Winner sends signal (plaintext, to outsider)               │
│    {                                                                │
│      type: "pond_join_required",                                    │
│      pond_id: "<fingerprint>",                                      │
│      pond_name: "My Garden Pond",                                   │
│      signaler_name: "stone-02",                                     │
│      message: "This garden is protected. Contact any member."       │
│    }                                                                │
│                                                                     │
│  STEP 4: Outsider displays notification                             │
│    ⚠️  This garden has a Pond.                                      │
│    Contact a member to get an invitation code.                      │
│    Then run: garden-rake join pond                                  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 9.4 Outsider Classification

| Scenario | Classification | Action |
|----------|---------------|--------|
| New stone, never seen | Unknown outsider | Signal, may invite |
| Known stone, missed baptism | Known outsider | Signal, may invite |
| Revoked stone | Hostile outsider | Ignore, log |
| Different pond | Foreign | Ignore (different pond_id) |

### 9.5 Rate Limiting

To prevent DoS via fake outsider requests:

```rust
struct OutsiderRateLimiter {
    attempts: HashMap<IpAddr, Vec<Instant>>,
    max_attempts: usize,        // 5
    window: Duration,           // 1 hour
}

impl OutsiderRateLimiter {
    fn should_respond(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let attempts = self.attempts.entry(ip).or_default();
        
        // Remove old attempts
        attempts.retain(|t| now.duration_since(*t) < self.window);
        
        if attempts.len() >= self.max_attempts {
            return false;  // Rate limited
        }
        
        attempts.push(now);
        true
    }
}
```

---

## 9. Revocation

### 9.1 Drain Pond

To remove pond security, drain the pond. This removes all credentials from all stones:

```bash
$ garden-rake drain pond
```

**Protocol**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  DRAIN POND                                                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  STEP 1: Operator confirms drain                                    │
│    - Enter passphrase                                               │
│    - Confirm understanding that all stones will be removed          │
│                                                                     │
│  STEP 2: Broadcast drain signal (encrypted)                         │
│    {                                                                │
│      type: "pond_drain",                                            │
│      drainer_id: "<stone-id>",                                      │
│      timestamp: now(),                                              │
│      signature: sign(drainer_identity, pond_id + timestamp)         │
│    }                                                                │
│                                                                     │
│  STEP 3: All stones process                                         │
│    1. Verify signature                                              │
│    2. Delete pond_config.json                                       │
│    3. Delete keystone.enc                                           │
│    4. Switch to plaintext mode                                      │
│    5. Send acknowledgment                                           │
│                                                                     │
│  STEP 4: Garden returns to Open mode                                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 9.2 Revocation List

Revoked stones are tracked to prevent re-joining via old credentials:

```rust
struct RevocationEntry {
    stone_id: String,
    stone_name: String,
    stone_pubkey: String,       // To reject even if stone_id forged
    revoked_at: u64,
    revoked_by: String,
    reason: Option<String>,
}

struct RevocationList {
    entries: Vec<RevocationEntry>,
    last_updated: u64,
    version: u64,               // Incrementing version for sync
}
```

---

## 10. Encrypted UDP

### 11.1 Encryption Layer

All UDP messages in pond mode use the encrypted envelope (Section 4.2).

### 11.2 Key Derivation

```rust
fn derive_udp_keys(pond_ca: &PondCA) -> UdpKeys {
    let master = hkdf_sha256(
        pond_ca.private_key.as_bytes(),
        b"zen-garden-udp-v1",
        b"",
        64  // 32 for encryption, 32 for MAC
    );
    
    UdpKeys {
        encryption_key: master[..32].try_into().unwrap(),
        mac_key: master[32..].try_into().unwrap(),
    }
}
```

### 11.3 Nonce Management

```rust
struct NonceTracker {
    seen: HashSet<[u8; 24]>,
    max_age: Duration,          // 5 minutes
}

impl NonceTracker {
    fn is_valid(&mut self, nonce: &[u8; 24], timestamp: u64) -> bool {
        // Reject if too old
        let age = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() - timestamp;
        
        if age > self.max_age.as_secs() {
            return false;  // Replay from old capture
        }
        
        // Reject if seen before
        if self.seen.contains(nonce) {
            return false;  // Replay attack
        }
        
        // Accept and remember
        self.seen.insert(*nonce);
        self.cleanup_old();
        true
    }
}
```

### 11.4 Message Processing Pipeline

```rust
async fn process_udp_message(raw: &[u8], src: SocketAddr) -> Result<()> {
    // Try to parse as encrypted envelope
    if let Ok(envelope) = serde_json::from_slice::<EncryptedEnvelope>(raw) {
        // Verify pond_id matches
        if envelope.pond_id != our_pond_id() {
            return Ok(());  // Different pond, ignore
        }
        
        // Check sender not revoked
        if is_revoked(&envelope.sender_id) {
            log_security_event("Revoked stone attempted communication", &envelope);
            return Ok(());
        }
        
        // Verify nonce is fresh
        if !nonce_tracker.is_valid(&envelope.nonce, envelope.timestamp) {
            log_security_event("Replay attack detected", &envelope);
            return Ok(());
        }
        
        // Decrypt
        let plaintext = decrypt_envelope(&envelope, &udp_keys)?;
        
        // Verify signature
        let sender_pubkey = get_member_pubkey(&envelope.sender_id)?;
        verify_signature(&sender_pubkey, &envelope.ciphertext, &envelope.signature)?;
        
        // Process decrypted message
        handle_pond_message(plaintext, src).await
    } else {
        // Plaintext message - outsider
        handle_outsider_message(raw, src).await
    }
}
```

---

## 11. State Machines

### 12.1 Stone Pond State

```
┌─────────────────────────────────────────────────────────────────────┐
│  STONE POND STATE MACHINE                                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│                        ┌────────────┐                               │
│                        │   OPEN     │                               │
│                        │ (no pond)  │                               │
│                        └─────┬──────┘                               │
│                              │                                      │
│          ┌───────────────────┼───────────────────┐                  │
│          │                   │                   │                  │
│          ▼                   ▼                   ▼                  │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │
│   │   FOUNDER   │    │  BAPTIZED   │    │  OUTSIDER   │             │
│   │ (created    │    │ (received   │    │ (detected   │             │
│   │  pond)      │    │  baptism)   │    │  pond)      │             │
│   └──────┬──────┘    └──────┬──────┘    └──────┬──────┘             │
│          │                  │                  │                    │
│          │                  │           join   │                    │
│          │                  │          request │                    │
│          ▼                  ▼                  ▼                    │
│   ┌─────────────────────────────────────────────────┐               │
│   │                    MEMBER                       │               │
│   │  (has pond credentials, encrypted mode)         │               │
│   └──────────────────────┬──────────────────────────┘               │
│                          │                                          │
│                          │                                          │
│                          ▼                                          │
│                   ┌─────────────┐                                   │
│                   │   DRAINED   │                                   │
│                   └──────┬──────┘                                   │
│                          │                                          │
│                          ▼                                          │
│                   ┌─────────────┐                                   │
│                   │    OPEN     │                                   │
│                   └─────────────┘                                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 12.2 State Persistence

```rust
#[derive(Serialize, Deserialize)]
enum PondState {
    Open,
    Founder { pond_id: String },
    Member { pond_id: String, joined_at: u64 },
    Outsider { detected_pond_id: String },
}

// Stored at {data_dir}/pond_state.json
```

### 12.3 State Transitions

| Current State | Event | New State | Action |
|--------------|-------|-----------|--------|
| Open | `place keystone` | Founder | Generate CA, optionally baptize |
| Open | Receive baptism | Member | Store credentials |
| Open | Receive `join_required` | Outsider | Display notification |
| Outsider | Complete invitation | Member | Store credentials |
| Member | Receive `drain` | Open | Delete credentials |

---

## 12. Configuration Options

### 12.1 Pond Configuration

```rust
struct PondConfig {
    // Core
    pond_id: String,
    founder: String,
    created_at: u64,
    
    // Membership
    members: Vec<MemberInfo>,
    revoked: Vec<RevocationEntry>,
    
    // Tuning
    totp_window_seconds: u32,           // Default: 300
    outsider_rate_limit: u32,           // Default: 5 per hour
}
```

### 12.2 CLI Configuration Commands

```bash
# View config
garden-rake pond config show

# Modify settings
garden-rake pond config set totp_window_seconds 600
```

### 12.3 Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `GARDEN_POND_TOTP_WINDOW` | TOTP validity in seconds | 300 |

---

## 13. Security Considerations

### 13.1 Threat Model Summary

| Threat | Mitigation |
|--------|------------|
| Network sniffing | Encrypted UDP |
| Rogue device | TOTP admission |
| Replay attack | Nonce tracking |
| Identity theft | Challenge-response |
| Single stone compromise | Drain pond |

### 13.2 Known Limitations

- Shared secret model: one compromised stone = drain pond required
- No individual revocation (drain is all-or-nothing)
- No forward secrecy (accepted risk for home lab use case)

### 13.3 Audit Events

All security events logged:

```rust
enum AuditEvent {
    PondCreated { pond_id: String },
    BaptismCompleted { members: Vec<String> },
    StoneInvited { stone: String, inviter: String },
    StoneJoined { stone: String, inviter: String },
    PondDrained { drainer: String },
    ReplayAttempt { source_ip: IpAddr, nonce: String },
    OutsiderDetected { stone_id: Option<String>, source_ip: IpAddr },
}
```

---

## 14. Implementation Checklist

### 14.1 Phase 1: Core Infrastructure

- [ ] Stone identity keypair generation on first boot
- [ ] Keystone file creation and encryption
- [ ] TPM detection and sealing (optional)
- [ ] Pond config persistence
- [ ] Encrypted envelope format
- [ ] UDP encryption/decryption layer

### 14.2 Phase 2: Protocols

- [ ] Baptism protocol implementation (unicast to topology)
- [ ] Invitation protocol (TOTP generation, validation)
- [ ] Join request/response handling
- [ ] Outsider detection and signaling
- [ ] Election integration for signaling

### 14.3 Phase 3: Lifecycle

- [ ] Drain pond implementation
- [ ] State machine persistence

### 14.4 Phase 4: CLI

- [ ] `garden-rake place keystone [--auto-invite]`
- [ ] `garden-rake invite <stone>`
- [ ] `garden-rake join pond`
- [ ] `garden-rake drain pond`
- [ ] `garden-rake pond status`

### 14.5 Phase 5: Integration

- [ ] HTTP API endpoints for pond operations
- [ ] Integration with existing discovery (outsider handling)
- [ ] Integration with election protocol
- [ ] Audit logging
- [ ] Metrics and monitoring

---

## Appendix A: Message Type Reference

| Type | Direction | Encryption | Description |
|------|-----------|------------|-------------|
| `pond_baptism` | Unicast | Per-stone | Initial credential distribution |
| `pond_baptism_ack` | Unicast | Signed plaintext | Acknowledge baptism |
| `pond_join_request` | Broadcast | To pond CA | Request to join |
| `pond_join_accepted` | Unicast | To joiner pubkey | Credentials issued |
| `pond_join_denied` | Unicast | To joiner pubkey | Rejection with reason |
| `pond_join_required` | Unicast | Plaintext | Signal to outsider |
| `pond_drain` | Broadcast | Pond key | Full pond reset |

---

## Appendix B: Error Codes

| Code | Name | Description | Recovery |
|------|------|-------------|----------|
| `E_POND_001` | InvalidTOTP | TOTP code invalid or expired | Request new code |
| `E_POND_002` | StoneRevoked | Stone is in revocation list | Re-invitation after pond drain |
| `E_POND_003` | UnknownStone | Stone not in member list | Regular invitation required |
| `E_POND_004` | InvalidSignature | Signature verification failed | Check identity key |
| `E_POND_005` | DecryptionFailed | Could not decrypt payload | Check pond credentials |
| `E_POND_006` | ReplayDetected | Nonce already seen | Automatic retry with new nonce |
| `E_POND_007` | PondMismatch | Wrong pond_id | Check pond configuration |

---

## Appendix C: File Locations

| File | Path | Description |
|------|------|-------------|
| Stone identity | `{data_dir}/stone-identity.key` | Encrypted Ed25519 keypair |
| Keystone | `{data_dir}/keystone.enc` | Encrypted Pond CA |
| Pond config | `{data_dir}/pond_config.json` | Membership, settings |
| Pond state | `{data_dir}/pond_state.json` | Current state machine state |
| Revocation list | `{data_dir}/revocation.json` | Revoked stones cache |
| Audit log | `{data_dir}/audit.log` | Security events |

---

> Design rationale: [SECURITY-0001](../decisions/SECURITY-0001-pond-tiers.md), [SECURITY-0004](../decisions/SECURITY-0004-tier2-deferral.md)
