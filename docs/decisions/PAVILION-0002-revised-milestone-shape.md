---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-05-05
canonical: true
---

# PAVILION-0002: Revised Milestone Shape — Native UI, Settings as Keystone, Facilitators-as-Sibling

**Status**: Accepted
**Date**: 2026-05-05
**Deciders**: Architecture
**Tags**: pavilion, milestones, ux, cloud-filter, settings, facilitators

---

## Context

[PAVILION-0001](PAVILION-0001-windows-client-separation.md) established Pavilion as a Tauri 2 Windows tray client and sketched a phased delivery — M0 scaffolding, M1 MVP with Lantern hosted via child-process bridge, M2 premium polish with 3D topology and ceremonies, etc. Several of those bullets carried hidden weight that only became visible after building M0/M0.5 against a live garden:

1. **Cloud Filter is half a system per callback.** The move/rename callback shipped today required a server-side endpoint, a typed client method, parent-dir auto-creation, and provider-side path resolution — none called out in M1 as line items. The same hidden depth applies to write/upload (which still doesn't work end-to-end) and to ceremonies. Treating "Cloud Filter mount" as a single bullet under-budgets the work.

2. **The native shell beat the embedded-Lantern bridge.** PAVILION-0001 §"Module structure" planned to ship M1 by spawning Lantern as a child and pointing WebView at `localhost:7186`. By the time the M0.5 native shell (tiles, lists, awareness, tending, activity) was running, the bridge had lost its appeal: the native surface using direct Tauri IPC is cleaner than embedding a browser dashboard, the two UIs already use different conventions (severity pips vs Lantern's 3D topology), and the M3 "hoist frontend package" cleanup was shaping up as a refactor we'd rather not pay.

3. **Settings is the keystone behind quiet hours, suppressions, autostart toggle, and facilitator dismissal.** The Announcer already has `STARTUP_QUIET_WINDOW` plus TODO hooks for quiet hours / per-source dismissal / cooldowns; the spec's facilitators rely on persistent "Hide this kind" decisions; the user-visible autostart toggle needs a settings UI. PAVILION-0001 listed Settings under M0 but didn't elaborate — in practice every M1+ piece has a dependency on it.

4. **Facilitators are a sibling pipeline of the Announcer, not a separate subsystem.** The interaction-design spec §5 calls them "suggestion sources" feeding a "suggestion engine." That's the same shape as today's `GardenEvent → Announcer → ActivityStore + ToastDispatcher`. Treating them as a parallel subsystem in M2 inflates surface area; reusing the Announcer's policy seam is cheaper and keeps the dedupe/cooldown/settings-suppression patterns shared.

5. **Pond ceremonies entail a state-machine refactor with Rake.** The interaction-design spec wants Pavilion modals to share Rake's `ceremony_render.rs` state machine. Bundling the extraction into M1 inflates the MVP. M1 should ship with onboarding (single-stone tend) and reads/writes; M2 takes on the multi-step ceremonies and the state-machine extraction.

This ADR refines PAVILION-0001's phased delivery without superseding the architectural decisions in §"Decision" (Tauri 2 client, separation from Moss, StoneApi reuse). PAVILION-0001 §"Phased Delivery" is replaced by the shape below.

---

## Decision

Five refinements to the milestone shape:

### 1. Drop the child-process Lantern hosting plan

Pavilion's UI is **native end-to-end** — Tauri webview rendering Pavilion's own React surface, fed by Tauri IPC commands. No child Lantern process, no `localhost:7186` bridge. Lantern continues to exist as the browser-side dashboard for visiting and read-only use cases.

Targeted exception: the **3D garden topology** component in M2 is hoisted from Lantern as a single shared package. Rebuilding three.js + r3f scene composition from scratch is the one place the bridge plan's reuse argument actually pays off; for the rest of the dashboard the native surface wins.

### 2. Promote Settings to an M0.5 blocker

Settings ships before any further announcer / facilitator policy work. Required surface for M0.5:

- Persistence to `~/.zen-garden/.pavilion-settings.json` (parallel to `.tending`).
- Typed fields the Announcer needs: `quiet_hours { start, end, enabled }`, `suppressed_kinds: Vec<String>`, `cooldown_overrides: HashMap<String, Duration>`.
- Typed fields the OS surfaces need: `autostart_enabled`, `theme: Auto | Dark | Light`.
- Tauri commands `get_settings`, `set_settings(SettingsPatch)`.
- Frontend Settings view that round-trips the values.

The Announcer's `past_warmup` gate grows to `should_promote(severity, kind)` reading the settings store.

### 3. Add Cloud Filter upload to M1 critical path

Today's move/rename work covers Explorer's manipulation of existing files. Dragging a *new* file from outside the sync root into `Zen Garden\storage\` doesn't push to the server — it stays as a non-placeholder. That gap violates the user's expectation of Explorer behaviour.

Implementation path uses what Cloud Filter already gives us:

- `state_changed` callback fires when the user creates new files / folders under the sync root.
- Provider diffs those changes against the known placeholder set; new entries are local-only.
- For each new file: PUT to `/api/v1/garden/storage/{name}/fs/{path}`, then convert the local file into a placeholder via `cloud_filter`'s convert API.
- For each new directory: serverside MKCOL-equivalent (a dedicated endpoint, since we don't auto-create empty dirs today).

This becomes an M1 line item, not background polish.

### 4. Treat Facilitators as a sibling of the Announcer

Instead of a parallel subsystem in M2, facilitators reuse the Announcer's shape:

```text
SuggestionSource ──► FacilitatorEngine ──┬──► InlineRenderer (banner / card / link)
                     (policy)             │
                                          └──► DismissalStore (settings-backed)
```

Same producer/policy/dismissal pattern as `GardenEvent → Announcer → ActivityStore + ToastDispatcher`. M1 ships with one or two suggestion sources (`tend-a-stone-if-none-tended`, `enroll-into-pond-if-2+-stones-and-no-pond`) — enough to validate the pattern without overcommitting. M2 grows the source set per spec §5.

### 5. Move pond ceremonies to M2

Ceremonies require extracting Rake's `ceremony_render.rs` state machine into a shared crate (likely `garden-common::ceremony` or a new `garden-ceremony` crate) so both Rake's terminal renderer and Pavilion's modal renderer drive the same model. The extraction is real refactor cost.

M1 ships with onboarding (single-stone `tend` UX) and reads/writes; M2 takes on pond init / join / invite / unlock as modal flows backed by the shared state machine. Replant likewise lives in M2 because it's drag-initiated and renders progress as a modal — same modal-flow pattern as ceremonies.

---

## Phased Delivery (replaces PAVILION-0001 §"Phased Delivery")

### M0 — Scaffold (shipped)

Tray icon, single-instance, autostart plugin loaded, awareness (chirp + provoked discovery + TTL eviction), tending file shared with Rake, native dashboard shell with Stones / Storage / Services / Pond tiles, Cloud Filter sync root registered with read + list + delete callbacks, holistic Cloud Filter callbacks (delete dir, rename file or dir).

### M0.5 — Close the gaps before M1

- **Settings store** (this ADR §"Decision" #2) — keystone for everything Announcer-shaped.
- **Cloud Filter upload** (#3) — `state_changed`-driven push for newly-created files and directories.
- **Activity view polish** — render the existing `get_activity` ring buffer as a scrollable destination, not a sidebar list slice on Home.
- **Storage browse inline** — directory listing for the tended bank, not just count.

### M1 — MVP (shippable)

- **Onboarding flow** — first-launch picker for which stone to tend; auto-tend retained for warm starts.
- **Command palette (`Ctrl+K`)** — fuzzy search across awareness + tended-stone state + a small action registry (~few hundred lines, highest leverage).
- **Native services view** — replicate the Storage tile pattern for services on the tended stone.
- **Native pond status view** — read-only display of pond membership, expiry, cornerstone.
- **Facilitators v0** — 1–2 suggestion sources wired through the new `FacilitatorEngine`.
- **Toasts pipeline complete** — quiet hours respected, per-source dismissal stored, cooldowns enforced.

### M2 — Premium

- **Pond ceremonies** — modal flows for `init`, `join`, `invite`, `unlock` driven by the shared state machine extracted from Rake.
- **3D garden topology** — hoisted as a single shared component from Lantern.
- **Pavilion-internal drag-drop** — stone → pond, bank → bank, etc. (Cloud Filter handles in-Explorer drag; this is the in-app surface.)
- **Replant ceremony** — drag-initiated, modal progress, shares state-machine pattern with pond ceremonies.
- **Multi-stone aggregation views** — garden-wide activity feed merging events from all reachable stones.
- **Tray popover polish** — Win11 acrylic backdrop on the popover only (the spec explicitly calls out NO Mica on the main window).

### M3 — Power user

- **Multi-garden switcher** — top-bar pill becomes a picker; supports multiple `.tending` profiles.
- **Embedded terminal** — Rake CLI in a panel for power users.
- **Keyboard navigation pass** — every action reachable from the keyboard with visible focus rings.
- **Jump lists** — Windows taskbar context menu shortcuts.

### M4 — Cross-platform (deferred)

macOS File Provider extension; Linux FUSE provider. Same Tauri shell, OS-specific integration adapters.

### M5 — Phase 4 trust upgrade

Pavilion enrols a per-installation client certificate via the new Moss endpoint; mTLS becomes the default transport.

---

## Rationale

- **Native UI all the way** is the simpler architecture once you're already building it. The bridge plan was a hedge against duplicated effort, but the duplication never materialised — the surfaces diverge in conventions, and direct IPC is cleaner than spawning a browser as a sidecar.
- **Settings before facilitators** because every "calm by default" promise depends on the user's settings being persistent. Without it, every announcer call has a TODO comment and the user can't actually configure their notifications.
- **Cloud Filter upload in M1** because a one-way mirror is broken UX. Users will drop files into the sync root and expect them to appear server-side. The `state_changed` infrastructure is already documented in cloud-filter; the work is integrating it.
- **Facilitators as sibling** is just code reuse — same shape, same policy seam, less surface area to maintain.
- **Ceremonies in M2** because the state-machine refactor isn't an MVP concern. A user can complete the most-common pond flow via Rake while M1 is in users' hands; promoting to M2 makes M1 actually shippable.

---

## Consequences

### Positive

- M1 ships sooner because ceremonies and Lantern hoisting are out of scope.
- Settings becomes a clear keystone with one home, not a feature scattered across milestones.
- Facilitators reuse the Announcer's tested policy patterns instead of duplicating them.
- Cloud Filter becomes a coherent feature (read + write + manipulate), not "mount" plus indefinite gaps.

### Negative

- M0.5 grows to swallow Settings + Upload before M1 work resumes — users see less feature velocity for a couple of weeks.
- The 3D topology hoist is a targeted exception to the "no child-process Lantern" rule, which adds nuance to the share-vs-rebuild decision matrix for future components.
- Ceremony users on Pavilion wait for M2 — Rake remains the canonical surface for pond flows in M1.

### Neutral

- The PAVILION-0001 module structure (`src/pavilion/src/{ipc, integration, client, settings.rs}`) is unchanged. This ADR refines what gets built in each phase, not the layout.
- StoneApi reuse policy is unchanged.

---

## Alternatives Considered

### Alternative 1: Ship the child-process Lantern bridge as planned

- **Pros**: Less rebuild work for the Garden / Activity / Storage views; a working full UI immediately.
- **Cons**: Two divergent UIs to maintain through M3; bridge fragility (process lifetime, IPC); already-established native conventions (severity pips, tray-popover) don't transfer; tray app embedding a browser is awkward.
- **Rejected because**: After building the native shell, the bridge looks like more total work, not less.

### Alternative 2: Defer Settings to M2 and stub quiet-hours / suppressions

- **Pros**: M1 ships sooner, Settings UI work doesn't block onboarding.
- **Cons**: Every Announcer / Facilitator call has a hardcoded policy; user can't actually configure notifications; "calm by default" becomes "calm because we said so."
- **Rejected because**: The Announcer's policy seam is already there with TODOs — finishing it costs less than carrying the half-built state through M1.

### Alternative 3: Skip Cloud Filter upload until M2

- **Pros**: M1 ships read + manipulate; upload comes with the broader storage browsing work.
- **Cons**: Users will drop files into the sync root in M1 and the files vanish at the next reboot or sync — that's a data-loss footgun, not a missing feature.
- **Rejected because**: It's a contract violation, not a missing feature.

### Alternative 4: Implement ceremonies in M1 via a Pavilion-specific state machine

- **Pros**: M1 has full pond support without waiting for the Rake refactor.
- **Cons**: Two state machines to maintain forever; the spec's "ceremonies are ceremonies" promise becomes a lie.
- **Rejected because**: Saving M1 ship time isn't worth permanent duplication of a load-bearing state machine.

---

## References

- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Architectural foundation; this ADR refines its phased delivery only.
- [pavilion-interaction-design](../specs/pavilion-interaction-design.md) — Visual / interaction language; unchanged.
- [DISC-0001](DISC-0001-discovery-as-first-class-crate.md) — Discovery primitive Pavilion consumes.
- [STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md), [STORAGE-0012](STORAGE-0012-cloud-filter-rebuild.md) — Cloud Filter architecture Pavilion inherits.
- [src/pavilion/src/announce/](../../src/pavilion/src/announce/) — Pipeline pattern that facilitators will mirror.
- [src/rake/src/commands/ceremony_render.rs](../../src/rake/src/commands/ceremony_render.rs) — State machine to be extracted in M2.
