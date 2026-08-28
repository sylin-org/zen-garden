# Architecture Decision Records — v1

Numbered, immutable records of structural decisions. Once Accepted, an ADR
is never edited — superseding decisions reference their predecessor.

## Convention

- **File**: `docs/v1/decisions/ADR-####-kebab-title.md`
- **Numbering**: sequential (0001, 0002, …); never reused, even if superseded
- **Status lifecycle**: Proposed → Accepted → Superseded by ADR-#### → (rarely) Rejected
- **Sections**: Title · Status · Context · Decision · Law encoded ·
  Alternatives considered · Consequences (positive/negative/neutral) · References
- Prose citations in other documents use `[ADR-####](decisions/ADR-####-title.md)`
- **What earns an ADR:** a *structural* choice with alternatives genuinely
  weighed — storage layout, protocol shapes, seam definitions. Domain
  semantics belong in OFFERINGS.md; experiential rules belong in lessons.md;
  borrowed shortcuts belong in DEBT.md. If no alternative was seriously
  considered, it isn't a decision record — it's documentation.

## Index

| ADR | Status | Title |
|-----|--------|-------|
| [ADR-0001](ADR-0001-offering-directory.md) | Accepted | The Offering Directory as the unit of deployment |
| [ADR-0002](ADR-0002-port-allocation-and-residence.md) | Accepted | Port addresses: stable allocation, honest residence |
| [ADR-0003](ADR-0003-offering-fqn-namespace.md) | Accepted | Offering identity: the FQN namespace, the reserved default, moniker surfaces |
| [ADR-0004](ADR-0004-discovery-envelope-and-uri-grammar.md) | Accepted | The discovery envelope: depth tiers, revision merges, and the URI grammar |
| [ADR-0005](ADR-0005-living-will-capture-replant.md) | Accepted | The living will: capture, checkpoints, and replant |
| [ADR-0006](ADR-0006-suzu-contract.md) | Accepted | The Suzu companion contract |
| [ADR-0007](ADR-0007-orthogonal-surface-degrees.md) | Accepted | Surface degrees: encoding, projection, extraction are orthogonal |

## Queued (expected soon)

- ADR-0008 — declared install forms (`inputs:`) replacing env-var incantations (§5.1)
- ADR-0009 — declarative garden: per-stone plans lifted to `garden.yaml` (M3 bridge)
- ADR-0010 — OCI-unified adapter: docker/podman as one engine, two sockets (§4 bet)
