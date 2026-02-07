# Garden Probe - Integration Testing Guide

> **Purpose**: Test a live garden to catch real bugs that unit tests miss.

## Quick Start

```powershell
# Discover stones and run all tests
cargo run -p garden-probe -- --udp run --all

# Run specific category
cargo run -p garden-probe -- --udp run --category smoke

# Run specific test
cargo run -p garden-probe -- --udp run tend.reachable

# List available tests
cargo run -p garden-probe -- list
```

## Discovery Modes

| Mode | Flag | Description |
|------|------|-------------|
| **UDP** | `--udp` | Broadcast discovery (like Rake). Finds all stones on network. |
| **HTTP** | `-e <url>` | Query a known stone's topology. Fallback if UDP fails. |
| **Auto** | (default with `--udp`) | Try UDP first, fall back to HTTP if provided. |

UDP discovery is preferred because it:
- Finds all stones regardless of their internal topology state
- Caches stone list for fast failover testing
- Shows network physics (response times per stone)

## Test Categories

| Category | Purpose | When to Use |
|----------|---------|-------------|
| `smoke` | Basic health checks | After deployments, quick sanity check |
| `discovery` | Topology and stone visibility | Network debugging |
| `tend` | Tended stone selection & fallback | Testing resilience |
| `interstone` | Cross-stone communication | Before/after offering deploys |

## Writing Tests

### Test Structure

Every test is a plain async Rust function:

```rust
use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

pub fn my_test() -> TestDef {
    TestDef {
        id: "category.test_name",           // Unique ID
        name: "Human Readable Name",         // Shown in output
        description: "What this test does",  // Shown in list
        category: "category",                // For --category filter
        tags: &["tag1", "tag2"],            // For --tag filter
        run: |garden, bag| Box::pin(test_impl(garden, bag)),
    }
}

async fn test_impl(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let start = Instant::now();
    
    // Your test logic here
    
    let result = StepResult::ok_with(serde_json::json!({
        "key": "value",
    }));
    
    bag.record_step(
        "step_id",                           // Unique within test
        "Description of step",               // Human readable
        start.elapsed().as_millis() as u64,  // Duration
        result,                              // StepResult
    );
    
    Ok(bag)
}
```

### StepResult Values

```rust
StepResult::ok()                         // Pass, no data
StepResult::ok_with(json!({"k": "v"}))   // Pass with data
StepResult::failed("error message")      // Fail with reason
StepResult::skipped("reason")            // Skipped (dependency missing, etc)
```

### Using the Bag

The `Bag` accumulates state across test steps:

```rust
// Store value
bag.put("key", value);           // Any serializable value
bag.put("key", "string_value");

// Retrieve value
let count: usize = bag.get("key").unwrap_or(0);
let name: String = bag.require("name")?;  // Returns error if missing

// Record step (always do this!)
bag.record_step("step_id", "description", duration_ms, result);
```

### Using LiveGarden

```rust
// Get tended stone
let tended = garden.tended().expect("No tended stone");

// Get specific stone by name
let stone = garden.stone("stone-crystal-forest").expect("Not found");

// Iterate all stones
for stone in &garden.stones {
    let resp = stone.get_json("/health").await?;
}

// Get stones OTHER than tended (for failover tests)
let alternatives = garden.other_stones();

// Discovery info
println!("Found {} stones in {}ms", 
    garden.discovery.responses,
    garden.discovery.duration_ms);
```

### Stone Methods

```rust
// HTTP requests
let health: Value = stone.get_json("/health").await?;
let caps: Capabilities = stone.get("/capabilities").await?;
let result: Value = stone.post_json("/api/v1/offerings", &body).await?;
let resp: Value = stone.delete_json("/api/v1/offerings/redis").await?;

// Wait for condition
stone.wait_offering_state("redis", "running", Duration::from_secs(30)).await?;

// Health check
if stone.is_healthy().await {
    // ...
}
```

### Registering Tests

Add your test to `src/probe/src/registry.rs`:

```rust
fn register_all(&mut self) {
    // ... existing tests ...
    
    // Your new test
    self.register(crate::tests::mymodule::my_test());
}
```

And export from `src/probe/src/tests/mod.rs`:

```rust
pub mod mymodule;
```

## What to Test For

### 1. API Contract Verification
Test that endpoints return expected fields and status codes:

```rust
// Verify response structure
let resp = stone.get_json("/health").await?;
let status = resp.get("status").and_then(|s| s.as_str());
assert!(status == Some("healthy"), "Unexpected status: {:?}", status);
```

### 2. Cross-Stone Consistency
Verify all stones agree on shared state:

```rust
let expected: HashSet<String> = garden.stone_names().iter().map(|s| s.to_string()).collect();

for stone in &garden.stones {
    let resp = stone.get_json("/api/v1/garden").await?;
    let visible: HashSet<String> = /* parse stones from response */;
    
    let missing: Vec<_> = expected.difference(&visible).collect();
    if !missing.is_empty() {
        return StepResult::failed(format!("{} missing: {:?}", stone.name, missing));
    }
}
```

### 3. State Propagation (Chirps)
Deploy something, verify others see it:

```rust
// Deploy offering on tended stone
let tended = garden.tended().unwrap();
tended.post_json("/api/v1/offerings", &json!({"offering": "redis"})).await?;

// Wait for it to be running
tended.wait_offering_state("redis", "running", Duration::from_secs(60)).await?;

// Verify other stones see it in topology
tokio::time::sleep(Duration::from_secs(5)).await;  // Wait for chirp propagation

for other in garden.other_stones() {
    let resp = other.get_json("/api/v1/garden/topology").await?;
    // Assert redis appears in tended stone's services
}
```

### 4. Failover Behavior
Test what happens when stones go offline:

```rust
// This is observational - check alternatives exist
let alternatives = garden.other_stones();
if alternatives.is_empty() {
    return StepResult::skipped("Need 2+ stones for failover test");
}

// Verify alternatives are reachable
for alt in alternatives {
    let healthy = alt.is_healthy().await;
    bag.put(format!("{}_reachable", alt.name), healthy);
}
```

### 5. Performance/Latency
Measure response times across stones:

```rust
let mut latencies = Vec::new();
for _ in 0..10 {
    let start = Instant::now();
    stone.get_json("/health").await?;
    latencies.push(start.elapsed().as_millis() as u64);
}

let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
let max = *latencies.iter().max().unwrap();

if max > 1000 {
    return StepResult::failed(format!("Max latency {}ms exceeds 1s threshold", max));
}
```

### 6. Edge Cases

- **Empty states**: What if no offerings installed?
- **Single stone**: Does everything work with just one?
- **Name collisions**: What if two offerings have similar names?
- **Concurrent access**: Multiple queries in parallel

## Test Naming Convention

```
{category}.{subject}[_{aspect}]

Examples:
  smoke.health              - Basic health check
  discovery.topology        - Topology visibility
  tend.reachable           - Tended stone reachable
  tend.switch_simulation   - Simulate switching stones
  interstone.cross_query   - Query between stones
  offerings.deploy_redis   - Deploy specific offering
```

## Running from CI

```bash
# In CI pipeline with known stone
garden-probe -e http://stone-01:7185 run --all

# Or with UDP discovery (requires network access)
garden-probe --udp run --all --timeout 10

# Exit code: 0 = all pass, 1 = failures
```

## Example: Complete Test

```rust
//! tests/offerings.rs

use crate::registry::TestDef;
use crate::{Bag, LiveGarden, StepResult};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

pub fn offerings_catalog_test() -> TestDef {
    TestDef {
        id: "offerings.catalog",
        name: "Offerings Catalog",
        description: "Verify all stones have consistent offering catalogs",
        category: "offerings",
        tags: &["offerings", "consistency"],
        run: |garden, bag| Box::pin(test_offerings_catalog(garden, bag)),
    }
}

async fn test_offerings_catalog(garden: Arc<LiveGarden>, mut bag: Bag) -> Result<Bag> {
    let mut catalogs: std::collections::HashMap<String, Vec<String>> = 
        std::collections::HashMap::new();
    
    for stone in &garden.stones {
        let start = Instant::now();
        
        match stone.get_json("/api/v1/offerings").await {
            Ok(resp) => {
                let offerings: Vec<String> = resp
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                
                catalogs.insert(stone.name.clone(), offerings.clone());
                
                bag.record_step(
                    format!("catalog_{}", stone.name),
                    format!("{} has {} offerings", stone.name, offerings.len()),
                    start.elapsed().as_millis() as u64,
                    StepResult::ok_with(serde_json::json!({
                        "count": offerings.len(),
                    })),
                );
            }
            Err(e) => {
                bag.record_step(
                    format!("catalog_{}", stone.name),
                    format!("{} catalog fetch failed", stone.name),
                    start.elapsed().as_millis() as u64,
                    StepResult::failed(e.to_string()),
                );
            }
        }
    }
    
    // Verify all have same count (catalogs should be identical)
    let counts: Vec<usize> = catalogs.values().map(|v| v.len()).collect();
    let all_same = counts.windows(2).all(|w| w[0] == w[1]);
    
    if all_same && !counts.is_empty() {
        bag.record_step(
            "catalog_consistency",
            format!("All {} stones have {} offerings", catalogs.len(), counts[0]),
            0,
            StepResult::ok(),
        );
    } else {
        bag.record_step(
            "catalog_consistency",
            "Catalog counts differ",
            0,
            StepResult::failed(format!("Counts: {:?}", counts)),
        );
    }
    
    Ok(bag)
}
```

## Bugs This Framework Has Caught

1. **`/api/v1/garden` only returned local stone** - Topology cache wasn't being used
2. **Health status `"healthy"` vs `"ok"`** - Tests expected wrong value
3. **Stones not seeing each other** - Would catch topology propagation failures

## Philosophy

> Tests should verify **real behavior against real stones**.
> Unit tests verify code works in isolation.
> Integration tests verify the **system** works.

The goal is to catch bugs like:
- API contracts changing unexpectedly
- Stones not communicating properly
- Race conditions in topology updates
- Performance regressions
- Configuration drift between stones
