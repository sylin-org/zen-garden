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
feat: add zen-garden ECC bundle (.claude/commands/refactoring.md)
```

*Commit message example*

```text
fix: drop systemd sandbox + moss-owned MOTD
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
feat: add zen-garden ECC bundle (.claude/commands/test-driven-development.md)
```

*Commit message example*

```text
feat: add zen-garden ECC bundle (.claude/commands/feature-development.md)
```

*Commit message example*

```text
feat: add zen-garden ECC bundle (.claude/homunculus/instincts/inherited/zen-garden-instincts.yaml)
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

**Frequency**: ~19 times per month

**Steps**:
1. Add feature implementation
2. Add tests for feature
3. Update documentation

**Files typically involved**:
- `**/*.test.*`
- `**/api/**`

**Example commit sequence**:
```
refactor: complete ARCH-0005 structural quality pass
fix: unify gateway registration into tool.registry, eliminating fqn_handler
refactor: add category to Offering struct, eliminate O(n×m) index lookups
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

**Frequency**: ~9 times per month

**Steps**:
1. Ensure tests pass before refactor
2. Refactor code structure
3. Verify tests still pass

**Files typically involved**:
- `src/**/*`

**Example commit sequence**:
```
refactor: complete ARCH-0005 structural quality pass
fix: unify gateway registration into tool.registry, eliminating fqn_handler
refactor: add category to Offering struct, eliminate O(n×m) index lookups
```

### Api Endpoint Addition Or Renaming

Adds or renames an API endpoint, including handler implementation, route registration, and often client/test updates.

**Frequency**: ~2 times per month

**Steps**:
1. Create or rename handler file in src/moss/src/api/v1/ (e.g., new_thing.rs or updates.rs)
2. Update src/moss/src/api/v1/mod.rs to register the new handler
3. Update src/moss/src/bootstrap/router.rs to add or change the route
4. Update related domain/service files (src/moss/src/domain/...) as needed
5. Update client code (e.g., src/rake/src/commands/..., src/common/src/client/...) to use the new endpoint
6. Update documentation if endpoint vocabulary changes

**Files typically involved**:
- `src/moss/src/api/v1/*.rs`
- `src/moss/src/api/v1/mod.rs`
- `src/moss/src/bootstrap/router.rs`
- `src/rake/src/commands/**/*.rs`
- `src/common/src/client/**/*.rs`

**Example commit sequence**:
```
Create or rename handler file in src/moss/src/api/v1/ (e.g., new_thing.rs or updates.rs)
Update src/moss/src/api/v1/mod.rs to register the new handler
Update src/moss/src/bootstrap/router.rs to add or change the route
Update related domain/service files (src/moss/src/domain/...) as needed
Update client code (e.g., src/rake/src/commands/..., src/common/src/client/...) to use the new endpoint
Update documentation if endpoint vocabulary changes
```

### Cli Command Surface Change

Adds, renames, or removes CLI commands/groups, including parser, manifest, and handler updates.

**Frequency**: ~2 times per month

**Steps**:
1. Edit src/rake/src/command_manifest.rs to add/remove/rename CommandDef entries
2. Edit src/rake/src/route.rs to add/remove route arms
3. Edit or create handler files in src/rake/src/commands/**/
4. Update parser/build logic in src/rake/src/cli_build.rs if grouping or aliasing changes
5. Update documentation and examples

**Files typically involved**:
- `src/rake/src/command_manifest.rs`
- `src/rake/src/route.rs`
- `src/rake/src/commands/**/*.rs`
- `src/rake/src/cli_build.rs`

**Example commit sequence**:
```
Edit src/rake/src/command_manifest.rs to add/remove/rename CommandDef entries
Edit src/rake/src/route.rs to add/remove route arms
Edit or create handler files in src/rake/src/commands/**/
Update parser/build logic in src/rake/src/cli_build.rs if grouping or aliasing changes
Update documentation and examples
```

### Dependency Version Bump

Upgrades Rust crate dependencies across workspace members and orchestrators, updating lockfiles and fixing breaking changes.

**Frequency**: ~2 times per month

**Steps**:
1. Edit Cargo.toml and/or src/*/Cargo.toml to bump dependency versions
2. Regenerate Cargo.lock and src/orchestrators/*/Cargo.lock
3. Fix breaking API changes in source files (e.g., src/moss/src/docker/*.rs, src/common/src/metrics/system.rs, etc.)
4. Update build scripts if needed (e.g., installer/build-*.ps1)
5. Test and fix compilation/runtime issues

**Files typically involved**:
- `Cargo.toml`
- `Cargo.lock`
- `src/*/Cargo.toml`
- `src/orchestrators/*/Cargo.lock`
- `src/**/*.rs`
- `installer/*.ps1`

**Example commit sequence**:
```
Edit Cargo.toml and/or src/*/Cargo.toml to bump dependency versions
Regenerate Cargo.lock and src/orchestrators/*/Cargo.lock
Fix breaking API changes in source files (e.g., src/moss/src/docker/*.rs, src/common/src/metrics/system.rs, etc.)
Update build scripts if needed (e.g., installer/build-*.ps1)
Test and fix compilation/runtime issues
```

### Structural Refactor With Adrs

Performs a large-scale structural refactor following an architectural decision record (ADR), often involving file moves, module decomposition, trait boundary enforcement, and code deduplication.

**Frequency**: ~1 times per month

**Steps**:
1. Write or update an ADR in docs/decisions/
2. Move, split, or rename modules/files (e.g., domain decomposition, god module breakup)
3. Update trait boundaries and interfaces (e.g., move traits to domain/traits/)
4. Deduplicate types and move shared DTOs to common/
5. Update all affected call sites and tests
6. Update documentation to reflect new structure

**Files typically involved**:
- `docs/decisions/*.md`
- `src/common/src/**/*.rs`
- `src/moss/src/domain/**/*.rs`
- `src/moss/src/api/v1/**/*.rs`
- `src/rake/src/commands/**/*.rs`

**Example commit sequence**:
```
Write or update an ADR in docs/decisions/
Move, split, or rename modules/files (e.g., domain decomposition, god module breakup)
Update trait boundaries and interfaces (e.g., move traits to domain/traits/)
Deduplicate types and move shared DTOs to common/
Update all affected call sites and tests
Update documentation to reflect new structure
```

### Build Pipeline Hardening

Updates build scripts and pipeline logic to ensure deterministic, cross-platform builds and prevent lockfile drift.

**Frequency**: ~2 times per month

**Steps**:
1. Edit installer/build-*.ps1 and installer/compile-*.ps1 scripts to add flags (e.g., --locked, --frozen)
2. Regenerate Cargo.lock as needed
3. Update Docker mount logic and CARGO_HOME/target dir handling
4. Strip problematic characters or logic from scripts for compatibility
5. Document pipeline architecture in comments or docs

**Files typically involved**:
- `installer/build-*.ps1`
- `installer/compile-*.ps1`
- `Cargo.lock`

**Example commit sequence**:
```
Edit installer/build-*.ps1 and installer/compile-*.ps1 scripts to add flags (e.g., --locked, --frozen)
Regenerate Cargo.lock as needed
Update Docker mount logic and CARGO_HOME/target dir handling
Strip problematic characters or logic from scripts for compatibility
Document pipeline architecture in comments or docs
```

### Documentation Synchronization With Code Changes

Updates documentation files to match codebase changes, especially after vocabulary, API, or CLI changes.

**Frequency**: ~2 times per month

**Steps**:
1. Edit docs/reference/*.md, docs/guides/*.md, docs/specs/*.md, and related files to match new vocabulary or API paths
2. Update migration tables, changelogs, and glossary entries
3. Archive or move obsolete proposals/specs
4. Ensure documentation matches code and CLI/API behavior

**Files typically involved**:
- `docs/reference/*.md`
- `docs/guides/*.md`
- `docs/specs/*.md`
- `docs/glossary.md`
- `docs/CHANGELOG.md`
- `docs/proposals/*.md`

**Example commit sequence**:
```
Edit docs/reference/*.md, docs/guides/*.md, docs/specs/*.md, and related files to match new vocabulary or API paths
Update migration tables, changelogs, and glossary entries
Archive or move obsolete proposals/specs
Ensure documentation matches code and CLI/API behavior
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
