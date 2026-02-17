# Compatibility Evaluation Specification

**Purpose:** How Zen Garden evaluates offering compatibility against Stone hardware at runtime.  
**Audience:** Developers working on fitness scoring, placement, offerings, or orchestration.

---

## Table of Contents

1. [Overview](#overview)
2. [Pipeline](#pipeline)
3. [Compatibility Rules (Manifest)](#compatibility-rules-manifest)
4. [Capability Detection](#capability-detection)
5. [Evaluation Logic](#evaluation-logic)
6. [Compiled Compatibility](#compiled-compatibility)
7. [Scoring Integration](#scoring-integration)
8. [Consumers](#consumers)
9. [Crate Map](#crate-map)

---

## Overview

Every Stone has different hardware — CPU architecture, instruction sets, GPU runtimes, memory, storage. Not every offering runs equally well on every Stone. A vector database that needs AVX2 won't run on older CPUs. An LLM inference server is useless without a GPU.

Zen Garden solves this with a **per-Stone compatibility evaluation** pipeline:

```
Manifest rules  ×  Stone capabilities  →  Decision  →  Compiled result
```

This evaluation runs **once per offering per Stone** when the offerings index is built (on startup and on manifest/capability changes). The result — a `CompiledCompatibility` struct — is cached and reused by every consumer: the offerings API, placement recommendations, and fitness scoring for orchestration elections.

**Design principle:** Manifests *declare* requirements. Stones *detect* capabilities. The evaluation *matches* them. No consumer reinvents constraint checking.

---

## Pipeline

```
┌──────────────────────┐      ┌─────────────────────────┐
│  CompatibilityRules  │      │  CompatCheckCapabilities │
│  (from manifest)     │      │  (from Stone hardware)   │
└──────────┬───────────┘      └────────────┬─────────────┘
           │                               │
           ▼                               ▼
      ┌────────────────────────────────────────┐
      │         evaluate_compatibility()       │
      │   First matching rule wins (AND logic) │
      └────────────────────┬───────────────────┘
                           │
                           ▼
                 ┌─────────────────────┐
                 │ CompatibilityDecision│
                 │ Pass│Warning│Fallback│Fail│
                 └──────────┬──────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ compile_compatibility()│──▶ mutates template image
                 │                      │    (on Fallback)
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ CompiledCompatibility │
                 │ (serialised, cached)  │
                 └──────────────────────┘
```

**Trigger points:**

- Moss startup (offerings index build)
- `garden-rake offer refresh` command
- Hardware capability detection completes (index invalidated by fingerprint change)

---

## Compatibility Rules (Manifest)

Each offering may include a `.compatibility.yaml` file alongside its manifest. This file is loaded into the `CompatibilityRules` struct (defined in `garden_common::types`).

### Schema

```yaml
version: "1.0"
compatibility_rules:
  - name: "no-avx-fallback"
    condition:
      cpu_features_missing: ["avx"]
    reason: "AVX instructions not available"
    suggestion: "Consider a Stone with a newer CPU"
    fallback:
      image: "mongo:4.4"

  - name: "arm64-not-supported"
    condition:
      architectures: ["aarch64"]
    reason: "No ARM64 image available"
    suggestion: null

  - name: "low-vram-warning"
    condition:
      vram_mb_less_than: 4096
    reason: "Less than 4GB VRAM — inference will be slow"
    warn_only: true

post_install_healthcheck:
  command: "curl -sf http://localhost:27017"
  timeout_seconds: 30
```

### Condition Fields

All fields within a single condition use AND logic — every specified field must match for the rule to fire.

| Field | Type | Semantics |
|---|---|---|
| `processor_models` | `Vec<String>` | Exact CPU model match (any-of) |
| `processor_patterns` | `Vec<String>` | Substring match against CPU model (any-of) |
| `cpu_features_missing` | `Vec<String>` | Fire if any listed feature is absent |
| `architectures` | `Vec<String>` | Fire if Stone arch is in this list |
| `memory_mb_less_than` | `u64` | Fire if total memory below threshold |
| `os_family` | `Vec<String>` | Fire if OS matches (e.g., `["linux"]`) |
| `os_family_not` | `Vec<String>` | Fire if OS is NOT in list |
| `requires_ai_any` | `Vec<String>` | Fire if ANY listed runtime present (OR) |
| `requires_ai_all` | `Vec<String>` | Fire if ALL listed runtimes present (AND) |
| `vram_mb_less_than` | `u64` | Fire if total VRAM below threshold |
| `vram_mb_at_least` | `u64` | Fire if total VRAM at or above threshold |

### Rule Outcomes

Each rule produces one of three outcomes:

| Outcome | When | Effect |
|---|---|---|
| **Fail** | Default (no `fallback`, no `warn_only`) | Stone cannot host this offering |
| **Fallback** | Rule has `fallback.image` | Offering runs with alternative image |
| **Warning** | Rule has `warn_only: true` | Offering runs but with advisory notice |

If no rule matches, the decision is **Pass**.

---

## Capability Detection

Stone capabilities are gathered by `get_current_compat_capabilities()` into a `CompatCheckCapabilities` struct.

### Detection Paths

1. **Fast path (cached):** If background hardware detection has completed (`DetectionStatus::Complete`), capabilities are built from cached `HardwareCapabilities` — no subprocess calls.

2. **Slow path (live):** Falls back to live detection: shells out to system tools (`lscpu`, `nvidia-smi`, `docker images`, etc.). Used only when cache is unavailable.

### Capability Fields

```rust
pub struct CompatCheckCapabilities {
    pub cpu_model: Option<String>,
    pub cpu_features: Option<Vec<String>>,
    pub architecture: Option<String>,
    pub total_memory_mb: Option<u64>,
    pub os_family: String,           // "linux" | "windows" | "macos"
    pub has_cuda: bool,
    pub has_rocm: bool,
    pub has_directml: bool,
    pub has_openvino: bool,
    pub gpu_vram_total_mb: u64,
}
```

GPU/AI runtime detection checks each GPU's `ai_runtimes` field for runtime names like `cuda`, `rocm`, `directml`, `openvino`. Supports both exact matches and versioned prefixes (`cuda:12.4`).

---

## Evaluation Logic

`evaluate_compatibility(rules, capabilities) → CompatibilityDecision`

**Algorithm:** First matching rule wins.

For each rule in declaration order:
1. Evaluate every condition field against capabilities (AND logic within a rule)
2. If all specified fields match → rule fires:
   - Has `fallback`? → `Fallback { image, reason }`
   - Has `warn_only: true`? → `Warning { reason, suggestion }`
   - Otherwise → `Fail { reason, suggestion }`
3. If no rule matches → `Pass`

**Key behaviours:**

- Undetected capabilities (e.g., `cpu_features: None`) do not trigger `cpu_features_missing` — the system doesn't assume features are absent when it can't confirm.
- Rule order matters — put more specific rules before general ones.
- Empty `compatibility_rules` array → always `Pass`.
- No `.compatibility.yaml` file → always `Pass`.

---

## Compiled Compatibility

`compile_compatibility()` wraps evaluation and produces a serialisable result stored on each `CompiledOffering` in the offerings index.

```rust
pub struct CompiledCompatibility {
    pub decision: String,          // "pass" | "fallback" | "warning" | "fail"
    pub reason: Option<String>,
    pub original_image: Option<String>,   // set on Fallback and Fail
    pub fallback_image: Option<String>,   // set on Fallback only
    pub suggestion: Option<String>,
}
```

**Side effect:** On `Fallback`, the template's `image` field is mutated to the fallback image before the `CompiledOffering` is built. This means every downstream consumer (installation, API response) automatically uses the correct image for the Stone's hardware.

### Storage

The `CompiledCompatibility` lives on `CompiledOffering.compatibility` inside the `OfferingsIndexCache`. This cache is:

- Built in memory on Moss startup
- Invalidated when the fingerprint changes (Moss version, capabilities hash, or templates hash)
- Accessible via `AppState.offerings_index` (shared `RwLock`)
- Served by the `GET /api/v1/offerings` endpoint

---

## Scoring Integration

The compatibility decision feeds into two scoring systems:

### Placement Scoring

`calculate_compatibility_penalty()` converts decisions to numeric placement penalties, composable with other scoring factors (memory, CPU, storage, distribution):

| Decision | Penalty | Effect |
|---|---|---|
| Pass | 0 | No impact |
| Fallback | -15 | Minor penalty (emulation/older version) |
| Warning | -50 | Significant penalty but still viable |
| Fail | -999 | Effectively filtered out |

These penalties compose with resource scores to produce per-Stone placement recommendations via `recommend_placement()`.

### Fitness Scoring (ORCH-0001)

Orchestration elections use `compute_fitness_score()` to determine which Stone should be primary for a replicated offering. This function:

1. Checks pinned status → `1001` (unconditional win)
2. Checks compiled compatibility → `Fail` means **ineligible** (returns `None`, Stone doesn't respond to election)
3. Applies compatibility penalty via `calculate_compatibility_penalty()`
4. Adds resource scores (memory headroom, CPU availability, storage capacity/type)
5. Adds health bonus and distribution penalty
6. Scales and clamps to `[-1000, 1000]`

**Key insight:** Fitness scoring does not re-evaluate compatibility rules. It reuses the `CompiledCompatibility` already stored on the offerings index — the same evaluation that the API and placement system use. One evaluation, many consumers.

---

## Consumers

| Consumer | What it reads | Purpose |
|---|---|---|
| **Offerings API** | `CompiledOffering.compatibility` | Exposes decision to CLI and UI |
| **Placement** | `CompiledCompatibility` from local + remote indices | Ranks Stones for cross-Stone recommendations |
| **Fitness (ORCH-0001)** | `CompiledCompatibility` from local index | Election eligibility + score penalty |
| **Installation** | `CompiledOffering.image` (post-fallback) | Uses correct image for Stone hardware |

---

## Crate Map

| Location | Contains |
|---|---|
| `garden_common::types::CompatibilityRules` | Manifest schema (rules + conditions) |
| `garden_common::types::RuleCondition` | Per-rule condition fields |
| `moss::domain::compatibility` | `evaluate_compatibility()`, `compile_compatibility()`, `get_current_compat_capabilities()` |
| `moss::domain::compatibility::CompatCheckCapabilities` | Runtime Stone capabilities |
| `moss::domain::compatibility::CompatibilityDecision` | Domain enum (Pass/Warning/Fallback/Fail) |
| `moss::domain::compatibility::CompiledCompatibility` | Serialised result struct |
| `moss::domain::scoring` | `calculate_compatibility_penalty()` + resource scoring functions |
| `moss::domain::offerings` | `rebuild_offerings_index()` — builds `CompiledOffering` with compatibility |
| `moss::domain::placement` | `recommend_placement()` — cross-Stone placement with compatibility |
| `moss::domain::fitness` | `compute_fitness_score()` — orchestration election scoring |
