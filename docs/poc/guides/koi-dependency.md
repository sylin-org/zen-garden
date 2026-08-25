# Koi Dependency Mode

**Purpose:** Explain how Zen Garden consumes the koi framework, and the exact procedure to switch from local-path development back to published crates.io versions.
**Audience:** Maintainers

---

## Current mode: local path dependencies

Zen Garden depends on koi ([github.com/sylin-org/koi](https://github.com/sylin-org/koi)) via **path
dependencies** to a sibling `../koi` checkout, declared in the root `Cargo.toml`
`[workspace.dependencies]`:

```toml
koi-embedded = { path = "../koi/crates/koi-embedded", default-features = false }
koi-certmesh = { path = "../koi/crates/koi-certmesh", default-features = false }
koi-common = { path = "../koi/crates/koi-common" }
koi-crypto = { path = "../koi/crates/koi-crypto", default-features = false, features = ["qr"] }
koi-truststore = { path = "../koi/crates/koi-truststore" }
```

**Why path deps:** koi and Zen Garden are co-developed. Zen Garden exercises koi's APIs and surfaces
real-world gaps, and koi changes land continuously. Pinning to published crates.io versions would
force a koi publish on every change — an artificial block during this phase.

**Trade-off:** a fresh `git clone` of Zen Garden needs a sibling `../koi` checkout to build; it is not
buildable from crates.io alone. This intentionally suspends the clean-clone-from-crates property until
koi stabilizes.

### Lean feature selection

`default-features = false` drops koi's optional `docker` (bollard) and `keyring` (OS credential store)
backends — Zen Garden (moss) runs its own bollard for Docker, and koi's vault falls back to its
machine-bound passphrase backend, which suits headless stones. `qr` is armed on `koi-crypto` so the
pond ceremony still renders PNG QR codes. Keep this feature selection when switching back to crates.

### Data path ownership

koi owns its data paths as a single source of truth: `CertmeshCore` holds a `CertmeshPaths` value
object resolved once at the composition root and injected. Zen Garden reads koi paths via
`core.paths()` — never `CertmeshPaths::default()` (removed in koi) — and provides the koi data root
once via `Builder::data_dir(data_dir()/koi)` in `src/moss/src/bootstrap/run.rs`. Everything else
(pond handlers, the ceremony rules, chirp verification) derives from that injected root.

---

## Switching back to crates.io (once koi stabilizes)

When koi reaches a stable, published release:

1. **Publish koi** to crates.io at a stable version `X.Y.Z` — every koi crate Zen Garden uses
   (`koi-embedded`, `koi-certmesh`, `koi-common`, `koi-crypto`, `koi-truststore`) **plus their
   transitive koi crates**, as one consistent set. Confirm:
   `curl https://index.crates.io/ko/i-/koi-embedded` lists `X.Y.Z`.
2. **Flip path deps → version deps** in the root `Cargo.toml` `[workspace.dependencies]`, keeping the
   lean feature selection:
   ```toml
   koi-embedded = { version = "X.Y", default-features = false }
   koi-certmesh = { version = "X.Y", default-features = false }
   koi-common = "X.Y"
   koi-crypto = { version = "X.Y", default-features = false, features = ["qr"] }
   koi-truststore = "X.Y"
   ```
3. **(Optional) Re-enable the maintainer local-koi override** so koi can still be hacked on without
   re-introducing path deps: add `include = [{ path = "config.local.toml", optional = true }]` to the
   committed `.cargo/config.toml`; commit `.cargo/config.local.toml.example` (a `[patch.crates-io]`
   block pointing every koi crate at `../koi/crates/*`); gitignore `.cargo/config.local.toml`. A clean
   clone skips the optional include and resolves koi from crates.io; the maintainer copies the example
   to `config.local.toml` to opt into local koi.
4. **Regenerate the lock and verify clean-clone-true:**
   ```bash
   cargo check --workspace
   # In a directory with NO sibling ../koi:
   git clone . /tmp/zg-clean && cd /tmp/zg-clean && cargo check --workspace
   ```
5. **Update this guide** to describe the crates mode as current, and remove the path-dep note from the
   koi block in `Cargo.toml`.

---

## Verifying which mode is active

```bash
grep -n "koi-embedded" Cargo.toml            # `path = ...` => local code;  `version = ...` => crates
grep -A2 'name = "koi-embedded"' Cargo.lock  # no `source` line => path dep; `registry+...` => crates
```
