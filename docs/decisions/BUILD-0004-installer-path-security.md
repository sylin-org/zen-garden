---
audience: [developer, contributor, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-17
---

# BUILD-0004: Installer Path Security

**Date**: 2026-03-17
**Status**: Accepted
**Depends on**: [BUILD-0003 (Self-Deploying Moss)](BUILD-0003-self-deploying-moss.md)
**Applies to**: `src/moss/src/infra/installer/` (all submodules)

## Context

BUILD-0003 collapsed all deployment logic into the `garden-moss` binary: fresh install, update, pre-start staged deployment, and OS provisioning. The binary runs as root on Linux (required for service registration, directory ownership, and package deployment).

A code review identified that the installer module accepted paths from untrusted sources (package tarballs, staging directories) without validation before writing them to the filesystem as root. Specific vectors:

1. **Path traversal via staged packages**: `deploy_scripts()` in `pre_start.rs` and `install_scripts()` in `linux.rs` walked a `scripts/` directory, stripped the prefix, and joined the relative path to `/`. A staged file at `scripts/../../etc/shadow` would resolve to `/etc/shadow`.

2. **Unsafe tar extraction**: `extract_tar_gz()` in `package.rs` invoked `tar xzf` without `--no-same-owner`, allowing archive entries to set arbitrary file ownership.

3. **Predictable temp directory**: `install_temp_dir()` returned the fixed path `/tmp/zen-garden-install`. On a multi-user system, an attacker could pre-create a symlink at that path, redirecting install writes to an arbitrary location.

4. **Silent privilege failures**: Several `Command::new("chown")` and `chpasswd` calls discarded their exit status with `let _ =`. A failed `chpasswd` left the stone user with no usable password; a failed `chown` left application data owned by root.

The project already had `garden_common::utils::validation::validate_safe_path()` which rejects `..` components, absolute paths, and backslashes on Linux — but the installer never called it.

## Decision

Four security invariants apply to all installer code that runs as root:

### 1. Path validation before filesystem write

All paths derived from untrusted sources (package contents, staging directories, tarball entries) must pass `validate_safe_path()` before any filesystem write. Untrusted sources include:

- Files walked from `scripts/` directories in packages
- Files extracted from `.tar.gz` or `.zip` archives
- Paths constructed from user-provided configuration values

The validation runs after `strip_prefix()` and before `Path::new("/").join(rel)`. Any path that fails validation aborts the operation with a descriptive error.

### 2. Safe archive extraction

Tar.gz extraction uses the Rust `tar` + `flate2` crates for in-process extraction with per-entry path validation. Each entry's path is validated BEFORE it is written to disk — no file touches the filesystem until its path passes `validate_safe_path()`. The `tar` crate's `set_preserve_permissions(false)` prevents archive metadata from setting arbitrary ownership.

Zip extraction (secondary format) uses platform shell commands (`Expand-Archive` on Windows, `unzip` on Linux) with post-extraction path validation as defense-in-depth.

### 3. Unpredictable temp directories

Install temp directories use `tempfile::Builder` (or equivalent OS-level `mktemp`) to generate unpredictable names. The `TempDir` handle is threaded through the install session and auto-cleans on drop. No hardcoded `/tmp/zen-garden-*` paths.

### 4. Auditable privilege commands

`Command::new()` calls that affect system state (file ownership, user credentials, service registration) must check their exit status. `let _ =` is permitted only for best-effort cleanup operations, annotated with `// best-effort:` explaining why failure is acceptable.

Categories:
- **Must-check**: `chpasswd`, `chown` on application data, `useradd`, `usermod -aG sudo`
- **Best-effort**: `systemctl enable --now` (service may already be running), `systemctl mask` (cosmetic), `timedatectl` (non-critical)

## Consequences

### Positive

- **Defense in depth**: A malicious or corrupted package cannot write outside the intended deployment paths, even when the installer runs as root.
- **Existing utility reused**: `validate_safe_path()` was already tested and available; wiring it in required no new validation code.
- **Silent failures surfaced**: Operators see warnings when `chown` or `chpasswd` fails, enabling diagnosis instead of discovering the problem later when the daemon fails to start.
- **Symlink attacks blocked**: Unpredictable temp directory names eliminate the TOCTOU race window.

### Negative

- **Stricter failure mode**: A package with unexpected path structure (e.g., symlinks in `scripts/`) now fails loudly instead of deploying silently. This is the correct behavior but could surprise operators of hand-crafted packages.

### Neutral

- **No performance impact**: `validate_safe_path()` is a string check. The temp directory randomization adds one `mktemp` syscall.

## References

- [BUILD-0003: Self-Deploying Moss](BUILD-0003-self-deploying-moss.md) — architecture being hardened
- `garden_common::utils::validation::validate_safe_path()` — path validation utility
- `garden_common::utils::fs` — safe filesystem helpers
