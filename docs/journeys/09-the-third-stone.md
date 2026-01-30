# The Third Stone

*Your garden grows. Where should things go?*

---

## The Story

Your garden has two Stones: stone-amber-ridge (a laptop) and stone-coral-reef (a thin client). They've been humming along for months. Now you've acquired something new: a small server with 32GB of RAM and an NVMe drive.

You prepare a USB installer, boot the server, and let it become a Stone. When it finishes:

```bash
garden-rake observe
```

```
Discovering garden...

●  stone-amber-ridge (192.168.1.42)
   Moss 0.2.1 • Up 67d

   OFFERINGS:
   ├─ mongodb     Running   Healthy   27017
   └─ redis       Running   Healthy   6379

●  stone-coral-reef (192.168.1.58)
   Moss 0.2.1 • Up 45d

   OFFERINGS:
   └─ postgres    Running   Healthy   5432

●  stone-bronze-canyon (192.168.1.73)
   Moss 0.2.1 • Up 2m

   OFFERINGS:
   (none)
```

Three Stones. The new one—stone-bronze-canyon—joined automatically. You didn't configure anything. It just... appeared.

---

You want to add Elasticsearch. It needs RAM. Where should it go?

```bash
garden-rake offer elasticsearch
```

```
Planning placement for elasticsearch...

Evaluating 3 stones:

  stone-bronze-canyon (192.168.1.73)           Score: 89
    Memory:  28 GB free / 32 GB               ████████████████████ 18 pts
    CPU:     3% load                          ████████████████████ 19 pts
    Storage: 450 GB NVMe                      █████████████████ 27 pts
    Services: 0 running                       ████████████████████ 0 pts
    ⭐ Best available resources

  stone-amber-ridge (192.168.1.42)             Score: 52
    Memory:  4 GB free / 8 GB                 ██████████░░░░░░░░░░ 10 pts
    CPU:     25% load                         ███████████████░░░░░ 15 pts
    Storage: 180 GB SSD                       ██████████████░░░░░░ 22 pts
    Services: 2 running                       ████████████░░░░░░░░ -6 pts
    ↳ tended stone (+3 pts)

  stone-coral-reef (192.168.1.58)              Score: 31
    Memory:  2 GB free / 4 GB                 █████░░░░░░░░░░░░░░░ 5 pts
    CPU:     12% load                         █████████████████░░░ 17 pts
    Storage: 60 GB HDD                        █████░░░░░░░░░░░░░░░ 5 pts
    Services: 1 running                       █████████████████░░░ -3 pts

Recommended: stone-bronze-canyon

  [1] stone-bronze-canyon (recommended)
  [2] stone-amber-ridge
  [3] stone-coral-reef
  [Q] Cancel

Choice: 1
```

The garden evaluated all three Stones and recommended the new server. The scoring is transparent—you can see why each Stone ranked where it did.

You accept the recommendation:

```
Deploying elasticsearch to stone-bronze-canyon...

  Pulling image... done (elasticsearch:8.12.0)
  Creating container... done
  Starting container... done
  Waiting for health check... passed

✓ elasticsearch deployed to stone-bronze-canyon

  Endpoint: http://stone-bronze-canyon.local:9200
```

---

A week later, you want to add Grafana. This time, the recommendation is different:

```bash
garden-rake offer grafana
```

```
Planning placement for grafana...

Evaluating 3 stones:

  stone-amber-ridge (192.168.1.42)             Score: 58
    Memory:  3.5 GB free / 8 GB               ████████░░░░░░░░░░░░ 9 pts
    CPU:     28% load                         ██████████████░░░░░░ 14 pts
    Storage: 175 GB SSD                       ██████████████░░░░░░ 22 pts
    Services: 2 running                       ████████████░░░░░░░░ -6 pts
    ↳ tended stone (+3 pts)

  stone-bronze-canyon (192.168.1.73)           Score: 56
    Memory:  20 GB free / 32 GB               ████████████░░░░░░░░ 12 pts
    CPU:     35% load                         █████████████░░░░░░░ 13 pts
    Storage: 420 GB NVMe                      █████████████████ 27 pts
    Services: 1 running                       █████████████████░░░ -3 pts

  stone-coral-reef (192.168.1.58)              Score: 28
    ...

Recommended: stone-amber-ridge
```

Wait—the beefy server isn't recommended? Look at the scores: stone-bronze-canyon now has Elasticsearch consuming CPU and memory. Stone-amber-ridge, with fewer services, is slightly better balanced for a lightweight dashboard like Grafana.

The garden spreads workloads across Stones rather than piling everything on the biggest one.

---

Some offerings can't go everywhere. You try to deploy a GPU-accelerated transcoding service:

```bash
garden-rake offer jellyfin
```

```
Planning placement for jellyfin...

Evaluating 3 stones:

  ⚠ stone-amber-ridge: Incompatible
    Reason: Missing hardware acceleration (no NVIDIA/AMD GPU detected)

  ⚠ stone-coral-reef: Incompatible
    Reason: Missing hardware acceleration (no NVIDIA/AMD GPU detected)

  ⚠ stone-bronze-canyon: Incompatible
    Reason: Missing hardware acceleration (no NVIDIA/AMD GPU detected)

No compatible stones found for jellyfin.

Suggestions:
  • Add a stone with NVIDIA or AMD GPU
  • Use 'garden-rake offer jellyfin recklessly' to deploy without hardware acceleration
    (software transcoding will be significantly slower)
```

None of your Stones have a GPU. You have options: add a Stone with a GPU, or accept software transcoding.

---

## What Just Happened

### Automatic Discovery

When stone-bronze-canyon booted, it sent its first chirp:

```json
{
  "type": "stone_chirp",
  "data": {
    "stone_id": "019c3a2b-4d5e-7f89-a1b2-c3d4e5f67890",
    "stone_name": "stone-bronze-canyon",
    "endpoint": "http://192.168.1.73:7185",
    "services": [],
    "moss_version": "0.2.1"
  }
}
```

The other Stones heard it via UDP multicast. They updated their topology caches. Within 30 seconds, the entire garden knew about the new Stone.

No configuration. No IP addresses to enter. No cluster management. Just plug in and join.

### The Placement Algorithm

When you run `garden-rake offer`, the garden evaluates every Stone using a multi-factor scoring system:

```
┌─────────────────────────────────────────────────────────────────┐
│  PLACEMENT SCORING                                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Factor                    Range      Calculation               │
│  ─────────────────────────────────────────────────────────────  │
│  Memory headroom           0-20 pts   20 × (free_mb / total_mb) │
│  CPU availability          0-20 pts   20 - (load% / 5)          │
│  Storage capacity          0-15 pts   Tiered by GB available    │
│  Hardware quality          0-12 pts   NVMe: 12, SSD: 10, HDD: 5 │
│  Service distribution      -N pts     -3 per existing service   │
│  Tended bonus              +3 pts     If command runs locally   │
│                                                                 │
│  Incompatibility           -999 pts   Effectively filtered out  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

The **distribution penalty** is key. Each service already running on a Stone costs -3 points. This encourages spreading workloads:

- Stone with 0 services: no penalty
- Stone with 2 services: -6 points
- Stone with 5 services: -15 points (significant disadvantage)

This prevents the "biggest Stone gets everything" problem.

### Hardware Compatibility

Before scoring, the garden checks compatibility:

```rust
// Simplified compatibility check
fn check_compatibility(offering: &Offering, stone: &Stone) -> CompatibilityResult {
    // Architecture check
    if !offering.supports_architecture(&stone.architecture) {
        return CompatibilityResult::Fail("Incompatible architecture");
    }

    // CPU features
    for required in &offering.requirements.cpu_features {
        if !stone.capabilities.has_cpu_feature(required) {
            return CompatibilityResult::Fail(
                format!("Missing CPU feature: {}", required)
            );
        }
    }

    // Memory requirements
    if let Some(min_mem) = offering.requirements.min_memory_mb {
        if stone.memory_free_mb < min_mem {
            return CompatibilityResult::Warning("Insufficient memory");
        }
    }

    // GPU requirements
    if offering.requires_gpu() && !stone.has_gpu() {
        return CompatibilityResult::Fail("No GPU detected");
    }

    CompatibilityResult::Pass
}
```

Compatibility levels:
- **Pass**: Native support, no penalty
- **Fallback**: Emulation needed (e.g., ARM image on x86), -15 points
- **Warning**: Marginal resources, -50 points
- **Fail**: Can't run, filtered out entirely

### The Recommendation Flow

When you run `garden-rake offer elasticsearch`:

```
1. FETCH TOPOLOGY
   ├─ Query local Moss for cached topology
   ├─ If stale (>5 min), trigger UDP discovery refresh
   └─ Returns: 3 stones

2. FETCH METRICS (parallel)
   ├─ GET stone-amber-ridge:7185/metrics
   ├─ GET stone-coral-reef:7185/metrics
   └─ GET stone-bronze-canyon:7185/metrics
   (3-second timeout, failed fetches excluded)

3. EVALUATE COMPATIBILITY
   ├─ stone-amber-ridge: Check arch, CPU, memory → Pass
   ├─ stone-coral-reef: Check arch, CPU, memory → Pass
   └─ stone-bronze-canyon: Check arch, CPU, memory → Pass

4. SCORE EACH STONE
   ├─ Apply scoring formula
   ├─ Add distribution penalties
   └─ Add tended bonus if applicable

5. RANK AND PRESENT
   └─ Sort by score descending, present to user
```

All metrics are fetched in parallel. The entire evaluation completes in under a second.

### Explicit Placement

You can skip the recommendation and place directly:

```bash
# Place on a specific Stone
garden-rake offer redis on stone-bronze-canyon

# Place on any Stone (skip interactive menu)
garden-rake offer redis somewhere

# Place despite compatibility warnings
garden-rake offer jellyfin recklessly
```

The `recklessly` modifier bypasses hardware checks—useful when you know better than the system (or when you're willing to accept degraded performance).

---

## Growing the Garden

The third Stone isn't just more capacity. It changes how the garden thinks:

**With 2 Stones:**
- Limited redundancy
- Placement often obvious
- Resource constraints felt quickly

**With 3+ Stones:**
- Real distribution becomes possible
- Placement decisions matter more
- Garden can balance workloads intelligently

Three is the beginning of a garden. Before that, it's just a pair of servers.

---

## Commands From This Journey

```bash
# See all Stones and their offerings
garden-rake observe

# Deploy with placement recommendation
garden-rake offer elasticsearch

# Deploy to specific Stone
garden-rake offer redis on stone-bronze-canyon

# Deploy anywhere (automatic selection)
garden-rake offer redis somewhere

# Deploy despite compatibility warnings
garden-rake offer jellyfin recklessly

# Check Stone capabilities
garden-rake status stone-bronze-canyon --capabilities

# See placement scoring details
garden-rake offer grafana --verbose
```

---

*Zen Garden Documentation — Journeys*
