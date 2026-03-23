---
name: zen-garden-conventions
description: Development conventions and patterns for zen-garden. Rust Rust project with mixed commits.
---

# Zen Garden Conventions

> Generated from [sylin-org/zen-garden](https://github.com/sylin-org/zen-garden) on 2026-03-23

## Overview

This skill teaches Claude the development patterns and conventions used in zen-garden.

## Tech Stack

- **Primary Language**: Rust
- **Framework**: Rust
- **Architecture**: type-based module organization
- **Test Location**: separate

## When to Use This Skill

Activate this skill when:
- Making changes to this repository
- Adding new features following established patterns
- Writing tests that match project conventions
- Creating commits with proper message format

## Commit Conventions

Follow these commit message conventions based on 200 analyzed commits.

### Commit Style: Mixed Style

### Prefixes Used

- `feat`
- `fix`
- `refactor`
- `docs`

### Message Guidelines

- Average message length: ~64 characters
- Keep first line concise and descriptive
- Use imperative mood ("Add feature" not "Added feature")


*Commit message example*

```text
fix: drop systemd sandbox + moss-owned MOTD
```

*Commit message example*

```text
feat: ARCH-0012 typed StoneApi client layer + fix edition 2024 inference
```

*Commit message example*

```text
chore: bump all dependencies to latest stable versions
```

*Commit message example*

```text
refactor: ARCH-0007 Rust 1.92 modernization — edition 2024, monomorphic traits, structured concurrency
```

*Commit message example*

```text
docs: add STORAGE-0016 unified S3 storage gateway ADR
```

*Commit message example*

```text
Merge pull request #3 from sylin-org/arch-0005/common-scope
```

*Commit message example*

```text
feat: leverage new dependency capabilities -- events stream, shared clients, thermal monitoring
```

*Commit message example*

```text
refactor(s3): replace hand-built XML with quick-xml serde serialization
```

## Architecture

### Project Structure: Single Package

This project uses **type-based** module organization.

### Source Layout

```
src/
├── build-utils/
├── common/
├── companion-sdk/
├── cricket/
├── firefly/
├── lantern/
├── moss/
├── orchestrators/
├── probe/
├── rake/
```

### Configuration Files

- `docs/presentations/package.json`
- `docs/presentations/tailwind.config.js`
- `src/lantern/frontend/package.json`
- `src/lantern/frontend/tsconfig.json`
- `src/lantern/frontend/vite.config.ts`
- `src/orchestrators/mongodb/Dockerfile`
- `src/orchestrators/ollama/Dockerfile`

### Guidelines

- Group code by type (components, services, utils)
- Keep related functionality in the same type folder
- Avoid circular dependencies between type folders

## Code Style

### Language: Rust

### Naming Conventions

| Element | Convention |
|---------|------------|
| Files | camelCase |
| Functions | camelCase |
| Classes | PascalCase |
| Constants | SCREAMING_SNAKE_CASE |

### Import Style: Relative Imports

### Export Style: Named Exports


*Preferred import style*

```typescript
// Use relative imports
import { Button } from '../components/Button'
import { useAuth } from './hooks/useAuth'
```

*Preferred export style*

```typescript
// Use named exports
export function calculateTotal() { ... }
export const TAX_RATE = 0.1
export interface Order { ... }
```

## Error Handling

### Error Handling Style: Try-Catch Blocks


*Standard error handling pattern*

```typescript
try {
  const result = await riskyOperation()
  return result
} catch (error) {
  console.error('Operation failed:', error)
  throw new Error('User-friendly message')
}
```

## Common Workflows

These workflows were detected from analyzing commit patterns.

### Feature Development

Standard feature implementation workflow

**Frequency**: ~11 times per month

**Steps**:
1. Add feature implementation
2. Add tests for feature
3. Update documentation

**Files typically involved**:
- `**/*.test.*`
- `**/api/**`

**Example commit sequence**:
```
refactor: replace VolumeHealth + online:bool with VolumeState aggregate
Merge pull request #2 from sylin-org/arch-0003-migration
refactor: move moss-only modules out of common (pure rename)
```

### Test Driven Development

Test-first development workflow (TDD)

**Frequency**: ~4 times per month

**Steps**:
1. Write failing test
2. Implement code to pass test
3. Refactor if needed

**Files typically involved**:
- `**/*.test.*`
- `**/*.spec.*`
- `src/**/*`

**Example commit sequence**:
```
test: add tests for user validation
feat: implement user validation
```

### Refactoring

Code refactoring and cleanup workflow

**Frequency**: ~14 times per month

**Steps**:
1. Ensure tests pass before refactor
2. Refactor code structure
3. Verify tests still pass

**Files typically involved**:
- `src/**/*`

**Example commit sequence**:
```
refactor: replace VolumeHealth + online:bool with VolumeState aggregate
Merge pull request #2 from sylin-org/arch-0003-migration
refactor: move moss-only modules out of common (pure rename)
```

### Api Endpoint Addition Or Rename

Adds or renames an API endpoint, including handler, routing, and sometimes related DTO/types and documentation.

**Frequency**: ~2 times per month

**Steps**:
1. Create or rename handler file in src/moss/src/api/v1/
2. Update src/moss/src/api/v1/mod.rs to include new handler
3. Update src/moss/src/bootstrap/router.rs to add/modify route
4. Update related domain/service files if needed (src/moss/src/domain/...)
5. Update client code (e.g., src/rake/src/commands/..., src/common/src/client/...) if endpoint is consumed
6. Update documentation if endpoint is public (docs/reference/api.md, .agentic/reference/api-endpoints.md, etc.)

**Files typically involved**:
- `src/moss/src/api/v1/*.rs`
- `src/moss/src/api/v1/mod.rs`
- `src/moss/src/bootstrap/router.rs`
- `src/rake/src/commands/**/*.rs`
- `src/common/src/client/**/*.rs`
- `docs/reference/api.md`
- `.agentic/reference/api-endpoints.md`

**Example commit sequence**:
```
Create or rename handler file in src/moss/src/api/v1/
Update src/moss/src/api/v1/mod.rs to include new handler
Update src/moss/src/bootstrap/router.rs to add/modify route
Update related domain/service files if needed (src/moss/src/domain/...)
Update client code (e.g., src/rake/src/commands/..., src/common/src/client/...) if endpoint is consumed
Update documentation if endpoint is public (docs/reference/api.md, .agentic/reference/api-endpoints.md, etc.)
```

### Cli Command Addition Or Refactor

Adds, renames, or reorganizes CLI commands and groups in the rake CLI, including manifest, routing, and handler logic.

**Frequency**: ~2 times per month

**Steps**:
1. Add or modify command handler in src/rake/src/commands/
2. Update src/rake/src/command_manifest.rs to register the command (add CommandDef, aliases, etc.)
3. Update src/rake/src/route.rs to add routing logic
4. Update src/rake/src/cli_build.rs if command grouping or flags change
5. Update documentation (docs/reference/cli.md, etc.)

**Files typically involved**:
- `src/rake/src/commands/**/*.rs`
- `src/rake/src/command_manifest.rs`
- `src/rake/src/route.rs`
- `src/rake/src/cli_build.rs`
- `docs/reference/cli.md`

**Example commit sequence**:
```
Add or modify command handler in src/rake/src/commands/
Update src/rake/src/command_manifest.rs to register the command (add CommandDef, aliases, etc.)
Update src/rake/src/route.rs to add routing logic
Update src/rake/src/cli_build.rs if command grouping or flags change
Update documentation (docs/reference/cli.md, etc.)
```

### Dependency Bump And Cargo Lock Update

Updates dependency versions across the workspace, including Cargo.toml and Cargo.lock, sometimes with code changes for breaking API updates.

**Frequency**: ~2 times per month

**Steps**:
1. Update version numbers in Cargo.toml files (root and/or per-crate)
2. Regenerate Cargo.lock (cargo update or cargo generate-lockfile)
3. Update code to accommodate breaking changes in dependencies
4. Update orchestrator lockfiles if present (src/orchestrators/*/Cargo.lock)
5. Update build scripts if dependency version stamping is affected

**Files typically involved**:
- `Cargo.toml`
- `Cargo.lock`
- `src/**/Cargo.toml`
- `src/**/Cargo.lock`
- `src/**/*.rs`
- `installer/build-*.ps1`

**Example commit sequence**:
```
Update version numbers in Cargo.toml files (root and/or per-crate)
Regenerate Cargo.lock (cargo update or cargo generate-lockfile)
Update code to accommodate breaking changes in dependencies
Update orchestrator lockfiles if present (src/orchestrators/*/Cargo.lock)
Update build scripts if dependency version stamping is affected
```

### Architectural Refactor With Adrs

Performs a large-scale refactor guided by an ADR (architecture decision record), including code, documentation, and sometimes file moves or renames.

**Frequency**: ~1 times per month

**Steps**:
1. Create or update docs/decisions/ARCH-xxxx-*.md with rationale and plan
2. Refactor codebase according to ADR (e.g., move files, rename modules, change trait boundaries, update API/CLI vocabulary)
3. Update related documentation (docs/reference/*, docs/guides/*, etc.)
4. Update tests and client code to match new architecture
5. Commit with reference to ADR

**Files typically involved**:
- `docs/decisions/ARCH-*.md`
- `src/**/*.rs`
- `src/**/*.toml`
- `docs/reference/*.md`
- `docs/guides/*.md`

**Example commit sequence**:
```
Create or update docs/decisions/ARCH-xxxx-*.md with rationale and plan
Refactor codebase according to ADR (e.g., move files, rename modules, change trait boundaries, update API/CLI vocabulary)
Update related documentation (docs/reference/*, docs/guides/*, etc.)
Update tests and client code to match new architecture
Commit with reference to ADR
```

### Build Pipeline Script Hardening

Updates or hardens build scripts (PowerShell, shell) to improve reproducibility, prevent lockfile drift, or adapt to new dependency management practices.

**Frequency**: ~1 times per month

**Steps**:
1. Edit installer/build-*.ps1 and installer/compile-*.ps1 scripts to change cargo invocation flags (--locked, --frozen, etc.)
2. Regenerate Cargo.lock if needed
3. Update documentation or comments in scripts to explain new pipeline
4. Test build on all platforms to ensure reproducibility

**Files typically involved**:
- `installer/build-*.ps1`
- `installer/compile-*.ps1`
- `Cargo.lock`

**Example commit sequence**:
```
Edit installer/build-*.ps1 and installer/compile-*.ps1 scripts to change cargo invocation flags (--locked, --frozen, etc.)
Regenerate Cargo.lock if needed
Update documentation or comments in scripts to explain new pipeline
Test build on all platforms to ensure reproducibility
```

### S3 Gateway Feature Development

Implements or extends S3-compatible storage gateway features, including new handlers, object store logic, tests, and documentation.

**Frequency**: ~2 times per month

**Steps**:
1. Add or update handler files in src/moss/src/api/v1/s3_*.rs
2. Update src/moss/src/infra/storage/* for object store and S3 logic
3. Update src/moss/src/api/v1/garden_storage/mod.rs and related endpoints
4. Add or update unit tests for S3 features
5. Update ADRs and documentation (docs/decisions/STORAGE-*.md)

**Files typically involved**:
- `src/moss/src/api/v1/s3_*.rs`
- `src/moss/src/infra/storage/*.rs`
- `src/moss/src/api/v1/garden_storage/mod.rs`
- `docs/decisions/STORAGE-*.md`

**Example commit sequence**:
```
Add or update handler files in src/moss/src/api/v1/s3_*.rs
Update src/moss/src/infra/storage/* for object store and S3 logic
Update src/moss/src/api/v1/garden_storage/mod.rs and related endpoints
Add or update unit tests for S3 features
Update ADRs and documentation (docs/decisions/STORAGE-*.md)
```

### Documentation Update For Vocabulary Or Feature

Updates documentation files to reflect new vocabulary, CLI/API changes, or major features.

**Frequency**: ~2 times per month

**Steps**:
1. Edit docs/reference/*.md, docs/guides/*.md, docs/glossary.md, etc.
2. Edit .agentic/reference/api-endpoints.md for API path changes
3. Update migration tables and examples in journeys and proposals
4. Commit with reference to related code changes or ADR

**Files typically involved**:
- `docs/reference/*.md`
- `docs/guides/*.md`
- `docs/glossary.md`
- `docs/journeys/*.md`
- `docs/proposals/*.md`
- `.agentic/reference/api-endpoints.md`

**Example commit sequence**:
```
Edit docs/reference/*.md, docs/guides/*.md, docs/glossary.md, etc.
Edit .agentic/reference/api-endpoints.md for API path changes
Update migration tables and examples in journeys and proposals
Commit with reference to related code changes or ADR
```


## Best Practices

Based on analysis of the codebase, follow these practices:

### Do

- Use camelCase for file names
- Prefer named exports

### Don't

- Don't deviate from established patterns without discussion

---

*This skill was auto-generated by [ECC Tools](https://ecc.tools). Review and customize as needed for your team.*
