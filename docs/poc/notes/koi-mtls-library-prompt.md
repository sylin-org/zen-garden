# Koi agent prompt — promote the mTLS adapter into a koi-certmesh library API

> Paste this to a koi-repo agent session. Drafted by the zen-garden side 2026-06-15. Greenfield, pre-1.0,
> no compat shims. Companion to zen's prompt 05 (security-baseline); zen will consume the result.

## Why

koi owns certmesh and the PKI/trust boundary (stack canon: consumers depend on koi for trust). koi
already implements mTLS enforcement in `crates/koi/src/adapters/mtls.rs`:

- `build_tls_config(cert_pem, key_pem, ca_cert_pem) -> rustls::ServerConfig` with a
  `WebPkiClientVerifier` built from the certmesh CA (requires + verifies client certs), and
- a `tokio-rustls` accept loop that extracts the peer certificate's CN via `x509-parser` and injects
  `koi_certmesh::http::ClientCn(cn)` as an axum `Extension` for handler authorization.

But this lives in the koi **binary**, hardcoded to nest `certmesh_core.inter_node_routes()`. A downstream
consumer (zen-garden's `moss` daemon) needs to stand up **its own** mTLS server — its per-stone HTTPS
listener on :7183, serving moss's full router — enforcing certmesh client certs and learning the caller's
identity for write-authorization. Today it can import the `ClientCn` *type* (public in
`koi-certmesh::http`) but not the server builder, so it would have to duplicate koi's rustls verifier and
the CA/trust wiring — exactly the duplication the stack canon forbids.

## Ask — expose the mTLS server as a koi-certmesh library API

Promote the adapter from the binary into **`koi-certmesh`** (it already owns `ClientCn` and the certmesh
trust). Provide a generic, router-agnostic API:

1. **`koi_certmesh::mtls::build_server_config(server_cert_pem, server_key_pem, ca_cert_pem)
   -> Result<rustls::ServerConfig, CertmeshError>`** — the promoted `build_tls_config`: a `ServerConfig`
   that *requires* client certs verified against `ca_cert_pem` (WebPkiClientVerifier).
2. **`koi_certmesh::mtls::serve(router: axum::Router, listener: tokio::net::TcpListener,
   config: rustls::ServerConfig, cancel: CancellationToken) -> Result<(), …>`** — the promoted accept
   loop: TLS-accept, extract the peer-cert CN, inject `Extension(ClientCn(cn))` into the per-connection
   router, and serve it with graceful shutdown on `cancel`. Generic over the caller's `Router` (do NOT
   hardcode `inter_node_routes`). Connections without a valid client cert/CN are rejected (as today).
3. Keep **`koi_certmesh::http::ClientCn`** public (it already is) as the identity handlers read.
4. Optionally expose **`koi_certmesh::mtls::extract_cn(cert_der: &[u8]) -> Option<String>`** for callers
   that run their own loop.

Refactor koi's own binary adapter to call the new library API (no behavior change for koi). Keep the
existing tests; add a library-level test that an mTLS server built this way rejects a no-cert client and
accepts a CA-signed client, surfacing the CN.

## Constraints

- koi-certmesh is consumed by zen via path deps; this must be a clean library addition (new `mtls` module
  in koi-certmesh, re-exported). No new heavy deps beyond what the adapter already uses
  (`tokio-rustls`, `hyper-util`, `x509-parser`, `rustls`).
- Pre-1.0 / greenfield: move the code, don't leave a shim in the binary.
- `cargo test && cargo clippy -- -D warnings && cargo fmt --check` green in koi.

## What zen does after this lands

`moss` replaces its `bootstrap/tls.rs` `with_no_client_auth()` :7183 path with
`koi_certmesh::mtls::build_server_config(...)` + `serve(...)`, reads `Extension(ClientCn)` in a
write-authorization layer, gates write routes on a valid CN, and rake presents its enrollment cert
(`enrollment::load_tls_materials`) via a reqwest `Identity`. See
[`security-rake-mtls-plan.md`](security-rake-mtls-plan.md) (the zen-side integration plan; update its
"moss enforcement" step to "delegate to `koi_certmesh::mtls`").
