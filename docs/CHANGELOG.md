# Changelog

All notable changes to Zen Garden will be documented in this file.

## 2026-02-20

- Rake CLI: manifest-driven SSOT — Clap builder API generated from CommandManifest, replacing derive macros
- Rake CLI: main.rs reduced from 3248 to 197 lines; routing moved to route.rs with string-based ArgMatches
- Parser: zen/normative verb detection now driven by manifest-provided sets instead of hardcoded match blocks

## 2026-02-19

- STORAGE-0008: Garden/Stone API split — name-based `/api/v1/garden/storage/{name}` routes with Primary-or-proxy
- STORAGE-0008: Discovery endpoint `GET /api/v1/garden/storage/{name}` returns all replicas across the garden
- STORAGE-0008: Stone-tier file routes now read-only (GET/HEAD); writes go through garden tier only
- STORAGE-0008: `X-Zen-Proxied` loop guard prevents infinite proxy chains during orchestration transitions
- STORAGE-0007: SeedBank lifecycle objects — single source of truth per seed bank (StorageDevice + SeedBankStore)
- STORAGE-0007: StorageDevice.ensure_mounted() verifies /proc/mounts before any I/O — prevents writes to unmounted dirs
- STORAGE-0007: Coordinator refresh_seed_banks_from_scan() + tick_seed_bank_health() wired to hotplug/persistence tasks
- STORAGE-0007: Orchestration, replication, nurturing scheduler migrated to use lifecycle objects with legacy fallback
- STORAGE-0007: Portrait builder reads from SeedBank lifecycle objects with legacy fallback path

## 2026-02-18

- Pin redesign: GUIDv7 pin_id with last-pin-wins semantics — any replica holder can claim Primary
- Pin redesign: auto-unpin when losing to a newer remote pin_id
- Pin persistence: pin_id survives restarts via `.zen-garden/pin.json` on mount
- Storage resilience: timed subprocess utility (30s mount, 10s query) prevents hung device commands
- Storage resilience: per-device mount recovery isolation — write lock released during I/O
- Storage resilience: circuit breaker with exponential backoff after 5 failures, abandon at 50
- Storage resilience: prepare-job guard prevents concurrent preparation of the same device
- All 25 Command::new call sites in registry.rs/device.rs wrapped with timed execution

## 2026-02-17

- STORAGE-0006 Phase 3: Write-to-primary routing in nurturing scheduler, storage gateway, and S3 gateway
- StorageCache: find_primary_by_name (prefers Primary role, falls back) and find_all_by_name
- NurturingScheduler: select_targets filters out Dormant replicas, only writes to Primary
- Gateway PUT/DELETE handlers route to remote Primary when local bank is Dormant
- STORAGE-0006 Phase 4: Cursor-based replication — changelog, squash, SSE doorbell, pull endpoint, replication task
- SeedBankStore: append-only changelog (.zen-garden/changelog.jsonl) with GUIDv7 cursors
- changes_since() squashes per-path entries to net-effect (C→M→D = omit, M→M→M = single M)
- ChangesResponse.full_sync_required flag for stale-cursor detection after compaction
- GET /api/v1/stone/storage/bank/{id}/changes — changelog pull endpoint
- GET /api/v1/stone/storage/stream — SSE doorbell for storage mutations (StorageTick)
- AppState.storage_tick_tx broadcast channel for storage replication notifications
- Seed bank replication background task syncs Dormant banks from Primary via changelog pull
- Periodic changelog compaction (7-day retention window) in orchestration task
- STORAGE-0006 Phase 5: CLI updates — pin/unpin, release disambiguation, show enhancement, default naming
- Pin/unpin API endpoints (POST /bank/pin, /bank/unpin) lock Primary role to a seed bank name
- Orchestration respects pinned state: locally pinned Primary never yields, remote pinned wins
- Rake: `pin seed-bank` / `unpin seed-bank` commands with grouped picker UX (● Primary, ★ pinned)
- Rake: `show seed-banks` garden-wide grouped view with replica count, role, encryption state
- Release command: name→id disambiguation picker when multiple same-name replicas on one stone
- Release command: fixed URL path to use /bank/{id}/release (was broken)
- PortraitSeedBank enriched with id, short_id, role, pinned, encrypted fields
- **BREAKING**: Default seed bank name changed from `seed-bank-zen-garden` to `public-seed-bank` / `private-seed-bank`
- Removed legacy DEFAULT_SEED_BANK_NAME constant — replaced with DEFAULT_PUBLIC/PRIVATE_SEED_BANK_NAME

## 2026-02-09

- MOSS-0004: Phased cooperative shutdown — CancellationToken threaded through all background tasks
- SSE streams (presence, logs, tools) break on shutdown token for clean HTTP drain
- systemd Type=notify with sd_notify READY/WATCHDOG/STOPPING integration
- All interval-loop tasks (topology, storage, announcer, health, scheduler, metrics, presence monitors) exit cooperatively on SIGTERM

## 2026-02-16

- KOI-0001: Embedded HTTP & UDP Bridging proposal — Phase 0 prerequisite for ORCH suite
- ORCH-0001/0002 updated with KOI-0001 Phase 0 dependency chain
- ORCH-0001: Wire orchestration activation — install hook + boot backfill for pre-existing offerings
- ORCH-0001 Phase 1-3: Offering orchestration types, fitness scoring, election runner, orchestration task
- Compatibility evaluation spec document (docs/specs/compatibility-evaluation.md)
- KOI-0001 Phase 0c: Container networking — extra_hosts, DNS config, env var injection in Docker
- Koi builder flags: HTTP(:5641), DNS, UDP enabled at Moss boot
- Added KOI_HTTP port constant (5641)
- tool.json: Koi standalone un-retired
- ORCH-EXECUTION-PROMPT updated with Koi HTTP/UDP structures and Phase 0 guard rails
- **BREAKING**: Removed `shutdown_tx` (Arc<Notify>) — CancellationToken is now the single shutdown source of truth
- Fixed crash loop: drain deadline timer started at boot instead of after shutdown signal
- Fixed UDP recv buffer too small (4 KB → 64 KB) — eliminates WSAEMSGSIZE errors on Windows
- COMM-0005: Chirp payload hygiene — strip cpu.features and dead-weight fields (~50% size reduction)
- Signal handler: one spawned task watches SIGTERM/SIGINT → cancels token, everything cascades
- Deploy/admin shutdown endpoints now call `shutdown_token.cancel()` instead of `notify_waiters()`
- Companion shutdown: SIGTERM all companions immediately on Moss shutdown, SIGKILL survivors before exit
- ORCH-0001/0002/0003: Offering orchestration proposal suite (fitness elections, AI router, DB choreographer)

## 2026-02-15

- Fixed update stuck: shutdown used notify_one() which only wakes one of HTTP/HTTPS servers
- Unified graceful shutdown: admin/deploy shutdown now fires goodbye callback properly
- Deadline shutdown: 8s drain deadline + 15s hard watchdog guarantees process exit
- Linux update progress: TTY1 console now shows update stages (receive → verify → stage → restart)
- Update helper TTY output: moss-update-helper.sh writes progress to /dev/tty1 during install
- Rake client enrollment: `garden-rake pond enroll` for mTLS on non-Moss machines
- New endpoint: `POST /api/v1/pond/enroll-client` for client certificate issuance
- Health: `/health` response includes optional `pond` field when stone is enrolled
- mDNS: cornerstone registers `_certmesh._tcp` for zero-config CA discovery
- Roster: `MemberRole::Client` variant distinguishes workstations from stones
- Rake: HTTP client auto-configures mTLS when enrollment certs are present
- Rake: endpoint resolution prefers HTTPS:7183 when enrolled in a pond
- Fix: `pond status` CLI now shows pond name
- Fix: `pond status -o json` outputs raw JSON (suppresses header/suggestions)
- Fix: `garden-rake api` uses tending resolution instead of raw mDNS
- Fix: pond.html shows human-readable certificate expiry dates
- Ceremony CLI: render `##` headings and `*italic*`/`**bold**` as ANSI styles
- Domain: auto-unlock lifecycle moved to CertmeshCore (single source of truth)
- Fix: JustMe ponds auto-unlock on reboot (pond_init_v1 now saves key)
- Fix: TOTP/FIDO2 unlock on incompatible pond returns 400 (not 500)
- Fix: sanitized error messages in pond API responses

## 2026-02-22

- Envelope encryption: LUKS-style key slots for pond CA (passphrase/TOTP/FIDO2/auto-unlock)
- Multi-method pond unlock: 3-way ceremony prompt (auto/token/passphrase)
- TOTP unlock slot: register authenticator during pond init, verify on unlock
- FIDO2 WebAuthn: security key registration and assertion in pond web UI
- Boot unlock: inspect slot table, log available unlock methods
- CLI: `garden-rake pond unlock --totp <code>` for authenticator-based unlock

## 2026-02-16

- Auto-unlock: JustMe/MyTeam profiles save passphrase locally, pond unlocks on reboot
- Ceremony-driven pond init: Moss hosts CeremonyHost, Rake drives via POST /api/v1/pond/ceremony
- Added ceremony_render module to Rake (async HTTP ceremony render loop)
- All pond routes moved to HTTP public lobby (bootstrap/recovery must not depend on HTTPS)
- Added pond-ceremony-engine proposal documenting constraint-satisfaction design
- Added /pond web UI SPA: browser-based ceremony wizard + pond status dashboard
- Added pond status summary to portrait JSON and portrait.html

## 2026-02-15

- Added ceremony engine: bag-of-kv framework (koi-common) + pond rules (koi-certmesh)
- Updated pond-ceremony-engine proposal with constraint-satisfaction model

## 2026-02-15

- **BREAKING**: Replaced `endpoint: String` + `pond_active` + `https_port` with `address: PeerAddress` across TopologyEntry, DiscoveredStone, DiscoveryResponse
- Added `PeerAddress` value object (ip, port, tls_port) with `http_base()`, `http_url()`, `https_base()`, `from_http_url()`
- Added `StoneClient` infrastructure gateway for centralized inter-stone HTTP with TLS reload
- Lantern `register_stone()` now takes `&PeerAddress` instead of `&str` endpoint
- Wired `StoneClient` into AppState; enrollment-change listener reloads TLS client
- Migrated pond proxy enrollment and cornerstone discovery to StoneClient
- Added `garden-rake pond rename` CLI command (decorative pond name change)
- Added Pond API endpoints to agentic reference (api-endpoints.md)

## 2026-02-14

- PondState domain surface: enrolled(), cornerstone(), PondEvent::EnrollmentChanged
- Event-driven HTTPS activation via enrollment-change listener (replaces per-handler calls)
- Moss-to-Moss proxy join: non-cornerstone stones forward enrollment to cornerstone
- Enrolled member detection at boot (cert files on disk, not just CA state)
- Removed ~130 lines of duplicated activate_pond_security code from pond handlers
- Consolidated chirp signing/verification + HTTPS binding into single activate_pond_security()

## 2026-02-16

- Pond naming: auto-generated water-themed names (pond-{adj}-{noun}, 4096 combos)
- Added PUT /api/v1/pond/name for renaming; name in init/status responses
- Pond name persisted to pond.json, seeded at boot
- Reserved-name guard: component names excluded from pond dictionary
- Added stone/deploy and stone/upgrade to HTTP public lobby router (fixes deploy to pond-enrolled stones)
- **BREAKING**: Moved MOSS_HTTPS from 7187 to 7183 — avoids bind conflict with companion port range
- Added COMPANION_PORT_BASE/MAX (7187/7199) to shared constants in garden-common
- Replaced hardcoded port numbers with garden_common::constants across codebase
- Phase 2 remaining: HTTPS binding on :7183 with TLS via certmesh certificates
- Route splitting: configure_public() (HTTP lobby) vs configure() (HTTPS all routes) when pond active
- Signed chirps: ECDSA P-256 envelope enricher/verifier hooks in p2p transport
- UdpAnnouncement gains signature + sender_cert fields for signed chirp verification
- Phase 2: Pond Security via Certmesh — rewired all pond handlers from stubs to live koi-certmesh ops
- Enabled certmesh in Koi embedded builder (init, status, enroll, unlock, destroy, revoke, promote)
- Added pond unlock, promote, ca.pem routes; Rake CLI gains unlock/promote/invite --passphrase
- mDNS TXT now advertises pond + https_port when pond is active; pond_active seeded at startup
- Added koi-certmesh, koi-crypto, tower dependencies to workspace and moss crate
- Documentation sweep: aligned all docs with implemented certmesh-backed Pond security

## 2026-02-15

- Embedded koi-embedded for unified mDNS: replaced platform-split mdns-sd + KoiClient HTTP with single KoiHandle
- Rewrote moss/mdns.rs (647→315 lines) and lantern/discovery.rs (197→137 lines) — no #[cfg] conditionals
- Slimmed koi_client.rs (900→50 lines): removed HTTP/SSE client, kept DiscoveredStone + is_lan_routable
- Removed mdns-sd direct dependency from moss, lantern, and common Cargo.toml

## 2026-02-14

- Upgraded workspace deps to Koi-aligned versions: axum 0.8, reqwest 0.12, thiserror 2, mdns-sd 0.17, tower-http 0.6
- Proposed Koi embedded integration: certmesh-backed Pond security, DNS, TLS proxy (docs/proposals)

## 2026-02-13

- Fixed systemd service: removed /etc/netplan from ReadWritePaths (doesn't exist on Debian 13, caused NAMESPACE crash loop)
- NewStone USB creator: auto-clear read-only flag on USB disk before writing
- **BREAKING**: Updated Koi mDNS client for refactored API (announce/unregister/heartbeat/subscribe/discover)

## 2026-02-12

- Fixed Koi mDNS client endpoints to use `/v1/mdns/*` paths
- Hardened Koi mDNS discovery with fallback browse, dedupe, and config knobs

- **`garden-moss install` / `garden-moss uninstall`** - cross-platform self-install from a single binary
  - Three tiers: online (GitHub download, future), offline (sibling package), USB (removable media)
  - Linux: systemd unit generation, binary + script deployment, directory creation
  - Windows: idempotent SCM registration (stop-delete-wait-recreate), recovery policy, firewall rules
  - Removable media detection: copies binary + package to permanent location before installing
  - Privilege checks: root (Linux) / Administrator (Windows) with clear error messages
  - Health check after start, success summary with management commands
  - Replaces Windows-only `take-root` / `install-service` (kept as hidden aliases)
  - Proposal: [moss-self-install](proposals/moss-self-install.md)
- **Nourishment: `Update::Moss` variant** - self-update type for Moss daemon alongside offerings and firmware
  - New `UpdateScope::Moss` for scoped update execution
  - Rake displays Moss updates in nourishment check output
- **main.rs restructured** - synchronous CLI dispatch before Tokio runtime (Koi pattern)
  - Install/uninstall run without async runtime, preventing accidental daemon startup

## 2026-02-06

- **Tools Domain implemented (greenfield)** - normative automation-grade tools projection and stream
  - New APIs: `GET /api/v1/garden/tools`, `GET /api/v1/garden/tools/stream`
  - New inter-Moss announcement: `TOOLS_BEACON` (`tools_beacon`) for offerings + seed banks
  - Unified `tool_fqid` identity (`{tool-type}:{fqid}`) with normalized tool projection and deltas
  - Event-driven readiness for `garden-rake find ... wishfully` (offering and capability-aware flows)
  - Capability state persistence + propagation through tools projection/beacons
- **Documentation added for tools domain**
  - Proposal status updated: `docs/proposals/moss-tools-domain.md`
  - Implementation report: `docs/archive/proposals/tools-domain-implementation.md`
  - User guide: `docs/guides/tools-domain.md`
- **Capability wishful syntax and semantics refined**
  - Canonical consumption format: `{offering}[{capability}[,{capability}...]]`
  - Multi-capability wishful requests now supported in one query (AND semantics)
  - `model/extension/module` treated as offering-local labels, not global nomenclature

## 2026-02-04

- **Fixed P2P discovery selecting wrong network interface** - Hyper-V/WSL virtual adapters were being selected over physical LAN
  - Root cause: IP-range blocklisting (`192.168.224.x`) was incomplete and brittle
  - Solution: MAC OUI-based detection using IEEE-assigned vendor prefixes
  - Switched from `if-addrs` to `network-interface` crate (provides MAC addresses)
  - Known virtual OUIs: Hyper-V (`00:15:5D`), VMware (`00:50:56`), VirtualBox (`08:00:27`), Docker (`02:42`), QEMU/KVM (`52:54:00`), Xen (`00:16:3E`)
  - Detection hierarchy: MAC OUI (primary) â†’ interface name patterns (secondary) â†’ Docker 172.17.x.x (tertiary)
  - Decision: [COMM-0003](decisions/COMM-0003-virtual-adapter-detection.md)
- **Fixed deploy.ps1 interface selection** - Added filtering for Hyper-V (`192.168.224+`), WSL, Docker Desktop ranges
  - Added priority tiers: `192.168.0-15.x` (priority 1) over higher subnets
- **Restored Windows-specific stone naming theme** - Platform-aware name generation
  - Linux: Nature theme (64Ã—64 = 4,096 names): `stone-golden-summit`, `stone-crystal-forest`
  - Windows: Stained glass/clarity theme (64Ã—64 = 4,096 names): `stone-pellucid-clarity`, `stone-crystalline-prism`
  - Windows theme evokes cathedral windows, light, transparency, and sacred spaces
  - Shared `generate_unique_name_from_dictionary()` helper for both platforms
- **Restored Windows first-boot DNS hostname setup** - Accidentally removed in commit 313e269
  - First-boot detection uses hardware-id cache existence (not flag file like Linux)
  - `ensure_windows_stone_name_config()` generates name synchronously before config loading
  - `set_windows_dns_hostname()` writes to registry (requires elevation)
  - DNS maintenance task runs on subsequent boots to retry failed DNS setup
  - Zero changes to Linux first-boot behavior
- **Fixed Windows chirping NetBIOS name instead of stone name**
  - Added `stone-name` cache file (`{data_dir}/stone-name`) for reliable persistence
  - `resolve_stone_name()` now checks cache before falling back to system hostname
  - Priority chain: CLI > config > **cached name** > system hostname > env > default
  - Prevents fallback to NetBIOS name (COMPUTERNAME) when config file has issues
- **Fixed Windows self-update port binding failure**
  - Root cause: Socket stayed in TIME_WAIT state after old process exited
  - Solution: Use `SO_REUSEADDR` when binding HTTP server socket
  - Allows new Moss instance to bind to port even if previous connection is in TIME_WAIT
  - Uses `socket2` crate for cross-platform socket options

## 2026-02-02

- **Unified Offering Model (Greenfield Refactor)** - merged dual-collection manifest system into single `Offering` struct
  - `SwEntry` + `OfferingManifest` â†’ unified `Offering` with mode-as-configuration
  - Mode support now derived from `Option<ManagedConfig>`, `Option<AdoptedConfig>`, `Option<BorrowedConfig>`
  - `OfferingRegistry` replaces `SwManifests`, `OfferingMetadata` replaces `SwFrontmatter`
  - Removed all 6 legacy type aliases (clean codebase, no backwards compatibility shims)
- **File renames to align with content**:
  - `common/manifests/sw.rs` â†’ `offering.rs` (contains `Offering` model)
  - `common/manifests/offering.rs` â†’ `detection.rs` (contains detection types for adopted mode)
- **Removed legacy code from ManifestRegistry**:
  - Deleted `offering_manifests` HashMap field
  - Removed `get_offering_manifest()`, `add_offering_manifest()`, `add_offering_manifests()`, `load_legacy_offering_manifests()`
- **Deleted orphaned duplicate files**:
  - `moss/infra/manifests/hw.rs` - duplicate of `common/manifests/hw.rs`, never imported
  - `moss/infra/manifests/sw.rs` - orphaned after unification
- **Updated all consumers**: DetectionOrchestrator, adoption APIs, embedded manifest loading now use unified model
- **Fixed adopted offering detection** - Ollama and other adopted offerings now detected correctly
  - Strip UTF-8 BOM from embedded manifest files before parsing
  - Removed `#[serde(flatten)]` from `DetectionRule.config` to match nested YAML format
  - Changed `DetectionMethod` enum from `lowercase` to `snake_case` (`http_probe` not `httpprobe`)

## 2026-01-29

- **Package Structure v2.0** - Simplified to mirror target filesystem exactly
  - Package now has just `bin/` (â†’ /usr/local/bin) and `lib/` (â†’ /var/lib) folders
  - Deploy is now two `cp -r` operations instead of multiple conditional blocks
  - Removed `dependencies` block from dist.json (Companions install deps at runtime)
  - Updated `moss-update-helper.sh` and `NewStone-linux-x64.ps1` to use new structure
- **Timezone/NTP Configuration** - Stones now sync timezone on deploy
  - New `garden.conf` with timezone setting (default: America/New_York)
  - `moss-update-helper.sh` applies timezone and enables NTP on upgrade
  - `debian-preseed.template` applies timezone on first boot
- **Path Cleanup** - Removed all stale `/etc/zen-garden/templates` references
  - Manifests at `/var/lib/zen-garden/manifests/{sw,hw}/`
  - Companions at `/usr/local/bin/companions/{Companion}/`
  - Fixed `RUNTIME_MANIFESTS_DIR` and `RUNTIME_HW_MANIFESTS_DIR` constants
- **NewStone-linux-x64.ps1: Package-based deployment** - USB creator now extracts from Linux package directly
  - Single source of truth: binaries, Companions, manifests, scripts all from `dist/packages/*.tar.gz`
  - Matches deployment layout used by `garden-upgrade.sh` and `moss-update-helper.sh`
  - Includes `dependencies.json` for post-install Companion dependency resolution
- **Test: storage.object_roundtrip** - new probe test verifying PUT/GET/DELETE object lifecycle in seed banks
- **Companion SDK: System Dependency Management** - Companions can auto-install missing dependencies
  - New `garden_companion_sdk::dependencies` module with `ensure_dependencies()` helper
  - `SystemDependency::apt_package(pkg, binary)` for declaring apt package requirements
  - Cricket now auto-installs `alsa-utils` (aplay/amixer) on first run if missing
- **Companion Endpoints: ApiResponse Wrapper** - consistent response format across all Companion APIs
  - All Companion endpoints now return `ApiResponse<T>` with `data` field
  - Companion detail endpoint includes `running`, `port`, `pid` fields alongside manifest
- **Companion Auto-Start with State Persistence** - Companions now start automatically on boot
  - Moss auto-starts all registered Companions unless explicitly disabled
  - New `Companion-state.json` ledger persists enabled/disabled state across restarts
  - `POST /companions/:id/down` now disables Companion (won't auto-start until `/up`)
  - `POST /companions/:id/up` re-enables Companion for auto-start
  - New `scan_and_autostart()` replaces simple `scan()` at startup
- **Storage Cache as Unified View** - storage_cache now serves as boundary between local and remote storage
  - Local seed banks are self-registered into storage_cache at startup
  - Remote banks populate via STORAGE_BEACON announcements
  - `/api/v1/stone/storage` now returns `garden_banks` field with all known banks across garden
  - Storage API can easily route requests: local bank â†’ local functions, remote bank â†’ proxy to owning stone
  - Added `update_local_storage_cache()` and `update_and_broadcast()` helpers to beacon module
- **API Surface Reorganization (Greenfield)** - clean semantic separation of stone-local vs garden-wide endpoints
  - All stone-local operations now under `/api/v1/stone/*`:
    - `/api/v1/stone/offerings/*` - local catalog and offering management
    - `/api/v1/stone/services/*` - local service management
    - `/api/v1/stone/capabilities` - hardware capabilities (moved from root `/capabilities`)
    - `/api/v1/stone/metrics` - Prometheus metrics (moved from root `/metrics`)
  - Garden-wide operations under `/api/v1/garden/*`:
    - `/api/v1/garden/services?q=` - service discovery across all stones (used by `rake find`)
    - `/api/v1/garden/topology` - garden topology overview
    - `/api/v1/garden/nourishment` - orchestrated updates
  - `/health` remains at root (industry standard)
  - Updated Rake to use new endpoints throughout
  - Updated garden-probe tests for new API structure
  - Updated ARCHITECTURE-REFERENCE.md with complete endpoint documentation
- **Storage Beacon Protocol (STORAGE-0003)** - event-driven storage announcements for cross-stone routing
  - New `STORAGE_BEACON` announcement type (~150-400 bytes, 10x smaller than chirps)
  - Broadcast on: seed bank mount/unmount, visibility change, new stone online
  - All stones lurk-listen and maintain separate `StorageCache` referencing topology
  - Added `StorageBeacon`, `SeedBankAnnouncement`, `StorageAccess` types to garden_common
  - Added `StorageCache` domain module with beacon update/prune operations
  - Added `broadcast_beacon()` and `broadcast_if_has_storage()` infra functions
  - Coordinator handles `STORAGE_BEACON` reception and updates storage cache
  - New stone trigger: when `STONE_CHIRP` received, storage-having stones broadcast beacon
  - Storage cache maintenance task runs every 60s to prune stale entries
- **Directory listing depth parameter** - recursive object listing support
  - `GET /api/v1/stone/storage/bank/:id/*path?depth=N` for N levels of subdirectories
  - `?depth=1` (default) immediate children, `?depth=all` or `?depth=-1` for full recursion
  - Returns `DirectoryListResponse` with entries array (name, type, size, modified)
- **STORAGE-0001 spec updates** - added Section 5.9 Object Operations with depth docs
- **ARCHITECTURE-REFERENCE.md** - updated Seed Bank Endpoints with STORAGE-0002/0003 structure

## 2026-01-28

- **Storage API restructured per STORAGE-0002** - dual-layer API for native and S3 gateway
  - Native Bank API: `/api/v1/stone/storage/bank/:id/*path` - ApiResponse JSON format
  - S3 Gateway: `/api/v1/stone/storage/s3/:bucket/*key` - S3-spec XML/raw bytes
  - Added `list_buckets()` method to ObjectStore for S3 ListBuckets operation
  - Cleaned up duplicate handlers (renamed from `*_seed_bank_v1` to `*_bank_v1`)
  - Rake commands updated to use new endpoints with proper ApiResponse parsing
- **Documentation alignment with service resolution and storage specs** - comprehensive update across all docs
  - **Protocol vs Offering distinction** - clarified: s3/mongodb = protocols (wire format), minio/mongodb = offerings (software)
  - **Connection string format** - unified: `zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>]`
  - **Environment variables** - standardized `ZG_` prefix (replacing `GARDEN_` and `ZEN_GARDEN_*`)
  - **Config file naming** - standardized on `moss.toml` (was `garden-moss.toml`)
  - **mDNS TXT records** - added fields: instance, admission, protocols, protocol_default
  - **Resolution API** - added `GET /api/v1/resolve?offering=&protocol=&instance=` endpoint spec
  - **Storage API** - added seed bank management and S3 gateway endpoints to specs
  - Updated 15+ documentation files for consistency with proposal specs

## 2026-01-27

- **Portrait at-a-glance panel** - Hero now shows stone uptime (ðŸª¨), Moss uptime (ðŸŒ¿), and offerings status dots
  - Added `moss_uptime` field to PortraitIdentity API response
  - Stone uptime = system uptime (how long machine running), Moss uptime = daemon uptime
  - Offerings glance shows count by status: running (green), stopped (gray), error (red)
- **Stone Portrait landing page** - SPA at root URL showing stone identity, metrics, offerings, and horizon
  - New endpoint: `GET /` returns Alpine.js SPA (embedded HTML at compile time)
  - New endpoint: `GET /api/v1/stone/portrait` returns JSON for reactive updates
  - Sections: Identity (name, role, version), Foundation (CPU/memory/disk), Offerings, Companions, Horizon
  - Stone color derived from stone_id hash for unique identity across garden
  - Vellum aesthetic with dark mode support, 4-second polling for "breathing" updates
  - See: docs/decisions/PORTRAIT-0001-stone-landing-page.md
- **API Manifest system** - structured endpoint documentation like CommandManifest for Companions
  - Created `garden_common::api_manifest` module with EndpointSpec, ApiManifest types
  - New endpoint: `GET /api/v1/manifest` returns live API documentation from Moss
  - New command: `garden-rake api` displays formatted API reference with curl examples
  - Usage: `garden-rake api`, `garden-rake api --category offerings`, `garden-rake api /api/v1/services`
  - Single source of truth for endpoint metadata (method, path, params, examples, notes)
- **Driver specification v2.0** - comprehensive rewrite with real-world scenarios and DX improvements
  - Added multicast-first transport architecture (239.255.42.99), directed broadcast fallback
  - Real-world scenarios: app startup, hardware failure reconnect, topology dashboard, cross-subnet
  - Complete implementation examples: Python discovery, tending with fallback, resilient requests
  - Type definitions: TypeScript interfaces for all API types (discovery, services, hardware, topology)
  - Troubleshooting guide: firewall, multicast, multi-homed systems (WSL/Hyper-V), slow discovery
- **Documentation consistency fixes** - updated 6 docs with correct ports and election delay formula
  - Ports: 3001â†’7185 (Moss), 3004â†’7184 (discovery), 3000â†’7186 (Lantern), 3002â†’7186 (Lantern)
  - Election delay: corrected `* 10` (0-2550ms) to `* 30` (0-7650ms) per implementation
  - Updated format string: `stone_name + request_id` â†’ `election:{stone_id}:{request_id}`
  - Affected: discovery.md, moss-daemon-lifecycle.md, rake-commands.md, config.md, connection-strings.md, glossary.md, ports.md
- **garden-companion-sdk crate** - shared infrastructure for Companions (DDD/SoC)
  - Created `src/companion-sdk/` with CommandHandler trait, CompanionRuntime, SSE client
  - Companions focus on domain logic only, SDK handles: HTTP server, shutdown, signals
  - Re-exports: CompanionConfig, CommandResult, EventHandler, SseEvent, async_trait
  - Standard endpoints: POST /command, POST /shutdown, GET /health
  - Refactored Cricket to use SDK - removed 200+ lines of boilerplate (command.rs, sse.rs)
- **Embedded asset framework for Moss** - manifests and Companions compiled into binary for portability
  - Added `rust-embed` v8 dependency to Moss for compile-time asset embedding
  - Created `src/moss/embedded/manifests/` - moved manifests from repo root for binary embedding
  - Created `src/moss/src/infra/embedded.rs` - overlay loading (filesystem > embedded), asset extraction
  - Taxonomy dictionary loading via embedded assets with filesystem overlay
- **Search API moved to Moss** - Rake is now a thin client, all search logic server-side
  - Added `GET /api/v1/offerings/search?q={query}&prefer={prefs}&limit={n}` endpoint to Moss
  - Created `garden_common::offerings` module: TaxonomyDictionary, OfferingSearchResponse types
  - Moved `normalize_tokens()`, `token_matches_category()`, `offering_relevance_score()` to Moss
  - Rake now calls Moss search API instead of local scoring - removed 60+ lines of search logic
  - Tests for scoring functions moved from Rake to Moss

## 2026-01-26

- **Companion port ledger system** - Moss-managed persistent port assignments (base 7187, range 7187-7199)
  - Created PortLedger: load/save to `{data_dir}/companion-ports.json`, incremental assignment from base 7187
  - Moss passes `--port {assigned}` to Companions during both `--dump-commands` and runtime startup
  - Command routing: Rake â†’ Moss:7185/api/v1/stone/companions/{id}/command â†’ Companion:{assigned_port}/command
  - Removed computed port logic from command_manifest, Cricket now requires port from Moss
  - Tested end-to-end: Cricket assigned 7187, plays audio via `hey tell cricket play stone-online`
- **Companion registry & service discovery** - Companions auto-discovered via `--dump-commands` protocol
  - Added `Companions_dir()` path function: `/usr/local/bin/companions/` (Linux), `.zen-garden/companions/` (Windows)
  - Added `CommandManifest::check_dump_commands()` helper for Companion main.rs
  - Created `infra/Companions.rs`: scans Companions folder, spawns `--dump-commands`, caches manifests
  - Updated Moss API: GET /stone/Companions, GET /stone/companions/:id, POST :id/command, POST refresh
  - Updated Rake hey.rs: fetches CommandManifest from Moss, displays rich help with examples
  - Cricket now implements `--dump-commands` (6 commands: select, volume, list, show, play, stop)
- **Cricket audio Companion implemented** - full Companion framework and Cricket crate with 180 CC0 samples
  - Expanded audio library: 42 â†’ 180 samples (5x growth, emphasis on notifications as requested)
  - Added garden_common::Companion module: CompanionCommandRequest/Response, CompanionManifest types
  - Added Moss endpoints: GET /api/v1/stone/companions, POST /api/v1/stone/presence/command
  - Created garden-cricket crate: 4-channel mixer (rodio), tune system (zen-garden/mr-robot/lo-fi-ops)
  - Created Rake hey-tell command: natural language Companion control (`hey cricket, play zen-garden`)
  - Implemented SSE client for presence stream, command server (port 7188), mixer with Send+Sync safety
  - Attribution maintained: full credit in attribution-extended.json despite CC0 license
- **Cricket & Companion Framework specs complete** - universal service communication layer designed
  - Created Companion-COMMAND-PROTOCOL.md: synchronous command flow via Moss proxy (5s timeout)
  - Created Companion-SERVICE-REGISTRY.md: service registration, manifests, lifecycle management
  - Created HEY-TELL-SYNTAX.md: Rake command grammar (`garden-rake hey tell {Companion} {cmd}`)
  - Created CRICKET-SPEC.md: Cricket implementation details (rodio, 4-channel mixer, tune system)
  - Created audio-sample-library.json: 52 CC0 samples from Freesound.org for official tunes
- **Cricket audio Companion proposal complete** - comprehensive spec with 6-expert specialist team assessment
  - Created CRICKET-0001-audio-Companion-spec.md: full design (4-layer audio, event mappings, config schema)
  - Created CRICKET-IMPLEMENTATION-ROADMAP.md: 3-phase build plan (6-8 weeks to v0.1.0)
  - Created CRICKET-EXECUTIVE-SUMMARY.md: stakeholder reference document
  - Validated against PRESENCE-0001: zero protocol deviations, pure consumer pattern
  - Objective alignment confirmed: "make home lab infrastructure feel intimate, tactile, and real"
- **METRICS-0001: Unified storage metrics** - eliminated deprecated StorageDevice struct, detect_storage() function, and HardwareCapabilities.storage field
  - Removed ~200 lines of redundant storage detection code (detect_storage_windows/linux functions)
  - Changed StoneResources.disk (single DiskMetrics) to storage (Vec<StorageMetrics>)
  - All storage data now from live metrics (30s refresh), no stale boot-time usage percentages
  - Handles hot-swap drives naturally (storage inventory refreshes every 30s)
  - Fixed observe/status commands: removed stale static storage display, replaced by live /metrics endpoint (future work)
- Fixed Windows self-update cleanup: corrected temp filename (garden-moss-new.exe â†’ garden-moss-temp.exe) with logging
- Fixed 38 manifest snippet files: converted port format from strings to tuples ([host, container])
- Fixed ServiceConfig struct: changed ports from Vec<String> to Vec<(u16, u16)> for direct tuple deserialization
- **Implemented Windows self-update (Phase 1)**: spawn-temp-process pattern for package-based updates
  - Added `spawn_windows_updater()` to copy moss â†’ garden-moss-temp.exe and spawn --finalize-update
  - Updated deploy_stone_v1 API to call Windows updater before shutdown
  - Added `--cleanup-updater` CLI flag for post-update cleanup
  - Added `cleanup_updater_process()` to remove temp binary after successful update
  - Added `update_transaction.rs` module for future transaction log implementation (Phase 2)
- Fixed Windows paths to maintain consistent manifest structure: `.zen-garden/manifests/{hw|sw}` (was using separate hw-manifests/, templates/ dirs)
- Windows self-update implementation designed: spawn-temp-process pattern with transaction log, rollback safety, automatic recovery
- Windows deployment analysis complete: identified missing self-update mechanism (Linux has systemd ExecStartPre scripts, Windows had none)
- Added UDP message deduplication in p2p.rs - GUIDv7 msg_id with 5s TTL cache to prevent duplicate processing from multicast/broadcast multi-path delivery
- Added `docs/reference/cost-analysis.md` - realistic cost comparison: Zen Garden on 3Ã— Dell Wyse 5070s vs AWS/Azure (~90% savings)
- Added `docs/philosophy/staying-focused.md` - north star document to prevent scope creep and maintain focus on core mission (e-waste reclamation, small business ownership, removing barriers)
- **Documentation cleanup**: Removed all Tier 2/Deep Pond references from foundational documentation
- Rewrote POND-0001 protocol spec: removed certificates, resurrection, individual revocation (Tier 2 features)
- Updated glossary.md: new definitions for Pond, Keystone, Cornerstone, Stone Admission, Drain aligned with P2P model
- Rewrote security/overview.md: removed Security Tiers section, simplified to single threat model
- Rewrote security/pond-setup.md: removed certificate management, added baptism/drain workflows
- Updated security/threat-analysis.md: added note about Tier 2 references being historical, simplified vuln matrix
- Updated maintainers.md: removed Mode 3 Deep Pond section, simplified threat model
- Updated roadmap.md: removed Tiers table (Open Garden/Garden Pond/Deep Pond)
- Added SECURITY-0004 decision: Tier 2 (Deep Pond) deferred until real demand exists
- Updated POND-0001 with Design Decisions section documenting unicast baptism, Tier 1 security value, shared secret rationale
- Changed baptism protocol from broadcast to unicast direct delivery (topology-based, per-stone addressing)
- Updated SECURITY-0001 status to Superseded (Tier 2 timeline removed)
- Added POND-0001 protocol specification for Pond security layer (baptism, invitation, drain protocols)
- Updated roadmap.md to reflect completed Phase 1 (discovery, topology, nourishment v0 all implemented)
- Optimized copilot-instructions.md for AI consumption - removed verbosity, emojis, conversational language (50% reduction)
- Added automatic changelog update instructions for AI agents in copilot-instructions.md (when to add, what format, commit workflow)
- Fixed syntax error in `delete_service_v1()` - Path extractor had `Path(String>` instead of `Path<String>`
- Fixed `remove` command to actually stop and remove containers (was only removing from registry, causing auto-adoption loops)
- Added changelog maintenance guidelines to copilot instructions for AI agents

## 2026-01-25

- Implemented multicast-first UDP discovery (239.255.42.99:7184) with directed broadcast fallback to solve multi-homed Windows 11 failures
- Added per-interface sender sockets to prevent OS routing packets through wrong interfaces (WSL/Hyper-V)
- Added virtual Companion detection and filtering (skips veth, docker, vmnet, vboxnet, hyperv, wsl interfaces)
- Added configurable discovery transport via environment variables (DISCOVERY_PORT, DISCOVERY_MCAST_GROUP, DISCOVERY_ENABLE_BCAST_FALLBACK)
- Reduced topology offline threshold from 90s to 45s (1.5 chirp cycles) for faster stale stone detection
- Added automatic topology maintenance task (runs every 30s, marks stale stones offline, evicts old entries)
- Fixed topology cache accumulating duplicate stone entries with different IDs

## Unreleased (Rake UI/UX Improvements)

- Progressive discovery display â€” stones appear as discovered with response times
- Streaming progress updates for container installations via SSE polling
- Garden vitality language: `[thriving]`, `[dormant]`, `[needs attention]`
- Standardized spatial prepositions: "on" (hosting), "at" (targeting), "present on" (topology)
- Wall-clock timestamps `[HH:MM:SS]` in Watch command
- Confirmation prompts for destructive operations with `--force` bypass
- Deprecated `status` command (use `observe` or `tend` instead)
- **BREAKING**: Removed `context` command (use `tend` instead)
- **BREAKING**: Changed `discover_all_moss()` to callback-based streaming API

---

See also:

- [Discovery Transport spec](specs/discovery-transport.md)
- [Topology Cache spec](specs/topology-cache.md)
- [COMM-0004: Multicast-First Discovery](decisions/COMM-0004-multicast-first-discovery.md)
