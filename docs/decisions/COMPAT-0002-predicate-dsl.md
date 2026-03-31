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

---

## Decision

Replace the flat condition struct with a **string-based predicate DSL** evaluated at
manifest parse time.

### Grammar

```
expression  = fact OPERATOR value_expr
value_expr  = value (('AND' | 'OR') value)*
            | '(' value (',' value)* ')'

OPERATOR    = HAS | LACKS | IS | IS NOT | IN
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

```
# Set membership
host.ai.runtime HAS cuda
host.ai.runtime HAS cuda,rocm                  # any of (OR)
host.ai.runtime HAS (cuda,rocm)                # any of (OR) — parens for grouping
host.ai.runtime HAS cuda OR rocm               # any of (OR) — keyword form
host.ai.runtime HAS cuda AND rocm              # all of (AND)
host.ai.runtime LACKS cuda
host.ai.runtime LACKS (cuda,rocm)              # none of listed

# Scalar equality
host.architecture IS armv7l
host.architecture IS NOT armv7l
host.architecture IN (armv7l,armv6l)

# Boolean presence
host.gpu IS present
host.gpu IS NOT present
host.npu IS present

# Numeric comparison
host.vram.total.mb >= 4096
host.ram.total.mb < 8192
```

Comma inside parentheses is OR (list of alternatives). Parentheses serve as scope
isolation (PEMDAS principle) — they can group a value set, and in future extensions,
a sub-expression.

### Fact Namespace

Facts use a dotted hierarchy rooted at `host`:

| Fact | Type | Description |
|------|------|-------------|
| `host.ai.runtime` | set | Detected AI runtimes: cuda, rocm, directml, openvino |
| `host.cpu.features` | set | CPU feature flags: avx, avx2, sse4_2, ... |
| `host.cpu.model` | scalar | CPU model string |
| `host.architecture` | scalar | Architecture: x86_64, aarch64, armv7l, ... |
| `host.os.family` | scalar | OS family: linux, windows |
| `host.gpu` | boolean | GPU hardware present |
| `host.gpu.count` | numeric | Number of GPUs |
| `host.npu` | boolean | NPU hardware present |
| `host.vram.total.mb` | numeric | Total GPU VRAM (aggregate across GPUs) |
| `host.vram.total.gb` | numeric | Total GPU VRAM in GB |
| `host.ram.total.mb` | numeric | Total system RAM |
| `host.ram.total.gb` | numeric | Total system RAM in GB |

The dotted namespace is extensible without schema changes. Future facts
(`host.vram.used.mb`, `host.vram.per_gpu.mb`, `host.disk.total.gb`,
`host.kernel.version`) register in the fact registry, not in the grammar.

### Manifest Format

```yaml
version: "1"

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
      - host.vram.total.mb < 4096
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

## Implementation Requirements

### Parser

Use `nom` or `winnow` for parsing. No hand-rolled regex. The parser must produce
clear error messages:

```
Error in rule 'no-nvidia-use-rocm', predicate 1:
  host.ai.runtime HAZ cuda
                   ^^^ Unknown operator 'HAZ'. Valid: HAS, LACKS, IS, IS NOT, IN, >=, <, <=, >
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
- **Sub-expressions in parens**: `(host.gpu IS present AND host.vram.total.mb >= 4096)`
- **Per-GPU facts**: `host.vram.per_gpu.mb >= 8192`

These are not implemented in the initial release.

---

## Migration

This is a break-and-rebuild. All existing `.compatibility.yaml` manifests are rewritten
to the new DSL format. The old `RuleCondition` struct and `evaluate_compatibility()`
function are removed entirely. No backward compatibility shim.

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
