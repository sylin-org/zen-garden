---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-25
canonical: true
---

# ARCH-0013: Capability Gap Closure — Audit, Plan, and Incremental Delivery

**Date**: 2026-03-25
**Status**: Accepted
**Depends on**: ARCH-0005 (Structural Quality Pass), ARCH-0007 (Modernization), ARCH-0012 (Typed StoneApi)

## Context

A full-project audit compared every planned capability (89 ADRs, 67 proposals, roadmap
phases) against the implemented codebase (264 Rust source files across 8 crates). The
audit validated implementation status for each feature and identified three categories
of gap:

1. **Documentation drift** — docs that describe APIs, commands, or features that no
   longer match the code (stale paths, renamed commands, missing endpoint coverage).
2. **Code TODOs** — stubbed or partially implemented features where the API surface
   exists but the logic is incomplete.
3. **Unstarted proposals** — designed capabilities with ADRs or proposals but no code.

The audit found **~73% of planned capabilities fully implemented**, **~9% partially
implemented**, and **~18% not started**. This ADR records the findings and establishes
a phased plan to close gaps incrementally, starting with zero-risk documentation fixes
and progressing to feature completion.

---

## Findings

### Category A: Documentation Drift (5 issues)

| # | File | Issue | Severity |
|---|------|-------|----------|
| A1 | `docs/reference/api.md` | API paths missing `/stone` prefix (stale since ARCH-0006 rename, 2026-01-19) | HIGH |
| A2 | `docs/specs/api-v1.md` | Same stale API paths — specification used for design reference | HIGH |
| A3 | `docs/guides/storage.md` | References `storage adopt` / `storage prepare` commands (now `storage add`) | HIGH |
| A4 | `docs/reference/driver-specification.md` | API path inconsistency (last verified 2026-02-18) | MEDIUM |
| A5 | `docs/specs/moss-daemon-lifecycle.md` | References `docker-compose.yml` (no longer exists) | LOW |

### Category B: Missing Documentation (10 items)

| # | Feature | Code Location | What's Missing |
|---|---------|---------------|----------------|
| B1 | S3 Presigned URLs | `api/v1/s3_presign.rs` | No client integration guide |
| B2 | Cloud Filter (Windows) | `infra/cloud_filter/` (5 modules) | No guide — only ADR STORAGE-0012 |
| B3 | Greenhouse UI | `api/v1/greenhouse/` | No user guide for web manifest authoring |
| B4 | Admin API | `api/v1/admin/` (shutdown, reboot, WoL) | Not in any reference doc |
| B5 | Snapshots (Nurturing) | Full A/B backup system | Guide uses old naming |
| B6 | Console mode control | `api/v1/console/` | Undocumented |
| B7 | Service reassign | `POST .../services/{s}/reassign` | Not in API reference |
| B8 | Service reconcile/refresh | `POST .../services/reconcile`, `/refresh` | Not in API reference |
| B9 | Capabilities CRUD | `GET/POST/DELETE .../offerings/{name}/capabilities` | Not in API reference |
| B10 | Adoption endpoints | `adoptable`, `adopted`, `borrowed`, `borrow` | Not in API reference |

### Category C: Proposal Housekeeping (3 items)

| # | Proposal | Status | Action |
|---|----------|--------|--------|
| C1 | `archive/proposals/moss-tools-domain.md` | Fully implemented (TOOLS-0001/0002/0003) | ✅ Archived |
| C2 | `archive/proposals/offering-sub-capabilities.md` | API endpoints exist | ✅ Archived |
| C3 | `archive/proposals/offering-unified-model.md` | Largely reflected in code | ✅ Archived |

### Category D: Terminology Drift (ARCH-0006 residual)

| Old Term | New Term | Affected Areas |
|----------|----------|----------------|
| `nourishment` | `updates` | Some guides, CHANGELOG inline |
| `nurturing` | `snapshots` | Guide titles, API descriptions |
| `store` (command) | `storage` | Older Rake references |

### Category E: Code TODOs — Incomplete Logic (15 items)

| # | Location | TODO | Domain |
|---|----------|------|--------|
| E1 | `services.rs:515` | Docker container log streaming | Services |
| E2 | `services.rs:718` | Cordon logic (mark non-schedulable) | Services |
| E3 | `service_manager.rs:137` | Full service installation logic | Services |
| E4 | `offerings.rs:276` | Simplified config transform for planting | Offerings |
| E5 | `updates.rs:407` | Granular update item selection (V1+) | Nourishment |
| E6 | `updates.rs:1130` | Manifest-based hardware requirements | Nourishment |
| E7 | `s3_gateway.rs:1502` | S3 copy proxy for remote storage | Storage |
| E8 | `s3_xml.rs:53` | Bucket creation date metadata | Storage |
| E9 | `collect.rs:32` | Quiesceable ceremony (graceful pre-harvest) | Ceremonies |
| E10 | `secrets.rs:94` | TPM secret backend | Security |
| E11 | `secrets.rs:132` | Platform keyring (macOS/Win/Linux) | Security |
| E12 | `presence.rs:243` | Lantern detection from offerings | Presence |
| E13 | `multipart.rs:135` | Streaming assembly for >500MB objects | Storage |
| E14 | `discovery.rs:22-46` | Build service list + health for Lantern | Discovery |
| E15 | `storage_replication.rs:236` | Full directory walk reconciliation (4e+) | Replication |

### Category F: Unstarted Proposals (16 items, by priority)

| # | Capability | ADR/Proposal | Complexity |
|---|------------|-------------|------------|
| F1 | Pond Passphrase Generation | Proposal | Low |
| F2 | CPU Inference Tier | ORCH-0005 | Low |
| F3 | Hardware Profiles | Proposal | Low |
| F4 | Self-Deploying Moss | BUILD-0003 (Draft) | Medium |
| F5 | AI Capability Router | ORCH-0002 | Medium |
| F6 | TPM Secret Backend | Code TODO E10 | Medium |
| F7 | Platform Keyring | Code TODO E11 | Medium |
| F8 | Connection String Drivers | Roadmap Phase 2 | Medium |
| F9 | Stone Lifecycle Ops | Proposal | Medium |
| F10 | Database Choreographer | ORCH-0003 | Medium |
| F11 | Web Dashboard (Phase 4) | Roadmap | High |
| F12 | Federation Bridges | Proposal | High |
| F13 | AWS Bridge | Proposal | Very High |
| F14 | Distributed Ceremonies | Proposal | High |
| F15 | Pebble Android Tier | Proposal | Very High |
| F16 | Phone Repurposing | Proposal | Very High |

---

## Decision

Close gaps in seven incremental phases. Each phase is self-contained: it can ship
independently, and later phases do not invalidate earlier ones.

### Phase 0: Documentation Alignment (zero code changes)

**Scope**: Fix all Category A, B, C, D items.
**Risk**: None — documentation-only changes.
**Effort**: ~2 sessions.

| Step | Items | Description |
|------|-------|-------------|
| 0a | A1, A2 | Update API paths in `reference/api.md` and `specs/api-v1.md` to include `/stone` prefix |
| 0b | A3 | Fix `guides/storage.md` — replace `adopt`/`prepare` with `add` |
| 0c | A4, A5 | Fix driver-specification paths; add archive note to moss-daemon-lifecycle |
| 0d | B4, B7–B10 | Add missing endpoints to `reference/api.md` (admin, reassign, reconcile, capabilities CRUD, adoption) |
| 0e | B5, D* | Fix terminology drift: nurturing→snapshots, nourishment→updates across guides |
| 0f | C1–C3 | Move implemented proposals to `archive/proposals/` |
| 0g | B1, B3, B6 | Write short guides: S3 presigned URLs, greenhouse UI, console mode |
| 0h | B2 | Write Cloud Filter guide (Windows storage integration) |

### Phase 1: Thin Code Completions (small, isolated changes)

**Scope**: Close Category E items that are isolated, low-risk, and don't cross domain boundaries.
**Risk**: Low — each change is a single function or handler, no architectural impact.
**Effort**: ~2–3 sessions.

| Step | Items | Description | DDD Approach |
|------|-------|-------------|--------------|
| 1a | E1 | Docker log streaming — wire `ContainerRuntime::logs()` to SSE handler | Infra implements stream; API handler consumes via trait |
| 1b | E2 | Cordon logic — mark service non-schedulable in registry | Domain enum variant `ServiceStatus::Cordoned`; registry update |
| 1c | E8 | S3 bucket creation date — store mtime on bucket creation | Infra `ObjectStore` method; S3 handler reads |
| 1d | E12 | Lantern detection — derive `is_lantern` from offerings list | Pure domain logic in `presence.rs` |
| 1e | E14 | Lantern service list — build from running containers | Infra `ContainerRuntime::list()` → domain projection |
| 1f | E4 | Simplified offering config transform | Domain layer config-to-full-request mapper |

### Phase 2: Storage Completions (bounded to storage domain)

**Scope**: Close storage-related Category E items.
**Risk**: Medium — touches I/O paths but stays within storage bounded context.
**Effort**: ~2 sessions.

| Step | Items | Description | DDD Approach |
|------|-------|-------------|--------------|
| 2a | E7 | S3 copy proxy for remote storage | Infra `ObjectStore::copy_remote()`; S3 handler delegates |
| 2b | E13 | Streaming multipart assembly >500MB | Infra `MultipartStore::assemble_streaming()` using `tokio::io::copy` |
| 2c | E15 | Full directory walk reconciliation | Domain `ReconciliationPolicy`; infra walks + diffs; replication task consumes |
| 2d | E9 | Quiesceable ceremony — graceful container prep | Domain `CeremonyMode::Quiesceable`; infra sends quiesce command before harvest |

### Phase 3: Service Lifecycle Completions

**Scope**: Close service management gaps (E3, E5, E6).
**Risk**: Medium — service installation touches Docker + registry.
**Effort**: ~2 sessions.

| Step | Items | Description | DDD Approach |
|------|-------|-------------|--------------|
| 3a | E3 | Full service installation logic | Domain `ServiceLifecycle::install()` orchestrates: validate → pull → create → start |
| 3b | E5 | Granular update item selection | Domain `NourishmentPolicy::select_items()` with filter predicate |
| 3c | E6 | Manifest-based hardware requirements | Domain `CompatibilityEvaluator` reads manifest metadata via `ManifestLookup` trait |

### Phase 4: Secret Backend Expansion

**Scope**: Close E10 (TPM) and E11 (Platform Keyring).
**Risk**: Medium — platform-specific, requires conditional compilation.
**Effort**: ~2 sessions.

| Step | Items | Description | DDD Approach |
|------|-------|-------------|--------------|
| 4a | E10 | TPM 2.0 backend | Infra `TpmSecretStore` behind `SecretBackend` trait; domain selects via capability detection |
| 4b | E11 | Platform keyring | Infra `KeyringSecretStore` (macOS Keychain, Windows Credential Manager, Linux Secret Service) |

### Phase 5: New Low-Complexity Features

**Scope**: Close F1–F3 (low-complexity unstarted proposals).
**Risk**: Low — self-contained features.
**Effort**: ~1–2 sessions each.

| Step | Items | Description |
|------|-------|-------------|
| 5a | F1 | Pond passphrase generation — word-list based, user-friendly |
| 5b | F3 | Hardware profiles — structured capability descriptions |
| 5c | F2 | CPU inference tier — `ollama-cpu` offering with separate tiering |

### Phase 6: Strategic Features (requires design iteration)

**Scope**: F4–F16 — larger features that need their own design sessions.
**Risk**: High — these reshape architecture or add new subsystems.
**Approach**: Each item gets its own ADR before implementation begins.

| Priority | Items | Feature | Prerequisite |
|----------|-------|---------|-------------|
| Near | F4 | Self-Deploying Moss | BUILD-0003 ADR finalization |
| Near | F5 | AI Capability Router | ORCH-0002 design session |
| Mid | F8 | Connection String Drivers | API stability (Phase 0 complete) |
| Mid | F9 | Stone Lifecycle Ops | Design session |
| Mid | F10 | Database Choreographer | ORCH-0003 design session |
| Far | F11 | Web Dashboard | Portrait/Pulse foundation stable |
| Far | F12–F16 | Federation, AWS, Android | Not scheduled — revisit after Phase 5 |

---

## Consequences

### Positive

- Documentation becomes trustworthy again — users and AI assistants get correct paths
  and commands on first try.
- Code TODOs shrink from 15 to 0 across Phases 1–4, eliminating stub responses and
  silent no-ops.
- Each phase is independently shippable — partial completion still delivers value.
- Phase ordering matches risk profile: docs (zero risk) → isolated code (low) →
  bounded-context code (medium) → platform-specific (medium) → new features (high).

### Negative

- Phase 6 items remain unscheduled — this ADR acknowledges them without committing to
  timelines.
- Documentation fixes in Phase 0 may reveal additional drift not caught by the audit.

### Neutral

- The audit snapshot reflects state as of 2026-03-25. New features merged after this
  date should be checked against the gap list to avoid regression.

---

## Tracking

Progress is tracked via this ADR's phase checklist. Each step is marked complete when
its changes are merged to `dev`.

- [x] Phase 0: Documentation Alignment (2026-03-25)
- [x] Phase 1: Thin Code Completions (2026-03-25) — 1a-1f complete
- [x] Phase 2: Storage Completions (2026-03-25) — 2a-2d complete
- [x] Phase 3: Service Lifecycle Completions (2026-03-25) — 3a-3c complete
- [x] Phase 4: Secret Backend Expansion (2026-03-25) — Koi vault replaces Moss stubs
- [x] Phase 5: New Low-Complexity Features (2026-03-25) — F1, F2, F3 complete
- [x] Phase 6/F4: Self-Deploying Moss — BUILD-0003 accepted (2026-03-25)
- [x] Phase 6/F5: AI Capability Router — already implemented (~92%), core complete (2026-03-25)
- [x] Phase 6/F6-F7: TPM + Platform Keyring — handled by Koi vault (Phase 4)
- [x] Phase 6/F10: Database Choreographer — MongoDB orchestrator core complete; generic extraction proposed as [ORCH-0012](ORCH-0012-cluster-adapter-extraction.md) (2026-03-25)
- [ ] Phase 6/F8: Connection String Driver Libraries
- [ ] Phase 6/F9: Stone Lifecycle Ops
- [ ] Phase 6/F11-F16: Web Dashboard, Federation, AWS Bridge, Android (individual ADRs)
