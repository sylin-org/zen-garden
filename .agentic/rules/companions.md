---
globs: src/moss/src/infra/companions/**/*.rs, src/cricket/**/*.rs, src/firefly/**/*.rs
alwaysApply: false
---
# Companion Framework

## Port Assignment
- Base port: 7187 (ASCII sum "moss Companion" = 1187 + 6000)
- Range: 7187-7199 (13 Companions max)
- Ledger: `{data_dir}/companion-ports.json`

## Rules
- ❌ NEVER hardcode Companion ports (use ledger)
- ❌ NEVER adopt non-Companion ports
- ✅ ALWAYS pass `--port` during `--dump-commands` and startup
- ✅ ALWAYS route commands through Moss (never direct to Companions)
- ✅ ALWAYS implement `/shutdown` for graceful upgrade support

## Required Companion Endpoints
| Method | Path | Purpose |
|--------|------|---------|
| POST | `/command` | Execute commands from Moss |
| POST | `/shutdown` | Graceful shutdown |
| GET | `/health` | Health check |

## SDK Usage (Rust Companions)
```rust
use garden_companion_sdk::{CompanionConfig, CommandHandler, CompanionRuntime};

impl CommandHandler for MyHandler {
    async fn handle(&self, args: Vec<String>) -> CompanionResult {
        // Handle command
    }
}
```

## Reference
- Spec: `docs/specs/companion-COMMAND-PROTOCOL.md`
- Guide: `docs/guides/companion-development.md`
