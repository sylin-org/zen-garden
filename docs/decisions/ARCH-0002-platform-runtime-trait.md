---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-10
---

# ARCH-0002: PlatformRuntime Trait for Cross-Cutting Platform Concerns

**Date**: 2026-03-10
**Status**: Accepted
**Depends on**: ARCH-0001 (SoC/DDD Architecture)

## Context

Moss runs on two platforms — Linux (headless stone daemon) and Windows (user-facing service). As features were added, platform differences were handled in two ways:

1. **Per-domain `platform.rs` files** — `#[cfg]`-gated implementations behind a clean public API. Used for infrastructure concerns: block device enumeration, FUSE mounting, filesystem probing, Cloud Filter. This pattern is correct and remains unchanged.

2. **Scattered `#[cfg]` and parallel write paths in business logic** — console output, ribbon notifications, and system lifecycle signals. These cross domain boundaries and had no parity guarantee. The result: features were silently absent on one platform without compiler enforcement.

### The Ribbon Problem

Storage hotplug ribbons (`print_storage_connected_ribbon`, `print_storage_released_ribbon`) were originally emitted from `infra/storage/monitor.rs`. When `monitor.rs` was deleted in the STORAGE-0011 refactor, the ribbon callsites were orphaned. The replacement code (`auto_mount_unmounted`, volume watcher) never restored them.

When ribbons were restored, the instinct was to gate the rendering task on `#[cfg(target_os = "linux")]` because ribbons write to `/dev/tty1`. This was wrong: the Windows service console (stdout) is an equally valid output surface — users actively monitor it. The asymmetry was an accident of implementation, not intent.

### The Root Cause

There was no abstraction enforcing that **cross-cutting announcements must be delivered on all platforms**. The `ConsolePrinter` type in `garden_common` is a partial attempt — it handles mode filtering and per-line output — but ribbon rendering bypassed it entirely via a separate `tty_write` → `/dev/tty1` path.

## Decision

Introduce a `PlatformRuntime` trait in `garden_common` as the **single abstraction for all cross-cutting platform concerns**. It covers:

- Console/ribbon output (physical console on Linux, service console on Windows)
- System lifecycle signals (systemd `sd_notify` on Linux, SCM `SetServiceStatus` on Windows)
- Shell integration hooks (future: context menus, `.desktop` files)

The trait is **not** a catch-all for all platform differences. Per-domain `platform.rs` files continue to handle infrastructure concerns (mounting, device enumeration, filesystem operations). `PlatformRuntime` covers only the cross-cutting announcement and signaling surface.

### Trait Definition

```rust
/// Cross-cutting platform concerns: console output and system signals.
///
/// Implemented by `LinuxRuntime` (→ /dev/tty1, sd_notify) and
/// `WindowsRuntime` (→ stdout, SCM). Injected into `AppState` at startup.
/// No `#[cfg]` above this layer.
pub trait PlatformRuntime: Send + Sync {
    // ---- Console output ----

    /// Print a multi-line ribbon with standard dividers.
    fn print_ribbon(&self, lines: &[&str]);

    /// Write a single line to the platform console.
    fn write_line(&self, text: &str);

    // ---- System lifecycle signals ----

    /// Signal readiness to the process supervisor.
    fn notify_ready(&self);

    /// Signal graceful shutdown to the process supervisor.
    fn notify_stopping(&self);
}
```

### Implementations

**`LinuxRuntime`** (in `moss/src/infra/platform/linux.rs`):
- `print_ribbon` / `write_line` → writes to `/dev/tty1`, falls back to stdout
- `notify_ready` → `sd_notify(false, "READY=1")`
- `notify_stopping` → `sd_notify(false, "STOPPING=1")`

**`WindowsRuntime`** (in `moss/src/infra/platform/windows.rs`):
- `print_ribbon` / `write_line` → writes to stdout (service console)
- `notify_ready` / `notify_stopping` → `SetServiceStatus` via Windows SCM

### Injection

`AppState` holds `Arc<dyn PlatformRuntime>`. The concrete implementation is selected once at startup in `bootstrap/run.rs`:

```rust
#[cfg(target_os = "linux")]
let runtime: Arc<dyn PlatformRuntime> = Arc::new(LinuxRuntime::new());

#[cfg(target_os = "windows")]
let runtime: Arc<dyn PlatformRuntime> = Arc::new(WindowsRuntime::new());
```

All code above this single `#[cfg]` block is platform-agnostic.

### Console Event Routing

`start_storage_console_task` in `coordinator.rs` receives `StorageChanged` events and calls `runtime.print_ribbon(...)`. It holds `Arc<dyn PlatformRuntime>` — no `#[cfg]`. The ribbon renders correctly on both platforms because the runtime implementation handles it.

The existing `ConsolePrinter` handles structured progress/status events (`ConsoleEvent`). It is not replaced by `PlatformRuntime` — the two serve different outputs:

| Concern | Owner |
|---|---|
| Structured progress events (Starting, Ready, etc.) | `ConsolePrinter` |
| Rich ribbon notifications (storage, boot, shutdown) | `PlatformRuntime` |
| System supervisor signals | `PlatformRuntime` |

The existing `tty_write` function and `print_storage_*_ribbon` public functions in `garden_common::console::tty` become private implementation details called only by `LinuxRuntime` and `WindowsRuntime` respectively.

## Consequences

**Positive**:
- Compiler enforces parity: adding a method to the trait requires implementing it on both platforms
- Zero `#[cfg]` in business logic, tasks, or domain code above the injection point
- Ribbon rendering reaches the Windows service console — users who monitor it see the same output as Linux TTY1
- `notify_ready` / `notify_stopping` become first-class — currently Linux-only via a scattered `#[cfg]` call in `bootstrap/run.rs`

**Negative / Trade-offs**:
- One additional abstraction layer; trait object dispatch (`dyn PlatformRuntime`) has negligible runtime cost for these infrequent calls
- `async` methods are intentionally excluded — console writes and supervisor signals are synchronous by design; mixing async into this trait would introduce complexity without benefit

## Out of Scope

- Per-domain infrastructure platform differences (mounting, device enumeration, Cloud Filter) — stay in per-domain `platform.rs` files
- Rendering format differences between platforms — both get the same ribbon ASCII art; the format is platform-agnostic, only the output destination differs
- Future desktop notifications (tray icons, toast notifications) — fits the trait's extension point but deferred
