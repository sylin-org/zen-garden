---
audience: [contributor, ai]
doc_type: notes
status: current
last_verified: 2026-01-18
canonical: true
---

# Architecture Decision Records (ADRs)

**Index of all architectural decisions**

---

## Active ADRs

### Architecture
- **[ARCH-0001](ARCH-0001-soc-ddd-architecture.md)**: SoC/DDD Architecture for Moss — Accepted (2026-01-20)
- **[ARCH-0002](ARCH-0002-platform-runtime-trait.md)**: PlatformRuntime Trait — Accepted (2026-03-10)
- **[ARCH-0003](ARCH-0003-code-standards-compliance-migration.md)**: Code Standards Compliance Migration — Accepted (2026-03-11)
- **[ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md)**: AppState Domain Context Extraction — Accepted (2026-03-11)
- **[ARCH-0005](ARCH-0005-structural-quality-pass.md)**: Structural Quality Pass — Accepted (2026-03-15)
- **[ARCH-0006](ARCH-0006-unified-interface-language.md)**: Unified Interface Language — Accepted (2026-03-17)
- **[ARCH-0007](ARCH-0007-monomorphic-domain-traits.md)**: Rust 1.92 Modernization — Monomorphic Traits, Edition 2024 — Accepted (2026-03-22)
- **[ARCH-0008](ARCH-0008-drop-systemd-sandbox.md)**: Drop systemd Sandbox Constraints — Accepted (2026-03-22)
- **[ARCH-0009](ARCH-0009-moss-owned-motd.md)**: Moss-Owned MOTD — Accepted (2026-03-22)
- **ARCH-0010**: _(ADR file not yet created — decision made but not documented)_
- **ARCH-0011**: _(ADR file not yet created — decision made but not documented)_
- **[ARCH-0012](ARCH-0012-typed-stone-api-client.md)**: Typed StoneApi Client Layer — Accepted (2026-03-22)

### Build & Distribution
- **[BUILD-0001](BUILD-0001-versioning.md)**: Natural Flow Versioning — Accepted (2026-01-15)
- **[BUILD-0002](BUILD-0002-unified-deployment-packages.md)**: Unified Deployment Packages — Accepted (2026-01-23)
- **[BUILD-0003](BUILD-0003-self-deploying-moss.md)**: Self-Deploying Moss — Accepted (2026-02-09)
- **[BUILD-0004](BUILD-0004-installer-path-security.md)**: Installer Path Security — Accepted

### Compatibility
- **[COMPAT-0001](COMPAT-0001-compatibility.md)**: Offering Compatibility Rules
  - **Status**: Accepted
  - **Rationale**: Version compatibility policies for service offerings
  - **Impact**: Clear upgrade/downgrade rules, semantic versioning for offerings

### Companions
- **[CRICKET-0001](CRICKET-0001-audio-Companion-spec.md)**: Cricket Audio Companion Specification
  - **Status**: Accepted (2026-01-26)
  - **Rationale**: Sonify infrastructure with 4-channel mixer and tune system
  - **Impact**: Physical presence feedback, event-to-audio mapping, 180 CC0 samples

### Metrics
- **[METRICS-0001](METRICS-0001-unified-storage-metrics.md)**: Unified Storage Metrics
  - **Status**: Accepted (2026-01-26)
  - **Rationale**: Eliminate duplicate storage detection, use live metrics only
  - **Impact**: Removed ~200 lines of redundant code, hot-swap drive support, 30s refresh

### Lantern (Registry)
- **[LANTERN-0001](LANTERN-0001-registry.md)**: Service Registry Architecture
  - **Status**: Accepted
  - **Rationale**: Optional central directory for service discovery
  - **Impact**: Faster discovery than mDNS, Windows compatibility

### Moss (Daemon)
- **[MOSS-0001](MOSS-0001-registry.md)**: Persistent Registry and Adoption — Accepted
- **[MOSS-0002](MOSS-0002-infrastructure-handlers.md)**: Infrastructure Handlers — Accepted (2026-01-31)
- **[MOSS-0003](MOSS-0003-docker-runtime-resilience.md)**: Docker Runtime Resilience — Accepted (2026-02-04)
- **[MOSS-0004](MOSS-0004-phased-cooperative-shutdown.md)**: Phased Cooperative Shutdown — Accepted (2026-02-09)
- **[MOSS-0005](MOSS-0005-manageable-env-vars.md)**: Manageable Environment Variables — Accepted (2026-03-06)

### Pavilion (Windows Client)
- **[PAVILION-0001](PAVILION-0001-windows-client-separation.md)**: Pavilion — Standalone Windows Client for Garden Visibility and OS Integration — Accepted (2026-05-04)

### URI / Resolution
- **[URI-0003](URI-0003-zen-garden-urn-form-scheme.md)**: `zen-garden:` URI Scheme — URN-Form Cascade Intent Resolution — Accepted (2026-05-04)

### Discovery
- **[DISC-0001](DISC-0001-discovery-as-first-class-crate.md)**: Discovery as a First-Class Crate (`garden-discovery`) — Accepted (2026-05-04)

### Orchestration
- **[ORCH-0001](ORCH-0001-replant-ceremony.md)**: Replant Ceremony — Offering State Transfer Between Stones — Proposed (2026-02-16) — _data plane reused; user-facing surface superseded by ORCH-0039_
- **[ORCH-0039](ORCH-0039-seed-based-offering-replication.md)**: Seed-Based Offering Replication (per-offering event log + drag-canvas surface) — Proposed (2026-05-05)
- **[ORCH-0002](ORCH-0002-routing-safety-net.md)**: Routing Safety Net — Never Refuse an Installed Model — Accepted (2026-02-18)
- **[ORCH-0003](ORCH-0003-fitness-profiler.md)**: Fitness Profiler — Model Benchmark System — Accepted (2026-02-18)
- **[ORCH-0004](ORCH-0004-gateway-announcement.md)**: Gateway Announcement — Accepted (2026-02-21)
- **[ORCH-0005](ORCH-0005-cpu-inference-tier.md)**: CPU Inference Tier — Accepted (2026-02-19)
- **[ORCH-0006](ORCH-0006-coordination-mode.md)**: CoordinationMode Enum — Accepted (2026-02-22)
- **[ORCH-0007](ORCH-0007-managed-logical-sets.md)**: MongoDB Replica Set Orchestrator — Accepted (2026-02-24)
- **[ORCH-0008](ORCH-0008-handler-election-suppression.md)**: Handler Election Suppression — Accepted (2026-03-01)
- **[ORCH-0009](ORCH-0009-demand-weighted-topology-advisor.md)**: Demand-Weighted Topology Advisor — Accepted (2026-03-06)
- **[ORCH-0010](ORCH-0010-extended-fitness-capabilities.md)**: Extended Fitness Capabilities (Tools + Think) — Accepted (2026-03-06)
- **[ORCH-0011](ORCH-0011-recommended-model-monikers.md)**: Recommended Model Monikers — Accepted (2026-03-06)

### Offerings (Services)
- **[OFFER-0001](OFFER-0001-taxonomy.md)**: Offering Taxonomy — Accepted
- **[OFFER-0002](OFFER-0002-container-namespace-collision.md)**: Container Namespace Collision Prevention — Accepted
- **[OFFER-0003](OFFER-0003-offering-fqn.md)**: Offering Fully-Qualified Names (FQN) v1 — Accepted (2026-02-06)
- **[OFFER-0004](OFFER-0004-intelligent-placement.md)**: Intelligent Offering Placement — Accepted (2026-01-23)
- **[OFFER-0005](OFFER-0005-offering-modes.md)**: Three Offering Modes (Managed / Adopted / Borrowed) — Accepted (2026-01-21)
- **[OFFER-0006](OFFER-0006-image-direct-and-fqn-v2.md)**: Image-Direct Deployment and FQN v2 — Accepted (2026-03-02)

### Rake (CLI)
- **[RAKE-0010](RAKE-0010-caching.md)**: Cached Endpoint Resolution
  - **Status**: Accepted
  - **Rationale**: Tending command for resolution cache management
  - **Impact**: Performance optimization, stale endpoint cleanup

### Storage (Seed Banks)
- **[STORAGE-0002](STORAGE-0002-api-structure.md)**: Storage API Structure (Native + S3 dual-layer) — Accepted
- **[STORAGE-0003](STORAGE-0003-beacon-protocol.md)**: Storage Beacon Protocol — Accepted
- **[STORAGE-0004](STORAGE-0004-seedbank-resilience.md)**: Seed Bank Resilience — Accepted
- **[STORAGE-0005](STORAGE-0005-manifest-first-discovery.md)**: Manifest-First Discovery — Accepted
- **[STORAGE-0006](STORAGE-0006-seed-bank-replication.md)**: Seed Bank Replication, Roles, and Pond Encryption — Accepted (2026-02-17)
- **[STORAGE-0007](STORAGE-0007-storage-lifecycle-objects.md)**: Storage Lifecycle Objects — Accepted (2026-02-17)
- **[STORAGE-0008](STORAGE-0008-garden-stone-api-split.md)**: Garden / Stone API Split for Storage — Accepted (2026-02-17)
- **[STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md)**: Managed Storage and File Sharing (WebDAV, Cloud Filter, S3) — Accepted (2026-03-07)
- **[STORAGE-0010](STORAGE-0010-unified-storage-add-command.md)**: Unified Storage Add Command — Accepted (2026-03-08)
- **[STORAGE-0011](STORAGE-0011-unified-storage-domain.md)**: Unified Storage Domain — Accepted (2026-03-10)
- **[STORAGE-0012](STORAGE-0012-cloud-filter-rebuild.md)**: Cloud Filter Rebuild (SoC, full callback coverage) — Accepted (2026-03-10)
- **[STORAGE-0013](STORAGE-0013-replica-set-identity.md)**: Replica Set Name as Display Identity — Accepted (2026-03-09)
- **[STORAGE-0014](STORAGE-0014-storage-platform-architecture.md)**: Storage Platform Architecture — Accepted (2026-03-11)
- **[STORAGE-0015](STORAGE-0015-cloud-drive-storage-router.md)**: Cloud Drive Storage Router (streaming I/O, StorageHandle) — Accepted (2026-03-12)
- **[STORAGE-0016](STORAGE-0016-s3-port-per-storage-listener.md)**: Unified S3 Storage Gateway (port-per-storage, unified namespace) — Accepted (2026-03-19)
- **[STORAGE-0017](STORAGE-0017-volume-state-machine.md)**: Volume Domain Object with Encapsulated State Machine — Proposed (2026-04-04)
- **[STORAGE-0018](STORAGE-0018-device-health-monitor.md)**: Device Health Monitor — Proposed (2026-04-04)
- **[STORAGE-0019](STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md)**: Candidate Storage Lifecycle and Foreign-Filesystem Adoption — Proposed (2026-05-05)

### Tools
- **[TOOLS-0001](TOOLS-0001-garden-tools-domain.md)**: Unified Garden Tools Domain — Accepted (2026-02-06)
- **[TOOLS-0002](TOOLS-0002-garden-tool-unified-contract.md)**: Unified GardenTool Contract and Projection Pipeline — Accepted (2026-03-02)
- **[TOOLS-0003](TOOLS-0003-unified-garden-registry.md)**: Unified Garden Registry (beacon-propagated gateways) — Accepted (2026-03-05)

---

## Pending (Under Review)

_No pending ADRs at this time._

See [archive/proposals/](../archive/proposals/) for historical proposals.

---

## Superseded / Deprecated

### URI / Resolution
- **[URI-0001](URI-0001-zen-garden-uri-scheme.md)**: `zen-garden://` URI Scheme — Cascade Intent Resolution — Superseded by URI-0003 (2026-05-04). The URL-form (`://`) was syntactically misleading for an intent scheme; URN-form (`:`) reflects intent semantics honestly.
- **[URI-0002](URI-0002-protocol-prefix-deprecation-and-extensions.md)**: Protocol-Prefix Deprecation and Capability/Action Extensions — Superseded by URI-0003 (2026-05-04). The cascade extensions (capability queries, categories, wish, at) carry forward into URI-0003 unchanged; only the surface syntax changed.

---

## ADR Process

1. **Proposal**: Create proposal document
2. **Review**: Discuss in GitHub Issues/Discussions
3. **Decision**: Accept → Create ADR | Reject → Archive with rationale
4. **Formalize**: Convert accepted proposal to ADR format
5. **Update Index**: Add to this README.md

### ADR Template

```markdown
---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted|proposed|superseded
last_verified: YYYY-MM-DD
canonical: true
---

# ADR-XXXX: Title

**Status**: Accepted | Proposed | Deprecated | Superseded by ADR-YYYY  
**Date**: YYYY-MM-DD  
**Deciders**: [names/roles]  
**Tags**: [relevant, tags]

---

## Context

[Problem statement and constraints]

What situation led to this decision? What requirements must be met?

---

## Decision

[What was decided - clear, unambiguous statement]

We will [ACTION] by [METHOD].

---

## Rationale

[Why this decision was made]

- Reason 1: [explanation]
- Reason 2: [explanation]
- Reason 3: [explanation]

---

## Consequences

### Positive
- Benefit 1
- Benefit 2

### Negative
- Trade-off 1
- Trade-off 2

### Neutral
- Implication 1
- Implication 2

---

## Alternatives Considered

### Alternative 1: [Name]
- **Description**: [brief]
- **Pros**: [list]
- **Cons**: [list]
- **Rejected because**: [reason]

### Alternative 2: [Name]
- **Description**: [brief]
- **Pros**: [list]
- **Cons**: [list]
- **Rejected because**: [reason]

---

## References

- [Related ADRs]
- [Proposals]
- [External resources]
```

### ADR Naming Convention

**Format**: `<PREFIX>-<NUMBER>-<slug>.md`

**Prefixes**:
- `BUILD-` - Build system, versioning, distribution
- `COMPAT-` - Compatibility policies
- `LANTERN-` - Lantern registry decisions
- `MOSS-` - Moss daemon decisions
- `OFFER-` - Offering/service decisions
- `RAKE-` - Rake CLI decisions
- `POND-` - Security/Pond decisions
- `CLI-` - CLI design decisions
- `API-` - API design decisions
- `PAVILION-` - Pavilion (Windows client) decisions
- `URI-` - URI scheme and intent resolution decisions
- `DISC-` - Discovery (mDNS, UDP, cache, providers)

**Examples**:
- `BUILD-0001-versioning.md`
- `POND-0002-totp-stone-admission.md`
- `CLI-0001-dual-syntax-taxonomy.md`

### Numbering

- Numbers are unique within prefix (not globally unique)
- Use leading zeros: 0001, 0010, 0100
- Gaps are acceptable (e.g., 0001, 0010, 0015)

---

## Guidelines

### When to Create an ADR

**Do create ADR for**:
- Architectural choices with long-term impact
- Trade-offs between competing approaches
- Decisions that affect multiple components
- Changes to core abstractions or protocols
- Security model changes

**Don't create ADR for**:
- Implementation details (code comments suffice)
- Trivial choices with no significant trade-offs
- Reversible decisions with low cost
- Temporary workarounds

### ADR Immutability

Once an ADR is **Accepted**:
- Content should not change (preserve decision context)
- Status can change: Accepted → Superseded
- If decision changes, create new ADR and mark old as superseded

### Linking ADRs

- Reference related ADRs in "References" section
- Use relative links: `[BUILD-0001](BUILD-0001-versioning.md)`
- Link from specifications to ADRs for design rationale
- Link from proposals to related ADRs

---

## ADR Statistics

- **Total ADR files**: 96 (2 planned but not yet filed: ARCH-0010, ARCH-0011)
- **By Status**:
  - Accepted: ~94
  - Proposed: 1 (ORCH-0001)
  - Superseded: 0
- **By Domain**:
  - Architecture: 12 (ARCH-0001 through 0012; 0010/0011 pending)
  - Build: 4
  - Communications: 5
  - Companions: 3 (CRICKET-0001, FIREFLY-0001..0003)
  - Compatibility: 1
  - Discovery/DNS/mDNS/Lantern: 4
  - Metrics: 1
  - Moss: 5
  - Offerings: 6
  - Orchestration: 11
  - Ports/Portrait: 4
  - Presence/Topology: 3
  - Security: 4
  - Storage: 15 (STORAGE-0002 through 0016)
  - Tools: 3

---

## Related Documentation

- [Archive](../archive/) - Historical decisions and proposals

---

**Last Updated**: 2026-03-22
**Maintained By**: Architecture Team
**Review Cycle**: As needed (updated when ADRs added/changed)
