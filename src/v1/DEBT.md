# DEBT — borrowed shortcuts (RC0 gates on zero-open)

| id | borrowed | pays at | notes |
|----|----------|---------|-------|
| D2 | Chirp wire fixtures are hand-written, not captured from the live fleet | Only if a cross-generation bridge is ever attempted | OfferingFqn encoding and v0's Option-missing tolerance are unverified against real PoC datagrams. With v1 owning its topology (D1 closed) there is no default contact with PoC stones; format fixtures remain the guard for on-media compat |
| D3 | No directed-broadcast fallback (multicast + unicast only) | Windows multi-homed support | COMM-0001..3 rationale; PoC p2p.rs is the reference |
| D4 | `STONE_DETAIL` capability beacon designed, not implemented | Placement features | v1-only type; harmless by construction |
| D5 | MCP surface deferred | Charter RC0 | HTTP observe/find first; contract crate is the generation source |
| D6 | `stone_id` generated per boot, not persisted | First on-media/identity milestone | PoC persisted GUIDv7 at Phase 0; v1 proto regenerates — peers see a new stone per restart until persisted |
| D7 | ~~Ingest/dispatch counters exist but no HTTP surface reads them yet~~ | — | **Closed 2026-08-25**: `/api/v1/local/posture` serves live ingest/dispatch/topology counters (B3) |
| D8 | Same-host stones rely on `SO_REUSEADDR` sharing one discovery port | Cross-platform verification | Witnessed on Windows only; Unix needs SO_REUSEPORT or per-host single-stone discipline; verify in Linux CI/container |
| D9 | Appliance host renames unimplemented (L23): identity records modality, nothing mutates hostnames yet | Dedicated-hardware installer (NewStone v1) | PoC parity lives in poc/moss/src/bootstrap/first_boot.rs: set hostname + hosts file at first boot, appliance stones only |
| D10 | Runtime adapters: docker DONE (O1, no events-stream yet); podman/systemd pending | Podman on demand; events with O2 reconcile | Seam in OFFERINGS.md §4; PoC touchpoint list is the porting checklist |
| D11 | Ceremonies (nourish/vacate/replant/store journals + rollback) deferred | Post-O2 | PoC reference poc/moss/src/domain/ceremony/ |
| D12 | Orchestration roles/elections (primary/replica/joining) deferred | Post-O2 | ORCH-0001/0006 in PoC; wire `role` field already carried |
| D13 | Borrow credentials vaulting (Koi vault keys) deferred | With borrow feature (O3) | PoC stored `borrowed:{name}:credentials` keys; v1 has no vault yet |

## Closed

| id | was | closed |
|----|-----|--------|
| D1 | v1 defaulted to the PoC-shared discovery room, gated behind `--isolate` | 2026-08-25 — charter amendment: v1 owns its topology (`7284`/`239.255.42.199`, block 7284–7299). The PoC proved the mechanisms work; v1 chooses its room deliberately (L20). No shared-room contact by construction |
| D6 | `stone_id` generated per boot, not persisted | 2026-08-25 — `~/.zen-garden/identity.json`: GUIDv7 minted once, immutable forever; poetical name from glossary::naming (PoC dictionaries), collision-checked against the room; explicit `--stone-name` = operator rename intent; `host_modality` records companion/appliance (L23) |
