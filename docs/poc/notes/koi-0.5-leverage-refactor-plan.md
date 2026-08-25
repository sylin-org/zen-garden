# koi 0.5.0 leverage — zen-garden refactor plan

> Working note (untracked stash). Drafted 2026-06-22 from a 10-axis cross-repo analyze→verify
> workflow against koi 0.5.0 (`../koi`, path-dep, unpinned). Each verdict is grounded in real
> code in both repos and adversarially re-checked. **Method matters here:** the verify stage
> overturned several plausible-but-wrong adoption claims, so the conclusions resist the
> "koi is miles ahead, rip out zen and adopt everything" reflex.

## Headline

koi's service surface genuinely exploded (koi-serve, koi-runtime, koi-proxy, koi-dns, koi-health,
koi-mcp, koi-udp, the ADR-020 posture/envelope facade, the capability cards). **But most of it is
the _inverse_ of what zen needs.** The recurring shape:

- koi **observes / bridges / serves-its-own-domains** (watch containers to auto-wire koi's mDNS/DNS;
  bridge a container UDP socket; L4 TLS passthrough; serve koi's own router).
- zen **controls / meshes / serves-its-own-domains** (drive container lifecycle; multicast LAN gossip;
  L7 model-aware routing; serve 281 domain routes).

They are adjacent layers, not the same layer. So the high-value leverage is **narrow and
concentrated in one place — the trust plane** — where koi's ADR-020 envelope is materially stronger
than zen's hand-rolled chirp crypto (which today is *both spoofable and replayable*). Almost
everything else is **keep-zen** or small hygiene. And there is a **P0 build-breaker to clear first.**

## Scorecard

| Axis | Verdict | Effort | LOC Δ | One-line reason |
|---|---|---|---|---|
| Crypto / trust (chirp envelope) | **delegate-partial** | L | −60..−150 | koi `verify_envelope` closes a spoof + replay + silent-downgrade hole zen hand-rolled |
| Discovery / mDNS | delegate-partial | S | −40 | already delegated; dedup the copy-pasted `extract_stone_from_record` |
| Serving / composition | deepen-existing | M | 0 | boundary already correct; reuse koi posture in `/v1/status`, config hygiene |
| Secrets / vault | (already adopt-full) | — | 0 | `secrets.rs` is a one-line `pub use koi_crypto::vault` — this is the model |
| Container runtime | keep-zen | S | 0 | koi-runtime is observe-only; zen needs a control plane — no overlap |
| Reverse proxy / gateway | keep-zen | S | 0 | koi-proxy is L4 passthrough; zen's ollama gateway is L7 model-aware |
| Health / observability | keep-zen | S | 0 | koi = peer-liveness; zen = offering+host health — different problems |
| DNS | keep-zen | S | 0 | DNS-0002 removed koi-dns after a prod incident; zen runs no DNS server |
| UDP transport | keep-zen | S | 0 | koi-udp rejects multicast/broadcast; STACK-0001 D4 already decided this |
| MCP / AI tooling | keep-zen / defer | S | 0 | koi-mcp hardcodes its tool set; zen has no MCP code; net-new, no consumer |

Net change surface: **one real refactor (trust), two small deepenings (discovery, serving), a P0
fix, and a hygiene sweep.** Seven of ten axes are confirmed keep-zen.

---

## P0 — Clear the build-breaker (do first; gates everything)

`koi-truststore` was **deleted** from koi 0.5.0 (commit d23f6bf, ADR-019) and replaced by the
published crates.io crate **`os-truststore = "0.0.2"`**. zen still references the dead crate, so
zen **will not build against the current `../koi` checkout**:

- `Cargo.toml:99` — `koi-truststore = { path = "../koi/crates/koi-truststore" }` (missing dir)
- `src/rake/Cargo.toml:11` — `koi-truststore.workspace = true`
- `src/rake/src/enrollment.rs:118,124` — `koi_truststore::install_ca_cert` / `is_ca_installed`
- `Cargo.lock` — stale entries (~1688, ~2874)

**Fix:** depend on `os-truststore = "0.0.2"` directly (a *published* dep — strictly better than the
old koi path dep; removes this capability from the dogfooding-churn surface). Repoint
`enrollment.rs`:

- `install_ca_cert(pem, label)` → `os_truststore::Cert::from_pem(pem)` then
  `os_truststore::Install::new(&cert).label(label).run()` (mirrors koi `trust.rs:install_cert`).
- `is_ca_installed(label)` has no direct `os-truststore` equivalent — koi tracks its own installed
  roots in `state/trust.json` (`koi_config::state`). zen should track its own installed-root state
  the same way (a small zen-side ledger), **or** drop the check if it is only an idempotence guard
  that `Install::run()` already handles. Decide during implementation.

Remove the root + rake `koi-truststore` entries, refresh `Cargo.lock`, and confirm
`cargo check --all` is green. **Every later phase's verification depends on this.** While here,
re-confirm the rest of zen still compiles against koi 0.5.0 (last synced at 0.4.2 in `aec0f024`).

---

## Phase 1 — Trust delegation: chirp signing → koi Envelope (the real refactor)

**Why this is the one high-value change.** zen's UDP-7184 chirp sign/verify is hand-rolled on the
low-level `koi_crypto::signing` primitives at `src/moss/src/bootstrap/run.rs:1633-1700`, and it is
weak in three independent ways:

1. **Spoofable** — the verifier checks the signature against the sender's *self-asserted*
   `sender_cert` public key with **no chain to the pinned CA** (`run.rs:1694-1696`). Any peer can
   forge an identity by attaching its own key.
2. **Replayable** — there is **no freshness/nonce/timestamp** binding at all.
3. **Silent downgrade** — `return true // Accept unsigned during transition` (`run.rs:1681`).

koi 0.5.0 (ADR-020) ships exactly the fix, in code and tested: `koi_certmesh::envelope::{build_envelope,
verify_envelope}` (sync free functions — note: the **logic is in koi-certmesh**, not `koi_common`,
which holds only the wire types `Envelope`/`Assurance`/`Sig`). `verify_envelope` chains the carried
leaf to the pinned CA, checks expiry + best-effort revocation, verifies ES256 with a
`koi-envelope-v1` domain-separation prefix, and enforces a ±300s freshness window — returning a
typed `Assurance` whose `identity()` is the single success door.

**The work:**

1. Replace the `p2p::set_envelope_enricher` closure (`run.rs:1642-1651`) to wrap the payload via
   `build_envelope` and carry the `Envelope` as the signed unit (instead of stuffing separate
   `signature`/`sender_cert` fields into `UdpAnnouncement`).
2. Replace `set_envelope_verifier` (`run.rs:1672-1697`) to `verify_envelope` → `Assurance`, gating
   acceptance on `Assurance::identity()` and an **explicit, typed** transition policy (e.g. accept
   `PostureLevel::Open` peers during rollout) so any downgrade is observable, not silent.
3. **Async bridge (the M→L driver):** the p2p hooks are *sync* closures
   (`ENVELOPE_ENRICHER`/`ENVELOPE_VERIFIER`, `src/common/src/infra/communications/p2p.rs:393,402`,
   invoked at `:925,:1032`). Prefer the **sync** `build_envelope`/`verify_envelope` to avoid an
   async bridge entirely; capture the CA PEM (already loaded at `run.rs:1661-1670`) and, ideally, a
   live roster snapshot at set-time (a sync closure can't read the live revocation set, so its
   revocation check will be weaker than `CertmeshCore::verify` — acceptable, document it). If an
   async path is unavoidable, change the hook type to async-returning; **do not** `block_on` inside
   the Tokio runtime.

**Two pre-cutover checks (verify-stage findings):**

- **Identity resolution:** zen's chirp leaf loads from `{data_dir}/koi/certs/{stone.name}/key.pem`,
  but `CertmeshCore`'s identity reads `{data_dir}/certs/{OS-hostname}/`. They match **only iff
  `stone.name == OS hostname`** (zen deploys usually align these, but confirm — Android/custom-name
  stones may not).
- **Wire-format break:** moving `{signature, sender_cert}` → a serialized `Envelope` is a chirp
  protocol change. rake (and the C# Koan client, if it ever speaks signed UDP) must cut over
  atomically. Greenfield/no-shim is the operator's stance, so a clean cutover is fine — but it is a
  coordinated, observable change, not a moss-local edit. **Keep signed UDP ZG-internal**
  (STACK-0001 D4): adopting koi's `Envelope` as an *internal* payload format does not export the
  mesh; do not promote UDP-7184 to a cross-project contract without an architect decision.

**Adjacent, smaller, same domain:** inbound HTTP **caller-identity** is still unbuilt
(`tls.rs:101` is `with_no_client_auth()`, "mTLS deferred to Phase 4"). koi's only auth middleware
gates on the mTLS-injected `ClientCn`, **not** an envelope signature, so a signature-based
caller-identity middleware on `:7183` is **net-new zen code on top of `verify_envelope`** (not a
koi adoption). Keep `tls.rs`'s server-auth listener (the browser-safe rung) and keep the same-port
HTTP↔HTTPS flip **zen-owned** — koi deliberately does **not** ship it (the dual-mode prompt scoped
it to "zen owns its listener; koi owns the contract + TLS material"). Required-mTLS must **not** go
on the browser-facing port. This subsumes the now-superseded `koi-mtls-library-prompt.md` and
`security-rake-mtls-plan.md`.

---

## Phase 2 — Hygiene & dedup (small, low-risk, independent)

- **Discovery dedup (S, ~−40 LOC):** `extract_stone_from_record` is copy-pasted near-identical in
  moss (`domain/discovery/mdns.rs:464-496`) and lantern (`tasks/discovery.rs:68-103`). Lift one
  canonical extractor; optionally collapse the two browse loops onto a shared helper. **Caveats:**
  (a) `garden-common` has **no** koi dep today and the natural home
  (`common/src/infra/koi_client.rs`) declares itself above the transport layer, while
  `ServiceRecord` is a transport type — so either accept adding `koi-embedded` to garden-common
  (widens the churn blast radius into the lowest layer) or share via a smaller seam; decide
  deliberately. (b) Preserve the moss/lantern **asymmetry** (lantern handles `Removed`→offline
  immediately and does not self-filter; moss self-filters its own name and only logs `Removed`) via
  caller-supplied closures. (c) Do **not** delete the certmesh CA backfill
  (`discovery/mdns.rs:233-320`) — it is load-bearing because koi shed `CertmeshCore::ca_announcement`
  in the P08 diet.

- **Serving deepen (M, 0 LOC):** the boundary is already right (koi via the `koi-embedded` Builder;
  zen owns its axum host — koi-serve **cannot** co-mount zen's 281 routes and is reserved for koi's
  own top-level hosts). Deepening is config-hygiene + single-source-of-truth: drive moss
  `/v1/status` posture from `KoiHandle…certmesh_status()` (zen already calls the accessor at
  `run.rs:617-620`) instead of a parallel `pond_active` AtomicBool; audit the two HTTP servers
  (koi-embedded's adapter on `KOI_HTTP` is loopback-only diagnostic — zen never sets
  `.announce_http`, so disabling `.dashboard` removes only the local koi dashboard, not a discovery
  contract). Add a **SURFACES.md tripwire** so future audits don't re-propose a koi-serve router
  merge.

- **DNS hygiene (S):** `installer/windows.rs:236` builds firewall config with `dns_enabled:true`,
  over-reporting port 53 that moss never binds (bootstrap sets `.dns_enabled(false)`) → set false.
  Reconcile DNS-0002's "moss writes nothing to `resolved.conf.d/`" against the surviving
  mDNS-forwarding-only `configure_resolved_for_containers` (`run.rs:1226`).

- **UDP hygiene (S):** `run.rs:576` sets `.udp(true)` but `koi_handle.udp()` is **never consumed**;
  flip to `.udp(false)` unless a zen-managed container actually bridges *into* the mesh (verify
  first — that is the STACK-0001 D4 use case).

- **Proxy cosmetic (S):** `orchestrators/common/src/gateway.rs` is registration clients, not a
  proxy — consider renaming to `registration.rs` so it stops reading as a gateway.

---

## Phase 3 — Deferred / net-new (no driver today; record, don't build)

- **Prometheus text exposition for moss** — only if external scraping is wanted. koi has none
  either (only JSON snapshots + `/v1/sd/prometheus` service discovery). zen's **own ai orchestrator
  already ships a `# HELP`/`# TYPE` emitter** (`orchestrators/ai/src/http/metrics.rs:52-113`) — copy
  that pattern into moss's Metrics aggregate; do not adopt koi-health.
- **AI-as-MCP-tools** — defer until a consumer asks. koi-mcp can't host external tools (hardcoded
  set, no registration API). If ever built: a standalone `rmcp` server inside the ollama
  orchestrator (keeps the crate boundary koi-free, STACK-0001-aligned). Do **not** archive `ai`.
- **Confidentiality rung (`seal`/`open`)** — koi ships passthrough on open nodes today; a future
  group-key encryption rung is a zero-change upgrade once chirp payloads ride the `Envelope`.
- **koi `koi.*` label auto-wire** — optional: zen could stamp `koi.*` labels on managed containers
  and let a koi daemon auto-wire mDNS/DNS/proxy, keeping zen's control plane and koi's observe plane
  cleanly separated. No driver now.

---

## Keep-zen ledger (record as tripwires; do not re-litigate)

| Surface | Why it stays zen-owned | Canon anchor |
|---|---|---|
| Container control plane (`src/moss/src/docker/**`, ~2.8k LOC) | koi-runtime is observe-only; no lifecycle verbs | — |
| L7 ollama router (`orchestrators/ollama`) | koi-proxy is L4 passthrough; defers L7 to Caddy/Traefik | STACK-0001 (ollama = AI contract) |
| Offering + host health | koi-health is peer-liveness; different data sources/consumers | — |
| DNS (no server) | re-adopting koi-dns reverses an accepted ADR + reintroduces a prod failure | DNS-0002 |
| UDP-7184 mesh transport | koi-udp rejects multicast/broadcast; bridges *into* the mesh | STACK-0001 D4 |
| HTTP client layer (`StoneApi`, `client_builder`) | koi-client is a blocking-ureq daemon client | — |
| S3/content HMAC (`s3_presign.rs`, garden HMAC, digests) | koi-crypto has no HMAC/SigV4/presign surface | — |

---

## Cross-cutting risks

- **koi pre-1.0 churn, unpinned path dep.** The `koi-truststore` deletion is the *second* koi crate
  move to break zen this cycle (after the 0.4.2 certmesh diet, `aec0f024`). The ADR-020
  envelope/posture types are brand-new. Re-verify every koi signature against the live `../koi`
  before and after each migration; keep the raw `koi_crypto::signing` path available as the stable
  fallback until the `Envelope` path is proven across one koi bump. (Project memory:
  koi-dogfooding-dependency.)
- **The freshness window is freshness, not replay protection.** `verify_envelope` enforces ±300s but
  has **no seen-nonce cache** — a captured chirp can be replayed within 300s. Strictly better than
  zen's current *zero* protection, but do not advertise it as full replay protection.
- **Verification gate (every phase):** `cargo check --all` + `cargo test --package moss` +
  `cargo clippy -- -D warnings`, against the live `../koi`. Leave SURFACES.md tripwires per the
  rotation contract.

## Suggested sequence

1. **P0** build-fix (`os-truststore`) — unblocks the build and all verification.
2. **Phase 1** trust delegation — the one high-value refactor; closes a real auth hole.
3. **Phase 2** hygiene & dedup — opportunistic, can interleave or follow.
4. **Phase 3** — deferred; only on a real driver.
