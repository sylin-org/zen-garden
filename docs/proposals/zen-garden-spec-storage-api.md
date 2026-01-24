# Zen Garden Storage API Specification

**S3-compatible object storage for seed banks**

**Status:** Proposal  
**Date:** January 2026  
**Authors:** Collaborative design session

---

## Table of Contents

1. [Overview](#overview)
2. [Connection Strings](#connection-strings)
3. [Namespace Enforcement](#namespace-enforcement)
4. [SDK Integration](#sdk-integration)
5. [External Tool Integration](#external-tool-integration)
6. [Presigned URLs](#presigned-urls)
7. [Design Principles](#design-principles)
8. [API Compatibility](#api-compatibility)
9. [Authentication](#authentication)
10. [Endpoints](#endpoints)
11. [Operations](#operations)
12. [Error Handling](#error-handling)
13. [Streaming and Large Files](#streaming-and-large-files)
14. [Implementation Notes](#implementation-notes)
15. [Examples](#examples)

---

## Overview

### Purpose

The Storage API provides a simple, standardized interface for reading and writing data to seed banks. Each stone that has access to a seed bank (directly or as proxy) exposes this API.

### Design Goals

1. **S3-compatible subset** — Familiar semantics, existing tools work
2. **Simple** — Only the operations we need, nothing more
3. **Streaming** — Handle large files without buffering entirely in memory
4. **Per-stone** — Each stone exposes its own endpoint
5. **Transparent proxying** — Same API whether direct or proxied
6. **Unified connection strings** — Same pattern as all Zen Garden resources

### Base URL

```
http://{stone}.local:7180/api/v1/storage
```

Example:
```
http://stone-jade-lake.local:7180/api/v1/storage
```

---

## Connection Strings

### Format

```
zen-garden:s3//{app-name}[@{seed-bank-name}]
```

| Component | Required | Description |
|-----------|----------|-------------|
| `zen-garden:s3//` | Yes | Protocol identifier |
| `{app-name}` | No | App namespace (derived from binary if omitted) |
| `@{seed-bank-name}` | No | Specific seed bank (any available if omitted) |

### Examples

| Connection String | App Name | Seed Bank | Path Prefix |
|-------------------|----------|-----------|-------------|
| `zen-garden:s3//` | (derived) | any | `apps/{binary-name}/` |
| `zen-garden:s3//my-app` | my-app | any | `apps/my-app/` |
| `zen-garden:s3//@seed-glorious-dawn` | (derived) | seed-glorious-dawn | `apps/{binary-name}/` |
| `zen-garden:s3//my-app@seed-glorious-dawn` | my-app | seed-glorious-dawn | `apps/my-app/` |

### App Name Derivation

When app name is omitted, the SDK derives it automatically:

```rust
fn derive_app_name() -> Result<String> {
    // 1. Try binary name
    if let Some(name) = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string())) 
    {
        return Ok(sanitize_app_name(&name));
    }
    
    // 2. Fallback: require explicit
    Err("Could not derive app name. Use zen-garden:s3//explicit-name")
}

fn sanitize_app_name(name: &str) -> String {
    // Lowercase, alphanumeric + hyphens only
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}
```

Example: `./my-backup-tool` using `zen-garden:s3//` gets namespace `apps/my-backup-tool/`.

### Unified Pattern

Storage follows the same connection string pattern as all Zen Garden resources:

```
┌─────────────────────────────────────────────────────────────────┐
│              UNIFIED CONNECTION STRING PATTERN                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   zen-garden:mongodb//mydb                                      │
│       → Resolve "who has mongodb?"                              │
│       → Connect via MongoDB wire protocol                       │
│                                                                 │
│   zen-garden:redis//                                            │
│       → Resolve "who has redis?"                                │
│       → Connect via Redis protocol                              │
│                                                                 │
│   zen-garden:s3//my-app                                         │
│       → Resolve "who can serve storage?"                        │
│       → Connect via S3 protocol                                 │
│       → Namespace: apps/my-app/                                 │
│                                                                 │
│   zen-garden:s3//my-app@seed-glorious-dawn                      │
│       → Resolve "who serves seed-glorious-dawn?"                │
│       → Connect via S3 protocol                                 │
│       → Namespace: apps/my-app/                                 │
│                                                                 │
│   SAME PATTERN. SAME DISCOVERY. DIFFERENT PROTOCOLS.            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Resolution Flow

```
Application:
  connection = "zen-garden:s3//my-app@seed-glorious-dawn"
  
Zen Garden SDK:
  1. Parse: protocol=s3, app=my-app, seed_bank=seed-glorious-dawn
  2. Discovery: broadcast SEED_BANK_QUERY or check cache
  3. Response: stone-jade-lake has it (direct or proxy)
  4. Return S3 client configured with:
       endpoint: http://stone-jade-lake.local:7180/api/v1/storage
       bucket: garden
       path_prefix: apps/my-app/
       credentials: (garden credentials or zen-garden/zen-garden)

Application:
  s3.put_object("config.json", data)
  # Actually writes to: apps/my-app/config.json
```

### Dynamic Endpoint Resolution

When a USB seed bank moves between stones, the connection string remains unchanged:

```
Before (USB in stone-jade-lake):
  zen-garden:s3//my-app@seed-glorious-dawn
    → http://stone-jade-lake.local:7180/api/v1/storage

USB moved to stone-silver-stream...

After:
  zen-garden:s3//my-app@seed-glorious-dawn
    → http://stone-silver-stream.local:7180/api/v1/storage

Connection string unchanged. Endpoint resolved dynamically.
```

---

## Namespace Enforcement

### Storage Layout

```
seed-bank/
├── .zen-garden/
│   └── greetings                    # Seed bank identity
│
├── garden/                          # PROTECTED (Moss/cultivation only)
│   ├── offerings/
│   │   └── {offering-id}/
│   │       ├── identity.yaml
│   │       ├── latest -> {timestamp}/
│   │       └── {timestamp}/
│   │           ├── manifest.yaml
│   │           └── data.archive.gz
│   ├── stones/
│   │   └── {stone-id}/
│   │       ├── identity.yaml
│   │       └── moss.toml
│   └── index.yaml
│
└── apps/                            # APP NAMESPACE (external apps)
    ├── my-backup-tool/
    │   └── data/
    │       └── config.json
    ├── restic/
    │   └── ...
    └── velero/
        └── ...
```

### Access Rules

```
┌─────────────────────────────────────────────────────────────────┐
│                   STORAGE NAMESPACE RULES                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   External API requests (from apps via SDK):                    │
│   ──────────────────────────────────────────                    │
│   • MUST include X-App-Name header                              │
│   • Paths automatically prefixed: apps/{app-name}/              │
│   • Writing to root or garden/ → 403 Forbidden                  │
│   • Path traversal (../) → 403 Forbidden                        │
│                                                                 │
│   Internal requests (from Moss cultivation):                    │
│   ──────────────────────────────────────────                    │
│   • Include X-Internal-Request: true + signature                │
│   • Full root access                                            │
│   • Writes to garden/offerings/, garden/stones/, etc.           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Request Examples

**App request (via SDK):**

```http
PUT /api/v1/storage/data/config.json
X-App-Name: my-backup-tool
Authorization: ZenGarden ...

{data}
```

Server writes to: `apps/my-backup-tool/data/config.json`

**Missing app name → DENIED:**

```http
PUT /api/v1/storage/data/config.json
Authorization: ZenGarden ...
```

```http
403 Forbidden

{
  "error": "AppNameRequired",
  "message": "X-App-Name header required for storage access",
  "hint": "Use SDK with zen-garden:s3//your-app-name or set X-App-Name header"
}
```

**Path traversal attempt → DENIED:**

```http
PUT /api/v1/storage/../garden/offerings/evil.txt
X-App-Name: my-app
Authorization: ZenGarden ...
```

```http
403 Forbidden

{
  "error": "PathTraversal",
  "message": "Path cannot escape app namespace",
  "attempted_path": "../garden/offerings/evil.txt"
}
```

**Direct garden access attempt → DENIED:**

```http
PUT /api/v1/storage/garden/offerings/abc123/manifest.yaml
X-App-Name: my-app
Authorization: ZenGarden ...
```

```http
403 Forbidden

{
  "error": "ProtectedNamespace",
  "message": "Cannot write to garden/ namespace",
  "hint": "The garden/ namespace is reserved for system use"
}
```

### Internal Requests (Moss)

Moss uses signed internal requests for cultivation:

```http
PUT /api/v1/storage/garden/offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml
X-Internal-Request: true
X-Stone-Id: stone-jade-lake-abc123
X-Signature: ed25519:base64...
Content-Type: application/yaml

{manifest content}
```

The signature covers the request path, timestamp, and stone ID — preventing forgery.

---

## SDK Integration

### Rust

```rust
// Minimal - derive app name from binary, any seed bank
let storage = zen_garden::s3("zen-garden:s3//").await?;

// Explicit app name
let storage = zen_garden::s3("zen-garden:s3//my-app").await?;

// Specific seed bank, derived app name
let storage = zen_garden::s3("zen-garden:s3//@seed-glorious-dawn").await?;

// Fully explicit
let storage = zen_garden::s3("zen-garden:s3//my-app@seed-glorious-dawn").await?;

// All operations are namespaced automatically
storage.put("config.json", data).await?;           // → apps/my-app/config.json
storage.put("data/backup.tar.gz", data).await?;    // → apps/my-app/data/backup.tar.gz
let config = storage.get("config.json").await?;    // ← apps/my-app/config.json

// List within namespace
let entries = storage.list("data/").await?;        // Lists apps/my-app/data/*

// Get raw S3 client (still namespaced)
let s3_client = storage.s3_client();
s3_client.put_object()
    .bucket("garden")
    .key("config.json")                            // SDK prefixes automatically
    .body(ByteStream::from(data))
    .send()
    .await?;
```

### Python

```python
# Minimal
storage = zen_garden.s3("zen-garden:s3//")

# Explicit app name  
storage = zen_garden.s3("zen-garden:s3//my-app")

# Specific seed bank
storage = zen_garden.s3("zen-garden:s3//my-app@seed-glorious-dawn")

# All operations namespaced
storage.put("config.json", data)                   # → apps/my-app/config.json
config = storage.get("config.json")                # ← apps/my-app/config.json

# List within namespace
for entry in storage.list("data/"):
    print(f"{entry.key}: {entry.size} bytes")

# Get boto3 client (still namespaced via path prefix)
s3 = storage.s3_client()
s3.put_object(
    Bucket="garden",
    Key="config.json",                             # SDK prefixes automatically
    Body=data
)
```

### C# / .NET

```csharp
// Minimal
var storage = await ZenGarden.S3("zen-garden:s3//");

// Explicit app name
var storage = await ZenGarden.S3("zen-garden:s3//my-app");

// Specific seed bank
var storage = await ZenGarden.S3("zen-garden:s3//my-app@seed-glorious-dawn");

// All operations namespaced
await storage.PutAsync("config.json", data);       // → apps/my-app/config.json
var config = await storage.GetAsync("config.json"); // ← apps/my-app/config.json

// List within namespace
await foreach (var entry in storage.ListAsync("data/"))
{
    Console.WriteLine($"{entry.Key}: {entry.Size} bytes");
}
```

### Node.js / TypeScript

```typescript
// Minimal
const storage = await zenGarden.s3("zen-garden:s3//");

// Explicit app name
const storage = await zenGarden.s3("zen-garden:s3//my-app");

// Specific seed bank
const storage = await zenGarden.s3("zen-garden:s3//my-app@seed-glorious-dawn");

// All operations namespaced
await storage.put("config.json", data);            // → apps/my-app/config.json
const config = await storage.get("config.json");   // ← apps/my-app/config.json

// List within namespace  
for await (const entry of storage.list("data/")) {
    console.log(`${entry.key}: ${entry.size} bytes`);
}
```

### Go

```go
// Minimal
storage, err := zenGarden.S3("zen-garden:s3//")

// Explicit app name
storage, err := zenGarden.S3("zen-garden:s3//my-app")

// Specific seed bank
storage, err := zenGarden.S3("zen-garden:s3//my-app@seed-glorious-dawn")

// All operations namespaced
err = storage.Put("config.json", data)             // → apps/my-app/config.json
config, err := storage.Get("config.json")          // ← apps/my-app/config.json

// List within namespace
entries, err := storage.List("data/")
for _, entry := range entries {
    fmt.Printf("%s: %d bytes\n", entry.Key, entry.Size)
}
```

---

## External Tool Integration

### Local Resolver Proxy

For tools that can't use the Zen Garden SDK (standard `aws` CLI, `rclone`, etc.), run a local resolver proxy:

```bash
# Start the resolver proxy with app name
$ zen-garden-s3-proxy start --app my-backup-tool
Listening on localhost:9000
App namespace: apps/my-backup-tool/
Resolving storage requests to garden seed banks...

# Now any S3 tool works with dynamic resolution:
$ aws --endpoint-url http://localhost:9000 s3 ls s3://zen-garden/
                           PRE data/
                           PRE config/
2026-01-23 03:00:00       1234 settings.json

# These paths are actually apps/my-backup-tool/* on the seed bank
```

The proxy:
1. Receives S3 request for bucket `zen-garden`
2. Resolves seed bank via Zen Garden discovery protocol
3. Prefixes all paths with `apps/{app-name}/`
4. Forwards to actual stone endpoint
5. Returns response transparently

### Proxy Configuration

```bash
# Start with app name (required)
zen-garden-s3-proxy start --app my-backup-tool

# Custom port
zen-garden-s3-proxy start --app my-backup-tool --port 9001

# Specific seed bank
zen-garden-s3-proxy start --app my-backup-tool --seed-bank seed-glorious-dawn

# Run as daemon
zen-garden-s3-proxy start --app my-backup-tool --daemon
zen-garden-s3-proxy stop

# Check status
zen-garden-s3-proxy status
```

### Bucket Mapping

The proxy maps bucket names to seed banks:

| Bucket Name | Resolves To |
|-------------|-------------|
| `zen-garden` | Any available seed bank |
| `seed-glorious-dawn` | Specific seed bank |
| `seed-patient-winter` | Specific seed bank |

```bash
# Any seed bank
aws --endpoint-url http://localhost:9000 s3 ls s3://zen-garden/

# Specific seed bank
aws --endpoint-url http://localhost:9000 s3 ls s3://seed-glorious-dawn/
```

### rclone Configuration

```ini
# ~/.config/rclone/rclone.conf

# Via resolver proxy (recommended)
[zen-garden]
type = s3
provider = Other
endpoint = http://localhost:9000
access_key_id = zen-garden
secret_access_key = zen-garden
```

```bash
# Start proxy first
zen-garden-s3-proxy start --app rclone-backup

# bucket = zen-garden for any seed bank
rclone ls zen-garden:zen-garden/

# bucket = seed bank name for specific
rclone ls zen-garden:seed-glorious-dawn/

# Sync to local (paths are within app namespace)
rclone sync zen-garden:zen-garden/data/ ./local-backup/

# Check integrity
rclone check zen-garden:zen-garden/data/ ./local-backup/

# Copy between seed banks
rclone copy zen-garden:seed-glorious-dawn/ zen-garden:seed-patient-winter/
```

### AWS CLI Configuration

```bash
# Configure credentials (one-time)
aws configure set aws_access_key_id zen-garden --profile zen-garden
aws configure set aws_secret_access_key zen-garden --profile zen-garden
aws configure set region zen-garden --profile zen-garden

# Start proxy
zen-garden-s3-proxy start --app aws-backup

# Create alias
alias zg-s3='aws --endpoint-url http://localhost:9000 --profile zen-garden s3'

# Now use naturally (all paths within apps/aws-backup/)
zg-s3 ls s3://zen-garden/
zg-s3 cp ./data.tar.gz s3://zen-garden/backups/
zg-s3 sync ./backup/ s3://zen-garden/daily/2026-01-24/

# Specific seed bank
zg-s3 ls s3://seed-glorious-dawn/
```

### MinIO Client (mc)

```bash
# Start proxy
zen-garden-s3-proxy start --app mc-tools

# Configure alias
mc alias set zg http://localhost:9000 zen-garden zen-garden

# Use naturally (all paths within apps/mc-tools/)
mc ls zg/zen-garden/
mc cp ./data.tar.gz zg/zen-garden/backups/
mc mirror ./backup/ zg/zen-garden/daily/2026-01-24/
```

### restic Integration

```bash
# Start proxy for restic
zen-garden-s3-proxy start --app restic

# Configure restic
export AWS_ACCESS_KEY_ID=zen-garden
export AWS_SECRET_ACCESS_KEY=zen-garden
export RESTIC_REPOSITORY=s3:http://localhost:9000/zen-garden/restic-repo

# Initialize repository (stored in apps/restic/restic-repo/)
restic init

# Backup
restic backup /home/user/documents

# Restore
restic restore latest --target /tmp/restore
```

---

## Resource Mapping Summary

| Resource | Connection String | Protocol | Namespace |
|----------|------------------|----------|-----------|
| MongoDB | `zen-garden:mongodb//mydb` | MongoDB | `mydb` |
| PostgreSQL | `zen-garden:postgresql//mydb` | PostgreSQL | `mydb` |
| Redis | `zen-garden:redis//` | Redis | - |
| Redis DB | `zen-garden:redis//0` | Redis | DB 0 |
| S3 Storage | `zen-garden:s3//app-name` | S3 | `apps/app-name/` |
| S3 Storage | `zen-garden:s3//app@seed-name` | S3 | `apps/app/` on seed-name |

All resources follow the pattern:
```
zen-garden:{protocol}//[{name}][@{target}]
```

Discovery is unified. Protocol is standard. Endpoints resolve dynamically. Namespaces are enforced.

---

## Presigned URLs

> **🔒 Pond Only** — Presigned URLs require the cryptographic infrastructure provided by Pond. Dry gardens do not support this feature.

### Overview

Presigned URLs allow temporary, scoped access to storage without requiring authentication. The URL itself contains a cryptographic signature that proves authorization.

```
┌─────────────────────────────────────────────────────────────────┐
│                    PRESIGNED URL FLOW                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. App requests presigned URL                                 │
│      POST /api/v1/storage/presign                               │
│                                                                 │
│   2. Moss generates signed URL                                  │
│      - Signs with garden private key (from Keystone)            │
│      - Embeds expiration, path, operation                       │
│                                                                 │
│   3. App shares URL                                             │
│      - Email, chat, CI/CD config, etc.                          │
│      - No credentials needed by recipient                       │
│                                                                 │
│   4. Recipient uses URL directly                                │
│      - curl, wget, browser, any HTTP client                     │
│      - Moss verifies signature with garden public key           │
│      - Access granted if valid and not expired                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Use Cases

| Scenario | Operation | Description |
|----------|-----------|-------------|
| Share backup with friend | `GET` | Temporary download link |
| External tool uploads | `PUT` | Let restic/rclone push without credentials |
| Cross-garden sharing | `GET` | Give another garden access to specific file |
| Audit access | `GET` | Auditor gets time-limited read access |
| CI/CD integration | `PUT`/`GET` | Build artifacts without storing credentials |
| Browser downloads | `GET` | Direct download link for web UI |
| Webhook uploads | `PUT` | External service pushes data |

### URL Format

```
http://{stone}.local:7180/api/v1/storage/{path}
  ?X-Zen-Signature={signature}
  &X-Zen-Expires={expiration}
  &X-Zen-App={app-name}
  &X-Zen-Operation={operation}
  &X-Zen-Garden={garden-id}
```

**Example:**

```
http://stone-jade-lake.local:7180/api/v1/storage/apps/my-app/data/backup.tar.gz
  ?X-Zen-Signature=ed25519:3Kf8sJ2mNpQrT...
  &X-Zen-Expires=2026-01-23T04:00:00Z
  &X-Zen-App=my-app
  &X-Zen-Operation=GET
  &X-Zen-Garden=jade-mountain-abc123
```

### API Endpoint

#### Generate Presigned URL

```http
POST /api/v1/storage/presign
Authorization: ZenGarden ...
Content-Type: application/json

{
  "path": "data/backup.tar.gz",
  "operation": "GET",
  "expires_in": "1h"
}
```

**Response:**

```http
201 Created
Content-Type: application/json

{
  "url": "http://stone-jade-lake.local:7180/api/v1/storage/apps/my-app/data/backup.tar.gz?X-Zen-Signature=ed25519:3Kf8sJ2mNpQrT...&X-Zen-Expires=2026-01-23T04:00:00Z&X-Zen-App=my-app&X-Zen-Operation=GET&X-Zen-Garden=jade-mountain-abc123",
  "expires": "2026-01-23T04:00:00Z",
  "path": "apps/my-app/data/backup.tar.gz",
  "operation": "GET"
}
```

#### Request Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path within app namespace |
| `operation` | string | Yes | `GET`, `PUT`, `DELETE`, or `LIST` |
| `expires_in` | duration | Yes | How long until expiration (e.g., `1h`, `30m`, `7d`) |
| `content_type` | string | No | Required Content-Type for PUT (validation) |
| `max_size` | integer | No | Maximum upload size in bytes for PUT |
| `filename` | string | No | Suggested filename for Content-Disposition header |

#### Duration Format

| Format | Meaning |
|--------|---------|
| `30s` | 30 seconds |
| `15m` | 15 minutes |
| `1h` | 1 hour |
| `24h` | 24 hours |
| `7d` | 7 days |
| `1h30m` | 1 hour 30 minutes |

**Maximum expiration:** 7 days (configurable per garden)

### Signature Structure

The signature covers all claims to prevent tampering:

```rust
struct PresignedClaims {
    // What
    path: String,              // "apps/my-app/data/backup.tar.gz"
    operation: Operation,      // GET, PUT, DELETE, LIST
    
    // When
    expires: DateTime<Utc>,    // When signature stops being valid
    issued_at: DateTime<Utc>,  // When signature was created
    
    // Who
    app: String,               // Issuing app namespace
    garden_id: String,         // Which garden issued it
    issued_by_stone: String,   // Which stone created it
    
    // Constraints (optional)
    content_type: Option<String>,   // Required Content-Type for PUT
    max_size: Option<u64>,          // Max upload size for PUT
}

impl PresignedClaims {
    fn to_signing_string(&self) -> String {
        // Canonical format for signing
        format!(
            "{}:{}:{}:{}:{}:{}",
            self.path,
            self.operation,
            self.expires.to_rfc3339(),
            self.app,
            self.garden_id,
            self.content_type.as_deref().unwrap_or(""),
        )
    }
}

// Sign with garden's private key
let signature = garden_private_key.sign(claims.to_signing_string().as_bytes());
```

### Verification Flow

Any Moss in the Pond can verify presigned URLs:

```rust
async fn verify_presigned_url(
    &self,
    url: &PresignedUrl,
    request: &Request,
) -> Result<(), PresignError> {
    // 1. Check garden ID matches
    if url.garden_id != self.garden.id {
        return Err(PresignError::WrongGarden);
    }
    
    // 2. Check not expired
    if Utc::now() > url.expires {
        return Err(PresignError::Expired);
    }
    
    // 3. Check operation matches
    if url.operation != request.method().into() {
        return Err(PresignError::OperationMismatch);
    }
    
    // 4. Check path matches
    if url.path != request.path() {
        return Err(PresignError::PathMismatch);
    }
    
    // 5. Verify signature with garden public key
    let claims_string = url.claims.to_signing_string();
    if !self.garden.public_key.verify(claims_string.as_bytes(), &url.signature) {
        return Err(PresignError::InvalidSignature);
    }
    
    // 6. Check content-type constraint (for PUT)
    if let Some(required_ct) = &url.content_type {
        if request.content_type() != Some(required_ct) {
            return Err(PresignError::ContentTypeMismatch);
        }
    }
    
    // 7. Check size constraint (for PUT)
    if let Some(max_size) = url.max_size {
        if request.content_length() > Some(max_size) {
            return Err(PresignError::TooLarge);
        }
    }
    
    Ok(())
}
```

### Cross-Stone Validity

Presigned URLs work across any stone in the garden:

```
1. stone-jade-lake issues presigned URL for seed-glorious-dawn
2. USB seed bank moves to stone-silver-stream
3. Client uses URL (now routes to stone-silver-stream)
4. stone-silver-stream verifies signature with garden public key
5. Signature valid → file served

The signature is garden-scoped, not stone-scoped.
Any Moss with the garden public key can verify.
```

### SDK Integration

#### Rust

```rust
// Presign GET (download)
let url = storage.presign_get("data/backup.tar.gz", Duration::hours(1)).await?;
println!("Download link: {}", url);

// Presign PUT (upload)
let url = storage.presign_put("uploads/incoming.tar.gz", Duration::minutes(30)).await?;
println!("Upload to: {}", url);

// Presign with full options
let url = storage.presign("data/backup.tar.gz", PresignOptions {
    operation: Operation::Get,
    expires_in: Duration::hours(24),
    filename: Some("my-backup.tar.gz".into()),  // Content-Disposition
}).await?;

// Presign PUT with constraints
let url = storage.presign("uploads/photo.jpg", PresignOptions {
    operation: Operation::Put,
    expires_in: Duration::minutes(15),
    content_type: Some("image/jpeg".into()),   // Must be JPEG
    max_size: Some(10 * 1024 * 1024),          // Max 10MB
}).await?;

// Presign LIST (directory listing)
let url = storage.presign("data/2026-01/", PresignOptions {
    operation: Operation::List,
    expires_in: Duration::hours(1),
}).await?;
```

#### Python

```python
# Presign GET
url = storage.presign_get("data/backup.tar.gz", expires_in=timedelta(hours=1))
print(f"Download link: {url}")

# Presign PUT
url = storage.presign_put("uploads/incoming.tar.gz", expires_in=timedelta(minutes=30))
print(f"Upload to: {url}")

# Presign with options
url = storage.presign("data/backup.tar.gz", 
    operation="GET",
    expires_in=timedelta(hours=24),
    filename="my-backup.tar.gz"
)

# Presign PUT with constraints
url = storage.presign("uploads/photo.jpg",
    operation="PUT",
    expires_in=timedelta(minutes=15),
    content_type="image/jpeg",
    max_size=10 * 1024 * 1024
)
```

#### Node.js / TypeScript

```typescript
// Presign GET
const url = await storage.presignGet("data/backup.tar.gz", { expiresIn: "1h" });
console.log(`Download link: ${url}`);

// Presign PUT
const url = await storage.presignPut("uploads/incoming.tar.gz", { expiresIn: "30m" });
console.log(`Upload to: ${url}`);

// Presign with options
const url = await storage.presign("data/backup.tar.gz", {
    operation: "GET",
    expiresIn: "24h",
    filename: "my-backup.tar.gz"
});

// Presign PUT with constraints
const url = await storage.presign("uploads/photo.jpg", {
    operation: "PUT",
    expiresIn: "15m",
    contentType: "image/jpeg",
    maxSize: 10 * 1024 * 1024
});
```

#### Go

```go
// Presign GET
url, err := storage.PresignGet("data/backup.tar.gz", 1*time.Hour)
fmt.Printf("Download link: %s\n", url)

// Presign PUT
url, err := storage.PresignPut("uploads/incoming.tar.gz", 30*time.Minute)
fmt.Printf("Upload to: %s\n", url)

// Presign with options
url, err := storage.Presign("data/backup.tar.gz", PresignOptions{
    Operation: OperationGet,
    ExpiresIn: 24 * time.Hour,
    Filename:  "my-backup.tar.gz",
})

// Presign PUT with constraints
url, err := storage.Presign("uploads/photo.jpg", PresignOptions{
    Operation:   OperationPut,
    ExpiresIn:   15 * time.Minute,
    ContentType: "image/jpeg",
    MaxSize:     10 * 1024 * 1024,
})
```

### Using Presigned URLs

#### Download (GET)

```bash
# Simple download
curl -o backup.tar.gz "http://stone.local:7180/api/v1/storage/apps/my-app/data/backup.tar.gz?X-Zen-Signature=..."

# With wget
wget -O backup.tar.gz "http://stone.local:7180/api/v1/storage/apps/my-app/data/backup.tar.gz?X-Zen-Signature=..."

# Browser: just open the URL
```

#### Upload (PUT)

```bash
# Simple upload
curl -X PUT \
  -H "Content-Type: application/gzip" \
  --data-binary @myfile.tar.gz \
  "http://stone.local:7180/api/v1/storage/apps/my-app/uploads/myfile.tar.gz?X-Zen-Signature=..."

# With content-type constraint (must match)
curl -X PUT \
  -H "Content-Type: image/jpeg" \
  --data-binary @photo.jpg \
  "http://stone.local:7180/api/v1/storage/apps/my-app/uploads/photo.jpg?X-Zen-Signature=..."
```

#### List Directory (LIST)

```bash
# Returns JSON listing
curl "http://stone.local:7180/api/v1/storage/apps/my-app/data/2026-01/?X-Zen-Signature=...&X-Zen-Operation=LIST"
```

### Error Responses

| Error | HTTP Status | Description |
|-------|-------------|-------------|
| `PresignedUrlExpired` | 403 | URL has expired |
| `InvalidSignature` | 403 | Signature verification failed |
| `OperationMismatch` | 403 | Request method doesn't match signed operation |
| `PathMismatch` | 403 | Request path doesn't match signed path |
| `ContentTypeMismatch` | 400 | Content-Type doesn't match constraint |
| `PayloadTooLarge` | 413 | Upload exceeds max_size constraint |
| `WrongGarden` | 403 | URL was signed by different garden |

**Example error response:**

```http
403 Forbidden
Content-Type: application/json

{
  "error": "PresignedUrlExpired",
  "message": "This presigned URL expired at 2026-01-23T04:00:00Z",
  "expired_at": "2026-01-23T04:00:00Z",
  "current_time": "2026-01-23T05:30:00Z"
}
```

### Security Considerations

#### Signature Binding

The signature is bound to:
- **Exact path** — Can't modify path to access other files
- **Operation** — GET URL can't be used for PUT
- **Expiration** — Time-limited by design
- **Garden** — Can't use URL from another garden

#### Revocation

Presigned URLs cannot be individually revoked. Mitigation strategies:

| Strategy | Description |
|----------|-------------|
| Short expiration | Use shortest practical duration |
| Rotate garden keys | Emergency: rotate Keystone (invalidates ALL presigned URLs) |
| Delete the file | URL becomes 404 |
| Move the file | URL becomes 404 (path no longer matches) |

#### Audit Trail

All presigned URL usage is logged:

```json
{
  "event": "presigned_url_used",
  "timestamp": "2026-01-23T03:45:00Z",
  "path": "apps/my-app/data/backup.tar.gz",
  "operation": "GET",
  "issued_by_app": "my-app",
  "issued_by_stone": "stone-jade-lake",
  "issued_at": "2026-01-23T03:00:00Z",
  "client_ip": "192.168.1.50",
  "user_agent": "curl/7.81.0"
}
```

#### Content-Type Enforcement

For PUT operations, `content_type` constraint prevents:
- Uploading executable disguised as image
- MIME confusion attacks
- Unexpected file types

```rust
// Only accept JPEG images
let url = storage.presign("uploads/photo.jpg", PresignOptions {
    operation: Operation::Put,
    content_type: Some("image/jpeg".into()),
    ..Default::default()
}).await?;

// Client must set Content-Type: image/jpeg or request fails
```

### Configuration

Garden-level presigned URL settings:

```toml
# garden.toml (via Keystone)

[storage.presign]
enabled = true
max_expiration = "7d"              # Maximum allowed expiration
default_expiration = "1h"          # Default if not specified
allowed_operations = ["GET", "PUT", "LIST"]  # DELETE disabled by default

# Per-app overrides
[storage.presign.apps.untrusted-app]
max_expiration = "1h"              # Shorter max for this app
allowed_operations = ["GET"]       # Read-only
```

### Dry Garden Behavior

In dry gardens (no Pond), presigned URL endpoints return:

```http
POST /api/v1/storage/presign

501 Not Implemented
Content-Type: application/json

{
  "error": "PondRequired",
  "message": "Presigned URLs require Pond security layer",
  "hint": "Enable Pond with: garden-rake pond"
}
```

### Path Model

Paths are hierarchical, like a filesystem:

```
/api/v1/storage/{path}

Examples:
/api/v1/storage/offerings/abc123/latest/manifest.yaml
/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz
/api/v1/storage/stones/xyz789/identity.yaml
/api/v1/storage/index.yaml
```

Paths map to the seed bank filesystem:

```
{seed-bank-mount}/garden/{path}
```

---

## Design Principles

### Why S3-Compatible?

S3 is the de facto standard for object storage. Benefits:

| Benefit | Description |
|---------|-------------|
| **Familiarity** | Developers know S3 semantics |
| **Tooling** | `aws cli`, `rclone`, `restic`, `mc` all work |
| **Libraries** | Every language has mature S3 clients |
| **Testing** | Can test against MinIO locally |
| **Future-proof** | Could add full S3 compatibility later |

### What We Implement

Core S3 operations only:

| Operation | S3 Equivalent | Included |
|-----------|---------------|----------|
| Write object | `PutObject` | ✓ |
| Read object | `GetObject` | ✓ |
| Delete object | `DeleteObject` | ✓ |
| Check exists / metadata | `HeadObject` | ✓ |
| List objects | `ListObjectsV2` | ✓ |
| Copy object | `CopyObject` | ✓ |
| Multipart upload | `CreateMultipartUpload`, etc. | ✓ (simplified) |

### What We Skip

Complex S3 features we don't need:

| Feature | Reason to Skip |
|---------|----------------|
| Buckets | We use path prefixes instead |
| ACLs | Pond handles security at garden level |
| Versioning | We use timestamped directories |
| Lifecycle policies | We handle retention in cultivation logic |
| Server-side encryption | Pond encrypts at application level |
| Presigned URLs | Not needed for stone-to-stone communication |

---

## API Compatibility

### S3 Signature

For tools that expect S3 authentication, we support AWS Signature Version 4 with:

- **Access Key**: `zen-garden` (or configurable)
- **Secret Key**: Garden-specific (from Pond) or `zen-garden` for dry gardens
- **Region**: `zen-garden`
- **Service**: `s3`

This allows using standard S3 tools:

```bash
# Configure AWS CLI
aws configure set aws_access_key_id zen-garden
aws configure set aws_secret_access_key zen-garden
aws configure set region zen-garden

# Use with endpoint override
aws --endpoint-url http://stone-jade-lake.local:7180/api/v1/storage \
    s3 ls s3://garden/offerings/

# Or use s3api
aws --endpoint-url http://stone-jade-lake.local:7180/api/v1/storage \
    s3api get-object --bucket garden --key offerings/abc123/latest/manifest.yaml output.yaml
```

### Path Style vs Virtual Host

We use **path-style** addressing only:

```
# Path style (supported)
http://stone.local:7180/api/v1/storage/garden/path/to/object

# Virtual host style (NOT supported)
http://garden.stone.local:7180/path/to/object
```

### Bucket Mapping

S3 clients expect a bucket. We map the bucket name `garden` to our storage root:

```
S3 path:     s3://garden/offerings/abc123/manifest.yaml
Maps to:     {seed-bank}/garden/offerings/abc123/manifest.yaml
API path:    /api/v1/storage/offerings/abc123/manifest.yaml
```

The `garden` bucket is implicit in our REST API but explicit for S3 clients.

---

## Authentication

### Dry Gardens

No authentication required. All stones in the garden are trusted.

```http
GET /api/v1/storage/offerings/abc123/manifest.yaml

# No Authorization header needed
```

### Pond Gardens

Use garden credentials derived from Keystone:

```http
GET /api/v1/storage/offerings/abc123/manifest.yaml
Authorization: ZenGarden stone-id=xyz789,signature=base64...
```

Or S3-compatible signature:

```http
GET /api/v1/storage/offerings/abc123/manifest.yaml
Authorization: AWS4-HMAC-SHA256 Credential=.../zen-garden/s3/aws4_request, ...
```

### Proxied Requests

When stone-01 proxies for stone-03, stone-03's credentials are forwarded:

```http
# stone-03 → stone-01 (proxy)
GET /api/v1/storage/offerings/abc123/manifest.yaml
Authorization: ZenGarden stone-id=stone-03-id,signature=...
X-Forwarded-For: stone-03
```

Stone-01 validates stone-03 is a garden member, then performs the operation.

---

## Endpoints

### Summary

| Method | Path | Operation |
|--------|------|-----------|
| `PUT` | `/{path}` | Write object |
| `GET` | `/{path}` | Read object |
| `DELETE` | `/{path}` | Delete object |
| `HEAD` | `/{path}` | Get metadata |
| `GET` | `/?prefix={p}&list` | List objects |
| `POST` | `/{path}?copy` | Copy object |
| `POST` | `/{path}?uploads` | Initiate multipart |
| `PUT` | `/{path}?partNumber={n}&uploadId={id}` | Upload part |
| `POST` | `/{path}?uploadId={id}` | Complete multipart |
| `DELETE` | `/{path}?uploadId={id}` | Abort multipart |

### URL Structure

```
http://{stone}.local:7180/api/v1/storage/{path}?{query}
```

---

## Operations

### PUT Object

Write an object to storage.

**Request:**

```http
PUT /api/v1/storage/{path}
Content-Type: {mime-type}
Content-Length: {size}
Content-MD5: {base64-md5}              # Optional, for integrity
X-Content-SHA256: {hex-sha256}         # Optional, for integrity

{binary data}
```

**Response (success):**

```http
201 Created
ETag: "{md5-hash}"
X-Content-SHA256: {hex-sha256}

{
  "path": "offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml",
  "size": 1234,
  "etag": "d41d8cd98f00b204e9800998ecf8427e",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

**Response (conflict):**

```http
409 Conflict

{
  "error": "ObjectAlreadyExists",
  "message": "Object already exists at this path",
  "path": "offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml"
}
```

**Overwrite behavior:**

By default, PUT fails if object exists. To overwrite:

```http
PUT /api/v1/storage/{path}
X-Allow-Overwrite: true
```

**Example:**

```bash
curl -X PUT \
  -H "Content-Type: application/yaml" \
  --data-binary @manifest.yaml \
  http://stone-jade-lake.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml
```

---

### GET Object

Read an object from storage.

**Request:**

```http
GET /api/v1/storage/{path}
```

**Response (success):**

```http
200 OK
Content-Type: application/yaml
Content-Length: 1234
ETag: "d41d8cd98f00b204e9800998ecf8427e"
Last-Modified: Sat, 23 Jan 2026 03:00:00 GMT
X-Content-SHA256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855

{file content}
```

**Response (not found):**

```http
404 Not Found

{
  "error": "NoSuchKey",
  "message": "The specified key does not exist",
  "path": "offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml"
}
```

**Range requests:**

For partial downloads (resume interrupted transfers):

```http
GET /api/v1/storage/{path}
Range: bytes=1000-1999
```

```http
206 Partial Content
Content-Range: bytes 1000-1999/5000
Content-Length: 1000

{partial content}
```

**Conditional requests:**

```http
GET /api/v1/storage/{path}
If-None-Match: "d41d8cd98f00b204e9800998ecf8427e"
```

```http
304 Not Modified
```

**Example:**

```bash
curl http://stone-jade-lake.local:7180/api/v1/storage/offerings/abc123/latest/manifest.yaml
```

---

### DELETE Object

Delete an object from storage.

**Request:**

```http
DELETE /api/v1/storage/{path}
```

**Response (success):**

```http
204 No Content
```

**Response (not found):**

```http
404 Not Found

{
  "error": "NoSuchKey",
  "message": "The specified key does not exist",
  "path": "offerings/abc123/old-backup/manifest.yaml"
}
```

**Delete directory (recursive):**

```http
DELETE /api/v1/storage/{path}/?recursive=true
```

```http
200 OK

{
  "deleted": 5,
  "paths": [
    "offerings/abc123/2026-01-20T03:00:00Z/manifest.yaml",
    "offerings/abc123/2026-01-20T03:00:00Z/data.archive.gz",
    ...
  ]
}
```

**Example:**

```bash
curl -X DELETE \
  http://stone-jade-lake.local:7180/api/v1/storage/offerings/abc123/2026-01-20T03:00:00Z/?recursive=true
```

---

### HEAD Object

Get object metadata without downloading content.

**Request:**

```http
HEAD /api/v1/storage/{path}
```

**Response (exists):**

```http
200 OK
Content-Type: application/gzip
Content-Length: 142000000
ETag: "d41d8cd98f00b204e9800998ecf8427e"
Last-Modified: Sat, 23 Jan 2026 03:00:00 GMT
X-Content-SHA256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

**Response (not found):**

```http
404 Not Found
```

**Example:**

```bash
curl -I http://stone-jade-lake.local:7180/api/v1/storage/offerings/abc123/latest/data.archive.gz
```

---

### LIST Objects

List objects with a given prefix.

**Request:**

```http
GET /api/v1/storage/?list&prefix={prefix}
```

**Query Parameters:**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `prefix` | Filter by path prefix | (none) |
| `delimiter` | Group by delimiter (e.g., `/` for directories) | (none) |
| `max-keys` | Maximum results | 1000 |
| `continuation-token` | Pagination token | (none) |

**Response:**

```http
200 OK
Content-Type: application/json

{
  "prefix": "offerings/",
  "delimiter": "/",
  "max_keys": 1000,
  "is_truncated": false,
  "continuation_token": null,
  "contents": [
    {
      "key": "offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml",
      "size": 1234,
      "last_modified": "2026-01-23T03:00:00Z",
      "etag": "d41d8cd98f00b204e9800998ecf8427e"
    },
    {
      "key": "offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz",
      "size": 142000000,
      "last_modified": "2026-01-23T03:00:00Z",
      "etag": "a1b2c3d4e5f6..."
    }
  ],
  "common_prefixes": [
    "offerings/abc123/",
    "offerings/def456/"
  ]
}
```

**Directory-style listing:**

```http
GET /api/v1/storage/?list&prefix=offerings/&delimiter=/
```

Returns only immediate children (like `ls`):

```json
{
  "prefix": "offerings/",
  "delimiter": "/",
  "contents": [],
  "common_prefixes": [
    "offerings/abc123/",
    "offerings/def456/",
    "offerings/ghi789/"
  ]
}
```

**Pagination:**

```http
GET /api/v1/storage/?list&prefix=offerings/&max-keys=100
```

```json
{
  "is_truncated": true,
  "continuation_token": "eyJrZXkiOiJvZmZlcmluZ3MvZGVmNDU2LyJ9",
  "contents": [ ... ]
}
```

Next page:

```http
GET /api/v1/storage/?list&prefix=offerings/&max-keys=100&continuation-token=eyJrZXkiOiJvZmZlcmluZ3MvZGVmNDU2LyJ9
```

**Example:**

```bash
curl "http://stone-jade-lake.local:7180/api/v1/storage/?list&prefix=offerings/abc123/&delimiter=/"
```

---

### COPY Object

Copy an object to a new location.

**Request:**

```http
POST /api/v1/storage/{destination-path}?copy
X-Copy-Source: {source-path}
```

**Response:**

```http
201 Created

{
  "source": "offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz",
  "destination": "offerings/abc123/backup/data.archive.gz",
  "size": 142000000,
  "etag": "a1b2c3d4e5f6..."
}
```

**Example:**

```bash
curl -X POST \
  -H "X-Copy-Source: offerings/abc123/latest/data.archive.gz" \
  "http://stone-jade-lake.local:7180/api/v1/storage/offerings/abc123/backup/data.archive.gz?copy"
```

---

### Multipart Upload

For large files (>100MB recommended), use multipart upload to:
- Resume interrupted uploads
- Parallelize upload
- Avoid memory issues

#### Initiate Multipart Upload

```http
POST /api/v1/storage/{path}?uploads
Content-Type: {mime-type}
```

```http
200 OK

{
  "path": "offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz",
  "upload_id": "upload-xyz789",
  "part_size_minimum": 5242880,
  "part_size_recommended": 104857600
}
```

#### Upload Part

```http
PUT /api/v1/storage/{path}?partNumber={n}&uploadId={id}
Content-Length: {size}

{part data}
```

```http
200 OK
ETag: "{part-etag}"

{
  "part_number": 1,
  "etag": "a1b2c3d4...",
  "size": 104857600
}
```

#### Complete Multipart Upload

```http
POST /api/v1/storage/{path}?uploadId={id}
Content-Type: application/json

{
  "parts": [
    { "part_number": 1, "etag": "a1b2c3d4..." },
    { "part_number": 2, "etag": "e5f6g7h8..." },
    { "part_number": 3, "etag": "i9j0k1l2..." }
  ]
}
```

```http
201 Created

{
  "path": "offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz",
  "size": 314572800,
  "etag": "final-etag-123",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

#### Abort Multipart Upload

```http
DELETE /api/v1/storage/{path}?uploadId={id}
```

```http
204 No Content
```

#### List In-Progress Uploads

```http
GET /api/v1/storage/?uploads
```

```http
200 OK

{
  "uploads": [
    {
      "path": "offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz",
      "upload_id": "upload-xyz789",
      "initiated": "2026-01-23T02:50:00Z",
      "parts_uploaded": 2,
      "bytes_uploaded": 209715200
    }
  ]
}
```

---

## Error Handling

### Error Response Format

```json
{
  "error": "ErrorCode",
  "message": "Human-readable description",
  "path": "path/that/caused/error",
  "request_id": "req-abc123"
}
```

### Error Codes

| HTTP Status | Error Code | Description |
|-------------|------------|-------------|
| 400 | `InvalidRequest` | Malformed request |
| 400 | `InvalidPath` | Path contains invalid characters |
| 400 | `InvalidRange` | Range header is invalid |
| 403 | `AccessDenied` | Authentication failed or insufficient permissions |
| 404 | `NoSuchKey` | Object does not exist |
| 404 | `NoSuchUpload` | Multipart upload ID not found |
| 409 | `ObjectAlreadyExists` | Object exists and overwrite not allowed |
| 409 | `UploadInProgress` | Conflicting multipart upload |
| 413 | `EntityTooLarge` | Request body exceeds limit |
| 416 | `InvalidRange` | Range not satisfiable |
| 500 | `InternalError` | Server error |
| 503 | `StorageUnavailable` | Seed bank not accessible |
| 507 | `InsufficientStorage` | Not enough space |

### S3-Compatible Error Format

For S3 clients, errors can also be returned as XML:

```http
GET /api/v1/storage/nonexistent
Accept: application/xml
```

```http
404 Not Found
Content-Type: application/xml

<?xml version="1.0" encoding="UTF-8"?>
<Error>
  <Code>NoSuchKey</Code>
  <Message>The specified key does not exist.</Message>
  <Key>nonexistent</Key>
  <RequestId>req-abc123</RequestId>
</Error>
```

---

## Streaming and Large Files

### Streaming Uploads

The server accepts chunked transfer encoding:

```http
PUT /api/v1/storage/{path}
Transfer-Encoding: chunked
Content-Type: application/octet-stream

{chunked data}
```

This allows uploading without knowing size in advance (e.g., piping from `mongodump`).

### Streaming Downloads

The server streams responses without buffering entire file:

```rust
async fn get_object(path: &str) -> impl Stream<Item = Bytes> {
    let file = File::open(path).await?;
    ReaderStream::new(file)
}
```

### Recommended Part Sizes

| File Size | Strategy |
|-----------|----------|
| < 100 MB | Single PUT |
| 100 MB - 5 GB | Multipart, 100 MB parts |
| > 5 GB | Multipart, 500 MB parts |

### Timeout Handling

| Operation | Timeout |
|-----------|---------|
| Connection | 30 seconds |
| Upload (per MB) | 10 seconds |
| Download (per MB) | 10 seconds |
| List | 60 seconds |

For large files, timeouts are calculated based on size:

```
timeout = base_timeout + (size_mb * per_mb_timeout)
```

---

## Implementation Notes

### Filesystem Mapping

```
API Path                                    Filesystem Path
────────────────────────────────────────    ─────────────────────────────────────
/api/v1/storage/offerings/abc123/...   →   {mount}/garden/offerings/abc123/...
/api/v1/storage/stones/xyz789/...      →   {mount}/garden/stones/xyz789/...
/api/v1/storage/index.yaml             →   {mount}/garden/index.yaml
```

### Atomic Writes

All writes are atomic (write to temp file, then rename):

```rust
async fn put_object(path: &Path, data: impl Stream<Item = Bytes>) -> Result<()> {
    let temp_path = path.with_extension("tmp");
    
    // Write to temp file
    let mut file = File::create(&temp_path).await?;
    while let Some(chunk) = data.next().await {
        file.write_all(&chunk?).await?;
    }
    file.sync_all().await?;
    
    // Atomic rename
    fs::rename(&temp_path, path).await?;
    
    Ok(())
}
```

### Directory Creation

Parent directories are created automatically:

```rust
async fn ensure_parent_exists(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    Ok(())
}
```

### Content-Type Detection

If `Content-Type` header is not provided on upload, detect from extension:

| Extension | Content-Type |
|-----------|--------------|
| `.yaml`, `.yml` | `application/yaml` |
| `.json` | `application/json` |
| `.gz` | `application/gzip` |
| `.tar` | `application/x-tar` |
| `.tar.gz`, `.tgz` | `application/gzip` |
| (other) | `application/octet-stream` |

### Checksum Calculation

Checksums are calculated on write and stored as extended attributes (if supported) or in a sidecar file:

```
data.archive.gz
data.archive.gz.sha256    # Contains: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

### Symlink Support

For `latest` pointers:

```http
PUT /api/v1/storage/offerings/abc123/latest
Content-Type: application/x-symlink

2026-01-23T03:00:00Z/
```

Creates a symlink:
```
offerings/abc123/latest -> 2026-01-23T03:00:00Z/
```

GET follows symlinks transparently.

---

## Examples

### Backup Workflow

```bash
# 1. Write manifest
curl -X PUT \
  -H "Content-Type: application/yaml" \
  --data-binary @manifest.yaml \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/manifest.yaml"

# 2. Upload data (multipart for large files)
# Initiate
UPLOAD_ID=$(curl -X POST \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz?uploads" \
  | jq -r '.upload_id')

# Upload parts
curl -X PUT \
  --data-binary @part1.bin \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz?partNumber=1&uploadId=$UPLOAD_ID"

curl -X PUT \
  --data-binary @part2.bin \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz?partNumber=2&uploadId=$UPLOAD_ID"

# Complete
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"parts":[{"part_number":1,"etag":"..."},{"part_number":2,"etag":"..."}]}' \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/2026-01-23T03:00:00Z/data.archive.gz?uploadId=$UPLOAD_ID"

# 3. Update latest symlink
curl -X PUT \
  -H "Content-Type: application/x-symlink" \
  -d "2026-01-23T03:00:00Z/" \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/latest"
```

### Restore Workflow

```bash
# 1. Get manifest
curl "http://stone.local:7180/api/v1/storage/offerings/abc123/latest/manifest.yaml" > manifest.yaml

# 2. Download data (with resume support)
curl -C - -o data.archive.gz \
  "http://stone.local:7180/api/v1/storage/offerings/abc123/latest/data.archive.gz"

# 3. Verify checksum
EXPECTED=$(curl -I "http://stone.local:7180/api/v1/storage/offerings/abc123/latest/data.archive.gz" \
  | grep X-Content-SHA256 | awk '{print $2}')
ACTUAL=$(sha256sum data.archive.gz | awk '{print $1}')
[ "$EXPECTED" = "$ACTUAL" ] && echo "Checksum OK"
```

### Using AWS CLI

```bash
# Configure
aws configure set aws_access_key_id zen-garden
aws configure set aws_secret_access_key zen-garden
aws configure set region zen-garden

# List offerings
aws --endpoint-url http://stone.local:7180/api/v1/storage \
  s3 ls s3://garden/offerings/

# Download backup
aws --endpoint-url http://stone.local:7180/api/v1/storage \
  s3 cp s3://garden/offerings/abc123/latest/data.archive.gz ./

# Upload backup
aws --endpoint-url http://stone.local:7180/api/v1/storage \
  s3 cp ./data.archive.gz s3://garden/offerings/abc123/2026-01-24T03:00:00Z/
```

### Using rclone

```ini
# ~/.config/rclone/rclone.conf
[zen-garden]
type = s3
provider = Other
endpoint = http://stone-jade-lake.local:7180/api/v1/storage
access_key_id = zen-garden
secret_access_key = zen-garden
```

```bash
# Sync backups
rclone sync zen-garden:garden/offerings/abc123/ ./local-backup/

# Check integrity
rclone check zen-garden:garden/offerings/abc123/ ./local-backup/
```

---

## Summary

The Storage API provides:

| Feature | Description |
|---------|-------------|
| **S3 compatibility** | Standard tools and libraries work |
| **Simple REST** | Easy to implement and debug |
| **Streaming** | Large files handled efficiently |
| **Multipart** | Resumable uploads for reliability |
| **Checksums** | Data integrity verification |
| **Range requests** | Resumable downloads |
| **Atomic writes** | No partial files on failure |
| **Transparent proxying** | Same API for direct and proxied access |

---

## References

- [Amazon S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/Welcome.html)
- [MinIO Client SDK](https://docs.min.io/docs/minio-client-complete-guide.html)
- [Seed Bank Specification](zen-garden-spec-seed-banks.md)
- [Cultivation Specification](zen-garden-spec-cultivation.md)

---

**Last Updated:** January 2026  
**Status:** Proposal — pending review and implementation
