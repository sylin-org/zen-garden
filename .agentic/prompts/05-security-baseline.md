# 05 — Security Baseline: Close the Unauthenticated LAN

> No mutating endpoint reachable without authentication; no default credentials; docs tell the truth about
> SSH. Phase: Harden. Depends on: 02 (CI). Blocks: 06 (the front door must not advertise unsafe defaults),
> and any public showing of the project.

## Mission

Today, anyone on the LAN can push code to a stone as root. With Pond inactive, port 7185 additionally
serves stone reboot/shutdown, offering deletion, and storage writes with no authentication of any kind —
and the auth module that exists (`NoAuth`) is wired into nothing. For a project whose audience is
data-sovereignty users, this is disqualifying. Introduce a minimal, ergonomic authentication baseline that
preserves the project's "security is opt-in, but exposure isn't" philosophy: read endpoints stay open on
the LAN (discovery must work zero-config), mutating endpoints require either Pond identity (when active)
or a stone-local token (always available, zero-ceremony).

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| `POST /api/v1/stone/deploy` is registered in BOTH route sets — `configure_public()` (router.rs ~356-359) and `configure()` (~460-463) — so root code-push is unauthenticated **even with Pond active** | `grep -n "deploy" src/moss/src/bootstrap/router.rs` |
| With Pond inactive, the FULL router (incl. `/api/v1/admin/stone/reboot`, `/shutdown`, offering DELETE, storage writes) is served plain-HTTP on :7185; zero auth middleware layers exist on the router | `grep -n "layer\|middleware" src/moss/src/bootstrap/router.rs \| head -20` |
| `infra/auth.rs` defines `NoAuth` ("always succeeds") with 4 test-only instantiations and zero production wiring (may already be deleted by prompt 03 — check) | `grep -rn "NoAuth" src/moss/src --include="*.rs"` |
| Dual-listener split: HTTP :7185 ("lobby" when pond active = `configure_public`), HTTPS :7183 (full, mTLS) — `router.rs:13-17` doc comment | read `src/moss/src/bootstrap/router.rs:1-60` |
| Default invite passphrase `changeme` in rake (`pond.rs` ~319-326) | `grep -n "changeme" src/rake/src -r` |
| NewStone preseed installs openssh-server with `stone`/`stone` and NOPASSWD sudo while `first-stone.md` claims "SSH is disabled by default" | `grep -n "ssh\|NOPASSWD" installer/*preseed* ; grep -rn "disabled by default" docs/guides/first-stone.md` |
| Drift bug from the manual route duplication: `GET /api/v1/stone/banks/{moniker}/seeds` exists ONLY in configure_public (~186-189) | `grep -n "seeds" src/moss/src/bootstrap/router.rs` |

## Research first (~60 min)

1. Read `src/moss/src/bootstrap/router.rs` end to end (1,317 lines) — you must know which routes mutate.
2. Read the pond lifecycle: `src/moss/src/domain/security/`, `api/v1/pond.rs` (how "pond active" is
   determined at request time), and `tls.rs` — your middleware must read the same state.
3. Read how rake authenticates today (it doesn't): `src/rake/src/connection/` — you will add token
   plumbing to `StoneApi`/the client layer.
4. Read `docs/philosophy/pond-security-model.md` and `docs/specs/security.md` so the design honors
   "fill the pond when ready" — the token is a floor, not a replacement for Pond.
5. Check `installer/deploy.ps1` — it must send the token after this change.

## Plan gate — OPERATOR decisions

1. **Token model** (recommend A): (A) stone-local bearer token, auto-generated at first boot into
   `{data_dir}/stone-token` (mode 0600), readable by the operator on the box, sent as
   `Authorization: Bearer`; rake auto-reads it when running ON the stone, `--token`/`ZG_STONE_TOKEN`
   otherwise. (B) Pond-by-default at first boot (bigger UX change — defer).
2. **Which routes gate.** Recommend: ALL non-GET routes + the mutating GETs if any, with an explicit
   allowlist of unauthenticated POSTs kept for the join/enrollment flow (`/api/v1/pond/join` must work
   pre-trust by design — verify what enrollment needs).
3. Whether `/api/v1/stone/deploy` should additionally require Pond when Pond is active (recommend yes:
   token-OR-mTLS when inactive, mTLS-only when active).

## Target shape

One middleware, applied once, route policy declared next to the route (this also seeds prompt 11's
declarative route table — coordinate the shape):

```rust
// bootstrap/router.rs — policy lives with the route registration
.route("/api/v1/stone/deploy", post(deploy_stone_v1).layer(require_write_auth()))

// infra/http_auth.rs (new, ~120 lines)
/// Write-auth: Pond mTLS identity (HTTPS listener) OR stone-local bearer token.
/// Read endpoints stay open: LAN discovery is the product.
pub fn require_write_auth() -> /* axum layer */ { ... }
```

Operator UX (document in the same session, `docs/security/stone-token.md`, ~30 lines):

```
$ garden-rake services restart mongodb --at stone-01
  → 401 stone-01 requires authentication for write operations
    hint: pass --token or set ZG_STONE_TOKEN; the token lives at /var/lib/zen-garden/stone-token on the stone
$ garden-rake services restart mongodb --at stone-01 --token $(ssh stone@stone-01 cat /var/lib/zen-garden/stone-token)
  ✓ restarted
```

Error shape: reuse the existing `ErrorResponse` envelope; 401 with a `hint` field — match the project's
"five-fix error" craft.

## Implementation

1. Token generation at first boot (`bootstrap/run.rs` first-boot path; use existing `data_dir()` +
   `garden_common` fs utils; 0600 perms via existing platform helpers).
2. `infra/http_auth.rs` middleware + unit tests (in-process axum harness exists — `src/moss/src/testing.rs`).
3. Apply to every mutating route in BOTH `configure()` and `configure_public()`; while in the file, add
   the missing `GET .../banks/{moniker}/seeds` to `configure()` (the verified drift bug — one line).
4. Remove `/api/v1/stone/deploy` from `configure_public()` if OPERATOR chose mTLS-only-when-pond-active;
   else gate it with the token in both.
5. Rake: token plumbing in the client layer + `--token` global flag + `ZG_STONE_TOKEN` env (follow the
   existing `ZG_*` EnvConfig pattern in `common/src/utils/env.rs`); update `installer/deploy.ps1` to read
   the token (document the fleet-deploy story: the operator's deploy host keeps tokens, or uses Pond).
6. Delete the `changeme` default: require `--passphrase` or generate-and-print one (use the existing
   entropy/ceremony utilities if suitable).
7. Fix the SSH documentation lie the cheap way for now: one-line correction in first-stone.md (full
   rewrite is prompt 06) + FINDINGS.md entry proposing preseed hardening (disable password auth
   post-enrollment) as future work.
8. Tests: 401 without token on a mutating route; 200 with token; enrollment flow still works without
   token; read endpoints untouched. Commit sequence: middleware → wiring → rake plumbing → docs.

## Definition of done

- [ ] `curl -X POST http://localhost:7185/api/v1/admin/stone/reboot` (or the in-process harness
      equivalent) → 401 with hint. Same for deploy, offering DELETE, storage write.
- [ ] `curl http://localhost:7185/api/v1/stone/services` (read) → 200, no auth. Discovery unaffected.
- [ ] Token file created at first boot, 0600; rake `--token`/`ZG_STONE_TOKEN` path works end-to-end
      against a local moss (`cargo run`-level test acceptable; paste the transcript).
- [ ] `grep -rn "changeme" src/` → empty.
- [ ] seeds-endpoint drift fixed (`grep -c "banks/{moniker}/seeds" src/moss/src/bootstrap/router.rs` → 2).
- [ ] `cargo test --workspace` green; new tests listed in the report.
- [ ] `docs/security/stone-token.md` written; FINDINGS.md notes the preseed follow-up.

## Out of scope

The declarative route-table refactor (prompt 11 — keep your changes minimally invasive to both route
fns). Pond-by-default first-boot UX. Rate limiting. Preseed/SSH hardening beyond the doc correction.
TLS for :7185.
