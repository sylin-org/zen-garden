---
audience: developer
doc_type: decision
status: accepted
---

# COMPAT-0003: Compatibility Enforcement & Validation Hardening

**Date**: 2026-05-29
**Status**: Accepted
**Builds on**: COMPAT-0002 (predicate DSL)

---

## Problem

The compatibility system reliably evaluates pre-flight `when:` rules at plant
time, but four gaps let manifests promise behavior that never happens or hide
authoring errors until runtime:

1. **`post_install_healthcheck` is dead config.** The block is declared in
   Pi-hole, SearXNG, MongoDB, and SQL Server manifests, parsed into
   `CompatibilityRules.post_install_healthcheck`, and embedded — but a grep of
   the whole tree finds **zero runtime consumers**. MongoDB's manifest promises
   "`Illegal instruction` → fall back to `mongo:4.2`" and it silently never
   fires. `compile_compatibility` even drops the field when building
   `CompiledOffering`, so the data is discarded before deploy.

2. **`manifest validate` never checks `when:` predicates.**
   `validate_compatibility` (`src/common/src/manifests/validation.rs:347`) only
   parses YAML and checks a `version` key. A typo like `host.architcture` or a
   type-mismatched operator parses as valid YAML, passes validation, and is then
   silently skipped at runtime (`evaluate_compatibility` logs and drops
   unparseable rules). The exact error class COMPAT-0002's parser was built to
   catch is invisible to the authoring CLI.

3. **`COMPAT002` is a false positive.** It warns when a compatibility file omits
   a top-level `version`, but `CompatibilityRules` (`types/compatibility.rs:16`)
   has **no `version` field** — only the unrelated hw/ recommendations schema
   uses one. Every `sw/*` compatibility file trips this warning for a key the
   type does not define.

4. **Frontmatter hints are silently dropped.** `minimum_memory_gb` and
   `gpu_recommended` appear in real frontmatter files but are not fields on
   `FrontmatterFile` — serde discards them. An author who writes
   `"minimum_memory_gb": 2` gets no RAM gate. Validation also never cross-checks
   `category` against the real category set, never compares frontmatter `port`
   to the snippet's `ports.default`, and never flags unknown frontmatter keys —
   so typos in any of these pass silently.

## Decision

### 1. Enforce `post_install_healthcheck` at deploy time

After `install_service` returns Ok in `install_service_task`
(`src/moss/src/tasks/job_executors.rs`, between the deploy at ~745 and the
registry-Running update at ~787), run a bounded post-install log scan:

- Add `ContainerRuntime::read_recent_logs(name, lines) -> Result<String>`
  (`docker/exec.rs`), mirroring `get_logs_stream` but with
  `LogsOptions { follow: false, tail: Some("<N>"), .. }`.
- Reach the healthcheck from the manifest (it is not on `CompiledOffering`):
  `get_manifest(offering).parse_template().compatibility.post_install_healthcheck`,
  gated on `enabled`.
- Scan via a new **pure** helper
  `scan_healthcheck(logs: &str, hc: &PostInstallHealthcheck) -> Option<&HealthcheckPattern>`
  in `domain/compatibility.rs` (unit-testable like `evaluate_compatibility`).
  Regexes from manifest YAML are compiled defensively (`Regex::new` handled, never
  `.unwrap()`); first match wins.
- On a match carrying a `fallback`: swap the image (clear device requests, per
  OFFER-0008) and `recreate_service` (preserves volumes — **not**
  `upgrade_service`, which deletes them). Swap **at most once** (guard against a
  fallback image that also matches a crash pattern). On a match without a
  fallback: emit a warning and continue.
- The scan is **best-effort**: any log-read or regex error logs a warning and
  leaves the healthy deploy standing. `OfferingEvent::deployed` is emitted once,
  after the final image is live.

### 2. Validate `when:` predicates in `manifest validate`

`validate_compatibility` walks `compatibility_rules[].when[]` and calls the
in-crate `garden_common::compatibility::Predicate::parse` on each expression. A
parse error becomes a `COMPAT003` **Error** finding (rule name + the
`PredicateError` Display, which includes position). Walking the untyped
`serde_yml::Value` (not deserializing into `CompatibilityRules`) keeps one bad
rule from aborting the whole check and naturally no-ops on the hw/ schema (no
`compatibility_rules` key).

`Predicate` is a sibling module in the same `garden-common` crate, so this is an
intra-crate reference — not a new external dependency. The module doc note is
reworded accordingly.

### 3. Drop `COMPAT002`

Remove the `version`-missing warning and its tests. `CompatibilityRules` has no
`version` field, so the warning is noise. The freed code slot is taken by the
`COMPAT003` predicate findings.

### 4. Frontmatter hardening

In `validate_frontmatter` and a new cross-file step in `validate_manifest_dir`:

- **`FM005` (Warning)** — `category` not in the category registry
  (`category::get_category_registry().category_names()`, alias-aware via
  `token_matches`). Skipped when the registry is empty (test/minimal contexts).
- **`FM006` (Warning)** — frontmatter `port` ≠ the host element of the snippet's
  `ports.default` (reuses `extract_port_pair`). Cross-file, so it runs in the
  directory aggregator and in the test endpoint where both files are in hand.
- **`FM007` (Warning)** — unknown top-level frontmatter key (allowlist: `name`,
  `description`, `category`, `tags`, `port`, `modes`, `volumes`,
  `gpu_recommended`, `minimum_memory_gb`, `connection`, `manageable_env`,
  `homepage`, `documentation`, `icon`, `coordination`, `ceremony`).

### 5. Honor `minimum_memory_gb`

Add `minimum_memory_gb: Option<u32>` to `FrontmatterFile`. At offering-load
time, when present, synthesize a `warn_only` compatibility rule
`host.ram.total.mb < minimum_memory_gb * 1024` and append it to
`Offering.compatibility` (it must be appended, not prepended — first-match-wins,
and a hand-authored deny should still take precedence). Warn rather than fail so
the hint is advisory; authors who want a hard floor write an explicit rule.

## Implementation Requirements

- `regex` must be a `moss` dependency (verify/add in `Cargo.toml`).
- `scan_healthcheck` and the `minimum_memory_gb` rule synthesis are pure and
  unit-tested. New validation findings get tests; the dropped `COMPAT002` tests
  are removed.
- The test endpoint `test_manifest_v1` (`api/v1/offerings.rs:683`) also calls
  `validate_compatibility` when `compatibility_yaml` is present, and surfaces
  warnings/info (today it drops everything but errors).
- The empty `host.ram.total.mb` fact resolves to 0 before hardware detection;
  the synthesized rule is `warn_only`, so a pre-detection false fire is a warning,
  not a block.

## Consequences

- A manifest's `post_install_healthcheck` finally does something: runtime crash
  patterns trigger the declared image fallback, closing the gap where MongoDB's
  AVX/SIGILL fallbacks were inert.
- Predicate typos and type mismatches fail at `manifest validate` with positioned
  errors instead of silently dropping a rule in production.
- The `COMPAT002` false positive stops firing on every offering.
- `minimum_memory_gb` becomes an enforced (advisory) RAM gate; category, port, and
  unknown-key typos surface as warnings at authoring time.
- Validation gains a single intra-crate dependency on the predicate parser; the
  pure-function character of `validation.rs` is preserved.
