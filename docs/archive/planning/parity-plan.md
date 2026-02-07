# Feature Parity Implementation Plan

**Date**: 2026-01-24  
**Scope**: Complete proposals #2 (Offering Modes) and #4 (CLI Taxonomy)

---

## Executive Summary

Two proposals need final work for 100% completion:

| Proposal | Current | Target | Effort | Priority |
|----------|---------|--------|--------|----------|
| **Offering Modes** | 98% | 100% | 1-2 days | **HIGH** |
| **CLI Taxonomy** | 70% | 100% | 3-5 days | **MEDIUM** |

**Recommendation**: Complete Offering Modes first (production-critical), then decide on CLI Taxonomy scope.

---

## Proposal #2: Offering Modes - Final 2%

### Current State ✅

**Fully Implemented**:
- ✅ All 3 modes (Managed, Adopted, Borrowed)
- ✅ Detection orchestrator with caching (`src/moss/src/domain/modes/detection.rs`)
- ✅ API endpoints (7 endpoints in `src/moss/src/api/v1/adoption.rs`)
- ✅ CLI commands (`adopt`, `release`, `borrow`, `return`, `find strays`)
- ✅ Auto-adoption on startup (`src/moss/src/tasks/auto_adoption.rs`)
- ✅ Manifest schema with control commands (`ControlConfig` in `src/common/src/manifests/offering.rs`)
- ✅ AppState registries (`adopted_offerings`, `borrowed_offerings`)

### Missing Features ⚠️

#### 1. Lifecycle Control Execution for Adopted Services
**Status**: Commands stored but never executed  
**Location**: `src/moss/src/api/v1/services.rs` (lines 395, 697, 711)  
**Issue**: `rest`/`wake` commands only handle Docker containers, not adopted services

**What's Needed**:
```rust
// In src/moss/src/api/v1/services.rs:start_service_v1()
// Check if service is adopted with Full control level
let adopted = state.adopted_offerings.read().await;
if let Some(adopted_svc) = adopted.iter().find(|s| s.name == service) {
    if adopted_svc.control_level == AdoptedControlLevel::Full {
        if let Some(cmd) = &adopted_svc.start_command {
            // Execute shell command
            return execute_control_command(cmd, &service).await;
        }
    }
    return Err("Cannot control adopted service - control_level is not Full");
}
```

**Files to Modify**:
- `src/moss/src/api/v1/services.rs` - Add adopted service handling in:
  - `start_service_v1()` (line ~395)
  - `stop_service_v1()` (line ~650)
  - `restart_service_v1()` (line ~697)
- `src/moss/src/domain/adoption.rs` - Add:
  - `execute_control_command(cmd: &str, service: &str) -> Result<()>`

**Effort**: 3-4 hours

---

#### 2. Health Monitoring for Adopted/Borrowed Services
**Status**: Only Docker containers monitored  
**Location**: `src/moss/src/tasks/health_monitor.rs` (line 41)  
**Issue**: Task only polls `state.registry` (Docker-based services)

**What's Needed**:
```rust
// In src/moss/src/tasks/health_monitor.rs:health_monitor_task()
// After Docker polling, add adopted/borrowed checks

// Check adopted services (HTTP or command-based health)
let adopted = state.adopted_offerings.read().await.clone();
for service in adopted {
    if let Some(health_config) = &service.health_check {
        let health = match health_config.method {
            HealthMethod::Http => check_http_health(&health_config).await,
            HealthMethod::Command => check_command_health(&health_config).await,
            _ => ServiceHealthStatus::Unknown,
        };
        
        // Update health if changed
        if health != service.health {
            update_adopted_health(&state, &service.name, health).await;
        }
    }
}

// Check borrowed services (ping or HTTP)
let borrowed = state.borrowed_offerings.read().await.clone();
for service in borrowed {
    let health = check_borrowed_health(&service).await;
    if health != service.health {
        update_borrowed_health(&state, &service.name, health).await;
    }
}
```

**Files to Modify**:
- `src/moss/src/tasks/health_monitor.rs`:
  - Add adopted service polling (after line 96)
  - Add borrowed service polling (after adopted)
- `src/moss/src/domain/health.rs` (new):
  - `check_http_health(config: &HealthConfig) -> ServiceHealthStatus`
  - `check_command_health(config: &HealthConfig) -> ServiceHealthStatus`
  - `check_borrowed_health(service: &BorrowedOfferingInfo) -> ServiceHealthStatus`

**Effort**: 4-6 hours

---

#### 3. Lantern Announcement Integration
**Status**: Unknown if adopted/borrowed are announced  
**Location**: Lantern registration (`src/moss/src/tasks/lantern.rs`)  
**Issue**: Need to verify topology includes all 3 modes

**What's Needed**:
```rust
// In lantern registration, include adopted/borrowed in capabilities
let adopted_capabilities = state.adopted_offerings.read().await
    .iter()
    .flat_map(|s| s.capabilities.clone())
    .collect();

let borrowed_capabilities = state.borrowed_offerings.read().await
    .iter()
    .flat_map(|s| s.capabilities.clone())
    .collect();

// Merge with managed capabilities
```

**Files to Check**:
- `src/moss/src/tasks/lantern.rs` - Verify announcement includes all modes
- `src/moss/src/api/v1/garden.rs` - Verify topology endpoint includes adopted/borrowed

**Effort**: 1-2 hours (validation only, likely already works)

---

### Testing Plan

**Manual Tests**:
1. **Adoption Workflow**:
   ```bash
   # Start ollama manually (not via Zen Garden)
   ollama serve &
   
   # Adopt it
   garden-rake find strays        # Should show ollama
   garden-rake adopt ollama       # Should adopt with Monitor level
   
   # Verify lifecycle control doesn't work (Monitor level)
   garden-rake rest ollama        # Should error: "Cannot control - Monitor level"
   
   # Re-adopt with Full control
   garden-rake release ollama
   garden-rake adopt ollama --control full --start-command "ollama serve" --stop-command "pkill ollama"
   
   # Now lifecycle should work
   garden-rake rest ollama        # Should execute stop_command
   garden-rake wake ollama        # Should execute start_command
   ```

2. **Health Monitoring**:
   ```bash
   # Adopt with HTTP health check
   garden-rake adopt ollama --health-method http --health-endpoint "http://localhost:11434/api/tags"
   
   # Stop ollama manually
   pkill ollama
   
   # Wait 30 seconds, check health
   garden-rake observe            # Should show ollama as Offline
   ```

3. **Borrowed Services**:
   ```bash
   # Borrow NAS
   garden-rake borrow synology-nas from nas.local:445 --protocol smb
   
   # Check announcement
   garden-rake observe            # Should show borrowed service
   ```

**Unit Tests**:
- `src/moss/src/domain/adoption.rs::tests::test_execute_control_command`
- `src/moss/src/domain/health.rs::tests::test_http_health_check`
- `src/moss/src/tasks/health_monitor.rs::tests::test_adopted_monitoring`

---

## Proposal #4: CLI Taxonomy - Decision Required

### Option A: Accept Zen-Only (Recommended)
**Effort**: 0 hours  
**Status**: Move to `implemented/` with note

**Rationale**:
- Zen path is 100% functional and production-ready
- 70% of proposal is complete (all zen verbs work)
- Dual syntax adds complexity without clear user demand
- Can add normative commands incrementally if needed

**Documentation Update**:
```markdown
## Implementation Note

The zen-only path was chosen for simplicity and consistency. 
All zen verbs are fully implemented with positional keyword syntax.

Normative resource commands (`services create`, `services list`) were 
deferred as they duplicate zen functionality without significant benefit.
If user demand emerges, they can be added incrementally.
```

---

### Option B: Implement Normative Dual Syntax
**Effort**: 3-5 days  
**Status**: Create new Phase 2 proposal

**Scope**:
1. **Add normative subcommand trees** (2 days):
   - `garden-rake services create <name> --at <stone>`
   - `garden-rake services stop <name> --at <stone>`
   - `garden-rake services start <name> --at <stone>`
   - `garden-rake services list --at <stone>`
   - `garden-rake offerings list --at <stone>`
   - `garden-rake stones list`

2. **Add syntax validation** (1 day):
   - Reject zen verbs with `--at` flag
   - Reject normative verbs with positional keywords
   - Update parser error messages

3. **Add missing zen verbs** (1-2 days):
   - `explore` → `list` (alias only)
   - `nourish` → `upgrade` (already mapped)
   - `touch` → deep diagnostics (new implementation)
   - `garden` → topology view (new implementation)

**Files to Create**:
- `src/rake/src/commands/normative/` - New directory
  - `services.rs` - Services resource commands
  - `offerings.rs` - Offerings resource commands
  - `stones.rs` - Stones resource commands
- `docs/proposals/cli-dual-syntax-phase2.md` - New proposal

---

## Recommendation

### Immediate (This Week):
1. ✅ **Complete Offering Modes** (1-2 days)
   - Implement lifecycle control execution
   - Add health monitoring for adopted/borrowed
   - Validate Lantern announcement
   - Run manual test suite
   - **Move to `implemented/`**

### Short-term (Next Sprint):
2. ⚠️ **Decide on CLI Taxonomy** (stakeholder decision required):
   - **Option A**: Accept zen-only, move to `implemented/` with note
   - **Option B**: Create Phase 2 proposal for normative dual syntax

---

## Success Criteria

### Offering Modes ✅ COMPLETE:
- [ ] Adopted services can be started/stopped via `rest`/`wake` when control_level=Full
- [ ] Health monitoring polls adopted services every 30s
- [ ] Health monitoring polls borrowed services every 60s
- [ ] Lantern announces all 3 modes (managed, adopted, borrowed)
- [ ] Manual test suite passes (3 scenarios)
- [ ] Documentation updated with control level examples

### CLI Taxonomy (if Option A):
- [ ] Proposal moved to `implemented/` with implementation note
- [ ] Status updated to "✅ Implemented (Zen-only path)"

### CLI Taxonomy (if Option B):
- [ ] Phase 2 proposal created with normative scope
- [ ] Original proposal stays in `ongoing/` with Phase 2 link
- [ ] Timeline: 3-5 days of implementation work

---

## Timeline

| Task | Effort | Assignee | Deadline |
|------|--------|----------|----------|
| Lifecycle control execution | 3-4h | Dev | Day 1 |
| Health monitoring extension | 4-6h | Dev | Day 1-2 |
| Lantern validation | 1-2h | Dev | Day 2 |
| Manual testing | 2-3h | QA | Day 2 |
| Documentation | 1h | Dev | Day 2 |
| **Total: Offering Modes** | **1-2 days** | | |
| CLI Taxonomy decision | 1h | PM | Day 3 |
| Phase 2 proposal (if needed) | 2h | Dev | Day 3 |

---

## Risk Assessment

### Offering Modes Completion
- **Risk**: Low
- **Complexity**: Low - straightforward command execution and HTTP polling
- **Dependencies**: None
- **Testing**: Manual tests sufficient (adoption workflow is interactive)

### CLI Taxonomy Dual Syntax
- **Risk**: Medium
- **Complexity**: Medium - parser changes + new command tree
- **Dependencies**: None
- **Testing**: Requires comprehensive syntax validation tests
- **ROI**: Uncertain - no user demand for normative syntax

---

## Appendix: Code Locations

### Offering Modes Implementation
```
src/moss/src/
├── api/v1/
│   ├── adoption.rs (7 endpoints) ✅
│   └── services.rs (needs adopted handling) ⚠️
├── domain/
│   ├── adoption.rs (adopt logic) ✅
│   ├── health.rs (needs HTTP/command checks) ⚠️
│   └── modes/
│       └── detection.rs (detection orchestrator) ✅
└── tasks/
    ├── auto_adoption.rs (startup adoption) ✅
    └── health_monitor.rs (needs adopted/borrowed) ⚠️

src/rake/src/commands/
├── adoption/ (all commands) ✅
└── discovery/ (list commands) ✅
```

### CLI Taxonomy Implementation
```
src/rake/src/
├── parser.rs (zen keyword extraction) ✅
├── main.rs (clap definitions) ✅
└── commands/ (zen verbs implemented) ✅
    ├── adoption/ ✅
    ├── lifecycle/ ✅
    ├── offering/ ✅
    └── normative/ (NOT CREATED) ❌
```

---

**Last Updated**: 2026-01-24
