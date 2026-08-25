---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-17
---

# STORAGE-0006: Seed Bank Replication and Primary/Dormant Roles

**Date**: 2026-02-16
**Status**: Accepted
**Depends on**: STORAGE-0003 (Beacon Protocol), STORAGE-0005 (Manifest-First Discovery)
**Depended on by**: ORCH-0001 (Replant Ceremony — Phase 4)

## Context

Seed banks are portable USB storage devices that hold harvested offering state (nurturing snapshots). Today, each seed bank has a unique name and stores data independently. There is no mechanism for:

1. **Redundancy** — if a USB drive dies, its data is gone.
2. **Geographic distribution** — a seed bank plugged into stone-01 can't serve stone-02 without physically moving it.
3. **Consistency** — two stones writing to two seed banks with different names produce divergent backup sets.

The offering orchestration system (ORCH-0001) already solves these problems for offerings using primary/dormant roles with automatic failover. Seed banks need the same pattern: multiple physical devices forming one logical seed bank, with one primary accepting writes and dormants replicating from it.

### Existing Infrastructure

The manifest already has `group` and `replica_id` fields (STORAGE-0005), designed for this use case but never wired up. The beacon protocol (STORAGE-0003) already broadcasts seed bank presence across stones. The offering orchestration task already implements the role assignment pattern (first-online-wins + reconciliation window + dual-primary resolution).

### Current Issues (Deep Sweep Findings)

| # | Issue | Severity |
|---|-------|----------|
| 1 | Cross-stone name collision: `StorageCacheInner::find_by_name()` returns first match across all stones — ambiguous routing | **High** |
| 2 | Replicated seed bank scan is 1-level deep; `mounts/{group}/replica-{id}` is 2 levels — replicas invisible to scanner | **High** |
| 3 | Release vs. mount persistence fight loop: `release_bank_v1()` unmounts but doesn't remove from `MountTracker`, persistence task re-mounts it 5s later | **High** |
| 4 | Remote `RemoteNurturingIndex` race: two stones writing to shared seed bank concurrently lose each other's updates (load → modify → save, no locking) | **High** |
| 5 | Rename doesn't re-mount to new derived path; `update_manifest_name()` isn't crash-safe (no tmp+rename) | **Medium** |
| 6 | Local `NurturingIndex` has same concurrent load/save race (two offerings nurturing simultaneously) | **Medium** |
| 7 | `PoolConflictData` type is dead code — defined but never instantiated | **Low** |
| 8 | `JournalEntry`/`JournalOp` types and `journal/` directory are unused scaffolding | **Low** |
| 9 | `SeedBankRegistry::scan()` rebuilds from filesystem on every call (5s, 10s, every API handler) — heavy I/O | **Low** |

## Decision

### 1. Identity Model — Name IS the FQN

Remove `group` and `replica_id` from the manifest. The identity model simplifies to:

| Field | Purpose |
|-------|---------|
| `id` (GUIDv7) | Unique per physical device. Primary key. Never changes. |
| `name` | Logical seed bank FQN. Shared across all replicas. |

Two seed banks with the same `name` and different `id`s are replicas of the same logical seed bank. No explicit "create as replica" step — if you prepare a second device with `name: "zen-garden"`, the garden recognizes it as a replica when both are visible in topology beacons.

**Mount path** changes from the group/replica scheme to ID-based:

```
# All seed banks, whether single or replicated:
{data_dir}/mounts/{name}/{short_id}/

# Where short_id = first 8 characters of the GUIDv7
# Example:
/var/lib/zen-garden/mounts/zen-garden/01956a3e/
/var/lib/zen-garden/mounts/zen-garden/0195b2c4/
```

This eliminates the 1-level scan bug (issue #2): the scanner walks `mounts/{name}/{short_id}/.zen-garden/manifest.json` consistently. Single seed banks and replicated seed banks use the same path structure.

**What gets removed from the data model:**

- `SeedBankManifest::group` — redundant; `name` IS the group
- `SeedBankManifest::replica_id` — redundant; `id` (GUIDv7) is the unique identifier
- `SeedBankManifest::new_replica()` — unnecessary; `new()` handles all cases
- `SeedBankManifest::is_replica()` — replaced by runtime topology check
- `SeedBankInfo::group` and `SeedBankInfo::replica_id` — same
- `PrepareSeedBankRequest::group` and `PrepareSeedBankRequest::replica_id` — same
- `PoolConflictData` — dead code (issue #7)
- `MergePolicy`, `MergeSeedBankRequest` — dead code, never used

### 2. Role Assignment — Runtime, Not Manifest

Primary/dormant is a **runtime role**, not a device property. It follows the same pattern as offering orchestration (`src/moss/src/tasks/offering_orchestration.rs`):

**Role assignment flow:**

```
Seed bank "zen-garden" mounted on stone-02
    │
    ├─ Check topology beacons: does any stone already hold Primary
    │  for a seed bank named "zen-garden"?
    │
    ├─ No  → I am Primary
    │
    └─ Yes → I am Dormant
```

**Startup reconciliation** (same 3s window as offerings):
- Stone boots, sees its local seed bank was Primary last time
- Waits 3s for topology to populate
- If another stone is already Primary for this name → yield to Dormant
- If two stones claim Primary simultaneously → lower `stone_id` yields (deterministic tiebreak)

**Failover:**
- Primary stone goes offline → its heartbeat goes stale in topology
- Dormant stone detects stale primary (same threshold as offerings: 6s)
- Dormant self-promotes to Primary
- When old primary returns, startup reconciliation detects the existing primary → yields to Dormant

**Pin override — last-pin-wins with GUIDv7:**

Pinning is a "claim Primary" operation, not a "lock what I have" operation.
Any stone holding a replica of a logical seed bank can pin it. The pin
carries a GUIDv7 identifier (`pin_id`) that encodes both the timestamp and
a global unique ID. When two stones both claim pinned Primary, the **higher
pin_id wins** (later timestamp = more recent human intent).

Flow:
1. Stone-b (Dormant) receives `POST /pin` for seed bank "zen-garden"
2. Stone-b generates a GUIDv7 `pin_id`, sets its local role to Primary,
   and broadcasts a beacon: `{role: Primary, pin_id: "019c6d..."}`.
3. Stone-a (was Primary) receives the beacon. Orchestration compares:
   - Remote has `pin_id`, local has none → yield to Dormant.
   - Both have `pin_id` → compare. Higher GUIDv7 wins (later pin).
     Loser auto-unpins, switches to Dormant, re-broadcasts.
4. Stone-a starts replicating from stone-b.

Conflicting pins (both stones pinned):
- Stone-a pinned at `019c6d5a-...`, stone-b pinned at `019c6d5b-...`
- `019c6d5b` > `019c6d5a` → stone-b wins. Stone-a auto-unpins.
- The losing stone removes its pin from local state and re-broadcasts.
  No manual intervention needed.

Unpin:
- `POST /unpin` clears the local `pin_id`. Beacon re-broadcasts with
  `pin_id: null`. Role assignment reverts to normal tiebreaker
  (higher `stone_id` keeps Primary).

Persistence:
- `pin_id` is written to `{mount}/.zen-garden/pin.json` on the seed bank
  device. Survives stone reboots. On startup, the stone reads
  `pin.json`, inserts into `seed_bank_pins`, and announces with the
  pin_id. The mesh resolves normally.
- If the seed bank is physically moved to a different stone, the pin
  travels with the device (pin is on the seed bank, not the stone).

Data model:
- `SeedBankAnnouncement::pinned: bool` → `pin_id: Option<String>`
- `AppState::seed_bank_pins: HashSet<String>` → `HashMap<String, String>`
  (name → pin_id GUIDv7)
- `resolve_role()` priority:
  1. Pinned with higher GUIDv7 → Primary
  2. Pinned with lower GUIDv7 → auto-unpin, Dormant
  3. Unpinned → existing tiebreaker (higher stone_id keeps Primary)

### 3. Prepare Flow — No Replica Declaration

Preparing a seed bank no longer requires declaring it as a replica. The flow:

```bash
# First device
garden-rake prepare seed-bank zen-garden          # name: zen-garden, id: unique GUIDv7

# Second device (different physical USB)
garden-rake prepare seed-bank zen-garden          # same name, different GUIDv7
```

The prepare handler's behavior changes:

**Before (current):** `registry.exists(&name)` → 409 NAME_COLLISION

**After:** Check if same `name` exists in garden (local registry + beacons).

- If same name exists → this is a new replica. Allow it. No error.
- Log: `"Preparing seed bank 'zen-garden' (replica — existing primary on stone-01)"`
- On first scan after mount, role assignment kicks in: existing primary elsewhere → this one is Dormant.

If the user genuinely made a naming mistake, they can rename afterward. The collision-as-error was overly protective.

### 4. Write Path — Always to Primary

When moss needs to write a nurturing snapshot to a seed bank:

```
NurturingScheduler wants to store harvest for offering "mongodb"
    │
    ├─ Find primary for logical seed bank "zen-garden"
    │  (from StorageCache beacons or local registry)
    │
    ├─ Primary is LOCAL (on this stone)?
    │   └─ Write directly to mount path
    │
    └─ Primary is REMOTE (on another stone)?
        └─ POST /api/v1/stone/storage/bank/{id}/garden/memories/...
           (stream upload to the remote stone holding the primary)
```

This solves the remote index race (issue #4): only the stone hosting the primary writes to the `RemoteNurturingIndex`. No concurrent writers.

### 5. Replication — Cursor-Based Changelog

Instead of pushing individual SSE events for each write, the Primary maintains an append-only **changelog** on the seed bank itself. Replication separates **notification** ("something changed") from **data transfer** ("here's what changed").

#### 5a. Changelog

Every write/delete through `SeedBankStore` appends an entry to `.zen-garden/changelog.jsonl` on the seed bank:

```jsonl
{"c":"01956a3e-1234-7def-8000-abcdef012345","op":"C","path":"garden/memories/mongodb/harvest-abc.tar.gz","bytes":524288000}
{"c":"01956a3f-5678-7abc-8000-123456789abc","op":"D","path":"garden/memories/mongodb/harvest-old.tar.gz"}
```

| Field | Type | Purpose |
|-------|------|---------|
| `c` | GUIDv7 | Cursor — time-sortable unique ID. Serves as sequence number AND timestamp. |
| `op` | `C\|M\|D` | Create, Modify, Delete |
| `path` | string | Relative path within mount root (same as `SeedBankStore` rel path) |
| `bytes` | u64? | Size for C/M operations. Omitted for D. |

The changelog lives on the seed bank drive — when the drive moves to a different stone, the changelog moves with it. No separate database, no in-memory state to reconstruct.

GUIDv7 as cursor gives time-sortable ordering, uniqueness, and extractable timestamps for debugging.

This replaces the existing `JournalEntry`/`JournalOp` scaffolding in `src/common/src/storage.rs` (issue #8), which was directionally right but over-engineered. This is the simpler version.

#### 5b. SSE Notification (Doorbell)

A lightweight SSE endpoint on the Primary's stone notifies Dormant subscribers that the changelog has advanced:

```
GET /api/v1/stone/storage/stream?seed-bank=zen-garden

event: storage.tick
data: {"cursor":"01956a3f...","seed_bank":"zen-garden","C":1,"M":0,"D":1}
```

This is a **doorbell, not a delivery truck**. The payload is ~100 bytes — just enough for the Dormant to know "something changed, go pull." The SSE stream is optional — a Dormant can poll `/changes` every 10s and be fine. SSE just reduces sync latency to sub-second.

The stream is separate from the existing presence stream (`/api/v1/stone/presence/stream`). Different consumers (machine-to-machine background tasks vs UI clients), different reliability needs, different lifecycle.

Backed by its own `broadcast::Sender<StorageTick>` on `AppState`. No coupling to the existing EventBus/SseListener pipeline.

#### 5c. Pull Endpoint (The Real Work)

Dormant stones pull changes from the Primary:

```
GET /api/v1/stone/storage/bank/{id}/changes?since=01956a3e

{
  "cursor": "01956a3f...",
  "changes": [
    {"c":"01956a3e...","op":"C","path":"garden/memories/mongodb/harvest-abc.tar.gz","bytes":524288000},
    {"c":"01956a3f...","op":"D","path":"garden/memories/mongodb/harvest-old.tar.gz"}
  ]
}
```

**One codepath for everything:**
- Initial sync = `GET /changes` (no `since` parameter) — returns all entries
- Incremental sync = `GET /changes?since={last_cursor}` — returns entries newer than cursor
- Reconnect after disconnect = same call with last known cursor
- No special cases, no `Last-Event-ID` complexity

#### 5d. Dormant Sync Loop

```
1. On startup: read local `.zen-garden/last_cursor` (or empty = initial sync)
2. Connect to primary's SSE stream (or poll /changes every 10s as fallback)
3. See tick with cursor > my_cursor
4. GET /changes?since=my_cursor → list of changes
5. For each C/M: GET /bank/{id}/{path} → write to local via SeedBankStore
6. For each D: delete from local via SeedBankStore
7. Write cursor to local `.zen-garden/last_cursor`
8. Repeat from 3
```

The local dormant maintains its own `last_cursor` file — a single GUIDv7 string. On startup, if the file doesn't exist, it performs a full initial sync.

#### 5e. Changelog Compaction & Full-Sync Fallback

The changelog grows unboundedly without policy. Three design decisions govern its lifecycle:

**Compaction — 7-day sliding window, no slicing.**

A periodic task (runs in the orchestration tick, daily cadence) rewrites `changelog.jsonl` keeping only entries from the last 7 days. At aggressive write rates (1,000 ops/day) the file stays under 1.5 MB. Atomic rewrite (tmp + rename) prevents corruption.

We do **not** slice the changelog into multiple segment files. Slicing adds directory management, segment rotation, cross-file cursor resolution — complexity for a problem that compaction eliminates. This is a USB seed bank, not a WAL for a database. A single JSONL file that stays small is simpler to reason about, move between stones, and debug.

**Stale cursor detection — `full_sync_required` flag.**

When a Dormant pulls `GET /changes?since={cursor}` and the cursor predates the oldest entry in the (compacted) changelog, the requested history is gone. Rather than silently returning a partial result, `ChangesResponse` includes a `full_sync_required: bool` flag:

```json
{
  "cursor": "01956a3f...",
  "full_sync_required": true,
  "changes": []
}
```

Detection is cheap: if `since` is non-empty and strictly less than the first entry's cursor, the cursor was compacted away. The response omits changes entirely — partial history would be worse than useless.

**Full sync fallback — directory walk + hash comparison.**

When the replication task receives `full_sync_required: true`, it abandons incremental sync and performs a full reconciliation: walk the Primary's file tree, compare against local, download missing/changed files, delete stale files. This is the same codepath as initial sync (no `since` parameter) but with a pre-existing local state to diff against.

This handles the "old seed bank reconnects after months" scenario gracefully. No special protocol, no error — just a flag that tells the Dormant to reconcile from scratch.

**When does this occur?**

| Scenario | Incremental | Full sync |
|----------|:-----------:|:---------:|
| Normal operation (cursor within 7-day window) | ✓ | |
| Brief disconnect (hours) | ✓ | |
| Extended disconnect (weeks+, cursor compacted) | | ✓ |
| First sync (no cursor at all) | ✓* | |

\* First sync returns the entire changelog — functionally equivalent to full sync but using the same pull endpoint.

#### 5f. Advantages Over Event-Push

| Concern | Event-Push (original) | Cursor-Based (adopted) |
|---------|----------------------|----------------------|
| Missed events | Need `Last-Event-ID` + replay buffer | Just pull since last cursor |
| Initial sync | Special `replication.manifest` event | Same `/changes` endpoint, no cursor |
| Reconnect | Replay from last ID, hope buffer hasn't rolled | Same `/changes` endpoint |
| Batching | One event per write | One pull gets all changes since last sync |
| Portability | Events in memory, lost on restart | Changelog on drive, moves with seed bank |
| Complexity | SSE event types, ordering, dedup | Doorbell + pull, one codepath |

### 6. Two Replicas on Same Stone

This is a valid configuration for single-stone setups or drive-failure resilience. Both mount under `mounts/zen-garden/{short_id_1}/` and `mounts/zen-garden/{short_id_2}/`. Role assignment: one is Primary, the other is Dormant. Replication between them is local disk-to-disk copy (no network).

No warning, no restriction. Two replicas on the same stone is a legitimate redundancy strategy.

### 7. CLI Disambiguation

Most commands operate on the **logical seed bank name** and don't need to identify a specific physical device:

| Command | Resolves by | Ambiguity on same stone? |
|---------|------------|--------------------------|
| `pin seed-bank <name>` | name | No — pins the Primary role, not a device |
| `store to seed-bank <name>` | name + primary role | No — routes to primary automatically |
| `replant from seed-bank <name>` | name + nearest replica with data | No — picks any replica |
| `release seed-bank <name>` | name + physical device | **Yes** — which drive to eject? |
| `show seed-bank <name>` | name | No — shows all replicas |

**Pin semantics**: `pin seed-bank <name>` locks the Primary role in place. Whoever currently holds Primary keeps it — the orchestration task will not reassign the role even if a new replica comes online or the current Primary has a lower stone_id. `unpin seed-bank <name>` releases this lock and returns to normal first-online-wins orchestration.

Pinning protects against unwanted role flips when a faster/newer stone joins the garden. It does NOT pin to a specific stone — if the pinned Primary's stone goes offline and comes back, it reclaims Primary (the pin travels with the role, not the machine).

For `release` (the only ambiguous case), when multiple replicas share the same name on the same stone:

```
Multiple "zen-garden" devices on this stone:

  [1] 01956a3e  64GB   /dev/sdb1  origin: stone-01  role: Primary
  [2] 0195b2c4  128GB  /dev/sdc1  origin: stone-02  role: Dormant

Release which device? [1/2]:
```

Short ID (first 8 chars of GUIDv7) + capacity + device path + origin + current role.

### 8. Beacon Protocol Extension

The `SeedBankAnnouncement` in the storage beacon needs a `role` field:

```rust
pub struct SeedBankAnnouncement {
    pub id: String,
    pub name: String,          // Logical FQN — shared across replicas
    pub role: SeedBankRole,    // Primary | Dormant (runtime-assigned)
    pub protocols: Vec<String>,
    pub access: StorageAccess,
    pub visibility: SeedBankVisibility,
    pub health: String,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedBankRole {
    Primary,
    Dormant,
}
```

This allows any stone to resolve "where is the primary for seed bank X?" from cached beacons without additional API calls.

### 9. Rename Semantics

Renaming a seed bank changes its logical FQN:

**Rename to a free name** (no other device shares the new name):
- Update manifest `name` field (atomic: tmp+rename)
- Unmount from old path (`mounts/old-name/{short_id}/`)
- Re-mount at new path (`mounts/new-name/{short_id}/`)
- Broadcast updated beacon
- Remove from `MountTracker` under old path, re-track under new path

**Rename into an existing FQN** (another device already uses that name):
- This is equivalent to "join this replica group."
- Update manifest `name` field
- Role assignment kicks in on next scan: existing primary → I'm Dormant
- Begin replicating from primary

**Rename away from a shared FQN** (leaving a replica group):
- Update manifest `name` field
- Role assignment: I'm the only device with this new name → I'm Primary
- Data stays on this device (no wipe). It's now the sole copy under the new name.
- Former group re-evaluates: if only one replica remains, it becomes Primary (or stays Primary if it already was).

No `--force` flag, no wipe/sync. Renaming is always safe — data is never deleted. The user manages what data stays on which device through normal nurturing retention.

**Encryption constraint on rename**: same name = same encryption state. You cannot rename a public seed bank into the FQN of an encrypted one, or vice versa. The prepare handler enforces this at creation time, and the rename handler enforces it on name change. If `private-seed-bank` is encrypted for pond `pond-moonlit-basin`, a public device cannot be renamed to `private-seed-bank`. The rename handler returns `409 ENCRYPTION_MISMATCH`.

### 10. Pond-Scoped Encryption

When a stone is in a **pond** (mTLS security boundary), seed banks can be encrypted at rest using **application-level encryption** (ChaCha20-Poly1305, pure Rust) so that data is accessible only by stones in the same pond. Cross-platform — works identically on Linux and Windows with no OS-specific primitives.

#### The Chokepoint: `SeedBankStore`

All seed bank I/O — nurturing harvests, object storage API, S3 surface, replication — flows through a single gateway. This is the only code that touches seed bank files on disk:

```rust
pub struct SeedBankStore {
    mount_root: PathBuf,
    dek: Option<[u8; 32]>,  // None = public, Some = pond-encrypted
}

impl SeedBankStore {
    pub fn read(&self, rel: &Path) -> Result<Vec<u8>>;
    pub fn write(&self, rel: &Path, data: &[u8]) -> Result<()>;
    pub fn delete(&self, rel: &Path) -> Result<()>;
    pub fn exists(&self, rel: &Path) -> bool;
}
```

- Callers always pass **plaintext**. If `dek` is `Some`, the store encrypts on write and decrypts on read. If `None`, passthrough.
- No caller ever touches the filesystem directly for seed bank content.
- Encryption is a two-line concern inside the store — not a trait, not a layer, not a separate module.
- The chokepoint also provides a natural place for metrics, audit, and future concerns.

```
Domain        │  SeedBankManifest, SeedBankRole, naming rules,
              │  encryption-state-must-match-on-rename
              │
Application   │  NurturingScheduler, ReplicationTask, StorageAPI handler
              │  ("store harvest", "sync artifact", "serve GET for object")
              │
Infrastructure│  SeedBankStore ← here. Filesystem + encrypt/decrypt.
              │  SeedBankRegistry (scan, mount, construct SeedBankStore)
```

```
Nurturing store ──┐
Object storage API ──┤──→ SeedBankStore ──→ filesystem
S3 surface ──┤     (encrypt/decrypt if dek present)
Replication ──┘
```

`SeedBankStore` is **infrastructure** — it moves bytes on/off disk with optional encryption. No business rules, no domain polymorphism, no trait. The variant (public vs encrypted) is handled by `dek: Option` inside one struct.

The registry constructs a `SeedBankStore` at mount time with `dek: None` (public) or `dek: Some(derived_key)` (encrypted). Every consumer receives the same `SeedBankStore` — encryption is invisible.

#### Key Hierarchy

```
Pond enrollment
  │
  └─ pond_data_key (symmetric, distributed to each stone during enrollment)
       │
       └─ DEK = BLAKE3-KDF(
              key: pond_data_key,
              context: "zen-garden-seedbank",
              input: seed_bank_name    ← name, not device id
          )
```

- `pond_data_key` — a pond-level symmetric key, derived from the CA at pond creation, distributed to each stone during enrollment (encrypted with the stone's certificate), stored in the stone's secrets backend (TPM / platform keyring / encrypted file fallback).
- DEK is derived from the **seed bank name** (FQN), not the device id. All replicas of `private-seed-bank` share the same DEK. **Replication is pure byte-copy** — encrypted files on the primary are bit-identical valid on any dormant.

**On-disk format**: `version(1) + nonce(12) + ciphertext + tag(16)`. Nonce is random per write. No AAD needed — the DEK is already name-scoped, and file relocation within the same seed bank is harmless.

#### Manifest Extension

The manifest stays plaintext (the scanner must read it without a key). Two new fields:

```json
{
  "name": "private-seed-bank",
  "id": "01956a3e...",
  "encrypted": true,
  "pond_fingerprint": "abc123def456"
}
```

- `encrypted: bool` — whether this seed bank's content is encrypted
- `pond_fingerprint: Option<String>` — fingerprint of the pond CA whose `pond_data_key` can derive the DEK

#### Mount Flow

1. Device appears → standard OS mount to `{data_dir}/mounts/{name}/{short_id}/`
2. Read `.zen-garden/manifest.json`
3. If `encrypted: true` → match pond fingerprint → derive DEK → `SeedBankStore { mount_root, dek: Some(dek) }`
4. If `encrypted: false` → `SeedBankStore { mount_root, dek: None }`

No two-stage mount, no device-mapper, no OS-specific encryption tools, no elevated privileges beyond the basic mount. Encryption is entirely in userspace inside `SeedBankStore`.

**Unmount (release)**: standard `umount` — no encryption-specific cleanup needed.

#### Access Matrix

| Seed bank | Stone | Behavior |
|-----------|-------|----------|
| Public (unencrypted) | No pond | Mount + R/W |
| Public (unencrypted) | In pond | Mount + R/W (warn on write if policy = `require-encrypted`) |
| Pond-encrypted | In same pond | Derive DEK → `SeedBankStore` encrypts/decrypts transparently |
| Pond-encrypted | In different pond | Detect → reject: "encrypted for pond X, you're in pond Y" |
| Pond-encrypted | No pond | Detect → reject: "encrypted, no pond active" |

#### Pond Encryption Policy

The pond CA metadata includes a seed bank encryption policy, set by the cornerstone admin and distributed to all stones during enrollment:

| Policy | Meaning | Use case |
|--------|---------|----------|
| `allow-public` | Pond stones can use both encrypted and public seed banks freely | Home lab, single user, convenience-first |
| `prefer-encrypted` | Warn on public seed bank writes but allow them | Mixed environments, transitional |
| `require-encrypted` | Refuse to write to public seed banks. Read-only access to public banks allowed. | Security-conscious, organizational |

Default policy: `allow-public` (non-breaking for existing gardens).

Policy is cached locally in the stone's `pond.json` for offline enforcement.

#### Prepare Flow (Encrypted)

```bash
# In a pond, preparing an encrypted seed bank
garden-rake prepare seed-bank

  This stone is in pond "pond-moonlit-basin".

  Encryption options:
    [1] Pond-encrypted — data accessible only by pond members
    [2] Public — data accessible by anyone

  Selected: [1] Pond-encrypted

  ⚠ WARNING:
    • If the pond is drained, all data on this seed bank becomes
      permanently unreadable. There is no recovery mechanism.

  Continue? [y/N]:
```

Under `require-encrypted` policy, option [2] is not offered. Under `allow-public`, both are shown. Under `prefer-encrypted`, both shown but [1] is default.

Prepare steps for encrypted: standard filesystem format → mount → write manifest with `encrypted: true` and `pond_fingerprint` → create directory layout → unmount. No special partitioning or encryption formatting step — encryption is applied transparently by the `SeedBankStore` on every write.

#### Drain Impact

When `garden-rake pond drain` is executed, the `pond_data_key` is destroyed. All encrypted seed banks become permanently unreadable. The drain confirmation lists affected seed banks:

```
garden-rake pond drain

  ⚠ EMERGENCY: This will destroy the pond "pond-moonlit-basin".

  Affected:
  • 3 stones will lose mTLS communication
  • 2 encrypted seed banks will become PERMANENTLY UNREADABLE:
      private-seed-bank  (2 replicas, 48GB data)
      offsite            (1 replica, 120GB data)

  This action cannot be undone.
  Type "drain pond-moonlit-basin" to confirm:
```

No re-key mechanism. Drain = data loss for encrypted seed banks. The user accepts this at prepare time and again at drain time.

#### Encryption Metrics

Surface encryption overhead in `garden-rake show seed-bank <name>` and in logs:

```
private-seed-bank (encrypted, pond: pond-moonlit-basin)
  Replicas: 2  Primary: stone-01  Dormant: stone-02
  Capacity: 64GB (38GB used)
  Last sync: 4s ago
  Encryption overhead: ~3% write throughput (ChaCha20-Poly1305, software-optimized)
  Last encrypt time: 0.4s for 487MB harvest
```

ChaCha20-Poly1305 is designed for high performance in software without hardware acceleration (unlike AES which depends on AES-NI). Overhead is minimal even on ARM devices. Timing metrics are captured inside `SeedBankStore` — one place, not scattered across callers.

### 11. Clone Command — Encrypted to Public Export

A user with access to an encrypted seed bank may want to create a public, unencrypted copy for sharing or offsite storage outside the pond.

```bash
garden-rake clone seed-bank private-seed-bank to public-seed-bank --unencrypted
```

This:
1. Reads from encrypted `private-seed-bank` (user must be in the pond)
2. Prepares a new public seed bank named `public-seed-bank` on the target device
3. Reads all `garden/memories/` and `garden/storage/` content through the source's `SeedBankStore` (transparent decryption), writes plaintext to target
4. The new seed bank has its own GUIDv7, its own manifest, `encrypted: false`
5. No replication relationship — it's a point-in-time snapshot, not a replica

**Rules:**
- Source and target must have **different names** (same-name = replica, and replicas must share encryption state)
- The clone is not kept in sync. It's a one-time copy.
- Under `require-encrypted` policy, the clone command warns: "Creating unencrypted copy. Data will be accessible outside the pond."

### 12. Default Naming

Sugar defaults at prepare time — not enforced, user overrides with `--name`:

| Context | Default name |
|---------|-------------|
| No pond | `public-seed-bank` |
| In pond, encrypted | `private-seed-bank` |
| In pond, public | `public-seed-bank` |

When same default name already exists in the garden (e.g., second `public-seed-bank`), the system recognizes it as a replica — no collision error, no name mangling. The user can override with `--name offsite` or any other name.

## Implementation Plan

### Phase 0: Fix Critical Bugs (Pre-requisite)

These must be fixed regardless of whether replication ships:

**0a. Release vs. mount persistence fight loop (issue #3):**
- Pass `MountTracker` to release handler via `AppState`
- On release: remove device from tracker before unmount
- **File**: `src/moss/src/api/v1/storage.rs` (release handler), `src/moss/src/app_state.rs` (add tracker field)

**0b. Non-atomic manifest write (issue #5):**
- Change `update_manifest_name()` to use tmp+rename pattern
- **File**: `src/moss/src/api/v1/storage.rs`

**0c. NurturingIndex concurrent access (issue #6):**
- Wrap `NurturingStore` index operations in `tokio::sync::Mutex`
- **File**: `src/moss/src/infra/nurturing_store.rs`

### Phase 1: Simplify Data Model

**1a. Remove `group`, `replica_id` from data model:**
- `SeedBankManifest`: remove `group`, `replica_id`, `new_replica()`, `is_replica()`
- `SeedBankInfo`: remove `group`, `replica_id`
- `PrepareSeedBankRequest`: remove `group`, `replica_id`
- Remove dead types: `PoolConflictData`, `MergePolicy`, `MergeSeedBankRequest`
- **File**: `src/common/src/storage.rs`
- **Backward compat**: `#[serde(default)]` on removed fields ensures old manifests deserialize without error. The fields are simply ignored.

**1b. Update `derive_mount_path()` to ID-based scheme:**
- All seed banks: `{data_dir}/mounts/{name}/{short_id}/`
- Where `short_id` = first 8 chars of `id`
- **File**: `src/common/src/storage.rs`

**1c. Fix scan depth (issue #2):**
- `SeedBankRegistry::scan()`: walk 2 levels deep (`mounts/{name}/{short_id}/`)
- **File**: `src/moss/src/infra/storage/registry.rs`

**1d. Update prepare handler and CLI:**
- Remove name collision rejection (allow same-name devices)
- Remove `--group` and `--replica-id` CLI args
- **Files**: `src/moss/src/api/v1/storage.rs`, `src/rake/src/commands/storage.rs`

**1e. Update rename handler:**
- Atomic manifest write (tmp+rename)
- Unmount old path → re-mount new path
- Update `MountTracker`
- **File**: `src/moss/src/api/v1/storage.rs`

**1f. Remove dead code (issues #7, #8):**
- Remove `PoolConflictData`, `MergePolicy`, `MergeSeedBankRequest`
- Remove or keep `JournalEntry`/`JournalOp` (decision: keep as scaffolding for future replication log)
- **File**: `src/common/src/storage.rs`

**1g. Update tests:**
- Remove `test_replica_manifest_creation` or adapt for new model
- Update `test_mount_path_derivation` for ID-based paths
- **File**: `src/common/src/storage.rs`

### Phase 2: Role Assignment

**2a. Add `SeedBankRole` enum:**
- `Primary`, `Dormant`
- **File**: `src/common/src/storage.rs`

**2b. Add `SeedBankOrchestrationState`:**
```rust
pub struct SeedBankOrchestrationState {
    pub role: SeedBankRole,
    pub primary_stone_id: Option<String>,
    pub primary_seed_bank_id: Option<String>,
    pub pinned: bool,
    pub pin_timestamp: Option<String>,
}
```
- Stored per logical seed bank name in a persisted registry on stone
- **File**: `src/common/src/storage.rs` (type), `src/moss/src/infra/storage/` (persistence)

**2c. Role assignment task:**
- Mirror `offering_orchestration.rs` pattern
- Startup reconciliation (3s window)
- Dual-primary resolution (lower stone_id yields)
- Stale primary detection + auto-promote
- Pin recovery on boot
- **File**: new `src/moss/src/tasks/seed_bank_orchestration.rs`

**2d. Beacon extension:**
- Add `role: SeedBankRole` to `SeedBankAnnouncement`
- Update beacon broadcast and parse
- **Files**: `src/common/src/storage.rs`, `src/moss/src/infra/storage/beacon.rs`

### Phase 3: Write-to-Primary Routing

**3a. Primary resolution in NurturingStore:**
- Before writing: resolve primary for the target seed bank name via `StorageCache`
- If local → direct write
- If remote → HTTP upload to remote stone's storage API
- **File**: `src/moss/src/infra/nurturing_store.rs`

**3b. Upload endpoint on primary stone:**
- Accept streamed artifact upload for a specific seed bank
- Write to local mount, update `RemoteNurturingIndex`
- Emit `storage.snapshot_stored` SSE event
- **File**: `src/moss/src/api/v1/storage.rs`

### Phase 4: Cursor-Based Replication

**4a. Replace `JournalEntry`/`JournalOp` with `ChangelogEntry`:**
- Simplified type: `c` (GUIDv7 cursor), `op` (C/M/D), `path`, `bytes`
- Remove `JournalOp::Snapshot`, `JournalOp::Merge`, `stone` field, `hash` field
- **File**: `src/common/src/storage.rs`

**4b. Changelog write in `SeedBankStore`:**
- `write()` and `delete()` append entries to `.zen-garden/changelog.jsonl`
- Append-only, file-lock-free (single-writer via Primary routing)
- **File**: `src/moss/src/infra/storage/store.rs`

**4c. Changes pull endpoint:**
- `GET /api/v1/stone/storage/bank/{id}/changes?since={cursor}` — returns changelog entries since cursor
- Reads `changelog.jsonl` from the specified seed bank, filters by cursor
- **Files**: `src/moss/src/api/v1/storage.rs`, `src/moss/src/bootstrap/router.rs`

**4d. Storage SSE notification stream:**
- `GET /api/v1/stone/storage/stream?seed-bank={name}` — lightweight doorbell
- Separate `broadcast::Sender<StorageTick>` on AppState
- `SeedBankStore::write()`/`delete()` emit ticks via `notify_tx`
- **Files**: `src/moss/src/api/v1/storage.rs`, `src/moss/src/app_state.rs`, `src/moss/src/bootstrap/router.rs`

**4e. Replication task:**
- Background task per Dormant seed bank name
- Subscribes to Primary's SSE stream (with poll fallback)
- Pulls changes, downloads artifacts, applies locally
- Persists `last_cursor` in `.zen-garden/last_cursor`
- **File**: new `src/moss/src/tasks/seed_bank_replication.rs`

**4f. Changelog compaction:**
- Periodic rewrite keeping entries newer than retention window (default 7 days)
- Atomic rewrite (tmp + rename)
- **File**: `src/moss/src/infra/storage/store.rs`

### Phase 5: CLI Updates

**5a. Pin command:**
- `garden-rake pin seed-bank [<name>]` — lock the Primary role to its current holder
- `garden-rake unpin seed-bank [<name>]` — release the lock, return to normal orchestration
- **Name resolution:**
  - If only one logical seed bank exists in the garden → auto-select, no prompt
  - If two or more logical seed banks exist → show a grouped selection view:
    ```
    Select a seed bank to pin:

      zen-garden
        ● 01956a3e  64GB   stone-01  Primary  ★ pinned
          0195b2c4  128GB  stone-02  Dormant

      backups
          0196d1f0  256GB  stone-01  Primary
          0196e4a2  256GB  stone-03  Dormant

    > [1] zen-garden  [2] backups
    ```
  - Grouped by logical name. Each replica shows: short_id, capacity, hosting stone, role
  - `●` marks the current Primary, `★ pinned` marks a pinned bank
  - Selection is by logical name (not individual replica)
- Pinning prevents the orchestration task from reassigning Primary (e.g., when a higher stone_id comes online)
- Stored as `pinned: true` in `seed_bank_roles` state; propagated via beacons so all stones respect it
- **Files**: `src/rake/src/commands/storage.rs`, `src/moss/src/api/v1/storage.rs`

**5b. Release disambiguation:**
- When multiple same-name replicas on one stone: show list with short ID, capacity, device, role
- Extract the compact seed-bank summary formatting (short_id, capacity, device, role) into a shared utility
  in `src/common/src/storage.rs` — reused by both the portrait endpoint (`PortraitSeedBank`) and the CLI
  release picker. Avoids duplicating the formatting logic.
- **Files**: `src/common/src/storage.rs`, `src/rake/src/commands/storage.rs`, `src/moss/src/api/v1/portrait.rs`

**5c. Show command enhancement:**
- `garden-rake show seed-banks` lists logical seed banks with replica count, primary location, encryption state
- **File**: `src/rake/src/commands/storage.rs`

**5d. Default naming:**
- `public-seed-bank` (no pond or public), `private-seed-bank` (pond-encrypted)
- User overrides with `--name`
- **File**: `src/moss/src/api/v1/storage.rs`, `src/rake/src/commands/storage.rs`

### Phase 6: Pond-Scoped Encryption

**6a. Manifest extension:**
- Add `encrypted: bool` and `pond_fingerprint: Option<String>` to `SeedBankManifest`
- **File**: `src/common/src/storage.rs`

**6b. Pond data key distribution:**
- Generate `pond_data_key` at pond init; derive from CA private key via BLAKE3-KDF
- Distribute to stones during enrollment (encrypted with stone certificate)
- Store in secrets backend (TPM/keyring/encrypted file)
- **Files**: `src/moss/src/api/v1/pond.rs`, `src/moss/src/infra/secrets.rs`

**6c. `SeedBankStore` — the I/O chokepoint:**
- Single struct with `mount_root: PathBuf` + `dek: Option<[u8; 32]>`
- Methods: `read()`, `write()`, `delete()`, `exists()`
- If `dek` is `Some` → ChaCha20-Poly1305 encrypt on write, decrypt on read
- If `dek` is `None` → passthrough to filesystem
- Registry constructs with `dek: Some(derived)` or `dek: None` at mount time
- All seed bank callers (nurturing, storage API, S3, replication) use `SeedBankStore` exclusively
- **Files**: `src/moss/src/infra/storage/registry.rs` (struct + impl), `src/moss/src/infra/nurturing_store.rs`, `src/moss/src/api/v1/storage.rs`

**6d. Encrypted prepare flow:**
- Standard filesystem format (no special encryption step)
- Write manifest with `encrypted: true` and `pond_fingerprint`
- Encryption choice prompt in CLI
- **Files**: `src/moss/src/api/v1/storage.rs`, `src/rake/src/commands/storage.rs`

**6e. Encryption policy:**
- Add `seed_bank_policy` to pond CA metadata / enrollment config
- Cache in stone's `pond.json`
- Enforce in prepare handler (block public under `require-encrypted`)
- Warn on public writes under `prefer-encrypted`
- **Files**: `src/moss/src/domain/pond.rs`, `src/moss/src/api/v1/storage.rs`

**6f. Drain enhancement:**
- List affected encrypted seed banks with data sizes in drain confirmation
- **File**: `src/moss/src/api/v1/pond.rs`, `src/rake/src/commands/management/pond.rs`

**6g. Encryption metrics:**
- Timing captured inside `SeedBankStore::read()` / `SeedBankStore::write()`
- Surface in `show seed-bank` and logs
- **Files**: `src/moss/src/infra/storage/registry.rs`, `src/rake/src/commands/storage.rs`

**6h. Rename encryption constraint:**
- Reject rename across encryption boundaries (409 ENCRYPTION_MISMATCH)
- **File**: `src/moss/src/api/v1/storage.rs`

### Phase 7: Clone Command

**7a. Clone API endpoint:**
- `POST /api/v1/stone/storage/clone` with source name, target name, target device, encryption flag
- Reads from source through `SeedBankStore` (transparent decryption if encrypted), writes to target
- Prepares target device, writes plaintext data
- **File**: `src/moss/src/api/v1/storage.rs`

**7b. Clone CLI:**
- `garden-rake clone seed-bank <source> to <target-name> [--unencrypted]`
- Warns under `require-encrypted` policy
- **File**: `src/rake/src/commands/storage.rs`

## Existing Code to Reuse

| Component | Location | Reuse |
|-----------|----------|-------|
| `SeedBankManifest` | `src/common/src/storage.rs` | Simplified (remove group/replica_id) |
| `SeedBankRegistry::scan()` | `src/moss/src/infra/storage/registry.rs` | Fix depth, reuse structure |
| `StorageBeacon` | `src/common/src/storage.rs` | Extend with role |
| `StorageCache` | `src/moss/src/domain/storage_cache.rs` | Add role-aware resolution |
| `NurturingStore` | `src/moss/src/infra/nurturing_store.rs` | Add primary routing |
| `NurturingScheduler` | `src/moss/src/tasks/nurturing_scheduler.rs` | Add short-circuit write |
| `offering_orchestration.rs` | `src/moss/src/tasks/offering_orchestration.rs` | Pattern for role assignment, reconciliation, failover, pinning |
| `MountTracker` | `src/moss/src/infra/storage/registry.rs` | Expose via AppState for release handler |
| `EventBus` / SSE | `src/moss/src/api/v1/events.rs` | Emit replication events |
| Object storage API | `src/moss/src/api/v1/storage.rs` | `GET /bank/:id/*path` for artifact download |
| Pond state | `src/moss/src/domain/pond.rs` | `PondState`, `PondMetadata`, pond active check |
| Secrets backend | `src/moss/src/infra/secrets.rs` | TPM/keyring/encrypted file for `pond_data_key` storage |
| koi-crypto | `tools/koi/crates/koi-crypto` | Key management, BLAKE3-KDF |
| BLAKE3 | `blake3` crate | DEK derivation from `pond_data_key` + seed bank name |
| ChaCha20-Poly1305 | `chacha20poly1305` crate | Encrypt/decrypt inside `SeedBankStore` |

## Consequences

**Positive:**
- Seed bank redundancy: USB drive failure doesn't lose data
- Geographic distribution: primary on stone-01, dormant on stone-02 — data accessible from either location
- Consistent model: same primary/dormant/pin/failover pattern as offerings — one mental model
- Eliminates remote index race: single-writer model
- Simplifies data model: fewer fields on manifest, fewer parameters on prepare
- Foundation for Replant ceremony (ORCH-0001): seed banks become reliable artifact sources for cross-stone offering replication

**Negative:**
- Write latency increases for remote primary: harvest must travel over network before it's "committed"
- SSE dependency for replication: if SSE stream disconnects, dormant falls behind (mitigated by polling fallback on reconnect)
- Mount path migration: existing seed banks mount at `mounts/{name}`, new scheme uses `mounts/{name}/{short_id}/` — needs migration logic or backward-compat scan
- Directory structure visible on encrypted seed banks: filenames and paths not encrypted, only content. An attacker with physical access can see which offerings have snapshots but not their data.
- `SeedBankStore` chokepoint: all seed bank I/O must go through one gateway. This is intentional but requires discipline — any direct filesystem access bypasses encryption.
- Drain destroys encrypted data permanently: no re-key, no recovery. By design, but high-consequence.
- 29-byte overhead per encrypted file: 1-byte version + 12-byte nonce + 16-byte auth tag. Negligible for harvest-sized files.

**Migration:**
- Existing v2 manifests with `group`/`replica_id`: fields ignored on deserialize (`serde(default)`, `skip_serializing_if`)
- Existing mount paths (`mounts/{name}/`): scan logic checks both old (`mounts/{name}/.zen-garden/manifest.json`) and new (`mounts/{name}/{short_id}/.zen-garden/manifest.json`) patterns. On first remount (reboot or release+plug), device moves to new path automatically.

## References

- STORAGE-0002: [Storage API Structure](STORAGE-0002-api-structure.md)
- STORAGE-0003: [Storage Beacon Protocol](STORAGE-0003-beacon-protocol.md)
- STORAGE-0004: [Seed Bank Plug-and-Play Resilience](STORAGE-0004-seedbank-resilience.md)
- STORAGE-0005: [Manifest-First Seed Bank Discovery](STORAGE-0005-manifest-first-discovery.md)
- ORCH-0001: [Replant Ceremony](ORCH-0001-replant-ceremony.md)
- Pond security model: `docs/philosophy/pond-security-model.md`
- Pond protocol spec: `docs/specs/POND-0001-protocol.md`
- Offering orchestration: `src/moss/src/tasks/offering_orchestration.rs`
- Seed bank registry: `src/moss/src/infra/storage/registry.rs`
- Nurturing store: `src/moss/src/infra/nurturing_store.rs`
- Storage cache: `src/moss/src/domain/storage_cache.rs`
- Pond state: `src/moss/src/domain/pond.rs`
- Secrets infrastructure: `src/moss/src/infra/secrets.rs`
- Stone client (mTLS): `src/moss/src/infra/stone_client.rs`
