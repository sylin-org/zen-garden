---
audience: developer
doc_type: decision
status: accepted
---

# COMPAT-0002: Hardware Compatibility Predicate DSL

**Date**: 2026-03-31
**Status**: Accepted
**Supersedes**: COMPAT-0001 (rule format only; three-tier evaluation model preserved)

---

## Problem

The COMPAT-0001 compatibility system uses a flat YAML struct with 15+ optional fields
(`requires_ai_any`, `has_ai_runtime`, `ai_present_any`, `vram_mb_less_than`, etc.) to
express hardware conditions. This design has three structural flaws:

1. **No negation.** There is no way to express "stone LACKS cuda." Every condition is a
   positive assertion. The ComfyUI three-tier fallback (CUDA → ROCm → CPU) requires
   negative conditions to work correctly.

2. **Silent field drops.** Serde ignores unknown fields by default. A manifest author
   wrote `has_ai_any: ['rocm']` — a field that does not exist on `RuleCondition`. Serde
   silently discarded it. The remaining condition `requires_ai_any: ['cuda']` evaluated
   as "match if stone has CUDA," deploying the CUDA image on an AMD-only stone. The
   container crash-looped.

3. **Confusing vocabulary.** `requires_ai_any`, `ai_present_any`, and `has_ai_runtime`
   are three different fields that read almost identically but have different semantics.
   Manifest authors cannot reliably distinguish them without reading Rust source code.

An audit of all 40 compatibility manifests revealed two silent field-drop bugs already
in production (`has_ai_any` in ComfyUI, `cpu_features_missing_all` in Milvus), and
six condition fields that were never used by any manifest (`os_family`, `has_ai_runtime`,
`requires_ai_all`, `ai_present_any`, `processor_models`, `vram_mb_at_least`).

---

## Decision

Replace the flat condition struct with a **string-based predicate DSL** evaluated at
manifest parse time.

### Grammar

```
expression  = fact OPERATOR value_expr
value_expr  = value (('AND' | 'OR') value)*
            | '(' value (',' value)* ')'

OPERATOR    = HAS | LACKS | IS | IS NOT | IN | NOT IN
            | '>=' | '>' | '<' | '<='

fact        = dotted identifier (lowercase)
value       = lowercase identifier | number | 'present'
```

- **UPPERCASE** tokens are operators.
- **lowercase** tokens are facts and values.
- Items in a `when:` list are AND'd together.
- First matching rule wins.
- AND and OR cannot mix in a single value expression.
- Unknown facts and malformed predicates fail at parse time, not at evaluation time.

### Operators

| Operator | Fact type | Semantics | Example |
|----------|-----------|-----------|---------|
| `HAS` | set | Contains any listed value (OR) | `host.ai.runtime HAS cuda` |
| `HAS ... AND` | set | Contains all listed values | `host.ai.runtime HAS cuda AND rocm` |
| `LACKS` | set | Contains none of listed values | `host.ai.runtime LACKS cuda` |
| `IS` | scalar/bool | Exact equality | `host.architecture IS armv7l` |
| `IS NOT` | scalar/bool | Not equal | `host.architecture IS NOT armv7l` |
| `IN` | scalar | Value is one of listed | `host.architecture IN (armv7l,armv6l)` |
| `NOT IN` | scalar | Value is none of listed | `host.os.family NOT IN (linux,macos)` |
| `>=` `>` `<` `<=` | numeric | Numeric comparison | `host.ram.total.mb < 8192` |

#### Set operators

```
# Single value
host.ai.runtime HAS cuda
host.ai.runtime LACKS cuda

# Multiple values — OR (has at least one)
host.ai.runtime HAS cuda,rocm
host.ai.runtime HAS (cuda,rocm)
host.ai.runtime HAS cuda OR rocm

# Multiple values — AND (has all)
host.ai.runtime HAS cuda AND rocm

# None of listed
host.ai.runtime LACKS (cuda,rocm)
```

Comma inside parentheses is OR (list of alternatives). Parentheses serve as scope
isolation (PEMDAS principle) — they can group a value set, and in future extensions,
a sub-expression.

#### Scalar operators

```
host.architecture IS armv7l
host.architecture IS NOT armv7l
host.architecture IN (armv7l,armv6l)
host.os.family NOT IN (linux,macos)
```

#### Boolean presence

```
host.gpu IS present
host.gpu IS NOT present
host.npu IS present
```

#### Numeric operators

```
host.ram.total.mb < 8192
host.gpu.vram.total.mb >= 4096
```

### Fact Namespace

Facts use a dotted hierarchy rooted at `host`. The hierarchy is typed — each fact has
a declared type that determines which operators are valid against it.

```
host.
├── architecture                # scalar: x86_64, aarch64, armv7l, armv6l
├── os.
│   └── family                  # scalar: linux, windows, macos
│
├── cpu.
│   ├── model                   # scalar: "Intel Celeron J4105", etc.
│   ├── pattern                 # set: j4105, j3455, n4100, ... (substring match)
│   └── features                # set: avx, avx2, sse4_2, avx512, ...
│
├── ram.
│   └── total.mb                # numeric: total system RAM in MB
│
├── gpu                         # boolean: GPU hardware present
├── gpu.
│   ├── count                   # numeric: number of GPUs
│   └── vram.
│       ├── total.mb            # numeric: aggregate VRAM in MB
│       └── total.gb            # numeric: aggregate VRAM in GB
│
├── npu                         # boolean: NPU hardware present
│
└── ai.
    └── runtime                 # set: cuda, rocm, directml, openvino
```

#### Fact registry

| Fact | Type | Source | Description |
|------|------|--------|-------------|
| `host.architecture` | scalar | `uname -m` | CPU architecture |
| `host.os.family` | scalar | compile target | Operating system family |
| `host.cpu.model` | scalar | `/proc/cpuinfo` | Full CPU model string |
| `host.cpu.pattern` | set | derived from model | Substring-matchable CPU identifiers |
| `host.cpu.features` | set | `/proc/cpuinfo` flags | CPU feature flags |
| `host.ram.total.mb` | numeric | sysinfo | Total system RAM |
| `host.gpu` | boolean | device detection | Any GPU hardware present |
| `host.gpu.count` | numeric | device enumeration | Number of GPUs |
| `host.gpu.vram.total.mb` | numeric | GPU driver query | Aggregate VRAM across all GPUs |
| `host.gpu.vram.total.gb` | numeric | derived | Aggregate VRAM in GB |
| `host.npu` | boolean | device detection | NPU hardware present |
| `host.ai.runtime` | set | toolkit detection | Detected AI runtime toolkits |

The dotted namespace is extensible without schema changes. Future facts register in
the fact registry, not in the grammar:

| Reserved fact | Type | Purpose |
|---------------|------|---------|
| `host.gpu.vram.per_gpu.mb` | numeric | Largest single-GPU VRAM |
| `host.ram.available.mb` | numeric | Available (not just total) RAM |
| `host.disk.total.gb` | numeric | Root filesystem size |
| `host.disk.available.gb` | numeric | Root filesystem free space |
| `host.kernel.version` | scalar | Kernel version string |
| `host.ai.runtime` + semver | set+ver | Versioned runtime (cuda >= 12.0) |

#### Validated value catalog

Exhaustive inventory of all values observed across the 39 migrated software manifests.
The parser should accept these and flag anything outside this set as a warning (not
an error — new values are valid, but typos should be catchable).

| Fact | Known values | Used by |
|------|-------------|---------|
| `host.architecture` | `x86_64`, `aarch64`, `arm64`, `armv7l`, `armv6l` | 25 predicates across 22 offerings |
| `host.os.family` | `linux`, `macos`, `windows` | 1 predicate (pihole) |
| `host.cpu.pattern` | `j4105`, `j3455`, `j3160`, `j4005`, `j5005`, `n4100`, `n5000` | 5 predicates (mongodb, ollama, ollama-cpu, milvus, weaviate) |
| `host.cpu.features` | `avx`, `avx2`, `avx512`, `sse4_2` | 4 predicates (mongodb, milvus, weaviate) |
| `host.ai.runtime` | `cuda`, `rocm`, `metal`, `directml`, `openvino` | 10 predicates across 8 AI offerings |
| `host.gpu` | `present` (boolean) | 1 predicate (ollama-cpu) |
| `host.ram.total.mb` | 64–16384 range | 65 predicates across 28 offerings |
| `host.gpu.vram.total.mb` | 1024–4096 range | 5 predicates across 5 AI offerings |

**Operator usage across all 116 predicates:**

| Operator | Count | Typical pattern |
|----------|-------|-----------------|
| `<` | 70 | `host.ram.total.mb < N` (memory/VRAM thresholds) |
| `IN` | 13 | `host.architecture IN (armv7l,armv6l)` |
| `IS` | 10 | `host.architecture IS armv6l` |
| `LACKS` | 14 | `host.ai.runtime LACKS cuda` |
| `HAS` | 7 | `host.ai.runtime HAS rocm`, `host.cpu.pattern HAS ...` |
| `IS present` | 1 | `host.gpu IS present` |
| `NOT IN` | 1 | `host.os.family NOT IN (linux,macos)` |

#### Type enforcement

The parser validates operator-fact type compatibility at parse time:

| Fact type | Valid operators |
|-----------|---------------|
| set | `HAS`, `LACKS` |
| scalar | `IS`, `IS NOT`, `IN`, `NOT IN` |
| boolean | `IS present`, `IS NOT present` |
| numeric | `>=`, `>`, `<`, `<=` |

A type mismatch (e.g., `host.ram.total.mb HAS 4096`) is a parse error, not a
runtime evaluation that silently returns false.

### Manifest Format

```yaml
compatibility_rules:
  - name: no-nvidia-use-rocm
    when:
      - host.ai.runtime LACKS cuda
      - host.ai.runtime HAS rocm
    fallback:
      image: "yanwk/comfyui-boot:rocm"
      name: rocm
    reason: "No NVIDIA GPU. Using AMD ROCm image."

  - name: no-gpu-use-cpu
    when:
      - host.ai.runtime LACKS cuda
      - host.ai.runtime LACKS rocm
    fallback:
      image: "yanwk/comfyui-boot:cpu"
      name: cpu
    reason: "No GPU detected. CPU mode (very slow)."

  - name: low-vram
    when:
      - host.gpu.vram.total.mb < 4096
    warn_only: true
    continue: true
    reason: "Most SD models need 4GB+ VRAM"

  - name: arm-not-supported
    when:
      - host.architecture IN (armv7l,armv6l)
    reason: "ComfyUI requires amd64 hardware"

  - name: insufficient-memory
    when:
      - host.ram.total.mb < 8192
    reason: "ComfyUI requires 8GB+ RAM"
```

### Rule Evaluation

- `when` items are AND'd. All must match for the rule to fire.
- First matching rule wins and determines the outcome (fallback, warning, or fail).
- `warn_only: true` emits a warning instead of failing.
- `continue: true` on warning rules allows evaluation to proceed to subsequent rules
  after emitting the warning, so warnings do not short-circuit fallback evaluation.
- `when: always` matches unconditionally (catch-all / default rule).

### Post-Install Health Check Patterns

The `post_install_healthcheck` section is unchanged — it uses regex patterns against
container logs, not the predicate DSL. These are runtime checks that fire after
deployment, complementing the pre-install DSL evaluation.

---

## Migration Reference

Every condition field in the current manifests maps to the new DSL. This table covers
the complete set of conditions found across all 40 manifests:

| Current field | Used by | New DSL |
|---------------|---------|---------|
| `memory_mb_less_than: N` | 28 offerings | `host.ram.total.mb < N` |
| `architectures: [a,b]` | 22 offerings | `host.architecture IN (a,b)` |
| `requires_ai_any: [x]` | 7 offerings | `host.ai.runtime HAS x` |
| `requires_ai_any: [x,y]` | 1 offering (ollama) | `host.ai.runtime HAS x,y` |
| `vram_mb_less_than: N` | 7 offerings | `host.gpu.vram.total.mb < N` |
| `processor_patterns: [p1,p2]` | 6 offerings | `host.cpu.pattern HAS p1,p2` |
| `cpu_features_missing: [f]` | 4 offerings | `host.cpu.features LACKS f` |
| `has_gpu: true` | 1 offering (ollama-cpu) | `host.gpu IS present` |
| `has_gpu: false` | — | `host.gpu IS NOT present` |
| `os_family_not: [a,b]` | 1 offering (pihole) | `host.os.family NOT IN (a,b)` |
| `os_family: [a]` | — (never used) | `host.os.family IS a` |
| `has_ai_runtime: true` | — (never used) | `host.ai.runtime HAS cuda,rocm,directml,openvino` |
| `vram_mb_at_least: N` | — (never used) | `host.gpu.vram.total.mb >= N` |
| **`has_ai_any`** (bug) | comfyui | `host.ai.runtime HAS rocm` |
| **`cpu_features_missing_all`** (bug) | milvus | `host.cpu.features LACKS sse4_2,avx,avx2,avx512` |

---

## Implementation Requirements

### Parser

Use `nom` or `winnow` for parsing. No hand-rolled regex. The parser must produce
clear error messages:

```
Error in rule 'no-nvidia-use-rocm', predicate 1:
  host.ai.runtime HAZ cuda
                   ^^^ Unknown operator 'HAZ'. Valid: HAS, LACKS, IS, IS NOT, IN, NOT IN, >=, <, <=, >
```

```
Error in rule 'low-vram', predicate 1:
  host.ram.total.mb HAS 4096
                    ^^^ Type mismatch: 'host.ram.total.mb' is numeric, but HAS requires a set fact.
                        Did you mean: host.ram.total.mb >= 4096
```

### Build-Time Validation

A `cargo test` must parse every embedded `.compatibility.yaml` file through the DSL
parser and fail the build on:
- Unknown facts (not in the fact registry)
- Malformed predicates (syntax errors)
- Type mismatches (numeric operator on a set fact, etc.)
- Mixed AND/OR in a single value expression

### Case Sensitivity

- Operators: case-insensitive (parsed as uppercase)
- Fact names: case-insensitive (parsed as lowercase)
- Values: case-sensitive (`armv7l` and `ARMV7L` are different)

### Reserved Syntax

The grammar reserves space for future extensions without breaking changes:
- **Semver comparison**: `host.ai.runtime HAS cuda >= 12.0` (requires a semver parser)
- **Sub-expressions in parens**: `(host.gpu IS present AND host.gpu.vram.total.mb >= 4096)`
- **Per-GPU facts**: `host.gpu.vram.per_gpu.mb >= 8192`

These are not implemented in the initial release.

---

## Migration

This is a break-and-rebuild. All 40 existing `.compatibility.yaml` manifests are
rewritten to the new DSL format. The old `RuleCondition` struct and
`evaluate_compatibility()` function are removed entirely. No backward compatibility
shim.

---

## Consequences

- Negation is a first-class operation. The ComfyUI CUDA-on-AMD bug class is eliminated.
- Unknown facts and typos fail at parse time with clear errors, not silently at runtime.
- The confusing `requires_ai_any` / `ai_present_any` / `has_ai_runtime` vocabulary is
  replaced by a single `host.ai.runtime` fact with `HAS` / `LACKS` operators.
- Warning rules with `continue: true` no longer short-circuit fallback evaluation.
- The dotted fact namespace is extensible without schema or struct changes.
- Manifest authors can read and write rules without consulting Rust source code.
- Build-time validation catches errors at CI, not in production.
- Type enforcement prevents operator/fact mismatches at parse time.
- `NOT IN` enables clean negated set-membership for scalar facts (pihole's
  "not linux, not macos" becomes `host.os.family NOT IN (linux,macos)`).
