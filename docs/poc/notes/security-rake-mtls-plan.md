# Prompt 05 (security-baseline) — rake Pond-mTLS implementation plan

> Drafted 2026-06-15 after the prompt-05 OPERATOR gate. The cleanups (changeme / seeds drift / SSH doc)
> are DONE and committed separately. This plan is the remaining, larger work: teach rake to authenticate
> writes via Pond mTLS, and gate writes to mTLS-only when Pond is active. Run as a focused fresh session.

## Operator decisions (2026-06-15) that govern this design

- **No stone-local token.** Security model = "Pond present → use it (mTLS); no Pond → open." Home-lab-open
  is acceptable/desirable when Pond is inactive.
- **Gate all mutating routes when Pond is active, exempt the enrollment flow.**
- **deploy/upgrade stay open** (decision 3 + the existing "must work over HTTP for infrastructure"
  comment) — do NOT move them to mTLS-only. Note the residual risk (unauthenticated root code-push even
  when Pond active) in FINDINGS as an accepted home-lab tradeoff.

## Verified ground truth (research done this session)

- **rake/StoneApi transport is plain `reqwest::Client::new()` over `http://stone:7185`** — no client
  cert, no mTLS. (`src/common/src/client/stone_api.rs`, `src/rake/src/connection/`.) This is why "Pond
  secures writes" can't reach the CLI today and why this feature is needed.
- **The certs rake needs already exist on disk after enrollment**: `src/rake/src/enrollment.rs`
  `certs_dir(hostname)` holds `cert.pem` (0644), `key.pem` (0600), `ca.pem` (0644), `fullchain.pem`.
  So rake can build a reqwest `Identity` (cert+key) + trust root (ca.pem).
- **moss :7183 is server-TLS-ONLY today** (`bootstrap/tls.rs:101` `with_no_client_auth()` — "mTLS
  deferred to Phase 4"). moss never verifies client certs anywhere; koi exposes NO mTLS server builder as
  a library (only the binary `koi/src/adapters/mtls.rs` + the public `koi_certmesh::http::ClientCn` type).
  **So this feature is BLOCKED on koi** exposing its mTLS adapter as a koi-certmesh library API — see
  [`koi-mtls-library-prompt.md`](koi-mtls-library-prompt.md). `configure()` (full router) is served on
  :7183; `configure_public()` (lobby) is the pre-trust subset on plain :7185.

> **koi SHIPPED (2026-06-15, dev@2ca7177):** `koi_certmesh::mtls::{build_server_config(cert,key,ca) ->
> ServerConfig, serve(router, listener, config, cancel), extract_cn}`; `koi_certmesh::http::ClientCn`
> public. zen `cargo check --workspace` green against it (lock synced, commit c51f0f58). So moss's :7183
> listener can delegate to `koi_certmesh::mtls::{build_server_config, serve}` (no rustls verifier in zen).
> moss's current `bootstrap/tls.rs` `try_start_https` loads `fullchain.pem`/`key.pem` from
> `{data_dir}/koi/certs/{stone}`; add `ca.pem` and swap `load_tls_config`(with_no_client_auth) + the
> `TlsListener`/`axum::serve` for koi's `build_server_config` + `serve`.

## ⚠️ OPEN DESIGN DECISION (resolve before integrating) — the browser dashboard can't do mTLS

`run.rs:1138-1145`: Pond-active → HTTP **:7185 serves `configure_public` (lobby)**, HTTPS **:7183 serves
`configure` (all routes)**. The **browser pond dashboard** (`src/moss/assets/pond.html`) is served on
:7185 and performs **pond ADMIN over the open lobby** via relative fetches — `pond/ceremony`, `pond`
DELETE, etc. Browsers cannot present a Pond client cert, so they cannot use mTLS :7183.

So "gate writes to mTLS-only :7183" **collides with browser admin**: removing pond-admin routes from the
:7185 lobby breaks the dashboard's actions. Pick one (OPERATOR):
- **(a) Browser admin stays on the open lobby** (pond-admin writes remain on :7185 when Pond active) —
  rake writes go mTLS, but browser admin stays HTTP-open (the security goal is only partly met).
- **(b) Dashboard goes read-only; admin is rake-only (mTLS)** — move pond-admin off the lobby; the
  browser shows status only, all mutations via `rake` over mTLS. Cleanest security; UX change.
- **(c) Require a browser client cert** (manual cert import into the browser) — heavy UX; not homelab-y.

Recommend (b) for the security goal, but it's a real UX call. Also: rake's current `https://:7183` path
(`resolution.rs:235,288`) uses a no-client-cert `reqwest::Client` → it will FAIL the handshake once :7183
is mTLS-required, until rake presents its enrollment cert (step 1 of the design). So the moss flip and
the rake cert must land together (or rake breaks against pond-active stones).
- **Every pond/stone route exists in BOTH `configure()` and `configure_public()`** (verified: count=2
  for invite/promote/untrust/name/unlock/join/enroll-client/ceremony/deploy/upgrade/pond-DELETE). So
  "gate a write when Pond active" = REMOVE it from `configure_public` (it stays in `configure()` = mTLS).
- The lobby's only WRITES are the pond-admin ops + deploy/upgrade; service/offering/storage mutations are
  already `configure()`-only (verify during impl). So the moss-side gating is small.

## Design

1. **rake mTLS transport** (new): a function that, given a hostname, loads `enrollment::certs_dir(host)`
   cert.pem+key.pem → `reqwest::Identity::from_pem(fullchain or cert+key)`, ca.pem →
   `reqwest::Certificate`, and builds `reqwest::Client::builder().identity(id).add_root_certificate(ca)
   .build()`. Returns `None` if certs are absent (not enrolled).
2. **Transport selection**: when the target stone's Pond is active (read `GET /api/v1/pond/status` on the
   open lobby; `active == true`) AND rake has enrollment certs for itself → talk `https://stone:7183`
   with the mTLS client. Else → plain `http://stone:7185`. Thread this through `StoneApi` construction /
   the rake `connection/` resolution layer (where the endpoint + client are built).
3. **moss route gating**: remove the pond-ADMIN writes from `configure_public` so they are mTLS-only when
   Pond is active: `pond/invite`, `pond/promote`, `pond/stones/{name}` DELETE (untrust), `pond/name` PUT
   (rename), `pond` DELETE (drain). KEEP in the lobby: enrollment (`pond/join`, `pond/enroll-client`,
   `pond/unlock`, `pond/ceremony`), reads (`pond/status`, `pond/ca.pem`, `console/mode`, service reads),
   and `stone/deploy` + `stone/upgrade` (decision 3). Verify each removed route still exists in
   `configure()` before removing (it does — count=2).
4. **Error UX**: when a write hits the lobby (plain :7185) on a Pond-active stone (now 404/405 because it
   moved to mTLS-only), rake should detect Pond-active and have already routed to :7183; if mTLS fails
   (no certs), return a clear "this stone's Pond is active — enroll (rake pond join) to get a client
   cert, or use the pond dashboard" error with a `hint`.

## Implementation steps (ordered; commit per step; `cargo check` + `cargo test` after each)

1. rake mTLS client builder (in `src/rake/src/enrollment.rs` or a new `connection/mtls.rs`): load certs →
   `reqwest::Client`. Unit test with the enrollment fixtures.
2. Transport selection in the connection layer: pond-active probe + endpoint/client choice. (Reuse the
   existing resolution provenance; add a "pond-active → https :7183 mTLS" branch.)
3. `StoneApi` accept an injected `reqwest::Client` (it already does — `StoneApi::new(client, endpoint)`),
   so feed it the mTLS client + the :7183 endpoint when appropriate.
4. moss: remove the 5 pond-admin writes from `configure_public` (step 3 above). `cargo check`.
5. Wire `installer/deploy.ps1` if it performs writes that now require mTLS (deploy/upgrade stay open, so
   likely no change — verify).
6. Tests (in-process axum harness `src/moss/src/testing.rs` + rake): write to a pond-active stone without
   a client cert → blocked; with mTLS → 200; reads + enrollment open; pond-inactive → writes open.
7. `docs/security/stone-mtls.md` (~30 lines): the model (pond-or-open), how rake uses enrollment certs,
   the deploy-stays-open caveat.

## Definition of done (adapted from prompt 05)

- [ ] rake performs an authenticated write (e.g. `pond invite`) against a Pond-active stone over mTLS
      :7183, end-to-end (paste transcript).
- [ ] The same write against a Pond-active stone WITHOUT enrollment certs → clear 401/refusal with hint.
- [ ] Reads + enrollment (`pond/join`) work with no certs. Pond-inactive stone: writes open on :7185.
- [ ] `cargo test --workspace` green; new tests listed.
- [ ] `docs/security/stone-mtls.md` written; FINDINGS notes the deploy-stays-open accepted risk + the
      preseed SSH hardening follow-up + (optional) interactive passphrase prompt for invite/unlock/promote.

## Out of scope (unchanged)

Declarative route-table refactor (prompt 11). Pond-by-default first boot. Rate limiting. TLS for :7185.
Preseed/SSH hardening beyond the doc correction (already done).
