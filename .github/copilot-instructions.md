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

**When to create/update a changelog:**

✅ **DO create a changelog for:**
- New features (multicast discovery, hardware ID generation, adoption mode)
- Breaking changes (API changes, data structure refactoring, detection schema changes)
- Architectural refactorings (moving modules, changing patterns)
- Bug fixes that affect user-visible behavior
- Performance improvements with measurable impact
- Security fixes

❌ **DON'T create a changelog for:**
- Typo fixes in comments
- Code formatting/linting changes
- Internal refactoring with no external impact
- Test-only changes

**Changelog format:**
- Place in `docs/` directory (e.g., `docs/CHANGELOG-feature-name.md`)
- Include: Problem, Solution, Impact, Files Changed, Testing
- Reference related ADRs/specs
- Add version/build date if applicable

**Example structure:**
```markdown
# Feature Name - Implementation Changelog

**Date**: YYYY-MM-DD
**Status**: Complete/In Progress
**Related**: [Decision/Spec references]

## Problem
Brief description of what was wrong or missing

## Solution
What was implemented and how

## Impact
Before/after comparison, user-visible changes

## Files Changed
| File | Change |
|------|--------|
| path/to/file | Description |

## Testing
Verification steps and results
```

---

**When uncertain, consult the human before proceeding.**
