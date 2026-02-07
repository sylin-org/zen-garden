# Zen Garden - Agentic Context

> **Tool-agnostic AI context.** Claude, Cursor, Copilot, and other AI assistants bootstrap from here.

---

## 🎯 Before Writing Code

**Check existing utilities**: [reference/utilities.md](reference/utilities.md)
- Formatting, paths, timeouts, limits, shared types

**Check API endpoints**: [reference/api-endpoints.md](reference/api-endpoints.md)
- Stone endpoints, garden endpoints, query parameters

---

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

// WRONG - hardcoded paths
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

### 6. Async I/O
- File I/O: `tokio::fs` (never `std::fs`)
- HTTP: `reqwest` with timeouts
- Background: `tokio::spawn` with **mandatory error handling**

### 7. Background Task Error Handling
```rust
// CORRECT - errors are visible
tokio::spawn(async move {
    if let Err(e) = do_background_work().await {
        tracing::error!(error = %e, "Task failed");
    }
});

// WRONG - silent failure
tokio::spawn(async move {
    let _ = do_background_work().await;
});
```

---

## Verification Commands

After making changes, run:
```bash
cargo check --all
cargo test --package moss
cargo clippy -- -D warnings
```

---

## Module Structure

```
src/
├── common/           # Shared: types, utils, constants, contracts
├── moss/             # Stone daemon
│   ├── domain/       # Business logic
│   ├── infra/        # External integrations
│   └── api/          # HTTP handlers
├── rake/             # CLI client
├── cricket/          # Audio companion
├── firefly/          # LED companion
└── lantern/          # Service registry
```

---

## Quick Reference

| Category | Location |
|----------|----------|
| Formatting utils | `common/src/utils.rs` |
| Platform paths | `common/src/constants/paths.rs` |
| Timeouts | `common/src/constants/timeouts.rs` |
| Limits | `common/src/constants/limits.rs` |
| Shared types | `common/src/` |

---

## Documentation

**Style guide**: `docs/DOCUMENTATION.md` (read before writing docs)

Key rules:
- **Guides/Specs/Reference** = current state only (present tense, no history)
- **ADRs** = historical decisions (past tense, immutable after acceptance)
- **Litmus test**: "If I deleted all ADRs, would every guide still make sense?" (must be YES)
- **Naming**: `lowercase-kebab-case.md` (UPPERCASE only for README/CHANGELOG/CONTRIBUTING)
- **Red flags**: Never use "What Changed", "Before/After", "We switched" in guides or specs

Templates: `docs/templates/`

## Changelog Maintenance

**File**: `docs/CHANGELOG.md` (single source of truth)

Add entry for: new features, breaking changes, architectural refactorings, user-visible bug fixes.

Skip entry for: typo fixes, formatting, internal refactoring, test-only changes.

Keep entries concise (3-5 lines). Link to ADRs and specs for details.

---

## Never Do

- Create format functions when they exist in `utils.rs`
- Define same struct in both moss and rake
- Hardcode paths (use `garden_common::constants::paths::*`)
- Use `unwrap()` in production
- Import infra into domain
- Use blocking I/O in async context

## Always Do

- Check ARCHITECTURE-REFERENCE.md first
- Use shared types from `garden_common`
- Propagate errors with `.context()`
- Use `tracing::*` for logging
- Keep domain pure
