# GitHub Copilot Instructions for Zen Garden

## Primary Reference

**Read before coding**: [docs/ARCHITECTURE-REFERENCE.md](../docs/ARCHITECTURE-REFERENCE.md)
- Existing utilities/functions
- Architectural conventions
- Shared contracts (moss/rake)
- Platform-aware patterns
- Error handling standards

## Critical Rules

### 1. Check for Existing Utilities
Before creating new code, verify `docs/ARCHITECTURE-REFERENCE.md`:
- Formatting: `format_bytes()`, `format_uptime()`
- Paths: `garden_common::constants::paths::*`
- Timeouts/Limits: Predefined constants

### 2. Shared Models (MANDATORY)
- Moss and Rake share API contracts via `garden_common`
- NO duplicate structs between moss and rake
- Example: `garden_common::nourishment::*`

### 3. Architecture Layers
- **Domain**: Pure business logic, no external deps
- **Infra**: External integrations (Docker, filesystem)
- **API**: Thin HTTP handlers
- **Rule**: Domain NEVER imports infra (use traits)

### 4. Platform Awareness
```rust
// CORRECT
use garden_common::constants::paths::{data_dir, config_dir};
let path = data_dir().join("my-file.json");

// WRONG
let path = "/var/lib/zen-garden/my-file.json";
```

### 5. Error Handling
```rust
// Domain: anyhow::Result with .context()
fn my_function() -> Result<()> {
    do_something().context("Failed to do something")?;
    Ok(())
}

// API: StatusCode + ErrorResponse
fn handler() -> Result<Json<T>, (StatusCode, Json<ErrorResponse>)>
```

### 6. Async Patterns
- File I/O: `tokio::fs` (never `std::fs`)
- HTTP: `reqwest` with timeouts
- Background: `tokio::spawn` with error logging

## Pre-Flight Checklist

- [ ] Read relevant ARCHITECTURE-REFERENCE.md sections
- [ ] Verify no existing utility
- [ ] Check if types belong in `garden_common`
- [ ] Use platform-aware paths
- [ ] Follow domain/infra separation
- [ ] Use P2P transport singleton (no direct UDP)

## Module Structure

```
src/
├── common/           # Shared: types, utils, constants, contracts
│   ├── nourishment/  # Shared update models
│   ├── utils/        # Common utilities
│   └── constants/    # Ports, timeouts, limits, paths
├── moss/             # Stone daemon
│   ├── domain/       # Business logic
│   ├── infra/        # External integrations
│   └── api/          # HTTP handlers
└── rake/             # CLI client
```

## Never Do

- Create format functions when they exist in `utils.rs`
- Define same struct in both moss and rake
- Hardcode paths (use `garden_common::constants::paths::*`)
- Use `unwrap()` in production
- Import infra into domain
- Use blocking I/O in async context
- Create separate changelog files

## Always Do

- Check ARCHITECTURE-REFERENCE.md first
- Use shared types from `garden_common`
- Use path functions for cross-platform
- Propagate errors with `.context()`
- Use `tracing::*` for logging
- Keep domain pure
- Update `docs/CHANGELOG.md` for significant changes

## Changelog Maintenance

**File**: `docs/CHANGELOG.md` (single source of truth)

### Add Entry For
- New features
- Breaking changes (prefix with `**BREAKING**:`)
- Architectural refactorings
- User-visible bug fixes
- Performance improvements
- Security fixes
- New environment variables
- Dependency changes

### Skip Entry For
- Comment typos
- Formatting/linting
- Internal refactoring (no external impact)
- Test-only changes
- Minor documentation updates

### On Commit

**Workflow**:
1. Review changed files
2. If significant change → update `docs/CHANGELOG.md`
3. Add to top of appropriate date section: `## YYYY-MM-DD`
4. Use one-liner format: `- Description (under 120 chars)`
5. Stage: `git add docs/CHANGELOG.md <other-files>`
6. Commit with message mentioning changelog update

**Format**:
```markdown
## 2026-01-26
- Fixed syntax error in delete_service_v1() - Path(String> → Path<String>
- Added retry logic to Docker ops (3 attempts, exponential backoff)
- **BREAKING**: Renamed GARDEN_STONE_URL to GARDEN_STONE_ENDPOINT
```

**Commit Message**:
```
fix(component): brief description

Details explaining what/why.

Updated docs/CHANGELOG.md with entry.
```
