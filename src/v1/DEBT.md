# DEBT — borrowed shortcuts (RC0 gates on zero-open)

| id | borrowed | pays at | notes |
|----|----------|---------|-------|
| D2 | Chirp wire fixtures are hand-written, not captured from the live fleet | Only if a cross-generation bridge is ever attempted | OfferingFqn encoding and v0's Option-missing tolerance are unverified against real PoC datagrams. With v1 owning its topology (D1 closed) there is no default contact with PoC stones; format fixtures remain the guard for on-media compat |
| D3 | No directed-broadcast fallback (multicast + unicast only) | Windows multi-homed support | COMM-0001..3 rationale; PoC p2p.rs is the reference |
| D4 | `STONE_DETAIL` capability beacon designed, not implemented | Placement features | v1-only type; harmless by construction |
| D5 | MCP surface deferred | Charter RC0 | HTTP observe/find first; contract crate is the generation source |
| D7 | ~~Ingest/dispatch counters exist but no HTTP surface reads them yet~~ | — | **Closed 2026-08-25**: `/api/v1/local/posture` serves live ingest/dispatch/topology counters (B3) |
| D8 | Same-host stones rely on `SO_REUSEADDR` sharing one discovery port | Cross-platform verification | Witnessed on Windows only; Unix needs SO_REUSEPORT or per-host single-stone discipline; verify in Linux CI/container |
| D9 | Appliance host renames unimplemented (L23): identity records modality, nothing mutates hostnames yet | Dedicated-hardware installer (NewStone v1) | PoC parity lives in poc/moss/src/bootstrap/first_boot.rs: set hostname + hosts file at first boot, appliance stones only |
| D10 | Runtime adapters: docker DONE (O1, no events-stream yet); podman/systemd pending | Podman on demand; events with O2 reconcile | Seam in OFFERINGS.md §4; PoC touchpoint list is the porting checklist |
| D11 | Ceremonies (nourish/vacate/replant/store journals + rollback) deferred | Post-O2 | PoC reference poc/moss/src/domain/ceremony/ |
| D12 | Orchestration roles/elections (primary/replica/joining) deferred | Post-O2 | ORCH-0001/0006 in PoC; wire `role` field already carried |
| D13 | Borrow credentials vaulting (Koi vault keys) deferred | M6 (Koi integration) | PoC stored `borrowed:{name}:credentials` keys; v1 has no vault yet |
| D14 | ~~`start()` cannot hold ledged host ports on containers created with dynamic bindings~~ | — | **Closed 2026-08-27** — W5 witnessed the whole chain live on stone-crystalline-dune: arbiter drew home 7300 (tier flexible), the allocation rode the stored spec, Docker bound it explicitly, and rest/wake re-emitted the SAME ledgered home (WITNESSES.md W5). Ledger wins over sockets (L26) |
| D17 | Capability model files are written by the in-container uid 0 — host-side non-root uproot cannot delete them | RC0 (uproot hygiene) | Witnessed W10: the garden's own volume dirs survived `rm -rf` as stone; cleared via a throwaway container. Options: moss pre-chowns, world runs user-mapped, or uproot uses the world |
| D18 | Release artifacts carry sha256 checksums only — no cryptographic signature | M6 (Koi integration: key ceremony + verify-chain) | The M1 trust anchor is TLS + the checksum manifest; B2's named trust anchor lands with M2 signing |
| ~~D15~~ | ~~Catalog corpus lacks capture declarations~~ | — | **Closed 2026-08-29 (D15 slice):** all 44 volume-bearing manifests declare their living will — export for dump-capable engines (postgres/mariadb/mongo), flush-quiesced lock-and-copy for ES/OS, copy-freely lock-and-copy for flat-file state (grammar amended: hooks optional, resume-without-quiesce still refused), stateless for re-fetchable model caches. Enforced by `every_stateful_manifest_declares_its_living_will` over the embedded floor | RC0 | ADR-0005's own review duty. Until declared, such offerings surface as capture-untrusted (never silently tarred) | 

## Closed
| id | was | closed |
|----|-----|--------|
| D1 | v1 defaulted to the PoC-shared discovery room, gated behind `--isolate` | 2026-08-25 — charter amendment: v1 owns its topology (`7284`/`239.255.42.199`, block 7284–7299). The PoC proved the mechanisms work; v1 chooses its room deliberately (L20). No shared-room contact by construction |
| D6 | `stone_id` generated per boot, not persisted | 2026-08-25 — `~/.zen-garden/identity.json`: GUIDv7 minted once, immutable forever; poetical name from glossary::naming (PoC dictionaries), collision-checked against the room; explicit `--stone-name` = operator rename intent; `host_modality` records companion/appliance (L23) |
