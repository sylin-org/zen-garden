# ELECTION-0001: Distributed Election Protocol

**Status**: Draft  
**Created**: 2026-01-25  
**Authors**: Leo Botinelly

---

## Abstract

This specification defines a lightweight, stateless distributed election protocol for Zen Garden. The protocol enables any stone to request a "winner" from a set of candidates without centralized coordination, using deterministic hash-based delays to prevent thundering herd problems.

---

## Motivation

Garden operations often require selecting ONE stone from many candidates:

- **Update distribution**: "Who should I download the new Moss binary from?"
- **Ceremony coordination**: "Who should coordinate my vacate ceremony?"
- **Failover**: "Which stone should take over this offering?"

Traditional approaches have drawbacks:

- **Random selection**: May pick unhealthy or overloaded stone
- **Fixed leader**: Single point of failure
- **Consensus protocols**: Overkill for ephemeral selections

This protocol provides:

- **Zero pre-coordination**: Any stone can initiate
- **Deterministic ordering**: Hash-based, reproducible delays
- **Failure resilience**: Requester owns the flow, retries on failure
- **Concurrent elections**: Multiple independent elections via `election_id`

---

## Design Goals

The election module is designed as a **generic, testable, service-agnostic library**:

| Goal                 | Description                                                  |
| -------------------- | ------------------------------------------------------------ |
| **Generic**          | Any Moss service can use elections (updates, ceremonies, failover) |
| **Testable**         | Pure functions, mockable transport, no global state          |
| **Transport-agnostic** | Core logic separate from UDP; transport injected           |
| **No domain knowledge** | Module knows nothing about updates, slots, or offerings   |
| **Criteria-driven**  | Eligibility is caller-defined via BSON-style predicates      |

### Separation of Concerns

```
┌─────────────────────────────────────────────────────────────────┐
│  Caller (e.g., UpdateService)                                   │
│  - Defines criteria: {"moss_version": {"$gt": "0.1.309"}}       │
│  - Handles winner: download update, manage slots                │
│  - Handles failure: retry election, backoff                     │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│  Election Module (generic)                                      │
│  - Broadcast request, collect candidates                        │
│  - Calculate delays, select winner                              │
│  - Evaluate criteria against local state                        │
│  - NO knowledge of what "update_source" means                   │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│  Transport (injected)                                           │
│  - UDP broadcast/unicast                                        │
│  - Mockable for testing                                         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Protocol Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│  ELECTION PROTOCOL                                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Requester                              Candidates                  │
│        │                                                            │
│        │─── ELECTION_REQUEST (broadcast) ───────────────────────>   │
│        │                                                            │
│        │                                [Each calculates delay]     │
│        │                                [Start countdown timer]     │
│        │                                                            │
│        │<── ELECTION_CANDIDATE ───────── First to finish delay      │
│        │                                                            │
│        │─── ELECTION_RESULT (broadcast) ────────────────────────>   │
│        │                                [Others cancel timers]      │
│        │                                                            │
│        │    [Requester proceeds with winner]                        │
│        │                                                            │
└─────────────────────────────────────────────────────────────────────┘
```

### Key Properties

| Property              | Description                                                      |
| --------------------- | ---------------------------------------------------------------- |
| **Stateless**         | No persistent state; candidates only hold timers during election |
| **Requester-owned**   | Requester initiates, selects, announces, and acts                |
| **Concurrent-safe**   | Multiple elections identified by unique `election_id`            |
| **Failure-resilient** | Lost messages cause wasted packets, not deadlocks                |
| **Deterministic**     | Same inputs produce same delay ordering                          |
| **Self-excluding**    | Requester ignores own elections via cached `election_id`         |
| **Service-agnostic**  | Generic module usable by any Moss service needing elections      |

---

## Message Types

### Transport

All election messages use **UDP broadcast** on port **7184** (shared with discovery).

Election messages use the existing `UdpAnnouncement` envelope format from `garden_common::types`:

```rust
pub struct UdpAnnouncement {
    #[serde(rename = "type")]
    pub announcement_type: String,  // "election_request", "election_candidate", "election_result"
    pub data: serde_json::Value,    // Typed payload
}
```

### New Announcement Types

Add to `garden_common::types::announcement_types`:

```rust
pub mod announcement_types {
    // Existing
    pub const DISCOVERY_REQUEST: &str = "discovery_request";
    pub const DISCOVERY_RESPONSE: &str = "discovery_response";
    pub const STONE_CHIRP: &str = "stone_chirp";
    pub const STONE_GOODBYE: &str = "stone_goodbye";
    
    // Election (new)
    pub const ELECTION_REQUEST: &str = "election_request";
    pub const ELECTION_CANDIDATE: &str = "election_candidate";
    pub const ELECTION_RESULT: &str = "election_result";
}
```

### 1. ELECTION_REQUEST

Broadcast by requester to initiate an election.

```json
{
  "election_id": "019bf5a2-1234-7abc-...",
  "election_type": "update_source",
  "criteria": {
    "moss_version": {"$gt": "0.1.202601250309"}
  }
}
```

| Field           | Type   | Description                              |
| --------------- | ------ | ---------------------------------------- |
| `election_id`   | GUIDv7 | Unique identifier for this election      |
| `election_type` | String | Type of election (see Election Types)    |
| `criteria`      | Object | BSON-style filter (see Criteria section) |

### 2. ELECTION_CANDIDATE

Unicast by candidate to requester after delay expires.

```json
{
  "election_id": "019bf5a2-1234-7abc-...",
  "stone_id": "019bece4-42e5-...",
  "stone_name": "stone-coral-prairie"
}
```

| Field         | Type   | Description                        |
| ------------- | ------ | ---------------------------------- |
| `election_id` | GUIDv7 | Must match request                 |
| `stone_id`    | String | Stone ID of candidate              |
| `stone_name`  | String | Name of candidate                  |

> **Note**: `election_id` is the only GUIDv7 generated for each election. `stone_id` values are existing identifiers from each stone's configuration.

> **Note**: Requester uses topology cache to resolve endpoint/metadata from `stone_id`.

### 3. ELECTION_RESULT

Broadcast by requester to announce winner and abort other candidates.

```json
{
  "election_id": "019bf5a2-1234-7abc-...",
  "winner_id": "019bece4-42e5-..."
}
```

| Field         | Type   | Description                            |
| ------------- | ------ | -------------------------------------- |
| `election_id` | GUIDv7 | Must match request                     |
| `winner_id`   | String | Stone ID of winner (from `stone_id`)   |

---

## Election Types

### `update_source`

Find a stone with newer Moss version to download update from.

**Criteria:**

```json
{
  "moss_version": {"$gt": "0.1.202601250309"}
}
```

### `ceremony_coordinator`

Find a stone to coordinate a multi-stone ceremony.

**Criteria:**

```json
{
  "health": {"$in": ["thriving", "recovering"]},
  "stone_id": {"$nin": ["019becd8-..."]}
}
```

### Future Types

- `replica_target`: Find stone to receive offering replica
- `backup_source`: Find stone with stored backup to restore from

---

## Criteria Evaluation

Criteria use BSON-style query operators. All conditions must match (implicit `$and`).

### Supported Operators

| Operator   | Description              | Example                                |
| ---------- | ------------------------ | -------------------------------------- |
| `$eq`      | Equals                   | `{"health": {"$eq": "thriving"}}`      |
| `$ne`      | Not equals               | `{"health": {"$ne": "dormant"}}`       |
| `$gt`      | Greater than             | `{"moss_version": {"$gt": "0.1.309"}}` |
| `$gte`     | Greater than or equal    | `{"slots": {"$gte": 1}}`               |
| `$lt`      | Less than                | `{"load": {"$lt": 0.8}}`               |
| `$lte`     | Less than or equal       | `{"load": {"$lte": 0.5}}`              |
| `$in`      | Value in array           | `{"health": {"$in": ["a", "b"]}}`      |
| `$nin`     | Value not in array       | `{"stone_id": {"$nin": ["..."]}}`      |
| `$exists`  | Field exists             | `{"gpu": {"$exists": true}}`           |

### Available Fields

Fields are resolved from `AppState` at evaluation time:

| Field          | Type   | Description                        |
| -------------- | ------ | ---------------------------------- |
| `stone_id`     | String | This stone's GUIDv7                |
| `stone_name`   | String | This stone's name                  |
| `moss_version` | String | Running Moss version (semver)      |
| `health`       | String | `thriving`, `recovering`, etc.     |
| `uptime`       | Number | Seconds since Moss started         |
| `load`         | Number | 0.0-1.0 normalized load            |
| `offerings`    | Number | Count of running offerings         |

### Version Comparison

Version strings use semver-aware comparison:

```
"0.1.202601250309" > "0.1.202601240101"  ✓
"0.2.0" > "0.1.999"                       ✓
```

### Evaluation Algorithm

```rust
fn matches_criteria(criteria: &Value, state: &AppState) -> bool {
    let obj = match criteria.as_object() {
        Some(o) => o,
        None => return true, // No criteria = always match
    };

    for (field, condition) in obj {
        let my_value = state.get(field);
        if !evaluate_condition(condition, my_value) {
            return false;
        }
    }
    true
}

fn evaluate_condition(condition: &Value, actual: Option<&Value>) -> bool {
    let cond_obj = condition.as_object().unwrap();
    
    for (op, expected) in cond_obj {
        let result = match op.as_str() {
            "$eq" => actual == Some(expected),
            "$ne" => actual != Some(expected),
            "$gt" => compare(actual, expected) == Ordering::Greater,
            "$gte" => compare(actual, expected) != Ordering::Less,
            "$lt" => compare(actual, expected) == Ordering::Less,
            "$lte" => compare(actual, expected) != Ordering::Greater,
            "$in" => expected.as_array().map(|a| a.contains(actual?)).unwrap_or(false),
            "$nin" => expected.as_array().map(|a| !a.contains(actual?)).unwrap_or(true),
            "$exists" => actual.is_some() == expected.as_bool().unwrap_or(true),
            _ => true, // Unknown operator = skip
        };
        if !result {
            return false;
        }
    }
    true
}
```

---

## Delay Calculation

### Algorithm

```rust
fn calculate_election_delay(my_stone_id: &str, election_id: &str) -> Duration {
    let input = format!("election:{}:{}", my_stone_id, election_id);
    let hash = blake3::hash(input.as_bytes());

    // First byte (0-255) × 30ms = 0-7650ms spread
    let delay_ms = (hash.as_bytes()[0] as u64) * 30;

    Duration::from_millis(delay_ms)
}
```

### Properties

| Property          | Value                                 |
| ----------------- | ------------------------------------- |
| **Hash function** | BLAKE3 (fast, cryptographic)          |
| **Delay range**   | 0 - 7650ms                            |
| **Granularity**   | 30ms steps (256 values)               |
| **Determinism**   | Same stone + election_id = same delay |

### Why BLAKE3?

- Already used in Lantern election
- Fast (3 GB/s on modern CPUs)
- Cryptographically secure (no gaming the delay)
- Available via `blake3` crate

---

## Candidate Behavior

### State Machine

```
                    ┌──────────────┐
                    │    IDLE      │
                    └──────┬───────┘
                           │ Receive ELECTION_REQUEST
                           │ (eligible)
                           ▼
                    ┌──────────────┐
   ELECTION_RESULT─>│   WAITING    │<─── timer expires
        (abort)     │  (timer on)  │     (respond)
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
       ┌──────────────┐          ┌──────────────┐
       │  SUPERSEDED  │          │  RESPONDED   │
       └──────────────┘          └──────────────┘
              │                         │
              └────────────┬────────────┘
                           │ (cleanup)
                           ▼
                    ┌──────────────┐
                    │    IDLE      │
                    └──────────────┘
```

### Algorithm

```
ON ELECTION_REQUEST:
  // Self-exclusion: requester caches election_id before broadcast
  IF election_id in my_initiated_elections:
    RETURN  // This is my own election, ignore

  IF not eligible(criteria):
    RETURN  // Ignore, not a candidate

  delay = calculate_delay(my_id, election_id)
  pending_elections[election_id] = Timer(delay)

  WHEN timer expires:
    SEND ELECTION_CANDIDATE to requester
    pending_elections[election_id].state = RESPONDED

  AFTER 10s:
    DELETE pending_elections[election_id]

ON ELECTION_RESULT:
  IF election_id in pending_elections:
    CANCEL timer
    DELETE pending_elections[election_id]
```

---

## Requester Behavior

### Algorithm

```
FUNCTION start_election(type, criteria, timeout=10s):
  election_id = generate_guidv7()

  // Cache before broadcast - prevents self-response
  my_initiated_elections.insert(election_id, now())

  BROADCAST ELECTION_REQUEST { election_id, type, criteria }

  winner = AWAIT first ELECTION_CANDIDATE with matching election_id
           TIMEOUT after `timeout`

  // Cleanup cache (with TTL, e.g., 60s)
  my_initiated_elections.remove_expired()

  IF winner:
    BROADCAST ELECTION_RESULT { election_id, winner.stone_id }
    RETURN winner
  ELSE:
    RETURN None  // No candidates responded
```

### Timeout Behavior

| Scenario               | Timeout | Action                            |
| ---------------------- | ------- | --------------------------------- |
| No candidates          | 10s     | Return None, caller decides retry |
| Winner dies during use | N/A     | Caller detects, new election      |
| Winner can't honor     | N/A     | Caller starts new election        |
| Network partition      | 10s     | May have partial candidates       |

---

## Concurrent Elections

Multiple elections can run simultaneously. Each is identified by unique `election_id`.

```
Time ──────────────────────────────────────────────────────>

Stone-A: ═══ REQ(e1) ═══════════════════ RESULT(e1) ════

Stone-B: ═══════════ REQ(e2) ═════════════════ RESULT(e2)
Stone-C:     [delay e1]  [delay e2]
                 │           │
                 └── CAND ───┴── CAND
```

Candidates track pending elections in a map:

```rust
pending_elections: HashMap<ElectionId, PendingElection>
```

---

## API Design

### Requester API

````rust
// In garden_common/src/election.rs

/// Election result
pub struct ElectionWinner {
    pub candidate_id: String,
    pub endpoint: String,
    pub metadata: serde_json::Value,
}

/// Start an election and await winner
///
/// # Example
/// ```rust
/// let winner = Election::new(ElectionType::UpdateSource)
///     .with_criteria(json!({ "min_version": "0.1.309" }))
///     .timeout(Duration::from_secs(10))
///     .run(&election_service)
///     .await?;
///
/// if let Some(w) = winner {
///     // Download update from w.endpoint
///     download_update(&w.endpoint).await?;
/// }
/// ```
pub struct Election {
    election_type: ElectionType,
    criteria: serde_json::Value,
    timeout: Duration,
}

impl Election {
    pub fn new(election_type: ElectionType) -> Self;
    pub fn with_criteria(self, criteria: serde_json::Value) -> Self;
    pub fn timeout(self, timeout: Duration) -> Self;

    /// Run election and await winner
    pub async fn run(self, service: &ElectionService) -> Result<Option<ElectionWinner>>;
}
````

### Callback Pattern (Alternative)

````rust
/// Fluent callback API
///
/// # Example
/// ```rust
/// election_service
///     .start(ElectionType::UpdateSource)
///     .with_criteria(json!({ "min_version": my_version }))
///     .on_winner(|winner| async move {
///         download_and_stage_update(&winner.endpoint).await
///     })
///     .on_no_candidates(|| async {
///         tracing::debug!("No update sources found");
///     })
///     .run()
///     .await;
/// ```
````

### Candidate Service

```rust
// In moss/src/tasks/election_service.rs

/// Background service handling election participation
pub struct ElectionService {
    my_stone_id: String,
    my_endpoint: String,
    pending: Arc<RwLock<HashMap<String, PendingElection>>>,
    eligibility_handlers: HashMap<ElectionType, Box<dyn EligibilityChecker>>,
}

impl ElectionService {
    /// Register handler for election type eligibility
    pub fn register_eligibility<F>(&mut self, election_type: ElectionType, checker: F)
    where
        F: Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + Sync + 'static;

    /// Called by UDP listener when election message received
    pub async fn handle_message(&self, msg: ElectionMessage, from: SocketAddr);
}

/// Trait for eligibility checking
pub trait EligibilityChecker: Send + Sync {
    /// Returns Some(metadata) if eligible, None if not
    fn check(&self, criteria: &serde_json::Value) -> Option<serde_json::Value>;
}
```

### Integration with Moss

```rust
// In moss bootstrap

// Register eligibility handlers
election_service.register_eligibility(
    ElectionType::UpdateSource,
    |criteria| {
        let min_version = criteria.get("min_version")?.as_str()?;
        if version_is_newer(&MOSS_VERSION, min_version) {
            Some(json!({
                "version": MOSS_VERSION,
                "binary_size": std::fs::metadata("/usr/local/bin/garden-moss")
                    .ok()?.len()
            }))
        } else {
            None
        }
    }
);

// Start background listener
tokio::spawn(election_service.run_listener());
```

---

## Implementation Location

```
src/
├── common/
│   └── src/
│       └── election.rs           # Core types, delay calculation
│           ├── ElectionType
│           ├── ElectionMessage
│           ├── ElectionWinner
│           ├── calculate_election_delay()
│           └── Election builder
│
└── moss/
    └── src/
        └── tasks/
            └── election_service.rs  # Background service
                ├── ElectionService
                ├── handle_message()
                ├── run_listener()
                └── EligibilityChecker trait
```

---

## Wire Format

### UDP Packet Structure

Election messages use the standard `UdpAnnouncement` JSON envelope:

```json
{
  "type": "election_request",
  "data": { ... payload ... }
}
```

Examples:

```json
{"type":"election_request","data":{"election_id":"019bf5a2...","election_type":"update_source",...}}
{"type":"election_candidate","data":{"election_id":"019bf5a2...","candidate_id":"019bece4...",...}}
{"type":"election_result","data":{"election_id":"019bf5a2...","winner_id":"019bece4..."}}
```

This reuses the existing UDP parsing infrastructure in `discovery.rs`.

### Size Budget

| Field       | Max Size    |
| ----------- | ----------- |
| Prefix      | 13 bytes    |
| election_id | 36 bytes    |
| stone IDs   | 72 bytes    |
| endpoint    | 64 bytes    |
| metadata    | ~200 bytes  |
| **Total**   | < 500 bytes |

Well under UDP MTU (1472 bytes typical).

---

## Security Considerations

### Phase 0.1 (Current)

- **Trust**: All garden stones trusted
- **No authentication**: Any stone can participate
- **No encryption**: Messages in plaintext

### Future Phases

- **Keystone validation**: Only keystoned stones can participate
- **Signed messages**: Prevent spoofing
- **Criteria validation**: Verify claimed metadata

---

## Failure Modes

| Failure           | Detection                   | Recovery             |
| ----------------- | --------------------------- | -------------------- |
| REQUEST lost      | No responses                | Timeout, retry       |
| CANDIDATE lost    | First CANDIDATE wins anyway | None needed          |
| RESULT lost       | Extra CANDIDATEs arrive     | Ignored by requester |
| Winner dies       | HTTP fails                  | New election         |
| Requester dies    | Candidates timeout (10s)    | Timers cleaned up    |
| Network partition | Partial candidates          | Best available wins  |

---

## Performance

### Latency

| Scenario                     | Latency         |
| ---------------------------- | --------------- |
| Best case (0ms delay winner) | ~1ms round-trip |
| Typical (middle delay)       | ~3.5s average   |
| Worst case (255×30ms)        | ~7.65s          |

### Bandwidth

| Message   | Size       | Frequency       |
| --------- | ---------- | --------------- |
| REQUEST   | ~200 bytes | Per election    |
| CANDIDATE | ~300 bytes | 1 per candidate |
| RESULT    | ~100 bytes | Per election    |

Minimal impact even with 100 stones and 10 concurrent elections.

---

## Testing Strategy

### Unit Tests

- Delay calculation determinism
- Message serialization/deserialization
- Timer cancellation on ELECTION_RESULT

### Integration Tests

- Two-stone election
- Multi-candidate election
- Concurrent elections
- Timeout behavior

### Chaos Tests

- Random message drops
- Network partitions
- Requester crash mid-election

---

## References

- [Lantern Election Implementation](../src/lantern/src/election.rs)
- [CSMA/CA Protocol](https://en.wikipedia.org/wiki/Carrier-sense_multiple_access_with_collision_avoidance)
- [BLAKE3 Hash Function](https://github.com/BLAKE3-team/BLAKE3)

