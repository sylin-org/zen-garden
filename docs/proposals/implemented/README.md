# Implemented Proposals

This directory contains proposals that have been fully implemented in the codebase.

## Tools Domain (2026-02-06)

Unified automation-grade tools projection and stream for offerings + seed banks, with event-driven wishful readiness and capability propagation.

### Proposals
1. **[../zen-garden-spec-tools-domain.md](../zen-garden-spec-tools-domain.md)** - Original specification
   - Status: ✅ Implemented (greenfield)
   - Implementation Date: 2026-02-06
   - API: `GET /api/v1/garden/tools`, `GET /api/v1/garden/tools/stream`
   - Beacon: `TOOLS_BEACON` (`tools_beacon`)
   - CLI: `garden-rake find <query> wishfully` waits on tools stream readiness

2. **[tools-domain-implementation.md](tools-domain-implementation.md)** - Implementation report
   - Delivered module map, stream semantics, and validation results

### Documentation
- User guide: [../../guides/tools-domain-user-guide.md](../../guides/tools-domain-user-guide.md)

---

## Intelligent Offering Placement (2026-01-23)

The intelligent placement system was fully implemented, enabling automatic stone selection based on compatibility and resource scoring.

### Proposals
1. **[intelligent-offering-placement.md](intelligent-offering-placement.md)** - Original specification
   - Status: ✅ Fully Implemented
   - Implementation Date: 2026-01-23
   - Moss: `src/moss/src/domain/placement.rs` (380 lines)
   - API: `POST /api/v1/garden/recommend`
   - CLI: `garden-rake offer <name> somewhere`

2. **[intelligent-offering-placement-delta.md](intelligent-offering-placement-delta.md)** - Implementation delta analysis
   - Comprehensive validation of implementation vs spec
   - No gaps identified

### Key Features
- ✅ Multi-factor scoring algorithm (compatibility, resources, distribution)
- ✅ Parallel metrics collection across all stones
- ✅ Interactive CLI with top-3 recommendations
- ✅ Quiet mode auto-install
- ✅ Exclusion summary for debugging
- ✅ Full compatibility integration

---

## Rust Refactoring (2026-01-20)

Complete architectural refactoring with domain/infra/API separation and main.rs reduced to 45 lines.

### Proposals
1. **[rust-refactoring-proposal.md](rust-refactoring-proposal.md)** - Original proposal
   - Status: ✅ 100% Complete
   - Implementation Report: [RUST-REFACTORING-STATUS.md](../../RUST-REFACTORING-STATUS.md)
   - main.rs: 45 lines (99% reduction from 3,976 lines)

2. **[main-rs-extraction-plan.md](main-rs-extraction-plan.md)** - Final extraction plan
   - All orchestration moved to `bootstrap/run.rs`
   - 74 focused modules established

### Architecture
- ✅ domain/ - Business logic (15 files, 1,819 lines)
- ✅ infra/ - Infrastructure (19 files, 2,920 lines)
- ✅ api/ - HTTP endpoints (12 files, 2,890 lines)
- ✅ bootstrap/ - Initialization (8 files, 1,023 lines)
- ✅ tasks/ - Background operations (7 files, 1,834 lines)
- ✅ 103 tests passing

---

## Unified Deployment Packages (2026-01-23)

Package-based deployment system with atomic upgrades and validation.

### Proposal
1. **[unified-deployment-packages.md](unified-deployment-packages.md)** - Complete specification
   - Status: ✅ 100% Complete (Windows uses flag-based upgrade)
   - Implementation Date: 2026-01-23
   - API: `POST /api/v1/stone/deploy`
   - Push script: `deploy.ps1` with package mode

### Key Features
- ✅ Platform-specific packages (tar.gz/zip)
- ✅ SHA256 validation
- ✅ Linux: `installer/garden-upgrade.sh` (ExecStartPre)
- ✅ Windows: Flag-based upgrade (`--update-finalize`, `--cleanup-old`)
- ✅ Atomic staging and validation
- ✅ Auto-restart when package contains moss
- ✅ Push script with package mode (`PublishMode = "Package"`)

---

## Offering Modes (2026-01-21)

The offering modes feature was fully implemented, enabling three deployment patterns:

### Proposals
1. **[offering-modes.md](offering-modes.md)** - Original specification
   - Status: ✅ Implemented with terminology change (Planted → Managed)
   - Date: January 2026
   - Note: Used "Planted" terminology; finalized as "Managed"

2. **[offering-modes-implementation.md](offering-modes-implementation.md)** - Intermediate implementation plan
   - Status: ✅ Superseded by refactoring plan
   - Provided design principles and data model foundation

3. **[offering-modes-refactoring-plan.md](offering-modes-refactoring-plan.md)** - Final implementation plan
   - Status: ✅ Fully implemented
   - Implementation Date: 2026-01-21
   - **This was the plan actually executed**

### Implementation Reports
- [OFFERING-MODES-IMPLEMENTATION-COMPLETE.md](../../OFFERING-MODES-IMPLEMENTATION-COMPLETE.md)
- [OFFERING-MODES-DATA-POPULATION.md](../../OFFERING-MODES-DATA-POPULATION.md)

### Key Deliverables

**Code Changes**:
- 18 new files (~1,800 lines of code)
- 7 modified files
- 99 tests (100% passing)

**Data**:
- 5 example offering manifests
- Complete manifest loader system
- 700+ lines of documentation

**Features**:
- ✅ Managed mode (container-based)
- ✅ Adopted mode (existing services)
- ✅ Borrowed mode (external network services)
- ✅ Auto-adoption with platform detection
- ✅ Detection orchestration (command, HTTP, container)
- ✅ Secrets management (encrypted file backend)
- ✅ REST API endpoints (5 endpoints)
- ✅ Minimal manifests (4-6 lines)

### Validation

**Architecture**:
- ✅ Zero hardcoded service names
- ✅ Optional fields completely omitted (not null/{}/[])
- ✅ 100% backwards compatible
- ✅ Clean domain/infra/API separation

**Testing**:
- ✅ 103 total tests passing
- ✅ 0 compilation errors
- ✅ 0 compilation warnings

---

## How to Use This Directory

When a proposal is implemented:

1. **Update the proposal status** at the top of the file
2. **Add implementation report link** to relevant documentation
3. **Move to implemented/** directory
4. **Update this README** with summary and links
5. **Create implementation report** in docs/ for reference

---

## Related Directories

### [ongoing/](../ongoing/)
Proposals that are substantially implemented (75-95%) with remaining work in progress.

**Current**: 1 proposal (CLI taxonomy)

### [proposals/](../)
Active proposals awaiting implementation.

**Current**: 13 proposals

---

**Last Updated**: 2026-02-06
