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

---

**When uncertain, consult the human before proceeding.**
