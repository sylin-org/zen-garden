# GitHub Copilot Instructions for Zen Garden

## 🎯 Essential Reading

**ALWAYS read this first before writing code:**

📖 **[docs/ARCHITECTURE-REFERENCE.md](../docs/ARCHITECTURE-REFERENCE.md)**

This is your primary reference for:
- All existing utilities and functions
- Core architectural conventions
- Shared contracts/models between moss and rake
- Platform-aware patterns
- Standard error handling

## 🚨 Critical Requirements

### 1. Don't Reinvent Wheels
Check `docs/ARCHITECTURE-REFERENCE.md` for existing utilities before creating new ones:
- Formatting: `format_bytes()`, `format_uptime()`
- Paths: Use `garden_common::constants::paths::*` functions
- Timeouts/Limits: Predefined constants exist

### 2. Shared Models (MANDATORY)
- Moss and Rake MUST share API contracts via `garden_common`
- Example: `garden_common::nourishment::*` for update types
- NO duplicate struct definitions between moss and rake
- NO bespoke structures unless explicitly approved

### 3. Architecture
- Domain = pure business logic (no external deps)
- Infra = external integrations (Docker, filesystem)
- API = thin HTTP handlers
- **Domain NEVER imports infra** - use traits

### 4. Platform Awareness
```rust
// ✅ CORRECT
use garden_common::constants::paths::{data_dir, config_dir};
let path = data_dir().join("my-file.json");

// ❌ WRONG
let path = "/var/lib/zen-garden/my-file.json";
```

### 5. Error Handling
```rust
// Domain code
use anyhow::{Context, Result};
fn my_function() -> Result<()> {
    do_something().context("Failed to do something")?;
    Ok(())
}

// API code
use axum::http::StatusCode;
fn handler() -> Result<Json<T>, (StatusCode, Json<ErrorResponse>)> {
    // Convert domain errors to HTTP responses
}
```

### 6. Async Patterns
- File I/O: Always `tokio::fs`, never blocking `std::fs`
- HTTP: Use `reqwest` with configured timeouts
- Background tasks: `tokio::spawn` with error logging

## 📋 Pre-Flight Checklist

Before generating code:
- [ ] Read relevant sections of ARCHITECTURE-REFERENCE.md
- [ ] Verify no existing utility already does this
- [ ] Check if types should be in `garden_common`
- [ ] Use platform-aware path functions
- [ ] Follow domain/infra separation
- [ ] Verify P2P transport usage (no direct UDP sockets)

## 🎯 Module Structure

```
src/
├── common/           # Shared: types, utils, constants, contracts
│   ├── nourishment/  # Shared update models (moss + rake)
│   ├── utils/        # Common utilities
│   └── constants/    # Ports, timeouts, limits, paths
├── moss/             # Stone daemon
│   ├── domain/       # Business logic only
│   ├── infra/        # Docker, filesystem, network
│   └── api/          # HTTP handlers (use garden_common types)
└── rake/             # CLI client (use garden_common types)
```

## ⛔ Never Do This

- ❌ Create format functions when they exist in `utils.rs`
- ❌ Define same struct in both moss and rake
- ❌ Hardcode `/var/lib/zen-garden` or Windows paths
- ❌ Use `unwrap()` in production code
- ❌ Import infra modules into domain
- ❌ Use blocking I/O in async context

## ✅ Always Do This

- ✅ Check ARCHITECTURE-REFERENCE.md first
- ✅ Use shared types from `garden_common`
- ✅ Use path functions for cross-platform compatibility
- ✅ Propagate errors with `.context()`
- ✅ Use `tracing::*` for logging
- ✅ Keep domain pure (no external deps)
- ✅ **Update changelog when implementing significant changes**

## 📝 Changelog Maintenance

**IMPORTANT**: Always update `docs/CHANGELOG.md` when committing significant changes.

### When to Update Changelog

✅ **DO add changelog entry for:**
- New features (multicast discovery, hardware ID generation, adoption mode)
- Breaking changes (API changes, data structure refactoring, detection schema changes)
- Architectural refactorings (moving modules, changing patterns)
- Bug fixes that affect user-visible behavior
- Performance improvements with measurable impact
- Security fixes
- New environment variables or configuration options
- Dependency additions/removals

❌ **DON'T add changelog entry for:**
- Typo fixes in comments
- Code formatting/linting changes
- Internal refactoring with no external impact
- Test-only changes
- Documentation-only updates (unless significant)

### How to Update Changelog on Commit

**When user requests a commit:**

1. **Add entry to `docs/CHANGELOG.md`** at the top of the appropriate date section
2. **Use one-liner format**: `- Brief description of change (keep under 120 chars)`
3. **Be specific**: Include key details (component, what changed, why it matters)
4. **Group by date**: Add to existing date section or create new one with format `## YYYY-MM-DD`

**Format examples:**

```markdown
## 2026-01-26
- Fixed syntax error in delete_service_v1() - Path extractor had Path(String> instead of Path<String>
- Added automatic retry logic to Docker operations with exponential backoff (3 retries, 1s/2s/4s delays)
- **BREAKING**: Renamed GARDEN_STONE_URL to GARDEN_STONE_ENDPOINT for consistency
```

**Commit message should mention changelog:**

```bash
git commit -m "fix(docker): add retry logic to container operations

Added exponential backoff retry (3 attempts) to prevent transient
Docker API failures from breaking deployments.

Updated docs/CHANGELOG.md with entry."
```

### Changelog File Location

**Single source of truth**: `docs/CHANGELOG.md`

- **Do NOT** create separate `CHANGELOG-feature-name.md` files
- **Do NOT** maintain changelogs in individual module directories
- All changes go into the main changelog with date-based sections
- Technical details belong in design docs (link from changelog if needed)

### When User Says "commit"

**Automatic workflow:**

1. Review changed files
2. Determine if changes warrant changelog entry (use criteria above)
3. **If yes**: Update `docs/CHANGELOG.md` BEFORE committing
4. Stage changelog with other changes: `git add docs/CHANGELOG.md <other-files>`
5. Write descriptive commit message
6. Execute commit

**Example:**

```bash
# User: "commit these changes"
# AI workflow:
1. Identify: Fixed bug in remove command
2. Update docs/CHANGELOG.md:
   - Added line: "Fixed remove command to actually stop containers (was registry-only)"
3. Stage all files: git add docs/CHANGELOG.md src/moss/src/api/v1/services.rs
4. Commit with message referencing changelog
```

---

**When uncertain, consult the human before proceeding.**
