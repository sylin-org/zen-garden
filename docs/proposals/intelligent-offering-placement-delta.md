# Intelligent Offering Placement: Implementation Delta Assessment

**Status**: Assessment Document
**Date**: 2026-01-23
**Purpose**: Identify gaps between the specification and actual implementation

---

## Executive Summary

The Intelligent Offering Placement feature is **substantially implemented** but contains **5 significant gaps** that undermine its usefulness:

| # | Gap | Severity | Impact |
|---|-----|----------|--------|
| 1 | Display format differs from spec | LOW | Cosmetic only |
| 2 | Compatibility icon always shows ❌ | MEDIUM | "pass" vs "compatible" string mismatch |
| 3 | Peers without offering silently skipped | HIGH | No visibility into excluded stones |
| 4 | **Bypasses tended stone architecture** | **CRITICAL** | **Root cause: wrong stone orchestrates** |
| 5 | Real-time metrics not used | HIGH | CPU=0, storage=50% always |

**Root Cause Identified**: Gap 4 is the primary issue. Rake discovers stones via mDNS and picks the **first responder** instead of using the user's **tended stone**. This causes:
- Wrong stone becomes coordinator (incomplete topology)
- Only 1 of 3 stones returned (coordinator doesn't know about peers)
- Wrong stone claims "tended" status

| Category | Status |
|----------|--------|
| API Endpoint | Fully Implemented |
| Scoring Algorithm | Implemented (metrics issue) |
| Peer Discovery | **BROKEN** (bypasses tended stone) |
| Interactive Menu | Implemented (format + icon issues) |
| Quiet Mode | Fully Implemented |
| Real-time Metrics | **NOT IMPLEMENTED** |
| Multi-Stone Results | **BROKEN** |

---

## What's Fully Implemented

### Moss Backend

#### 1. API Endpoint
**Location**: `src/moss/src/api/v1/garden.rs`, `src/moss/src/bootstrap/router.rs`

- `POST /api/v1/garden/recommend` registered and functional
- Accepts `PlacementRequest` with offering name, preferences, top_n
- Returns `PlacementResponse` with full recommendation details

#### 2. Orchestration Flow
**Location**: `src/moss/src/domain/placement.rs`

The `recommend_placement()` function implements complete orchestration:

1. **Local Stone Evaluation** - Zero-latency self-assessment with +3 tended bonus
2. **Peer Discovery** - Loads stones from topology cache via `topology::get_all_stones()`
3. **Parallel Data Fetching** - Concurrent fetch of metrics and offerings with 3-second timeout
4. **Comprehensive Scoring** - All 7 factors applied per stone
5. **Filtering & Ranking** - Incompatible stones filtered, results sorted by score DESC

#### 3. Multi-Factor Scoring
**Location**: `src/moss/src/domain/scoring.rs`

All scoring functions are implemented with comprehensive unit tests:

| Factor | Points | Function |
|--------|--------|----------|
| Compatibility | 0 / -15 / -999 | `calculate_compatibility_penalty()` |
| Memory | 0-20 | `score_memory_headroom()` |
| CPU | 0-20 | `score_cpu_availability()` |
| Storage Capacity | 0-15 | `score_storage_capacity()` |
| Hardware Type | 0-12 | `score_storage_type()` |
| Distribution | -3 per service | `calculate_distribution_penalty()` |
| Tended Bonus | +3 | Applied in placement.rs |

#### 4. API Response Structure
**Location**: `src/moss/src/domain/placement.rs`

Complete response structure matching spec:

```rust
PlacementResponse {
    recommendations: Vec<PlacementRecommendation>,
    evaluated_stones: usize,
    timestamp: String,
}

PlacementRecommendation {
    stone_id: String,
    hostname: String,
    score: i32,
    is_local: bool,
    compatibility: String,  // "compatible", "fallback", "incompatible"
    metrics: PlacementMetrics,
    services_count: usize,
    breakdown: ScoreBreakdown,
}

ScoreBreakdown {
    compatibility: i32,
    memory: i32,
    cpu: i32,
    storage: i32,
    hardware: i32,
    distribution: i32,
    tended_bonus: i32,
}
```

### Rake Client

#### 1. Keyword Parsing
**Location**: `src/rake/src/parser.rs`

- `somewhere` keyword recognized in zen syntax (line 159-161)
- `ParsedKeywords.somewhere` field populated correctly
- Works with `quietly` modifier for mode selection

#### 2. Keyword Translation
**Location**: `src/rake/src/main.rs`

```rust
if parsed.keywords.somewhere {
    let mode = if parsed.keywords.quietly { "auto" } else { "interactive" };
    args.push("--placement-mode".to_string());
    args.push(mode.to_string());
}
```

| Zen Syntax | Translated Flag | Behavior |
|------------|-----------------|----------|
| `offer mongodb somewhere` | `--placement-mode interactive` | Shows menu |
| `offer mongodb somewhere quietly` | `--placement-mode auto` | Auto-selects |

#### 3. Interactive Menu
**Location**: `src/rake/src/commands/offering/mod.rs` (lines 872-1030)

Full implementation of `handle_placement_recommendation()`:

- Discovers available stones
- Calls Moss API: `POST /api/v1/garden/recommend`
- Displays top 3 recommendations with:
  - Rank number (1, 2, 3)
  - Compatibility icon (check, warning, X)
  - Stone hostname and ID
  - Total score
  - Resource breakdown (memory %, CPU %, storage %)
  - Service count
  - Local stone indicator
- Handles user input (numeric selection, q/quit/exit)
- Single option: Y/n confirmation
- Multiple options: "Select stone (1-N)" prompt

#### 4. Quiet Mode
**Location**: `src/rake/src/commands/offering/mod.rs` (lines 939-950)

- Auto-selects top recommendation without prompting
- Finds stone endpoint from discovered stones
- Calls `install_on_stone()` directly

---

## Implementation Gaps

### Gap 1: Interactive Menu Display Format

**Severity**: LOW (cosmetic)
**Location**: `src/rake/src/commands/offering/mod.rs` (lines 959-982)

The interactive menu is implemented but the display format differs from the spec.

**Spec shows:**
```
Select target stone:
  1. ⭐ oak.local     [Score: 87/100] ← tended stone
     Memory: 24 GB free | CPU: 12% | Storage: 450 GB (NVMe)
     Services: 3 running
```

**Actual implementation shows:**
```
1. ✅ hostname (score: 87)
   Stone: stone-id
   Resources: 75% mem free, 12% CPU load, 50% storage free
   Services: 3 running
   🏠 (tended stone)
```

#### Differences

| Aspect | Spec | Implemented | Impact |
|--------|------|-------------|--------|
| Tended stone marker | `⭐` inline with hostname | `🏠` on separate line | Minor UX difference |
| Score format | `[Score: 87/100]` | `(score: 87)` | Missing "/100" context |
| Memory display | Absolute: `24 GB free` | Percentage: `75% mem free` | Less intuitive |
| Storage display | Absolute + type: `450 GB (NVMe)` | Percentage only: `50% storage free` | **Missing storage type** |
| Storage type | Shown (NVMe/SSD/HDD) | **Not displayed** | Available in API but unused |
| Layout | Compact, pipe-separated | Multi-line, verbose | Different aesthetic |

#### Note on Storage Type

The `storage_type` field **exists in the API response** (`PlacementMetrics.storage_type: String` at `placement.rs:56`) but the Rake client doesn't display it. This is a simple display omission, not a backend gap.

---

### Gap 2: Compatibility String Mismatch

**Severity**: MEDIUM
**Location**: `src/moss/src/domain/placement.rs` (line 244), `src/rake/src/commands/offering/mod.rs` (lines 961-965)

The backend and client use different strings for compatibility status:

| Backend Returns | Client Expects | Icon Shown |
|-----------------|----------------|------------|
| `"pass"` | `"compatible"` | ❌ (wrong!) |
| `"fallback"` | `"fallback"` | ⚠️ (correct) |
| `"fail"` | anything else | ❌ (correct) |

**Result**: Compatible offerings show ❌ instead of ✅.

**Fix**: Either change backend to return "compatible" or change client to accept "pass":

```rust
// Option A: Fix client (offering/mod.rs)
let compat_icon = match rec.compatibility.as_str() {
    "compatible" | "pass" => "✅",
    "fallback" => "⚠️",
    _ => "❌",
};

// Option B: Fix backend (placement.rs line 244)
compatibility: match compat_str.as_str() {
    "pass" => "compatible".to_string(),
    other => other.to_string(),
},
```

---

### Gap 3: Peers Without Offering Silently Skipped

**Severity**: HIGH
**Location**: `src/moss/src/domain/placement.rs` (lines 124-143)

When a peer stone doesn't have the requested offering in its manifest catalog, it's **silently skipped** with only a debug log:

```rust
None => {
    tracing::debug!(
        "Offering not available on remote stone"  // Silent skip!
    );
}
```

**Impact**: If 3 stones exist but only 1 has "mariadb" in its offerings catalog, only 1 recommendation is returned with no explanation.

**User sees**: 1 recommendation instead of 3, with no indication why others were excluded.

**Fix**: Include unavailable stones in response with a status field, or add a summary like "2 stones excluded: offering not in catalog".

---

### Gap 4: Architectural Violation - Bypasses Tended Stone

**Severity**: CRITICAL
**Location**: `src/rake/src/commands/offering/mod.rs` (lines 886-921)

The placement command **violates Zen Garden's architecture** by bypassing the tended stone entirely.

**Current (Wrong) Flow:**
```
Rake → mDNS discover ALL stones → pick FIRST that responds → that stone orchestrates
```

**Expected Flow** (mirrors `observe.rs` pattern):
```
1. Try cached tending → if responds, use it
2. If no tending OR tended stone offline → fallback to mDNS discovery
3. Pick first available stone → auto-tend to it → use it
```

The key difference: **try tended stone first, with automatic fallback**. The current code skips step 1 entirely.

**Code Evidence:**
```rust
// Line 887: WRONG - immediately discovers all stones, never tries tended first
let stones = discovery::discover_moss_auto(Duration::from_secs(3))?;

// Lines 895-921: Loops through discovered stones, picks first responder
for stone in &stones {
    // ...tries first stone that responds
}
```

**Correct Pattern** (from `observe.rs:140-227`):
```rust
// 1. Try tended stone first
if let Some(ref tended) = tending::read_tending().ok() {
    let topology_url = format!("{}/api/v1/garden/topology", tended.endpoint);
    match ctx.client.get(&topology_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            // Use tended stone's data and return
            return Ok(());
        }
        Ok(_) | Err(_) => {
            // "Stone is sleeping (offline). Picking a new stone..."
            // Fall through to discovery
        }
    }
}
// 2. Fallback: discover via Lantern or mDNS
discovery::discover_lantern_background();
```

**Impact:**
1. Wrong stone becomes coordinator (first discovered via mDNS, not tended)
2. That stone's topology cache may be incomplete → missing peers
3. That stone claims `is_local: true` → wrong +3 bonus
4. User's actual tended stone (stone-bronze-canyon) is ignored
5. Random stone (stone-crystal-forest) takes over

**This is the root cause of "only 1 of 3 stones returned":**
- The first-discovered stone has incomplete topology (hasn't been tended, hasn't received chirps)
- The tended stone (which has complete topology from active use) is never queried

---

### Gap 5: Real-Time Metrics Not Used

The `fetch_stone_metrics()` function queries `/capabilities` instead of `/metrics`:

```rust
// Line 63 in metrics_collection.rs
let capabilities_url = format!("{}/capabilities", endpoint.trim_end_matches('/'));
```

The `/capabilities` endpoint returns static hardware specs, not real-time metrics. The function then **hardcodes estimates**:

```rust
// Lines 94-95 in metrics_collection.rs
cpu_load_percent: 0,                                          // HARDCODED
storage_free_gb: caps.hardware.disk.as_ref()
    .map(|d| d.total_gb / 2).unwrap_or(0),                    // ESTIMATED AS 50%
```

#### What Exists But Is Unused

A real `/metrics` endpoint exists at `bootstrap/router.rs` line 31:

```rust
.route("/metrics", get(api::v1::metrics::get_metrics))
```

The `src/moss/src/api/v1/metrics.rs` module provides actual real-time data:
- CPU usage percentage (actual, not 0)
- Memory usage (actual used/available)
- Disk usage (actual used/available)
- System uptime

#### Impact on Scoring

| Factor | Expected Input | Actual Input | Impact |
|--------|---------------|--------------|--------|
| CPU | Real 0-100% load | Always 0 | All stones appear idle |
| Storage | Real free GB | 50% of total | Inaccurate capacity assessment |

**Result**: CPU scoring is meaningless. Storage scoring is unreliable. Placement decisions are biased toward whatever other factors differentiate stones (compatibility, memory, service count).

---

## Spec vs Implementation Matrix

| Specification Requirement | Implemented | Notes |
|--------------------------|-------------|-------|
| `POST /api/v1/garden/recommend` endpoint | YES | Fully functional |
| Request schema (offering, preferences, top_n) | YES | Matches spec |
| Response schema (recommendations, breakdown) | YES | Matches spec |
| Compatibility filtering (arch match) | YES | Compatible/Fallback/Incompatible |
| Compatibility icon (✅/⚠️/❌) | **NO** | Always shows ❌ due to string mismatch |
| Memory headroom scoring (0-20 pts) | YES | Real data used |
| CPU availability scoring (0-20 pts) | **PARTIAL** | Always receives 0 |
| Storage capacity scoring (0-15 pts) | **PARTIAL** | 50% estimate, not real |
| Hardware type scoring (0-12 pts) | YES | NVMe/SSD/HDD detected |
| Distribution penalty (-3 per service) | YES | Real service count |
| Tended bonus (+3 for local) | **BROKEN** | Goes to coordinator, not user's tended stone |
| Peer stone discovery | YES | Via topology cache |
| Parallel metrics fetch | YES | 3-second timeout |
| Parallel offerings fetch | YES | Zipped with metrics |
| Multi-stone recommendations | **BROKEN** | Peers without offering silently skipped |
| "somewhere" keyword parsing | YES | Zen syntax works |
| "quietly" modifier | YES | Auto-selects top |
| Interactive menu (top 3) | YES | Functional but format differs |
| ⭐ tended marker inline | **NO** | Uses 🏠 on separate line |
| Score format "[87/100]" | **NO** | Shows "(score: 87)" |
| Absolute memory display | **NO** | Shows percentage instead |
| Storage type in display | **NO** | In API but not displayed |
| Score breakdown in menu | **PARTIAL** | Shows resources, not factor breakdown |
| Error handling (no stones, timeout) | YES | Graceful degradation |

---

## Missing Features (Per Spec)

### Future Enhancements Not Implemented

These were explicitly listed as "Non-Goals" or "Future" in the spec:

1. **Preference hints** - `preferring ssd`, `with plenty of memory`
2. **Machine learning** - Historical placement optimization
3. **Cross-subnet with Lantern** - Distributed placement
4. **Placement policies** - Declarative constraints in TOML

These are correctly deferred; they do not represent implementation gaps.

---

## Recommended Fixes

### Priority 1: Real-Time Metrics (Critical)

**Severity**: HIGH
**Effort**: ~1-2 hours

**File**: `src/moss/src/domain/metrics_collection.rs`

Change `fetch_stone_metrics()` to call `/metrics` endpoint:

```rust
// Before
let capabilities_url = format!("{}/capabilities", endpoint);

// After
let metrics_url = format!("{}/metrics", endpoint);
let metrics: MetricsResponse = client.get(&metrics_url).send().await?.json().await?;

StoneMetrics {
    cpu_load_percent: metrics.cpu_usage_percent,
    storage_free_gb: metrics.disk_available_gb,
    // ... other real values
}
```

**Estimated effort**: ~10-20 lines changed, 1-2 hours including testing.

### Priority 2: Compatibility String Mismatch (Quick Fix)

**Severity**: MEDIUM
**Effort**: ~5 minutes
**File**: `src/rake/src/commands/offering/mod.rs` (line 961)

Fix the compatibility icon to accept both "pass" and "compatible":

```rust
// Before:
let compat_icon = match rec.compatibility.as_str() {
    "compatible" => "✅",
    "fallback" => "⚠️",
    _ => "❌",
};

// After:
let compat_icon = match rec.compatibility.as_str() {
    "compatible" | "pass" => "✅",
    "fallback" => "⚠️",
    _ => "❌",
};
```

### Priority 3: Multi-Stone Visibility (Important UX)

**Severity**: HIGH
**Effort**: ~2-3 hours
**Files**: `src/moss/src/domain/placement.rs`, `src/rake/src/commands/offering/mod.rs`

**Option A**: Include excluded stones in response with status

```rust
// Add to PlacementResponse
pub struct PlacementResponse {
    pub recommendations: Vec<PlacementRecommendation>,
    pub excluded: Vec<ExcludedStone>,  // NEW
    pub evaluated_stones: usize,
    pub timestamp: String,
}

pub struct ExcludedStone {
    pub hostname: String,
    pub reason: ExclusionReason,
}

pub enum ExclusionReason {
    OfferingNotInCatalog,
    MetricsFetchFailed,
    OfferingsFetchFailed,
    Incompatible,
}
```

**Option B**: Add summary to response (simpler)

```rust
// Add to PlacementResponse
pub struct PlacementResponse {
    // ...existing fields...
    pub exclusion_summary: Option<String>,  // e.g., "2 stones excluded: offering not available"
}
```

Then display in Rake:
```rust
if let Some(summary) = &placement.exclusion_summary {
    println!("{}ℹ️ {}", indent, summary);
}
```

### Priority 4: Architectural Fix - Use Tended Stone with Fallback (CRITICAL)

**Severity**: CRITICAL
**Effort**: ~45 minutes
**File**: `src/rake/src/commands/offering/mod.rs` (lines 886-921)

**Replace the discovery logic with the observe.rs pattern** — try tended stone first, with automatic fallback:

```rust
// BEFORE (wrong):
let stones = discovery::discover_moss_auto(Duration::from_secs(3))?;
for stone in &stones {
    // Try first stone that responds
}

// AFTER (correct - mirrors observe.rs pattern):
use crate::tending;

// Helper to send placement request to a stone
async fn try_placement_on_stone(
    client: &reqwest::Client,
    endpoint: &str,
    offering: &str,
) -> anyhow::Result<PlacementResponse> {
    let url = format!("{}/api/v1/garden/recommend", endpoint.trim_end_matches('/'));
    let payload = serde_json::json!({
        "offering": offering,
        "preferences": [],
        "top_n": 3
    });

    let response = client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Stone returned error: {}", response.status());
    }

    Ok(response.json().await?)
}

// 1. Try tended stone first (if available)
if let Ok(tended) = tending::read_tending() {
    match try_placement_on_stone(&client, &tended.endpoint, offering).await {
        Ok(placement) => {
            // Tended stone responded - use its recommendations
            return Ok(placement);
        }
        Err(e) => {
            // Tended stone offline - fall through to discovery
            tracing::warn!(
                "Tended stone '{}' is sleeping (offline): {}. Picking a new stone...",
                tended.stone_name, e
            );
        }
    }
}

// 2. Fallback: discover available stones
let stones = discovery::discover_moss_auto(Duration::from_secs(3))?;
if stones.is_empty() {
    anyhow::bail!("No stones found in garden");
}

// 3. Try first available stone and auto-tend to it
for stone in &stones {
    match try_placement_on_stone(&client, &stone.stone_endpoint, offering).await {
        Ok(placement) => {
            // Auto-tend to this stone (matches observe.rs behavior)
            tending::write_tending(stone.stone_name.clone(), stone.stone_endpoint.clone())?;
            return Ok(placement);
        }
        Err(e) => {
            tracing::debug!("Stone {} failed: {}", stone.stone_name, e);
            continue;
        }
    }
}

anyhow::bail!("No stones available for placement orchestration")
```

**This fix:**
1. Tries the user's tended stone first (from `~/.zen-garden/.tending`)
2. Falls back to discovery if no tending or tended stone is offline
3. Auto-tends to whichever stone responds (matching observe.rs behavior)
4. Follows the same pattern as `observe` command — no explicit "tend first" required
5. Tended Moss has complete topology (it's been actively used)
6. Tended Moss correctly identifies itself as `is_local: true`
7. All 3 stones will be discovered via tended Moss's topology cache

### Priority 5: Metrics Fallback Handling

**Severity**: MEDIUM
**Effort**: ~30 minutes

If `/metrics` endpoint is unavailable (older Moss versions), fall back to `/capabilities` with estimates but log a warning:

```rust
match fetch_metrics(endpoint).await {
    Ok(m) => m,
    Err(_) => {
        warn!("Falling back to estimated metrics for {}", endpoint);
        fetch_capabilities_estimated(endpoint).await?
    }
}
```

### Priority 6: Display Format Alignment (Cosmetic)

**Severity**: LOW
**Effort**: ~1-2 hours
**File**: `src/rake/src/commands/offering/mod.rs` (lines 959-982)

Update the interactive menu display to match the spec format:

```rust
// Current (lines 967-979):
println!("{}{}. {} {} (score: {})", indent, rank, compat_icon, rec.hostname, rec.score);
println!("{}   Stone: {}", indent, rec.stone_id);
println!("{}   Resources: {}% mem free, {}% CPU load, {}% storage free", ...);

// Proposed:
let tended_marker = if rec.is_local { "⭐ " } else { "  " };
let tended_label = if rec.is_local { " ← tended stone" } else { "" };
println!("{}  {}. {}{:<16} [Score: {}/100]{}",
    indent, rank, tended_marker, rec.hostname, rec.score, tended_label);

let mem_gb = rec.metrics.memory_free_mb / 1024;
println!("{}     Memory: {} GB free | CPU: {}% | Storage: {} GB ({})",
    indent,
    mem_gb,
    rec.metrics.cpu_load_percent,
    rec.metrics.storage_free_gb,
    rec.metrics.storage_type);
println!("{}     Services: {} running", indent, rec.services_count);
```

Key changes:
- Use `⭐` inline with hostname for tended stone
- Format score as `[Score: 87/100]` not `(score: 87)`
- Show absolute memory (`24 GB free`) not percentage
- Include storage type (`NVMe`, `SSD`, `HDD`)
- Compact single-line format for metrics

---

## Verification Checklist

After implementing fixes, verify:

**Priority 1 (Real-Time Metrics):**
- [ ] `garden-rake offer redis somewhere` shows real CPU load percentages (not all 0%)
- [ ] Storage free values differ between stones with different disk usage
- [ ] Scoring properly differentiates stones under different loads

**Priority 2 (Compatibility Icon):**
- [ ] Compatible offerings show ✅ (not ❌)
- [ ] Fallback offerings show ⚠️
- [ ] Incompatible offerings show ❌

**Priority 3 (Multi-Stone Visibility):**
- [ ] All 3 stones appear when all have the offering in catalog
- [ ] When stones are excluded, user sees why (e.g., "2 stones excluded: offering not in catalog")
- [ ] Response includes `excluded` array or `exclusion_summary`

**Priority 4 (Tended Stone with Fallback):**
- [ ] When tended stone is online, it receives the placement request
- [ ] When tended stone is online, it gets the +3 bonus as `is_local: true`
- [ ] When tended stone is offline, fallback to mDNS discovery occurs automatically
- [ ] When no tending exists, mDNS discovery proceeds without error
- [ ] Fallback discovery auto-tends to whichever stone responds
- [ ] `is_local` field accurately reflects which stone is the coordinator

**Priority 5 (Fallback):**
- [ ] Fallback works gracefully when /metrics unavailable

**Priority 6 (Display):**
- [ ] Tended stone shows `⭐` marker inline with hostname
- [ ] Score displays as `[Score: 87/100]` format
- [ ] Memory shows absolute value (`24 GB free`)
- [ ] Storage type shows (`NVMe`, `SSD`, `HDD`)
- [ ] Layout matches spec (compact, pipe-separated metrics)

**Regressions:**
- [ ] All existing interactive menu functionality still works
- [ ] Quiet mode still auto-selects correctly
- [ ] User selection (1-N, q to quit) still works

---

## Conclusion

The Intelligent Offering Placement feature has the **infrastructure in place** but is **not production-ready** due to a critical architectural violation and several secondary issues.

### Root Cause

**Gap 4 (Bypasses Tended Stone)** is the primary issue:
- Rake immediately does mDNS discovery, skipping the tended stone entirely
- The first-discovered stone (not the tended stone) becomes coordinator
- That stone's topology cache is incomplete (it hasn't been actively used)
- Result: "only 1 of 3 stones returned"

**Fix Priority 4 first** — apply the `observe.rs` pattern (try tended → fallback to discovery → auto-tend). This ~45 minute fix will likely resolve the multi-stone issue automatically.

### All Issues

1. **Architectural violation** — Bypasses tended stone, wrong coordinator
2. **Compatibility icons wrong** — String mismatch ("pass" vs "compatible")
3. **Metrics fabricated** — CPU always 0%, storage always 50%
4. **Silent exclusion** — No visibility into why stones are excluded
5. **Display format** — Minor cosmetic differences from spec

**Total estimated fix effort**: ~6-8 hours across 6 priorities

| Priority | Fix | Effort | Impact |
|----------|-----|--------|--------|
| 4 | **Use tended stone + fallback** | 45 min | **Fixes multi-stone issue** |
| 2 | Compatibility string | 5 min | Fixes ❌ icon |
| 1 | Real-time metrics | 1-2 hrs | Accurate scoring |
| 3 | Exclusion visibility | 2-3 hrs | Better UX |
| 5 | Metrics fallback | 30 min | Robustness |
| 6 | Display format | 1-2 hrs | Cosmetic |

**Recommended order**: 4 → 2 → 1 → 3 → 5 → 6

The spec's "Implementation Summary" marking this as "✅ Complete" should be revised to reflect these gaps.

---

## References

- Original spec: `docs/proposals/intelligent-offering-placement.md`
- Placement domain: `src/moss/src/domain/placement.rs`
- Scoring functions: `src/moss/src/domain/scoring.rs`
- Metrics collection: `src/moss/src/domain/metrics_collection.rs`
- Metrics API: `src/moss/src/api/v1/metrics.rs`
