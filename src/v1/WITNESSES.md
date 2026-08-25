# WITNESSES — live fleet proof

The bar. A v1 milestone isn't done until it matches or beats the PoC's
opening score. Recorded in the inventory tradition: what happened, what
proved it.

## PoC bar (2026-08-25, stone-leaded-sparkle workstation + 5-stone fleet)

- **Unprompted failover**: rake query hit sleeping stone, auto re-resolved,
  answered by another. (inventory: clients-companions)
- **Wipe recovery**: containers vanished → boot reconcile pulled 6 images,
  recreated with ports preserved, dormant/running desired-state honored.
- **Goodbye observed**: emerald-vale goodbye ×3 in pulse feed, then return —
  graceful restart seen garden-wide in seconds.
- **Adopted-service loop**: ollama::adopted flipped failing → thriving live.
- **Wall monitor**: pulse rendering 6/6 garden with per-peer freshness at
  32 evt/min.

## v1

- **W1 — two stones meet, and ask** (2026-08-25, isolated room
  239.255.42.199:7284, Windows workstation, `garden` debug build).
  `stone-smoke-alpha` boots, chirps, and *asks the room who's here*;
  ten seconds later `stone-smoke-beta` does the same. Both `/api/v1/observe`
  views list both stones within ~2s of beta's boot — no heartbeat waited:
  - beta heard alpha's `discovery_response` instantly (alpha as
    `health=starting` hint — honest: capabilities unknown until alpha's own
    chirp arrives) while already holding beta's full truth from beta's chirp;
  - alpha answered beta's request via multicast (`answered discovery request`
    in the log), matching PoC moss behavior where the whole room hears every
    answer;
  - chirps carry v1 markers (`proto: zg/1`, monotonic `seq`, per-boot
    `boot_id`) over the v0-compatible core.
  - Also witnessed en route: L17 abort fired exactly as designed on a port
    collision (`startup step 'ingress-bind' failed: os error 10048`) — loud,
    named, no half-garden.
  - Wire correction caught before first contact: discriminators are lowercase
    on the v0 wire; v1's constants were fixed and re-pinned by fixture (L19).
  - Not yet witnessed: graceful-goodbye path (force-killed here; needs a
    console ctrl_c harness), expiry, cross-machine delivery, PoC interop.
