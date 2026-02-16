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
- **[ARCH-0001](ARCH-0001-soc-ddd-architecture.md)**: SoC/DDD Architecture for Moss
  - **Status**: Accepted (2026-01-20)
  - **Rationale**: Domain/infra/API separation, main.rs reduced to 45 lines
  - **Impact**: Testable domain logic, clear module boundaries

### Build & Distribution
- **[BUILD-0001](BUILD-0001-versioning.md)**: Natural Flow Versioning
  - **Status**: Accepted (2026-01-15)
  - **Rationale**: major.minor.timestamp format, timestamp = truth
  - **Impact**: Predictable versioning, build-time revision injection
- **[BUILD-0002](BUILD-0002-unified-deployment-packages.md)**: Unified Deployment Packages
  - **Status**: Accepted (2026-01-23)
  - **Rationale**: One package format for all deployment methods
  - **Impact**: SHA256 validation, atomic staging, platform-specific finalization

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
- **[MOSS-0001](MOSS-0001-registry.md)**: Persistent Registry and Adoption
  - **Status**: Accepted
  - **Rationale**: Stone-local service registry with persistence
  - **Impact**: Survives reboots, enables offline operation

- **[MOSS-0002](MOSS-0002-infrastructure-handlers.md)**: Infrastructure Handlers
  - **Status**: Accepted (2026-01-31)
  - **Rationale**: Self-contained handlers for garden-wide effects (registry trust, DNS, etc.)
  - **Impact**: Distributed autonomous configuration, Docker daemon auto-trust for registries

- **[MOSS-0003](MOSS-0003-docker-runtime-resilience.md)**: Docker Runtime Resilience
  - **Status**: Accepted (2026-02-04)
  - **Rationale**: Mirror NetworkMonitor pattern for Docker daemon availability tracking
  - **Impact**: Graceful degradation when Docker unavailable, automatic recovery on reconnect

- **[MOSS-0004](MOSS-0004-phased-cooperative-shutdown.md)**: Phased Cooperative Shutdown
  - **Status**: Accepted (2026-02-09)
  - **Rationale**: CancellationToken + sd_notify + cooperative task exit prevent stuck updates
  - **Impact**: Background tasks exit cleanly on SIGTERM, SSE streams drain, systemd Type=notify

### Offerings (Services)
- **[OFFER-0001](OFFER-0001-taxonomy.md)**: Offering Taxonomy
  - **Status**: Accepted
  - **Rationale**: Categorization scheme for service offerings
  - **Impact**: Organized service catalog (data, cache, compute, ai, storage, messaging, web)
- **[OFFER-0003](OFFER-0003-offering-fqn.md)**: Offering Fully-Qualified Names (FQN)
  - **Status**: Accepted (2026-02-06)
  - **Rationale**: Separate offering type from instance identity
  - **Impact**: Multi-instance support, consistent naming across APIs and containers
- **[OFFER-0004](OFFER-0004-intelligent-placement.md)**: Intelligent Offering Placement
  - **Status**: Accepted (2026-01-23)
  - **Rationale**: Multi-factor scoring for automatic stone selection
  - **Impact**: `garden-rake offer <name> somewhere` with ranked recommendations
- **[OFFER-0005](OFFER-0005-offering-modes.md)**: Three Offering Modes
  - **Status**: Accepted (2026-01-21)
  - **Rationale**: Managed (container), Adopted (existing), Borrowed (network) modes
  - **Impact**: Garden represents full infrastructure, not just containers

### Rake (CLI)
- **[RAKE-0010](RAKE-0010-caching.md)**: Cached Endpoint Resolution
  - **Status**: Accepted
  - **Rationale**: Tending command for resolution cache management
  - **Impact**: Performance optimization, stale endpoint cleanup

### Tools
- **[TOOLS-0001](TOOLS-0001-garden-tools-domain.md)**: Unified Garden Tools Domain
  - **Status**: Accepted (2026-02-06)
  - **Rationale**: Single normalized projection of all offerings and seed banks
  - **Impact**: One API surface for automation, SSE stream, wishful readiness

---

## Pending (Under Review)

_No pending ADRs at this time._

See [archive/proposals/](../archive/proposals/) for historical proposals.

---

## Superseded / Deprecated

_No superseded ADRs at this time._

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

- **Total ADRs**: 10
- **By Status**:
  - Accepted: 9
  - Proposed: 0
  - Superseded: 0
- **By Domain**:
  - Build: 1
  - Companions: 1
  - Compatibility: 1
  - Lantern: 1
  - Metrics: 1
  - Moss: 3
  - Offerings: 2
  - Rake: 1

---

## Related Documentation

- [Archive](../archive/) - Historical decisions and proposals

---

**Last Updated**: February 6, 2026
**Maintained By**: Architecture Team
**Review Cycle**: As needed (updated when ADRs added/changed)
