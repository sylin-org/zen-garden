# DEBT — borrowed shortcuts (RC0 gates on zero-open)

| id | borrowed | pays at | notes |
|----|----------|---------|-------|
| D1 | v1 experiments must run `--isolate`; default discovery group/port are PoC-shared | Interop milestone | R0.5 compat is designed but not yet fleet-proven; until then v1 chirps would be adopted by production stones |
| D2 | Chirp wire fixtures are hand-written, not captured from the live fleet | First interop test | OfferingFqn encoding and v0's Option-missing tolerance are unverified against real datagrams; capture with PoC rake + packet sniff |
| D3 | No directed-broadcast fallback (multicast + unicast only) | Windows multi-homed support | COMM-0001..3 rationale; PoC p2p.rs is the reference |
| D4 | `STONE_DETAIL` capability beacon designed, not implemented | Placement features | v1-only type; v0 stones ignore unknown types |
| D5 | MCP surface deferred | Charter RC0 | HTTP observe/find first; contract crate is the generation source |
| D6 | `stone_id` generated per boot, not persisted | First on-media/identity milestone | PoC persisted GUIDv7 at Phase 0; v1 proto regenerates — peers see a new stone per restart until persisted |
