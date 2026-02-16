# Pond Ceremony Engine

**Status:** Implemented (Phases 1–5 complete)
**Priority:** High
**Created:** 2025-06-13
**Updated:** 2026-02-15
**Authors:** Leo Botinelly
**Related:** [koi-embedded-integration](koi-embedded-integration.md), [pond-totp-admission](pond-totp-admission.md), [rake-client-enrollment](rake-client-enrollment.md), [SECURITY-0001](../decisions/SECURITY-0001-pond-tiers.md)

---

## Abstract

Replace Zen Garden's current non-interactive pond initialization with a **server-driven ceremony engine** — a multi-step state machine that guides users through pond creation, enrollment, and recovery. The engine lives in koi-certmesh, Moss wraps it as a single HTTP endpoint, and clients (Rake CLI, `/pond` web UI) render whatever the server sends. No client carries ceremony logic.

This addresses a critical UX gap: today, `garden-rake pond init` defaults the passphrase to `"changeme"`, skips profile selection, never shows a QR code, and offers no entropy collection. The ceremony engine makes every security decision explicit and interactive.

---

## Motivation

### Current State

Pond init is a single POST → response exchange:

```
Rake: POST /api/v1/pond/init { passphrase: "changeme", profile: "JustMe" }
Moss: tower::Service::oneshot("/create", CreateCaRequest) → CreateCaResponse
Moss: → 200 { cornerstone, totp_uri, ca_fingerprint }
```

Problems:

1. **No interactive prompts.** Profile defaults to `JustMe`. Passphrase defaults to `"changeme"`. The user is never asked.
2. **No QR code.** The TOTP URI is printed as raw text. Authenticator apps expect a scannable QR code.
3. **No entropy collection.** The 32-byte entropy seed required by `CreateCaRequest.entropy_hex` is generated server-side without user contribution.
4. **No auth mode selection.** koi-certmesh supports TOTP and FIDO2. The user is never offered the choice.
5. **No verification step.** After TOTP setup, the user should verify their authenticator produces a valid code before the ceremony completes.
6. **Ceremony logic leaks into clients.** If Rake ever needs a wizard, Rake carries the wizard. If the `/pond` web UI needs a wizard, it carries its own. Every client re-invents the flow.

### Target State

A single `POST /api/v1/pond/ceremony` endpoint that returns typed stages. Clients are dumb render loops — they display what the server says, collect what the server asks for, and post it back. The ceremony engine owns all sequencing, validation, and branching.

---

## Design

### Core Concept: Ceremonies as Constraint Satisfaction

A ceremony is **not** a linear sequence of stages. It is a **bag of key-value pairs** evaluated by **rules**. The rules function is:

```
evaluate(bag, render_hints) → { prompts[] | complete | error }
```

The rules inspect the current bag contents and determine:

1. **What data is missing?** — return prompts requesting it.
2. **Is any data contradictory?** — force re-collection of the conflicting keys.
3. **Is the bag complete and consistent?** — ceremony done, execute the action.

There is no stage index. There is no forward/backward cursor. The session is just a `Map<String, Value>` and the rules are a pure function over it.

### Why This Model

A linear pipeline (stage 1 → 2 → 3 → … → N) breaks the moment inputs interact:

- User selects `auth_mode = fido2`, then the client requests `qr_format = png`. FIDO2 has no QR code. A pipeline can't handle this without special "go back" logic.
- User selects profile `just_me` — fewer follow-up questions needed than `my_organization`.
- Prefill covers all keys — ceremony should complete in one round-trip, not march through N empty stages.

In the constraint-satisfaction model, none of these are edge cases. The rules simply see the bag, notice the contradiction or completeness, and respond accordingly.

### Architecture

```
┌──────────┐        ┌──────────┐        ┌───────────────┐
│  Client   │ ←───→ │   Moss   │ ←───→ │ koi-common    │
│ (Rake,    │  HTTP  │ (thin    │ in-   │ CeremonyHost  │
│  /pond)   │       │  wrapper) │ proc  │ + domain Rules│
└──────────┘        └──────────┘        └───────────────┘
```

- **koi-common** owns the generic `CeremonyHost` — session store, TTL, the `evaluate()` loop.
- **koi-certmesh** implements `CeremonyRules` for pond ceremonies (init, join, invite, unlock).
- **Moss** exposes a single `POST /api/v1/pond/ceremony` endpoint that calls `host.step()`.
- **Clients** are dumb render loops: display prompts, collect input, post it back.

### Session Lifecycle

Sessions are ephemeral, in-memory, and short-lived:

| Property | Value |
|----------|-------|
| Storage | `Mutex<HashMap<Uuid, Session>>` in `CeremonyHost` |
| ID | UUIDv7, returned in the first response |
| TTL | 5 minutes from last activity |
| Cleanup | Background task sweeps expired sessions every 60s |
| Concurrency | One active ceremony per engine (init is a singleton operation) |

The session is just a bag:

```rust
pub struct Session {
    pub id: Uuid,
    pub ceremony_type: String,
    pub bag: Map<String, Value>,       // the accumulated data
    pub render: RenderHints,           // client preferences
    pub created_at: Instant,
    pub last_active: Instant,
    pub complete: bool,
}
```

There is no `stage_name`, no `stages_completed`, no `total_stages`. The rules derive everything from the bag contents.

### The Step Protocol

Every interaction follows the same request/response shape:

**Request:**
```json
{
  "session_id": "uuid-or-null",
  "ceremony": "init",
  "data": { "profile": "just_me", "auth_mode": "totp" },
  "render": { "qr": "utf8" }
}
```

- `session_id`: `null` on first call to create a new session; the returned UUID on subsequent calls.
- `ceremony`: required on first call, ignored after that.
- `data`: 0-N key-value pairs. Merged into the bag. This serves both as "initial prefill from CLI flags" and as "the user's answer to whatever was last asked."
- `render`: client preferences for rich content (QR format, etc.).

**Response:**
```json
{
  "session_id": "018f...",
  "prompts": [ ... ],
  "messages": [ ... ],
  "complete": false,
  "error": null
}
```

- `prompts`: zero or more data requests. Empty only when `complete=true` or a fatal `error` is set.
- `messages`: informational content to display (instructions, QR codes, summaries). Can appear alongside prompts.
- `complete`: true when the ceremony is finished (success or fatal).
- `error`: validation or fatal error string.

### Prompts: How the Server Asks for Data

Each prompt tells the client exactly what to collect:

```json
{
  "key": "profile",
  "prompt": "Select a trust profile for your pond",
  "input_type": "select_one",
  "options": [
    { "value": "just_me",          "label": "Just Me",          "description": "..." },
    { "value": "my_team",          "label": "My Team",          "description": "..." },
    { "value": "my_organization",  "label": "My Organization",  "description": "..." }
  ],
  "required": true
}
```

| `input_type` | Meaning | Client rendering |
|-------------|---------|-----------------|
| `select_one` | Pick exactly one from `options` | Radio buttons / numbered list |
| `select_many` | Pick 1+ from `options` | Checkboxes |
| `text` | Free text input | Text field |
| `secret` | Like `text` but masked (passphrase) | Password field |
| `secret_confirm` | Two masked inputs that must match | Password + confirm |
| `code` | Short numeric/alphanumeric code (TOTP) | Code input |
| `entropy` | Raw bytes / keyboard mashing | Interactive area |
| `fido2` | Hardware key tap | WebAuthn prompt |

The server can return **multiple prompts in one response**. For example, the init ceremony's first evaluation of an empty bag might return:

```json
{
  "prompts": [
    { "key": "profile", "input_type": "select_one", "prompt": "Select a trust profile", "options": [...] },
    { "key": "auth_mode", "input_type": "select_one", "prompt": "Choose authentication method", "options": [...] }
  ]
}
```

The client collects both answers and sends them in one `data` payload.

### Messages: How the Server Shows Content

Messages carry informational content that doesn't require user input:

```json
{
  "kind": "qr_code",
  "title": "Scan this QR code with your authenticator app",
  "content": "data:image/png;base64,iVBOR..."
}
```

| `kind` | Content | Use |
|--------|---------|-----|
| `info` | Plain text instruction | Setup guidance, warnings |
| `qr_code` | QR data (format per `render.qr` hint) | TOTP enrollment |
| `summary` | Key-value pairs of completed ceremony data | Final results |
| `error` | Error detail with context | Non-fatal context |

Messages and prompts can appear in the **same response**. For instance: after the user selects `auth_mode=totp`, the rules evaluate the bag and return both a QR code message AND a verification code prompt:

```json
{
  "messages": [
    { "kind": "qr_code", "title": "Scan with your authenticator", "content": "data:image/png;base64,..." }
  ],
  "prompts": [
    { "key": "verification_code", "input_type": "code", "prompt": "Enter the 6-digit code" }
  ]
}
```

### Contradiction Handling

When the bag contains contradictory data, the rules **remove the conflicting keys** and re-prompt for them. Example:

1. User sends `{ "auth_mode": "fido2" }` — bag now `{ profile: "just_me", auth_mode: "fido2" }`.
2. Client sends `{ "qr_render": "png" }` in render hints. 
3. Rules notice: FIDO2 + QR makes no sense. Rules return a prompt for `auth_mode` again, with a message explaining the conflict.

The rules don't "go back" — they just observe that the bag is in an inconsistent state and ask for the key that needs correcting. The client sees a fresh prompt, doesn't know or care that it's a "retry."

### Prefill / CLI Automation

CLI flags populate the initial `data` payload:

```bash
garden-rake pond init --passphrase "hunter2" --profile just_me
```

Produces:
```json
{ "session_id": null, "ceremony": "init", "data": { "profile": "just_me", "passphrase": "hunter2" } }
```

The rules evaluate the bag. If all required keys are present and consistent, the ceremony completes in one round-trip. If something is invalid (e.g., passphrase = "changeme"), the response contains a prompt for the offending key with an error message.

### QR Code Rendering

QR rendering is controlled by the `render` hints in the request:

```json
{ "render": { "qr": "utf8" } }
```

| Value | Output | Use case |
|-------|--------|----------|
| `utf8` | Unicode block characters (█ ░) | Terminal / CLI |
| `png_base64` | Base64-encoded PNG data URI | Web UI / `<img>` tag |
| `uri_only` | Raw `otpauth://` URI, no visual | Programmatic use |

The server renders the QR code — clients never need a QR library. koi-crypto already depends on the `qrcode` crate.

### Error Handling

Validation errors include the offending key and re-prompt for it:

```json
{
  "session_id": "...",
  "prompts": [
    { "key": "passphrase", "input_type": "secret_confirm", "prompt": "Choose a passphrase..." }
  ],
  "error": "Passphrase must be at least 8 characters",
  "complete": false
}
```

Fatal errors (certmesh failure, I/O error) set `complete=true` with an error:

```json
{
  "session_id": "...",
  "prompts": [],
  "messages": [{ "kind": "error", "title": "Ceremony failed", "content": "Failed to create CA: disk full" }],
  "error": "Failed to create CA: disk full",
  "complete": true
}
```

---

## Ceremony Types

The engine supports multiple ceremony types. Each ceremony type is a set of **rules** that define what keys are required, how they interact, and what action to execute when the bag is complete.

| Ceremony | Required keys | Trigger |
|----------|--------------|---------|
| **Init** | `profile`, `entropy`, `passphrase_choice`, `passphrase`, `auth_mode`, `verification_code` | `garden-rake pond init` |
| **Join** | `join_code`, `verification_code` | `garden-rake pond join <code>` |
| **Invite** | `passphrase` | `garden-rake pond invite` |
| **Unlock** | `passphrase` | `garden-rake pond unlock` |

The init ceremony collects entropy first ("Mash your keyboard!"), derives an XKCD-style passphrase suggestion from it, and presents three choices: **Keep** the suggestion, **Mash again** (re-collect entropy), or **Enter my own** (manual `secret_confirm`). This matches the original Koi certmesh create experience.

### Auto-Unlock on Boot

After CA creation, the passphrase can optionally be saved to a local file (`auto-unlock-key` in the koi data directory) so the CA unlocks automatically when Moss restarts. This avoids requiring human intervention after power outages on headless machines.

**Mechanism:** If `auto-unlock-key` exists in the koi data directory, Moss reads it at boot and unlocks the CA. If the file is absent, the CA stays locked until `pond unlock`. The file contains the raw passphrase with restrictive permissions (0600 / ACL). Deleting the file switches to manual unlock; restoring it switches back. `pond remove` deletes it.

The threat model is honest: if an attacker has root on your Pi, they already have everything. Auto-unlock trades a theoretical protection (encrypted key with passphrase next to it) for real-world usability (garden survives power outages without human intervention).

### Unlock Method Selection

Step 3b of the ceremony asks how the pond should unlock after reboot:

| Option | Label | Description |
|--------|-------|-------------|
| `auto` | Auto-unlock (recommended) | Passphrase saved locally. Pond unlocks without human intervention. |
| `token` | Token authentication | Register a TOTP app or security key. Operator authenticates to unlock. |
| `passphrase` | Manual passphrase | Enter the passphrase on every boot. Most secure, least convenient. |

Standard profiles set a default: JustMe/MyTeam → `auto`, MyOrganization → `passphrase`. Custom profiles see a `select_one` prompt.

When `token` is selected, a **token registration sub-flow** runs at the end of the ceremony (after enrollment TOTP verification, before `Complete`):

```
... → 5. Enrollment TOTP (QR + verify code) →

6. Token registration sub-flow:
   6a. Token type? → select_one: "Authenticator app (TOTP)" / "Security key (FIDO2)"
   6b. If TOTP:
       - Generate unlock TOTP shared_secret (separate from enrollment TOTP)
       - Display QR code: "Scan with your authenticator app"
       - Prompt: "Enter the 6-digit code to verify"
       - Validate code → create TOTP unlock slot
   6c. If FIDO2:
       - Generate WebAuthn registration challenge
       - Prompt: "Tap your security key" (InputType::Fido2)
       - Validate attestation → create FIDO2 unlock slot

→ Complete (summary includes unlock method)
```

The sequencing is deliberate: the passphrase is still in memory (in the bag) when slot creation runs. The ceremony process can create the envelope encryption key slots atomically — no deferred setup, no second ceremony, no peers required.

### Envelope Encryption (Key Slots)

Token unlock requires changing the storage model from **passphrase-direct** (passphrase is the KDF input) to **envelope encryption with key slots** (LUKS-inspired):

```
CA Private Key
  └─ encrypted by ─→ Master Key (random 256-bit, generated once at init)
                        └─ unwrapped by any one Key Slot:
                           ┌─────────────────────────────────────────┐
                           │ Slot 0: Passphrase       (always)       │
                           │ Slot 1: Auto-unlock file (if selected)  │
                           │ Slot 2: TOTP             (if selected)  │
                           │ Slot 3: FIDO2 / FIDO2-PRF(if selected)  │
                           └─────────────────────────────────────────┘
```

Any single slot can independently unwrap the master key. Slot 0 (passphrase) is always present — you cannot lock yourself out.

#### Slot types and threat model

| Slot | Unlock gate | `slot_kek` location | Disk theft | Remote API attack |
|------|-------------|---------------------|------------|-------------------|
| Passphrase | User types passphrase | Derived via Argon2id (not stored) | Hard (brute-force KDF) | Blocked |
| Auto-unlock | None | Plaintext file on disk | Trivial | Trivial |
| TOTP | Valid 6-digit code | Derived from `shared_secret` on disk | Trivial (secret readable) | **Blocked** (need valid code) |
| FIDO2 (no PRF) | Physical key tap | Gated by assertion, on disk | Trivial (software gate) | **Blocked** (need physical key) |
| FIDO2-PRF | Physical key tap | In hardware (PRF output) | **Blocked** | **Blocked** |

TOTP is not meaningful against disk theft — but it blocks remote unlock without a valid code. This is a real threat for a Pi with port forwarding. FIDO2-PRF is the strongest local option: the secret never touches disk.

#### TOTP unlock slot — creation and use

**Creation** (during init ceremony, step 6b):
1. Generate `shared_secret` (RFC 6238), compute TOTP URI.
2. Display QR code. User scans with authenticator app.
3. User enters verification code. Validate against `shared_secret`.
4. `HKDF(shared_secret, "pond-unlock-slot") → slot_kek`.
5. `AES-256-GCM(master_key, slot_kek) → wrapped_blob`.
6. Store on disk: `{ shared_secret, salt, nonce, wrapped_blob }`.

**Unlock** (at boot, via `pond unlock --totp`):
1. User enters 6-digit TOTP code.
2. Verify against stored `shared_secret`.
3. Derive `slot_kek` from `shared_secret` using same HKDF.
4. Unwrap `master_key` from `wrapped_blob`.

Note: the `shared_secret` is on disk, so an attacker with disk access can derive `slot_kek` directly. The TOTP verification gates *remote* access (API callers who don't have disk access). For disk-theft protection, use FIDO2-PRF or passphrase.

#### FIDO2 unlock slot — creation and use

**Creation** (during init ceremony, step 6c, via `/pond` web UI):
1. Generate WebAuthn registration options (`challenge`, `rp`, `user`).
2. Client calls `navigator.credentials.create()`. User taps security key.
3. If key supports **PRF extension**: request PRF evaluation with a fixed salt.
   - `PRF(credential_id, salt) → prf_output → slot_kek`.
   - `AES-256-GCM(master_key, slot_kek) → wrapped_blob`.
   - Store: `{ credential_id, salt, nonce, wrapped_blob }`. No secret on disk.
4. If key lacks PRF: fall back to assertion-gated slot.
   - Generate random `slot_kek`, wrap master key.
   - Store: `{ credential_id, public_key, nonce, wrapped_blob, encrypted_slot_kek }`.
   - `encrypted_slot_kek` is released only after successful assertion verification.

**Unlock** (at boot, via `/pond` web UI):
1. Stone generates assertion challenge.
2. User taps security key. Browser returns assertion (+ PRF output if capable).
3. PRF path: derive `slot_kek` from PRF output → unwrap `master_key`.
4. No-PRF path: verify assertion against stored `public_key` → decrypt `slot_kek` → unwrap.

FIDO2 requires a browser — available only through the `/pond` web UI, not Rake CLI.

#### Peer escrow (future enhancement)

For MyOrganization deployments wanting defense-in-depth, the TOTP `shared_secret` (or FIDO2 `public_key` + `encrypted_slot_kek`) can be escrowed to a peer stone instead of stored locally. This eliminates the disk-theft weakness of TOTP and no-PRF FIDO2 slots by ensuring the verification secret and the unlock material are on different machines. Requires ≥2 stones in the pond. See peer-escrow protocol design (TBD).

#### Implementation status

| Component | Phase | Status |
|-----------|-------|--------|
| Envelope encryption in koi-crypto (master key + slot table) | Phase 5 | ✅ `unlock_slots.rs` |
| TOTP unlock slot creation + unlock ceremony | Phase 5 | ✅ `SlotTable::add_totp_slot()` |
| FIDO2 WebAuthn registration in `/pond` web UI | Phase 5 | ✅ `SlotTable::add_fido2_slot()` |
| FIDO2-PRF support (if key supports it) | Phase 5 | ✅ PRF path in `unwrap_with_fido2()` |
| Migration from passphrase-direct to envelope model | Phase 5 | ✅ `CertmeshCore` uses `SlotTable` |
| Peer escrow protocol | Phase 6 | Pending |
| Client enrollment (non-Moss Rake) | Phase 7 | Proposed — see [rake-client-enrollment](rake-client-enrollment.md) |

The rules are free to add derived keys to the bag during evaluation. For example, when `auth_mode=totp` is set, the rules generate a TOTP secret internally and store it in the bag as `_totp_secret` (underscore = internal, not prompted). The QR code message is derived from this internal key.

Future ceremonies (key rotation, backup/restore, promote) add new rule sets without changing the protocol.

---

## Pond Web UI (`/pond`)

### Serving Model

The `/pond` web UI follows the same embedded SPA pattern as the portrait:

```rust
const POND_HTML: &str = include_str!("../../../assets/pond.html");

pub async fn get_pond_page() -> impl IntoResponse {
    Html(POND_HTML)
}
```

Mounted in `configure_public()` alongside portrait — pond operations must always be reachable over HTTP (see router fix context below).

### Design Language

The `/pond` UI inherits the portrait's design system exactly:

| Element | Portrait Pattern | Pond Equivalent |
|---------|-----------------|-----------------|
| Color palette | `--bg-base: #f4f2ee`, sage/clay/gold accents | Same palette, same variables |
| Cards | `backdrop-filter: blur(14px)`, `--vellum-white` bg | Prompt/message cards |
| Typography | `ui-monospace` for data, system sans for prose | Same split |
| Gauges | 2px fills, accent colors | Activity indicator |
| Status dots | 5px with pulse animation | Ceremony state indicator |
| Accordions | Chevron + `max-height` transition | Message history (prior responses collapse) |
| Dark mode | `prefers-color-scheme: dark` with adjusted variables | Same media query |
| Texture | SVG fractalNoise grain overlay | Same `<svg class="vellum-grain">` |
| Transitions | `cubic-bezier(0.22, 1, 0.36, 1)`, 0.3s | Same easing for prompt transitions |
| Errors | `.panel-error` red background, dismissible | Validation errors same treatment |
| Buttons | `.action-btn` with two-tap confirmation | Ceremony "Continue" button |

### Page Structure

```
/pond
├── Header: "Pond" + status summary (active/locked/inactive)
├── Ceremony Panel (when ceremony is active):
│   ├── Messages area (QR codes, instructions, summaries)
│   ├── Prompts area (input fields, selections, code inputs)
│   ├── Error banner (validation errors, conflicts)
│   └── "Continue" button (submits all prompt answers)
├── Status Panel (when no ceremony is active):
│   ├── Pond name, profile, cornerstone
│   ├── Enrolled stones list (expandable items)
│   ├── CA fingerprint (monospace)
│   └── Actions: Invite, Unlock, Rename, Remove
└── Footer: same as portrait
```

### Ceremony Render Loop (JavaScript)

```javascript
async function advanceCeremony(data) {
    const res = await fetch('/api/v1/pond/ceremony', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            session_id: currentSessionId,
            ceremony: currentCeremony,
            data: data,
            render: { qr: 'png_base64' }
        })
    });
    const response = await res.json();
    currentSessionId = response.session_id;
    renderResponse(response);
}

function renderResponse(response) {
    if (response.complete) { renderComplete(response); return; }
    if (response.error) { showError(response.error); }
    // Render messages (QR codes, instructions, summaries)
    for (const msg of response.messages) { renderMessage(msg); }
    // Render prompts (inputs the user needs to fill)
    for (const prompt of response.prompts) { renderPrompt(prompt); }
    // All rendering is data-driven — no ceremony logic in JS
}
```

### Portrait Integration

The portrait (`/`) gains a read-only pond status block — not a management UI, just awareness:

```html
<!-- Pond Status (read-only) -->
<section id="pond-section" class="hidden" aria-labelledby="pond-title">
    <h2 class="section-title" id="pond-title">Pond</h2>
    <div class="card">
        <div class="pond-summary">
            <div class="dot"></div>
            <span id="pond-status">Active · 3 stones enrolled</span>
        </div>
        <a href="/pond" class="pond-manage-link">Manage →</a>
    </div>
</section>
```

This renders between the Offerings and Guidance sections. When pond is not active, the section shows "Not initialized" with a link to `/pond` to start the ceremony.

---

## Implementation Plan

### Phase 1: Engine Core (koi-common + koi-certmesh) ✅

1. Rewrite `CeremonyHost` with bag-based session model (koi-common)
2. Define `CeremonyRules` trait with single `evaluate()` method
3. Define `CeremonyRequest` / `CeremonyResponse` / `Prompt` / `Message` protocol types
4. Implement `PondCeremonyRules` in koi-certmesh with init ceremony rules
5. Add QR rendering support (utf8 + png_base64) using existing `qrcode` crate in koi-crypto
6. Unit tests for full ceremony flow with mock rules

### Phase 2: Moss Integration ✅

1. Add `POST /api/v1/pond/ceremony` handler in `pond.rs`
2. Wire `CeremonyHost<PondCeremonyRules>` into `AppState`
3. Add `/pond` route serving embedded `pond.html` in `configure_public()`
4. Add pond status data to portrait JSON response
5. Keep existing `POST /api/v1/pond/init` as deprecated (direct, non-interactive)

### Phase 3: Client Updates ✅

1. Update `garden-rake pond init` to use ceremony endpoint with terminal render loop
2. Add `--non-interactive` flag for CI/automation (sends all data in first call)
3. Build `pond.html` — ceremony wizard + status dashboard (with parseMarkdown renderer)
4. Update `garden-rake pond invite` to display QR code
5. Update `garden-rake pond join` to use ceremony for code input + verification

### Phase 4: Extended Ceremonies ✅

1. Add invite ceremony rules (`eval_invite` in `pond_ceremony.rs:863-886`)
2. Add join ceremony rules (`eval_join` in `pond_ceremony.rs:826-859`)
3. Add unlock ceremony rules (`eval_unlock` in `pond_ceremony.rs:890-1004`)
4. Portrait pond status section

**Note:** Join, invite, and unlock rules were implemented during Phase 1 rather than deferred. The unlock ceremony reads the slot table at evaluation time to determine available methods (passphrase, TOTP, FIDO2) and presents them as a `select_one` prompt when multiple methods exist.

### Phase 5: Envelope Encryption and Token Unlock ✅

1. Envelope encryption in koi-crypto (`unlock_slots.rs`) — master key + `SlotTable`
2. TOTP unlock slot creation + unlock ceremony
3. FIDO2 unlock slot creation (WebAuthn registration via `/pond` web UI)
4. FIDO2-PRF support (if key supports it)
5. Migration from passphrase-direct to envelope model
6. Unlock ceremony integration in Moss `pond.rs` (passphrase, TOTP, FIDO2 dispatch in `execute_pond_unlock_from_ceremony`)
7. Init ceremony token registration sub-flow (TOTP slot: lines 1437-1459, FIDO2 slot: lines 1461-1496 in `pond.rs`)

### Phase 6: Peer Escrow

1. Peer escrow protocol for TOTP `shared_secret` / FIDO2 credentials
2. Requires ≥2 stones in the pond

### Phase 7: Client Enrollment

1. Non-Moss Rake client enrollment via mDNS cornerstone discovery
2. Dedicated `POST /api/v1/pond/enroll-client` endpoint
3. Certificate installation in Moss-compatible directory
4. System trust store installation via `koi-truststore`
5. See [rake-client-enrollment](rake-client-enrollment.md) for full proposal

---

## Router Fix (Applied)

During early testing, `pond unlock` returned 404 because the `unlock` route was only in `configure()` (HTTPS), not `configure_public()` (HTTP). When pond is active, HTTPS may not work (CA is locked — that's WHY unlock is needed). This is a chicken-and-egg deadlock.

**Resolution:** All 12 pond routes are now in `configure_public()` (`router.rs:160-173`), including the ceremony endpoint. The rationale:

> Pond operations are the bootstrap/recovery path for the trust infrastructure itself.
> They are self-securing at the application layer (passphrases, TOTP codes), so must
> always be reachable over plain HTTP.

---

## Rust Types (Actual Implementation)

### Generic Framework (koi-common)

```rust
// koi-common/src/ceremony.rs — 1310 lines

/// The generic ceremony host — owns sessions, handles TTL, dispatches to rules.
pub struct CeremonyHost<R: CeremonyRules> {
    sessions: Mutex<HashMap<Uuid, Session>>,
    rules: R,
    ttl: Duration,  // default 5 min
}

/// A single ceremony session — just a bag + metadata.
pub struct Session {
    pub id: Uuid,
    pub ceremony_type: String,
    pub bag: Map<String, Value>,
    pub render: RenderHints,
    pub created_at: Instant,
    pub last_active: Instant,
    pub complete: bool,
}

/// The trait that domain logic implements.
pub trait CeremonyRules: Send + Sync {
    /// Validate ceremony type before creating a session.
    fn validate_ceremony_type(&self, ceremony: &str) -> Result<(), String>;

    /// Evaluate the bag, return what to show / ask / do next.
    fn evaluate(
        &self,
        ceremony_type: &str,
        bag: &mut Map<String, Value>,
        render: &RenderHints,
    ) -> EvalResult;
}

/// Input from the client.
pub struct CeremonyRequest {
    pub session_id: Option<Uuid>,
    pub ceremony: Option<String>,
    pub data: Map<String, Value>,
    pub render: Option<RenderHints>,
}

/// Output to the client.
pub struct CeremonyResponse {
    pub session_id: Uuid,
    pub prompts: Vec<Prompt>,
    pub messages: Vec<Message>,
    pub complete: bool,
    pub error: Option<String>,
    pub result_data: Option<Map<String, Value>>,  // final bag on completion
}

/// What the rules return.
pub enum EvalResult {
    /// Ceremony needs more data.
    NeedInput {
        prompts: Vec<Prompt>,
        messages: Vec<Message>,
    },
    /// Validation failed — re-prompt with error context.
    ValidationError {
        prompts: Vec<Prompt>,
        messages: Vec<Message>,
        error: String,
    },
    /// Bag is complete and consistent.
    Complete {
        messages: Vec<Message>,
    },
    /// Something is terminally wrong.
    Fatal(String),
}

/// A single data request.
pub struct Prompt {
    pub key: String,
    pub prompt: String,
    pub input_type: InputType,
    pub options: Vec<SelectOption>,
    pub required: bool,
}

pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

pub enum InputType {
    SelectOne,
    SelectMany,
    Text,
    Secret,
    SecretConfirm,
    Code,
    Entropy,
    Fido2,
}

pub struct Message {
    pub kind: MessageKind,
    pub title: String,
    pub content: String,
}

pub enum MessageKind {
    Info,
    QrCode,
    Summary,
    Error,
}

pub struct RenderHints {
    pub qr: QrFormat,
}

pub enum QrFormat {
    Utf8,
    PngBase64,
    UriOnly,
}
```

### Domain Logic (koi-certmesh)

```rust
// koi-certmesh/src/pond_ceremony.rs — 1417 lines

/// Stateless rule evaluator. All state lives in the session bag.
/// The host (and the HTTP handler above it) hold the CertmeshCore
/// needed to execute the terminal action.
pub struct PondCeremonyRules;

impl CeremonyRules for PondCeremonyRules {
    fn validate_ceremony_type(&self, ceremony: &str) -> Result<(), String> {
        match ceremony {
            "init" | "join" | "invite" | "unlock" => Ok(()),
            other => Err(format!("unknown pond ceremony: {other}")),
        }
    }

    fn evaluate(
        &self,
        ceremony_type: &str,
        bag: &mut Map<String, Value>,
        render: &RenderHints,
    ) -> EvalResult {
        match ceremony_type {
            "init"   => eval_init(bag, render),
            "join"   => eval_join(bag, render),
            "invite" => eval_invite(bag, render),
            "unlock" => eval_unlock(bag, render),
            _        => EvalResult::Fatal(format!("unknown ceremony")),
        }
    }
}

// eval_init: 740 lines of constraint-satisfaction logic
// eval_join: 33 lines (join_code → verification_code → complete)
// eval_invite: 23 lines (passphrase → complete)
// eval_unlock: 114 lines (reads SlotTable, offers select_one if multiple methods)
```

---

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| Engine lives in koi-certmesh, not Moss | CertMesh owns CA lifecycle; ceremony is CA ceremony |
| Single endpoint, not per-stage routes | Clients don't need to know stage order; server controls flow |
| `SecretVerification` naming | Implementation-agnostic — works for TOTP, FIDO2, future methods |
| Server-side QR rendering | Clients shouldn't need QR libraries; koi-crypto already has `qrcode` |
| Per-request `render` parameter | Respects client capabilities (terminal vs browser) |
| Prefill for CLI automation | `--passphrase` and `--profile` flags fast-forward past stages |
| Sessions in-memory, 5-min TTL | Ceremonies are short-lived; no persistence needed |
| No separate auth for ceremonies | The passphrase IS the authentication — ceremonies are self-securing |
| `/pond` as separate page from `/` | Portrait is read-only identity; pond is interactive management |
| All pond routes in HTTP lobby | Bootstrap/recovery path must not depend on the infrastructure it manages |
| Existing `pond/init` endpoint kept | Deprecated but preserved for backwards compatibility and CI scripts |

---

## Open Questions

1. **Back navigation:** Resolved — not needed. The constraint-satisfaction model inherently supports re-collection: when a value is invalid or contradictory, the rules remove the offending key from the bag and re-prompt. The "Mash again" flow (`pond_ceremony.rs:386-394`) demonstrates this — it clears entropy-related keys and re-evaluates. No explicit "back" action is required because there is no stage cursor to move backward through.
2. **Ceremony cancellation:** Resolved — session TTL handles this. Sessions expire after 5 minutes of inactivity (`ceremony.rs:67`). `sweep_expired()` cleans up on a background timer. No explicit cancel action was implemented; clients simply stop sending requests. This is acceptable given the short TTL and in-memory storage.
3. **Rate limiting:** Partially resolved. The ceremony rules themselves do not rate-limit TOTP verification retries — a bad code just re-prompts (`pond_ceremony.rs:589-606`). However, the operational enrollment layer enforces rate limiting: `RateLimiter` in `koi-crypto/src/totp.rs:19-260` locks out after 3 consecutive failures for 5 minutes. The ceremony's verification step is checking that the user set up their authenticator correctly (UX), while the enrollment endpoint gates actual certificate issuance (security). This split is intentional.
4. **Audit logging:** Resolved. The ceremony rules are pure functions with no side effects — they generate no audit entries. Audit logging lives in the certmesh operational layer (`koi-certmesh/src/audit.rs`): append-only timestamped entries for `member_joined`, `scope_violation`, `enrollment_closed`, `auth_rotated`, `backup_created/restored`, etc. The operator name collected during the init ceremony (bag key `operator`) flows through to the `approved_by` field in audit entries.
