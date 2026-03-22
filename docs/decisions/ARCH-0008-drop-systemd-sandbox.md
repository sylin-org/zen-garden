# ARCH-0008: Drop systemd sandbox constraints from garden-moss

**Status**: Accepted
**Date**: 2026-03-22

## Context

The `garden-moss` service unit included `ProtectSystem`, `PrivateTmp`, and `ReadWritePaths`
directives. These are standard systemd hardening options that restrict which paths a service
can write to.

During a fresh Debian installation, moss failed to complete first-boot initialization. The
service unit listed paths that did not yet exist (`/etc/systemd/resolved.conf.d`), causing
systemd's namespace setup to fail with `exit-code 226/NAMESPACE` before the process started.
The fix to `ProtectSystem=strict` → `ProtectSystem=full` moved the error: `/etc` is read-only
under `full`, so the first-boot hostname write entered a retry loop that exhausted all attempts
and gave up, leaving the stone permanently unnamed.

## Decision

Removed all sandbox directives (`ProtectSystem`, `PrivateTmp`, `ReadWritePaths`,
`ProtectHome`, `NoNewPrivileges`) from all three sources of the service unit:

- `installer/templates/garden-moss.service.template`
- `installer/package-assets/garden-moss.service`
- `src/moss/src/infra/installer/linux.rs`

Also removed the `ensure_etc_writable()` retry loop from `garden-common` and the outer
writable-wait loop in `start_first_boot_task()` in `run.rs`. First-boot initialization now
runs directly without first probing filesystem writability.

## Rationale

`garden-moss` runs as root on a dedicated single-purpose appliance. It is the system
management daemon — the stone's equivalent of `systemd`. There are no other tenants,
no untrusted workloads, no shared environment.

`ProtectSystem` on a root process provides no meaningful security boundary: a root process
can `mount --bind` or `mount -o remount,rw` to bypass any read-only namespace the sandbox
creates. The protection is notional. In this deployment model the threat model does not
include "moss is compromised and tries to escape its sandbox" — if moss is compromised,
the box is compromised.

The concrete cost of the sandbox was high:

- Path enumeration must be kept in sync with every feature that writes to a new location.
  Any miss causes silent startup failure or feature regression (as happened here).
- The retry/probe loop in the binary was purely compensating for a constraint that had no
  security value.
- Three copies of the service unit exist (template, package asset, code-generated). All three
  drifted: the code-generated copy was already missing `resolved.conf.d` before this incident.

The correct security boundary for moss is the OS: the stone runs nothing else as root, the
network is LAN-only, and the `stone` user account (password `stone`) is already the weaker
link in any threat model.

## Consequences

- First-boot initialization runs immediately without any writability probe or retry.
- New features that write to previously unlisted paths work without service unit changes.
- The `ensure_etc_writable()` function is removed; it no longer has a reason to exist.
- If a future deployment context requires tighter constraints (multi-tenant, cloud), a
  dedicated service unit variant should be introduced rather than re-adding sandbox directives
  to the current unit.
