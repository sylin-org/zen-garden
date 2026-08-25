# koi 0.5.1 migration — in-progress state (resume here)

> Working note. 2026-06-22. zen is being migrated to compile against koi **0.5.1** (path deps,
> dogfooding — operator chose keep-path-deps, NOT pin-to-published). Build is **NOT green yet** —
> one migration chunk (CSR-only enrollment) remains. Nothing committed yet.

---

## ⏩ HANDOFF (session 3 end, 2026-06-24) — read this first

**State:** branch `fix/snapshot-scheduler-disposal`, **tracked tree CLEAN**, **NOT pushed**, HEAD
**`1fb8205a`**, all green against koi **0.9.0** (path-dep dogfooding). Stage 4 (envelope ENFORCEMENT)
is now CODE-COMPLETE through the clear-plane end-state. Commits added since the session-2 handoff
(on top of `7b5231ef`…`ca4132ed`):

```
1fb8205a feat(pond): clear-plane end-state under enforce — retire mTLS listener (Stage 4)
4a8efa09 feat(pond): enforce /api/v1/admin/ too (decision #3)
e3d3aa57 fix(rake): sign raw-client mutations via StoneApi::send_signed_raw (#5/#7)
37d164cb chore(deps): sync Cargo.lock for koi 0.9.0 (node_has_identity member-fix)
9654d730 feat(pond): Stage 4a — sign inter-stone Moss→Moss mutations
d6362397 feat(pond): Stage 4b — clear-plane envelope enforcement middleware
```

**DONE & green (Stage 4):**
- **4a (`9654d730`)** — inter-stone Moss→Moss mutations sign (`domain/security/signing.rs`:
  `core_for_signing`, `inter_stone_envelope`; companions broadcast, updates dispatch, capability mirror).
- **4b (`d6362397`)** — `domain/security/enforcement.rs`: `PondEnforceMode{Off,Observe,Enforce}` via
  `ZG_POND_ENFORCE` (default Observe); `requires_envelope(method,path)` scope (mutating `/api/v1/stone/`
  + `/api/v1/garden/`, MINUS deploy/upgrade, `/garden/storage/`, `/garden/gateway/`,
  `/stone/offerings/*/volumes/*`); `enforce_envelope` middleware (verify→`identity_for`→nonce). Plus
  `domain/security/replay.rs` `NonceCache` (key `(cn,nonce)`, evict >300s, MAX_ENTRIES fail-closed).
- **#5/#7 (`e3d3aa57`)** — new public `StoneApi::send_signed_raw(method,path,body)`; rake raw-client
  mutations (rest/remove/uproot/release/return/snapshot-restore/manifest-test + garden fan-outs) now sign.
- **#3 (`4a8efa09`)** — `/api/v1/admin/` enforced too (rake signs shutdown/reboot/wake via `api.stone()`).
- **clear-plane end-state (`1fb8205a`)** — under `enforce`, two `run.rs` levers fire: skip the HTTPS :7183
  start (`https_started` stays false → `run.rs:1137` keeps the FULL router on :7185) + stop advertising the
  TLS port (`run.rs:668`) → peers' `StoneClient` reaches the full API over **clear :7185** with a signed
  `X-Koi-Envelope`. Sidesteps the inter-stone mTLS OS-trust gap. Observe/off unchanged (two-listener).
- koi **0.9.0** includes the `node_has_identity` ca.pem member-fix (a CA-chained leaf = identity, no
  member.json) — members can sign + renew. Verified live (`signed:true`).

**FIELD TEST this session** (capability suite, build `0.2.0.202606242100+1fb8205` on both test stones):
- ✅ **Step 1 (no pond):** `offer searxng` deployed+ran; `remove` tore down.
- ✅ **Step 4 (portrait):** `GET /` → identical 74 KB portrait on BOTH http :7185 AND https :7183.
- ✅ Envelope accept/reject/replay field-proven (observe) on capabilities/refresh earlier.
- Pond re-init'd on **topaz** (cornerstone): `pond-swirling-reservoir`, **observe**, ca_fp `5fb401964106e065…`,
  passphrase `spousal-mosaic-breezy-06` (TOTP secret in scratchpad `totp-secret.txt`); HTTPS :7183 up.
  **limpid is pond-LESS** (drained). topaz booted pond-less then init'd at runtime → its :7185 already serves
  the FULL API (router set at boot; runtime init doesn't swap it), so flipping the middleware live is all the
  offer test needs.
- ⛔ Steps 2–3 (non-member offer→401, member-join→202) NOT run — they need enforcement turned ON.

**⚠️⚠️ DESIGN SUPERSEDED TWICE → settled as BINARY "membership = enforcement" (operator, 2026-06-24).**
History: I first proposed a runtime `/api/v1/admin/enforce` toggle (rejected: "why under admin?? no user
delight"), then a delightful `pond seal`/`pond open` command with an open/watching/sealed tri-state. The
operator then collapsed the WHOLE thing: *"I'm not seeing a need for this differentiation… there's no
enforcement — either you're part of the pond, or not."* **There is NO enforce mode, NO `ZG_POND_ENFORCE`,
NO observe phase, NO seal/open command, NO tri-state.** A stone enforces (verify signed envelopes, reject
non-member/unsigned control-plane mutations) **iff it holds a pond identity**; an open stone is permissive;
the only thing a non-member may send a member is a join/bootstrap request. Forging or joining a pond IS the
act that turns enforcement on. (4 open stones; 01 forges → 01 rejects 02/03/04 except joins; 02 joins →
01↔02 trust signed commands, 02 now rejects 03/04, 03/04 still open among themselves.)

A 6-reader+synthesis workflow (run wf_4bf675ed-717) mapped the blast radius and confirmed the model is
**coherent** and the change is **mostly DELETION** — the middleware already contains the switch
(`enforcement.rs:173-182`: `local_identity().is_none()` → passthrough = "open ⇒ no enforcement"); the mode
layer on top is removable; the **signing side needs ZERO changes** (koi signs iff identity).

**EXACT IMPLEMENTATION CHANGE-LIST (from the map, file:line at HEAD 1fb8205a):**
*Step A — behavior (one logical change, gate `cargo check --all` + `cargo test -p garden-moss`):*
- DELETE `enum PondEnforceMode` + `from_env()` — `enforcement.rs:40-65`.
- DELETE `let mode = …enforce_mode()` + the `mode == Off` early return — `enforcement.rs:162-165`.
- COLLAPSE the final match to `Ok => forward; Err(denial) => denial.into_response()` (ALWAYS reject) —
  `enforcement.rs:237-258`. **The Observe warn-and-allow arm (247-256) is the silent one — missing it
  leaves would-allow and defeats the model.** KEEP the membership gate at 173-182 (now the SOLE switch)
  and the verify core at 184-235.
- DELETE the `enforce_mode` field/seed/accessor — `aggregate.rs:79-81, 113-114, 126, 164-167`.
- DELETE `default_mode_is_observe` test — `enforcement.rs:305-310`. KEEP the 3 `requires_envelope` tests.
- run.rs listeners: re-key serve() router on `pond_active()` alone (drop `&& https_started()`) —
  `run.rs:1134-1153`; stop advertising the TLS port (a member never advertises TLS) — `run.rs:667-682`;
  delete the `enforce_mode()==Enforce` HTTPS-listener block — `run.rs:1709-1743`. **The two run.rs levers
  MUST change in lockstep with the accessor deletion or it won't compile.**
- REWRITE the `enforcement.rs:1-21` module doc to the binary rule.
*Step B — dead-code (compiler-flagged after A):*
- DELETE `configure_public()` — `router.rs:53-381` (single clear plane; removes route-dup hazard).
- DELETE the mTLS `:7183` listener module `bootstrap/tls.rs` (operator's standing "retire mTLS, rely on
  clear plane" — MEMORY-confirmed) + `https_started` field & its 3 methods (`aggregate.rs:57, 260-273`) +
  the `pond_enrollment_listener.rs:33-44` `clear_https_started` no-op.
- REWORD `router.rs` enforce-layer comments (drop ZG_POND_ENFORCE; "gated by pond membership").

**KEEP (unchanged):** `requires_envelope` scope + all carve-outs (`enforcement.rs:67-103`); `replay.rs`
NonceCache; the entire signing/oracle/rake side (`signing.rs`, `common/pond_authz.rs`, `stone_api.rs`,
`sign_listener.rs`, `pond.rs` `pond_sign_v1`, rake `context.rs`).

**REVOCATION — the model's one real weakness; operator chose to FIX by PROPAGATION.** Under "membership =
enforcement" it's really "leaf-on-disk = enforcement," and untrust is **cornerstone-only**: `pond.rs:1060-1076`
`pond_untrust_v1` calls only `core.revoke_member` (no member fanout, no wiping the revoked stone's leaf), and
koi `verify` honors revocation only via the local roster — a pure member's roster is empty by koi's design
(`core_identity.rs revoked_fingerprints` doc: *"eventual-consistent; the CA chain remains the hard gate"*).
So a revoked stone stays trusted by every member + keeps enforcing/signing. **Fix = propagate revocation to
members** → koi wishlist written: `docs/notes/koi-revocation-propagation-wishlist.md` (CA `revoked_fingerprints()`
pub export + member `set_revoked(&[fp])` ingest; zen adds signed `GET /api/v1/pond/revocations` + pulls on the
`CertRenewalTask` tick). Needs koi APIs (dogfood — operator implements koi, zen consumes). Does NOT block the
binary model (Step A is independent).

**ROLLOUT TRADEOFF (minor, accepted):** dropping observe loses the dress-rehearsal; a rolling upgrade
transiently 401s old(non-signing)→new(enforcing), but `/stone/deploy`+`/stone/upgrade` stay allowlisted so the
upgrade path never bricks; self-healing in minutes; mitigate by upgrading the dispatcher Moss + operator's rake
LAST. Observe was already field-validated once.

**Capture as ADR SECURITY-0005 ("membership is enforcement") + CHANGELOG** (membership framing, no
Off/Observe/ZG_POND_ENFORCE per docs/DOCUMENTATION.md red-flags). After Step A+B: rebuild + redeploy both
stones, run capability steps 2–3 (non-member offer→401, member-join→202 — now automatic from membership).
**Flipping a live fleet remains operator-gated** (classifier blocked the systemd flip twice) — but under the
binary model there is no flip: a stone enforces the moment it joins a pond.

**SEPARATE BUG (flagged, NOT fixed — different subsystem):** `mongodb::legacy` config-volume mount breaks
Docker — the host path is built from the raw FQN (`…/config/mongodb::legacy/mongod.conf:/etc/mongod.conf:ro`)
and the `::` collides with Docker's `host:container:mode` parser. Use `OfferingFqn::encoded_for_container()`
for the config-volume host path. Triggers on AVX-less stones (Wyse) that fall back to `mongodb::legacy`.

**Boundaries:** dogfood koi (don't pin); koi **0.9.0** is the floor — re-verify if `../koi` moves.
Detailed single-file manual work; no stubs; verify against koi at file:line. Gate each slice:
`cargo check --workspace` → `cargo test -p garden-moss` → changed-file clippy-clean. Commit on
`fix/snapshot-scheduler-disposal`, split logically, stage own files by explicit path (never `git add -A`).
⚠️ Bash tool is POSIX sh — commit via `git commit -F <file>`, never PowerShell `@'...'@` (injects a literal
`@`). Recall memory **`project_koi_0_5_1_migration`** to resume.

---

## Done (uncommitted, on branch `fix/snapshot-scheduler-disposal`)

1. **P0 — koi-truststore → os-truststore** (koi deleted koi-truststore in 0.5.0, ADR-019; uses
   published `os-truststore`). DONE:
   - `Cargo.toml`: dropped `koi-truststore` path dep, added `os-truststore = "0.0.2"` (with comment).
   - `src/rake/Cargo.toml:11`: `koi-truststore.workspace` → `os-truststore.workspace`.
   - `src/rake/src/enrollment.rs`: `install_ca_in_trust_store` now uses
     `os_truststore::Cert::from_pem` + `Install::new(&cert).label("zen-garden-pond").run()`;
     `is_ca_installed(ca_cert_pem: &str)` (signature CHANGED — now takes PEM) uses
     `os_truststore::is_installed(&cert)` (real query; was label-based).
   - `src/rake/src/commands/management/pond.rs` (`execute_pond_trust`): reads `ca.pem` BEFORE the
     install-check and passes it to `is_ca_installed(&ca_pem)`.
   - Verified: zero `koi_truststore::` code refs remain; os-truststore 0.0.2 API confirmed
     (`Cert::from_pem`, `Install::new().label().run()`, `is_installed(&Cert)->Result<bool>`,
     `uninstall`, `Scope`, `Report`).

2. **Ceremony rename** (koi renamed `pond_ceremony::PondCeremonyRules` → `init_ceremony::InitCeremonyRules`;
   NOT split — one `CeremonyRules` impl; constructor identical `new(paths: CertmeshPaths)`). DONE via
   replace_all at all 5 sites:
   - `src/moss/src/domain/security/aggregate.rs` (3 sites: type fields/params + `ceremony_host()` return)
   - `src/moss/src/bootstrap/run.rs:865` (construction)
   - `src/moss/src/testing.rs:262` (construction)

## REMAINING blocker — CSR-only enrollment migration (4 errors, security path)

koi 0.5.1 `JoinRequest` (koi-certmesh/src/protocol.rs:17) is now **CSR-only** (ADR-015):
```rust
pub struct JoinRequest { hostname: String, auth: Option<AuthResponse>, invite_token: Option<String>, csr: String, sans: ... }
```
zen is still on the OLD model (CA generates keypair, returns cert+key). **zen has ZERO csr code today.**

Compile errors (garden-moss), both sites in `src/moss/src/api/v1/pond.rs`:
- `462` + `1490`: `JoinRequest { ... }` missing fields `csr` and `invite_token`.
- `464` + `1492`: `auth:` must be `Some(AuthResponse::Totp{..})` not bare.

`local_enrollment` (pond.rs:452) is the CORNERSTONE/CA-side handler: receives zen's `PondJoinRequest`
(pond.rs:61), builds koi `JoinRequest`, calls `core.enroll(&join_req)` → returns `PondJoinResponse`
(pond.rs:139). The OTHER site (~1490) is a second enroll caller — inspect it.

**The correct migration (CSR-only model, security-sensitive, cross-component):**
1. Joiner (rake `pond enroll`) must generate keypair + CSR and KEEP its key. Use koi's
   `CertmeshCore::prepare_member_csr(hostname, sans)` (joiner-side, writes key 0600, returns CSR PEM)
   then `install_member_cert(hostname, cert, ca, ca_endpoint, ca_fingerprint, sans, policy)` — pass
   ca_endpoint+ca_fingerprint to write member.json so auto-renew arms. (koi has NO single-call join;
   it's a 3-call compose: prepare_member_csr → remote enroll → install_member_cert.)
2. `PondJoinRequest` (pond.rs:61): add `csr: String` (the joiner-generated CSR).
3. `local_enrollment` (pond.rs:462): `auth: Some(...)`, `csr: payload.csr`, `invite_token: <None or payload>`.
4. `PondJoinResponse` (pond.rs:139) + rake cert install: CA now returns only the SIGNED CERT (no key);
   rake installs with its OWN generated key (NOT a CA-returned key). `enrollment.rs::write_enrollment_certs`
   currently writes a received `service_key` — that flow inverts (key is local now).
5. Re-verify both enroll callers (462 and ~1490 — the second may be a re-enroll/refresh path).

This changes **key custody** (joiner keeps key — a security invariant) and the **rake↔moss join wire
contract** — treat as a focused, careful pass (like the 0.4.2 certmesh-diet, aec0f024). Greenfield/no-shim
is the operator's stance → clean cutover acceptable. Confirm direction before rewriting key-custody.

## Verify gate
`cd f:/Replica/NAS/Files/repo/github/sylin-org/zen-garden && cargo check --workspace 2>&1 | tail -40`
(orchestrators are excluded from the workspace). After green: `cargo test --package moss`,
`cargo clippy -- -D warnings`. Then commit (P0 + ceremony + CSR as one or split logically).

## Context
- koi at 0.5.1, tagged v0.5.0/v0.5.1. koi ALSO shipped almost the whole zen wishlist (events/posture
  SSE, cert-lifecycle events, reqwest_client_for, require_auth_with, etc.) — see
  [koi-0.5-wishlist.md](koi-0.5-wishlist.md) (delivered items marked) and
  [koi-0.5-capability-discovery.md](koi-0.5-capability-discovery.md).
- Bigger refactor plan (Phase-1 trust delegation etc.): [koi-0.5-leverage-refactor-plan.md](koi-0.5-leverage-refactor-plan.md).
- Operator decision: keep path deps (dogfooding); prompts 01/02 stay postponed.

## Resolved architecture (operator, 2026-06-23) — supersedes the deliberation above

The CSR-only migration was clarified into a coherent dual-mode security model. Decisions:

1. **Joining the certmesh is a MOSS operation.** Rake is a thin command sender — it does
   NO certmesh work (no CSR, no keygen, no enrollment) and needs NO `koi-certmesh` dep.
   `rake pond join` forwards `{code}` to a Moss; that Moss is the member. Rake's only
   cert role is *consuming* the local Moss-issued cert for client auth (already wired:
   `main.rs:124-135` `load_tls_materials` → reqwest `Identity`; member leaf is dual-EKU
   server+client per koi `ca.rs:85-87`; koi pins CN==OS-hostname so the on-disk cert dir
   key matches between writer Moss and reader rake).

2. **CSR cutover scope = Moss only** (Step 1):
   - `proxy_enrollment` (joining Moss): `core.prepare_member_csr(name, sans)` (keeps key
     0600 locally) → forward CSR to cornerstone → `core.install_member_cert(...)`.
   - `local_enrollment` (cornerstone Moss): pure CA signer — `core.enroll(JoinRequest{csr})`,
     returns cert (no key). koi `process_enrollment` HARD-REJECTS `csr:None`.
   - Wire: `PondJoinRequest` += `csr`; `PondJoinResponse` −= `service_key`.

3. **`rake pond enroll` / `/api/v1/pond/enroll-client` is REMOVED** (Step 2). It was the only
   path where rake minted its own cert without a Moss — invalid under "membership ⇒ Moss
   installed ⇒ cert locally issued." Deleting it also resolves the pond.rs:1490 compile error
   by deletion. Keep rake read helpers (`load_tls_materials`, `is_enrolled`); drop the write
   path (`write_enrollment_certs`, the enroll-client driver/CLI).

4. **Authorization plane = clear signed requests** (Step 5, later, distinct workstream):
   - Transport stays CLEAR (plain 7185) — needed for bootstrap (a pondless stone has no cert
     yet so it MUST reach the pond in the clear to join) + open-garden + public/read commands.
     mTLS is NOT the auth gate. (koi envelope is sign-only; `Posture{signed, encrypted}`,
     `encrypted` always false in 0.5.x — we have authenticity, not confidentiality.)
   - Per-command authz: pond active + valid envelope signature → `verify_envelope` →
     `Assurance::identity()` → authorize; no signature → allow iff public command; else deny.
     Open garden → all public. Primitive: `koi_certmesh::envelope::{build_envelope,verify_envelope}`,
     ±300s freshness.
   - **Signature failure is reactive, not terminal**: expired/unknown signer (stone dark >90d,
     cert past grace) → actionable response ("rejoin — enter TOTP"), not an opaque 403.

5. **Renewal = zen-native, envelope-signed CSR over the clear Moss plane** (Step 4, operator
   chose wire-now) — NOT koi's mTLS 5642 loop (koi's `renew_self_if_due` dials the CA mTLS
   authority, which zen's EmbeddedOnly cornerstone does not serve). Rides the same clear+signed
   plane as Step 5.

Sequencing: Steps 1-2 (cutover, green, commit) are the prerequisite for everything; then Step 4
(renewal) and Step 5 (authz plane).

### Status 2026-06-23

- **Steps 1-2 DONE; build GREEN.** `cargo check --workspace` ✓; `cargo test --package garden-moss` ✓
  (939 passed, 0 failed). My edited files are clippy-clean.
- Ceremony rename was **6** sites, not 5 — `src/moss/src/domain/security/tests.rs` only surfaced under `--tests`.
- `cargo clippy -- -D warnings` is RED on **pre-existing** debt in untouched files (garden-common
  `manual_checked_ops`/`unnecessary_sort_by` ×8; misc moss/rake style lints) — not a regression from this work.
- **COMMITTED** on `fix/snapshot-scheduler-disposal` (operator chose to commit on the current branch
  despite the name mismatch; not pushed): `3b8c1115` ceremony rename, `00bc29b4` CSR-only enrollment +
  os-truststore + drop enroll-client. Untracked working stash left uncommitted.
- Next: Step 4 (zen-native renewal) then Step 5 (clear-signed authz plane).

### Status 2026-06-24 — koi 0.6.0, wishlist delivered, NEXT = authz plane

- koi moved **0.5.1 → 0.6.0** (embedded API completion). zen **re-verified green** (`cargo check
  --workspace`). Cargo.lock synced + committed: `ac37a5d2`. So branch now has 3 commits
  (`3b8c1115`, `00bc29b4`, `ac37a5d2`), not pushed.
- The koi wishlist (`koi-envelope-authz-wishlist.md`) **shipped in 0.6.0**:
  `CertmeshCore::renew_member(authenticated_cn, csr)` (core_renewal.rs:245), `member_cert_expiry()`
  pub (core_renewal.rs:77), top-level `KoiHandle::sign`/`verify` (handle.rs:297/305). All other
  needed primitives already existed (sign/verify, prepare_member_csr/install_member_cert,
  csr::sign_csr, certmesh_status, posture/watch_posture, KoiEvent stream).

**NEXT BUILD — clear-signed dual-mode authz plane + renewal (the "Resolved architecture" section
above is canon). Build in delight order (invisible > warm-recovery > felt-safety):**

1. **verify + event-routing** (cheapest; no signing decision needed). Route koi `PostureChanged` +
   cert-lifecycle `KoiEvent`s out of the `.events()` callback at `src/moss/src/bootstrap/run.rs:579`
   (currently only `tracing::debug!`) into zen's event bus → a humane `rake pond status` line. Wire
   `core.verify(env) -> Assurance` on the inbound path.
2. **loopback `/pond/sign` + rake sign-via-local-Moss.** New Moss endpoint calling
   `state.discovery.koi().sign(bytes)`; rake asks its LOCAL Moss to sign each request, sends the
   envelope (clear) to the target. **⚠️ `/pond/sign` is an impersonation oracle — MUST be
   loopback-only** (Moss binds 0.0.0.0:7185 `server.rs:51` / 0.0.0.0:7183 `tls.rs:153`): add a
   `ConnectInfo<SocketAddr>` loopback guard or a dedicated 127.0.0.1 listener. **Sign canonical
   `method+path+body`** (not just body). koi keeps NO nonce cache → replay within ±300s is zen's
   concern (server-side single-use for destructive ops if wanted).
3. **renewal** — member timer (`member_cert_expiry`) → `prepare_member_csr` → wrap renew request in
   a signed envelope → `POST /api/v1/pond/renew` → CA `core.verify`→identity → `core.renew_member(cn,
   csr)` → return leaf → member `install_member_cert`. **Graceful welcome-back:** map
   `verify`/`verify_envelope` `RejectReason::{Expired,UnknownSigner}` to a structured "rejoin — enter
   TOTP" response; rake renders it and re-runs the existing join (not an opaque 403).
4. Later/aspirational: route posture events to companions (firefly green pulse / cricket chirp) for
   ambient felt-safety.

Per-command authz at the target: pond-active + valid envelope identity → authorize; no signature →
allow iff public command (join/status/health/ca.pem/reads) else deny/welcome-back; open garden →
all public. No user identity anywhere (stone CN is the actor — zen is infra-only).

### Status 2026-06-24 (session 2) — authz plane build UNDERWAY

Operator confirmed three security-path decisions (via AskUserQuestion):
1. **Loopback guard** = **dedicated 127.0.0.1 listener** (not a ConnectInfo guard) — OS-enforced
   isolation for the sign oracle.
2. **Signed bytes** = `zen-pond-request-v1\nMETHOD\n{path?query}\n{audience}\n{blake3_hex(body)}` —
   verb + exact path + **audience (target stone NAME)** + body hash. Audience binding kills
   cross-stone replay (all stones share one CA). koi's envelope wraps these with its own
   `koi-envelope-v1` domain prefix.
3. **Enforcement** = **enforce on the clear plane and begin retiring the mTLS gate** (canon
   end-state). Reached via a SAFE ORDERING so the fleet (phone/Windows stones) never bricks: the
   deny-flip lands only after signing is proven, with 7183 kept alive through the transition.

**Staged plan (each stage green/committable/non-bricking):**
- **Stage 1 — DONE, committed** (`7b5231ef` event routing, `78182fec` humane status; NOT pushed):
  koi `events()` → `PondEvent::{PostureChanged,CertRenewed,CertExpiring,CertRenewalFailed}` →
  EventBus (bridge task `moss/tasks/koi_events.rs`, spawned in coordinator). Humane `pond status`:
  `PondStatusResponse.identity {signed, expires_at, expires_in_days}` from `core.posture()` +
  `core.member_cert_expiry()`; rake renders member/keystone standing + "valid for N days / expired
  — rejoin". Fixed a real gap: enrolled member (no local CA) used to read "not initialized".
- **Stage 2A — DONE, committed** (`697e9cff`; NOT pushed): loopback `/api/v1/pond/sign` on a
  dedicated `127.0.0.1:7182` listener (`MOSS_SIGN_LOOPBACK`, SO_REUSEADDR; bootstrap/sign_listener.rs;
  spawned in coordinator). Shared primitive `garden_common::pond_authz::{canonical_request_bytes,
  canonical_request_bytes_for, body_hash_hex}` (pure, tested). `HEADER_KOI_ENVELOPE = "x-koi-envelope"`.
  `pond_sign_v1` returns `{envelope, signed}`. NOTE: koi `KoiHandle::sign` returns
  `Result<Envelope>`; `core.sign`/`core.verify` are infallible. `core.posture()` is sync.
- **Stage 2B FOLDED INTO Stage 4** — the verify-side bind check is just `decode(env.payload) ==
  canonical_request_bytes_for(method, path, MY_stone_name, body)`; it's inherently moss-side (needs
  `core.verify` + the CA anchor) and is consumed by enforcement, so building it earlier = dead code.
  `canonical_request_bytes_for` already provides the verifier half. (garden_common deps are koi-free
  and base64-free; do the payload decode + compare in moss.)
- **Stage 2C — NEXT**: rake signs each outbound MUTATING request via its LOCAL Moss
  (`http://127.0.0.1:7182/api/v1/pond/sign`) and attaches `X-Koi-Envelope`. Targets ignore it until
  Stage 4 (non-breaking). OPEN DESIGN POINT before building: WHERE to hook signing in rake
  (StoneApi optional injected `RequestSigner` vs reqwest-middleware vs per-command) and how rake
  resolves the target stone NAME for the audience (`Stone::name()` is lazy/tending-cached —
  `connection/stone.rs:67`). A tiny uncommitted tree change exists: `headers.rs` doc-link reworded
  (rides into the 2C commit).
- **Stage 3**: renewal timer (`member_cert_expiry`) → `prepare_member_csr` → signed envelope →
  `POST /api/v1/pond/renew` → CA `core.verify`→`identity()`→`core.renew_member(cn, csr)` (returns
  `RenewResponse{service_cert, ca_cert, ca_fingerprint, expires}`) → `install_member_cert`. Mirrors
  the existing `proxy_enrollment` flow (pond.rs:497). Welcome-back: `RejectReason::{Expired,
  UnknownSigner}` → structured "rejoin, enter TOTP" (rake re-runs join).
- **Stage 4**: enforcement middleware on the clear plane — `core.verify(env)` → `Assurance::identity()`
  AND `decode(payload)==canonical_for(...with my stone name...)` ⇒ authenticated actor. Public
  allowlist = all GETs + init/join/unlock/status/ca.pem/health. Per-nonce single-use cache (±300s)
  for destructive ops (koi keeps NO nonce cache). Full API onto clear 7185; keep 7183 as fallback.
  **Coordinated rake+moss deploy** (the one breaking step). Folds in Stage 2B's verify wiring.
- **Stage 5**: retire the mTLS auth gate (drop/optionalize 7183) once Stage 4 proven fleet-wide.

Gate each slice: `cargo check --workspace` → `cargo test -p garden-moss` → changed-file clippy-clean
(pre-existing `unnecessary_sort_by` at `api/v1/pond.rs` in `discover_cornerstone`, and garden-common
debt, are NOT ours). Commit on `fix/snapshot-scheduler-disposal`, split logically, stage only own
files by explicit path (untracked stash stays untracked), NOT pushed. ⚠️ The Bash tool is POSIX sh —
do NOT use PowerShell `@'...'@` heredocs for commit messages (it injected a literal `@`; use
`git commit -F <file>`).
