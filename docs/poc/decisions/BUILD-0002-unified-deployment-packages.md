---
audience: developer
doc_type: decision
status: current
last_verified: 2026-02-07
---

# BUILD-0002: Unified Deployment Packages

**Date**: 2026-01-23
**Status**: Accepted

## Context

Deployment used two separate code paths: HTTP upload and SSH push. Each had its own staging logic, validation, and upgrade mechanism. This duplication meant bugs fixed in one path could persist in the other.

## Decision

Unify all deployment under a single package-based approach:

1. **One package format**: `zen-garden-{version}-{platform}-{arch}.{ext}` (tar.gz for Linux, zip for Windows)
2. **One staging path**: Both HTTP and SSH write to the same staging location
3. **One upgrade mechanism**: Package is validated (SHA256) and applied on restart
4. **Platform-specific finalization**: Linux uses `ExecStartPre` script; Windows uses flag-based upgrade (`--update-finalize`, `--cleanup-old`)

## Consequences

**Positive:**
- Single code path for all deployments reduces bugs
- SHA256 validation prevents corrupted upgrades
- Atomic staging ensures all-or-nothing deployment
- Auto-restart when package contains moss binary

**Negative:**
- Package must include all components even for single-binary updates
- Windows upgrade path is more complex (flag-based vs script-based)

## References

- Push script: `installer/deploy.ps1`
- Linux upgrade: `installer/garden-upgrade.sh`
- Original proposal: [archive/proposals/unified-deployment-packages.md](../archive/proposals/unified-deployment-packages.md)
