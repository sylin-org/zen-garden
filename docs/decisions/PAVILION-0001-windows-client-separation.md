---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-05-04
canonical: true
---

# PAVILION-0001: Pavilion — Standalone Windows Client for Garden Visibility and OS Integration

**Status**: Accepted
**Date**: 2026-05-04
**Deciders**: Architecture
**Tags**: windows, client, ui, soc, cloud-filter, tauri

---

## Context

Moss currently bundles two distinct concerns:

1. **Stone daemon** — the cross-platform server that runs on a stone, owns
   storage, hosts services, participates in the pond, exposes the HTTP API.
2. **Windows OS integration** — Cloud Filter sync root, WinRT toast
   notifications, AUMID registration, registry writes, drive letter
   discovery — all per-user-session, all only meaningful on a Windows
   workstation.

The Windows integration lives in [src/moss/src/infra/cloud_filter/](../../src/moss/src/infra/cloud_filter/)
(~2,500 LOC) and pulls `windows`, `windows-sys`, `wmi`, `winreg`,
`cloud-filter`, and `nt-time` into Moss's Cargo dependencies via a
`[target.'cfg(windows)']` block. These dependencies have no role on
Linux stones (the primary deployment target — typically Pi-class hardware)
and conflate two different release cadences: a server daemon that should
move slowly and predictably, and an end-user OS-integration surface that
benefits from rapid iteration.

A separate management UI already exists in
[src/lantern/frontend/](../../src/lantern/frontend/) — React 19 + Vite +
Three.js, served by the Lantern crate at port 7186, with views for garden
topology, stones, pond, banks, offerings, and activity. It runs in a
browser tab and has no native OS surface (no tray, no toasts, no Cloud
Filter, no Explorer integration, no autostart, no single-instance
enforcement).

Users need a Windows-side experience that:

- Sits in the system tray and is always available
- Provides Cloud Filter sync, WebDAV/S3 mount actions, and Explorer overlays
- Drives discovery and onboarding (mDNS scan, claim, pond enrollment)
- Hosts the existing rich management UI rather than duplicating it
- Surfaces garden events as native Windows toasts
- Acts as the user's single dashboard for everything they do with the
  garden from a Windows machine

Moss cannot be that surface — it runs as a system daemon, often on a
different machine, often headless. Lantern cannot be that surface — it
is a browser-served HTTP service with no OS hooks.

---

## Decision

Extract Windows OS integration from Moss into a new standalone client,
**Pavilion**, built as a Tauri 2 application that is the user's single
Windows dashboard for everything in their garden.

We will:

1. Add `src/pavilion/` to the workspace as a Tauri 2 application crate.
2. Move the entire `cloud_filter/` module from `src/moss/src/infra/`
   to `src/pavilion/src/integration/cloud_filter/`, with no behavioural
   changes. Moss loses ~2,500 LOC and six Windows-only Cargo deps.
3. Reuse the existing `garden-common` typed `StoneApi` client for all
   network calls — Pavilion does not get its own protocol.
4. Embed the Lantern frontend as Pavilion's main UI surface. M1 ships by
   spawning Lantern as a child process and pointing WebView2 at
   `http://localhost:7186`. M3 hoists `frontend/` into a workspace
   package consumed by both Lantern and Pavilion, with Pavilion swapping
   `fetch()` for Tauri IPC where the call belongs to the client (LAN
   discovery, mTLS-bound calls, OS actions).
5. Run Pavilion as a per-user tray-resident process — autostart via HKCU,
   single-instance via named mutex, native window chrome, Mica/acrylic
   backdrop, system theme follow-through.
6. Keep Pavilion cooperative with Moss, never replacing it. Moss remains
   the source of truth for stone state; Pavilion is a view + OS-integration
   layer.

### Module structure

```
src/pavilion/
├── Cargo.toml                     # Tauri 2 app, Windows-only build
├── tauri.conf.json
├── src/
│   ├── main.rs                    # Tauri entry; tray, single-instance, autostart
│   ├── ipc/                       # Tauri commands exposed to the WebView
│   │   ├── discovery.rs           # mDNS scan, stone claim
│   │   ├── storage.rs             # mount/unmount, sync controls
│   │   ├── pond.rs                # ceremony bridge
│   │   └── system.rs              # autostart, theme, quiet hours
│   ├── integration/
│   │   ├── cloud_filter/          # moved from src/moss/src/infra/cloud_filter
│   │   │   ├── registration.rs
│   │   │   ├── provider.rs
│   │   │   ├── placeholders.rs
│   │   │   ├── ingest.rs
│   │   │   └── signaling.rs
│   │   ├── tray.rs                # Tray icon + popover
│   │   ├── toasts.rs              # WinRT toast dispatcher (driven by SSE)
│   │   ├── autostart.rs           # HKCU registry
│   │   └── single_instance.rs     # Named mutex
│   ├── client/                    # Thin wrappers over StoneApi from garden-common
│   │   ├── tended.rs              # Tended-stone resolution
│   │   ├── events.rs              # SSE subscription dispatcher
│   │   └── credentials.rs         # Cert/keystore loading
│   └── settings.rs                # Persisted user preferences
└── frontend/                      # M1: stub; M3: hoisted shared package
```

### Naming and identity

- Crate name: `garden-pavilion`
- Binary: `garden-pavilion.exe`
- AUMID: `garden-pavilion` (replaces `garden-moss` in [signaling.rs](../../src/moss/src/infra/cloud_filter/signaling.rs))
- Sync root display name: "Zen Garden" (unchanged — user-facing label)
- Tray tooltip prefix: "Pavilion"

### Build and distribution

- Pavilion is a workspace member, not a standalone crate.
- Built only on Windows targets — Cargo.toml gates the binary on
  `cfg(target_os = "windows")`.
- Shipped as both NSIS installer (sideload) and MSIX (Microsoft Store
  ready), via Tauri's bundler.
- Distribution tier: a new `client` tier in [installer/dist.json](../../installer/dist.json),
  separate from `core` (moss/rake) and `full` (companions/lantern).
- WebView2 runtime: rely on Windows 11 preinstall; ship Evergreen
  bootstrapper for Windows 10.

### Authentication boundary

Pavilion connects to Moss over the existing HTTP API. Today (Phase 2)
this is server-only TLS; clients do not present certificates. Pavilion
inherits this constraint. When Moss ships Phase 4 (mTLS for clients,
client enrollment endpoint), Pavilion gains a credentials module that
holds a per-installation enrolled client certificate. Pavilion ships
before Phase 4; the Phase 4 upgrade is additive.

---

## Rationale

- **Separation of concerns.** Moss is a server daemon. Tray icons, Cloud
  Filter, registry writes, WinRT toasts, autostart, and single-instance
  mutexes are not server concerns. Conflating them inflates Moss's build
  matrix, dependency graph, and surface area for Windows-only bugs.
- **Release cadence.** A user-facing dashboard wants weekly polish
  iteration. A stone daemon wants slow, careful releases. Separate
  binaries, separate cadences.
- **Per-user-session is the correct shape.** Cloud Filter must run in
  the user's session with their SID — it cannot run as a system service.
  Moss running as a service can never own this surface; a tray app must.
- **Reuse over rebuild.** The Lantern frontend already implements
  Overview, Garden, StoneDetail, Pond, SeedBanks, Activity, Offerings.
  Rebuilding these in WinUI XAML or WPF would duplicate UX without
  benefit. Tauri lets us host them in a native window without rewriting.
- **Codebase fit.** The workspace is Rust + React. Tauri matches both
  with a single new dependency layer. Electron would add Node.js as a
  third language seam. Native WinUI would fork the type system.
- **Cross-platform headroom.** macOS File Provider and Linux FUSE are
  realistic future additions to the same shell, with the same React UI.
  WinUI 3 closes that door.

---

## Consequences

### Positive

- Moss sheds ~2,500 LOC and six Windows-only crates; Linux build becomes
  the canonical target with no cross-cutting concerns.
- Cloud Filter, toasts, and Explorer overlays move to the process that
  owns the user session.
- Single tray surface for the user — discover stones, mount drives,
  manage pond, watch activity, all from one place.
- Existing React UI is reused as the dashboard surface; no UI rewrite.
- Tauri 2 provides tray, single-instance, autostart, IPC, packaging out
  of the box — no scaffolding effort.
- Cross-platform path stays open (macOS, Linux) using the same shell.

### Negative

- A new top-level component to maintain, document, and version.
- Two-process model on Windows (Pavilion + a tended Moss somewhere) —
  the user must understand the relationship between them.
- React frontend is shared between Lantern and Pavilion; coupling
  requires either a child-process bridge (M1 simple, fragile) or a
  hoisted package (M3 clean, more work).
- WebView2 dependency on Windows 10 (handled by Evergreen bootstrapper
  but adds an install step for offline machines).
- The team takes on Tauri as a new framework dependency.

### Neutral

- Pavilion is Windows-first. macOS and Linux clients are deferred until
  there's demand; the Tauri shell makes them additive rather than
  rewrites.
- Pavilion cannot improve on what Moss exposes — if an action isn't on
  the API, Pavilion can't surface it. This is correct (one source of
  truth) but means new features land in Moss first, Pavilion second.

---

## Alternatives Considered

### Alternative 1: Native WinUI 3 / WPF client

- **Description**: Pure native Windows app with XAML UI, P/Invoke or
  C++/WinRT for OS integration, gRPC/REST to Moss.
- **Pros**: Best native look and feel; deepest Win11 integration; real
  Win32 controls; tightest performance.
- **Cons**: Forks the type system (Rust domain types vs C# bindings);
  discards the existing React UI; commits to Windows-only forever; adds
  a .NET/C++ stack to a Rust codebase.
- **Rejected because**: The cost of duplicating the Lantern UI in XAML
  and maintaining two source-of-truth type systems is large, and the
  marginal "feels native" gain over Tauri (whose window chrome is
  already real Win32) is small for our use case.

### Alternative 2: Electron client

- **Description**: Same shape as Tauri (web UI + native shell) but
  bundles Chromium and runs the backend in Node.js.
- **Pros**: Mature ecosystem; identical rendering across OSes;
  battle-tested by VS Code, Slack, Discord.
- **Cons**: ~80–150 MB bundle vs Tauri's ~10–15 MB; ~150–300 MB idle
  RAM vs ~50–80 MB; Node.js as a third language seam; no straightforward
  way to host the Cloud Filter Rust module without an additional
  sidecar process and IPC.
- **Rejected because**: Bundle size and memory footprint matter for a
  tray-resident app; Node.js would create awkward IPC for the Rust
  Cloud Filter code that already works as a Rust module.

### Alternative 3: Tray helper that opens Edge to Lantern's URL

- **Description**: Minimal Rust binary in the tray, "Open Pavilion"
  launches Edge to `http://localhost:7186`.
- **Pros**: Days to ship; trivial maintenance.
- **Cons**: Browser tab is not an app — no native window, no proper
  notifications, no offline shell, awkward focus, no Cloud Filter (still
  needs a Rust agent), no proper modal flows for ceremonies.
- **Rejected because**: A native tray agent that hosts Cloud Filter is
  required regardless. Once we have that agent, hosting WebView2 inside
  it costs little more than launching a browser, and the UX gap is
  large enough to justify the small extra investment.

### Alternative 4: Keep Cloud Filter in Moss; build no client

- **Description**: Status quo — Moss stays mixed-concern, no dashboard
  beyond Lantern.
- **Pros**: Zero new code.
- **Cons**: Moss never sheds Windows-specific weight; no native
  notifications; no tray surface; onboarding flow remains terminal-only;
  no place to put future macOS/Linux client integrations.
- **Rejected because**: It does not address the SoC problem and leaves
  the user without the dashboard the project needs.

---

## Phased Delivery

A roadmap is maintained outside this ADR and may evolve. The high-level
shape:

- **M0** — Workspace scaffold; tray; single-instance; autostart; mDNS
  discovery; settings.
- **M1 (MVP)** — Onboarding (claim + pond ceremony); Cloud Filter mount;
  WebDAV mount; toasts for the events that matter; command palette;
  Lantern UI hosted via child-process bridge.
- **M2 (Premium)** — 3D garden topology, storage browse, services
  management, native theme/Mica polish, multi-stone aggregation, log
  streaming.
- **M3 (Power)** — Embedded terminal, jump lists, multi-garden switcher,
  hoisted frontend package (replaces child-process bridge), keyboard
  navigation pass.
- **M4 (Cross-platform)** — macOS File Provider extension, Linux FUSE.
- **M5 (Phase 4)** — Pavilion enrols a client certificate via the new
  Moss endpoint; mTLS becomes the default transport.

---

## References

- [STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md) — Original Cloud Filter introduction
- [STORAGE-0012](STORAGE-0012-cloud-filter-rebuild.md) — Cloud Filter architecture this client inherits
- [STORAGE-0015](STORAGE-0015-cloud-drive-storage-router.md) — Storage routing the client consumes
- [STORAGE-0016](STORAGE-0016-s3-port-per-storage-listener.md) — S3 mount surface Pavilion exposes
- [LANTERN-0001](LANTERN-0001-registry.md) — Registry Pavilion uses for discovery
- [ARCH-0012](ARCH-0012-typed-stone-api-client.md) — `StoneApi` typed client Pavilion reuses
- [SECURITY-0001](SECURITY-0001-pond-tiers.md) — Trust profiles surfaced in Pavilion
- [src/moss/src/infra/cloud_filter/](../../src/moss/src/infra/cloud_filter/) — Code being moved
- [src/lantern/frontend/](../../src/lantern/frontend/) — Frontend being embedded
