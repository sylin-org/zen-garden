# ADR-0016 — The delivery is npm-native: `npx zen-garden`

**Status:** Accepted (2026-08-30)
**Trigger:** test-02's provisioning (fleet-facts.md's bubbles) +
operator direction: public release must be *delightfully easy* on
Linux, Windows, Apple — compatible with things like `npx`.

## Context

The garden ships two Rust binaries (moss the daemon, rake the client).
The deploy ritual that got .195 and the workstation online is
hand-rolled: cross-build in a docker, scp as `.new`, mv over, setsid
nohup, env vars by memory. Every fresh machine (test-02 taught us:
no sshd, docker group, session refresh) re-pays that cost. The
install-delight law (design/install-delight.md) demands the first
five minutes Just Work.

## Decision

**One npm package, `zen-garden`, whose `zen` binary is the whole
stack's front door** — the pattern proven by biome, turbo, and rspack
(Rust cores shipped through npm):

```
npx zen-garden                 → the zen CLI (rake verbs + stack verbs)
npx zen-garden install         → deploy moss for THIS platform,
                                 mint identity, start it, verify song
npx zen-garden doctor          → pop the bubbles: docker (API probe,
                                 not systemctl), group membership,
                                 ports, platform firewall remedy
npx zen-garden up | down | status
npx zen-garden observe …       → passthrough to rake
```

### Package shape (private first, public ready)

```
dist/npm/
  package.json          bin: { zen: bin/zen.js }   (node >= 18)
  bin/zen.js            ~200 lines of plain Node: platform shim,
                        verbs, exec passthrough — no deps, no build
  binaries/
    linux-x64/{moss,rake}
    win32-x64/{moss.exe,rake.exe}
    darwin-{arm64,x64}/…      (when a Mac joins the fleet)
```

Today the binaries ride inside the one package (fine for `file:`
installs and `npm pack` tarballs). For public npm, the SAME shim
grows `optionalDependencies: @zen-garden/<platform>` packages and
resolves from those — one line of lookup changes, zero user-visible
change. Static musl builds later remove the glibc question entirely.

### Why npm as the spine

- `npx` IS the zero-install install: try the whole garden in one
  command, no commit, no PATH pollution.
- One package.json describes every platform; CI already cross-builds;
  `npm pack` tarballs give us `curl | sh`-grade simplicity over SSH
  for machines without node (test-02 today: `npm i -g tarball`).
- Node is present on both ends of our fleet TODAY (v24 workstation,
  v26 test-02) and the shim is optional — the binaries remain
  self-contained; npm is the courier, never a runtime dependency of
  the moss itself.

### Non-goals / later

- winget/Homebrew/apt: after npm proves the shape.
- Signed notarized macOS builds: blocked on fleet Mac (flagged gap).
- `zen install --systemd`: a `--user` unit + linger; the v1 default
  is the battle-proven setsid/nohup until a Linux release-epic owns
  service managers per distro.

## Consequences

- The installer becomes code we test ON REAL MACHINES, not a wiki
  page — every fleet-facts quirk becomes a doctor check.
- `zen` is the single name users learn; rake/moss remain the honest
  nouns underneath (binaries unchanged, wire unchanged).
- First acceptance target: test-02 (Arch/Omarchy) — `npm i -g` the
  packed tarball, `zen install`, third stone singing in the room.
