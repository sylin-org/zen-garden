# PORTRAIT-0003: Seed Bank Visibility Signals

**Status:** Accepted
**Date:** 2026-02-05
**Supersedes:** PORTRAIT-0002 (candidate label wording)
**Related:** PORTRAIT-0001 (Stone Landing Page), PORTRAIT-0002 (Expandable Panels), STORAGE-0003 (Beacon Protocol)

---

## Executive Summary

Seed bank visibility in the portrait must be unambiguous: local seed banks belong on the stone portrait, remote seed banks belong in the horizon. Candidates are the same object in an earlier life stage, so they appear alongside seed banks with a clear label and hopeful styling.

---

## Decision

### 1. Stone Portrait (Local)
- **Seed Banks list** shows **locally connected seed banks** plus **candidate devices**.
- Candidates are labeled **`[Candidate]`** and styled in the hopeful palette.
- No separate "Local Seed Banks" counter; the list itself is the signal.

### 2. Horizon (Remote)
- Horizon header shows compressed remote info: **`N stones visible | M seed banks`**.
- Horizon list displays a **sprout indicator (seedling icon)** next to any stone with at least one connected seed bank.
- Horizon counts and indicators **exclude the local stone** and use **fresh remote data only**.

### 3. Data Sources
- Use **TopologyCache** for visible stones (online only).
- Use **StorageCache** (beacons) to determine remote seed bank counts and per-stone presence.

---

## Rationale

- **Clarity of scope:** local view shows what is attached here; horizon shows what exists elsewhere.
- **State continuity:** candidates are seed banks in a prepped-not-yet-mounted state.
- **Trustworthy signal:** only show what is visible right now; avoid stale or local leakage into horizon.

---

## Consequences

### Positive
- Users can immediately tell whether storage exists elsewhere in the garden.
- Local seed bank state and candidate readiness are visible without extra counters.
- A consistent lifecycle story: candidate → seed bank.

### Negative
- Horizon seed bank count can be temporarily zero until storage beacons arrive.
- Requires a small extension to portrait JSON payloads.

---

## Implementation Notes

- Extend `PortraitHorizon` with `seed_bank_count`.
- Extend `HorizonStone` with `has_seed_banks`.
- Update portrait UI to:
  - Render `[Candidate]` status.
  - Render sprout indicator next to stone names.
  - Display `N stones visible | M seed banks` in the horizon header.

---

## References

- [PORTRAIT-0001](PORTRAIT-0001-stone-landing-page.md)
- [PORTRAIT-0002](PORTRAIT-0002-expandable-panels.md)
- [STORAGE-0003](STORAGE-0003-beacon-protocol.md)
