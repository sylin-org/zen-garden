# PORTRAIT-0002: Expandable Panels

**Status:** Accepted
**Date:** 2026-02-03
**Supersedes:** None
**Related:** PORTRAIT-0001 (Stone Landing Page)

---

## Executive Summary

The Stone Portrait (PORTRAIT-0001) is intentionally observational — a "portrait, not dashboard." However, beginner users need a gentle path to common actions: preparing seed banks, resting/waking offerings, managing companions. This decision introduces **expandable panels** that preserve the portrait's glanceable nature while adding depth on demand.

**Core principle:** Information is always visible. Actions are revealed through intent.

---

## Problem Statement

The current portrait shows state but offers no path to action. Users must:

1. Learn CLI commands (`garden-rake service rest mongodb`)
2. Know API endpoints (`POST /api/v1/stone/services/mongodb/rest`)
3. Leave the portrait to perform basic operations

For candidate seed banks (empty USB drives awaiting preparation), this is particularly problematic — the portrait doesn't show them at all, yet they represent an immediate opportunity for action.

**Goal:** Enable common operations without transforming the portrait into a control panel.

---

## Design Philosophy

### Specialist Team Assessment

**Semiotics (Dr. Mira Chen):**
> "A chevron is a disclosure indicator — universally understood as 'more here if you want it.' It doesn't demand attention; it invites exploration. The portrait remains observational by default, becomes interactive on intent."

**Semantics (Prof. Theo Nakamura):**
> "The confirmation hint must be outcome-oriented: 'Again to put to rest' tells you what will happen. And we stay verb-agnostic — 'Again to...' works for tap, click, or keyboard."

**Operations (Priya Sharma):**
> "The two-second delay prevents accidental double-taps on wall-mounted touchscreens. And critically — no destructive actions. Rest/Wake/Release are reversible. Remove/Destroy stay in the CLI."

**UX (Elena Rodriguez):**
> "Right-align all actions. This creates spatial grammar: left hemisphere for observation, right hemisphere for consequence. The eye scans information first, arrives at actions second."

**DX (Marcus Webb):**
> "A unified panel renderer means consistent behavior everywhere. One pattern to learn, one pattern to test."

### The Spatial Principle

**Information left, consequences right.**

```
┌─────────────────────────────────────────────────────────────────┐
│  ◀─────────── OBSERVE ───────────▶ │ ◀────── ACT ──────▶       │
│                                    │                            │
│  ▶ mongodb   mongodb :27017 ● OK   │                   [Tend]   │
└─────────────────────────────────────────────────────────────────┘
```

This applies everywhere:
- Collapsed rows: info left, inline actions right
- Expanded panels: details left, action buttons right
- Confirmation hints: outcome text left, buttons right
- Error messages: message left, dismiss affordance right

---

## Specification

### 1. Expandable Rows

All list items (offerings, seed banks, companions) become expandable:

**Collapsed state:**
```
▶ mongodb        mongodb  :27017              ● HEALTHY    [Tend]
```

**Expanded state:**
```
▼ mongodb        mongodb  :27017              ● HEALTHY    [Tend]

  In-memory document database for flexible schemas.

  Image:        mongo:7.0
  Container:    zen-offering-mongodb
  Capabilities: database, document, nosql

  Again to put to rest                        [Rest]  [Restart]
```

**Chevron position:** Left of name (disclosure indicator, not action)

**Touch target:** Entire row *except* the right action zone (where [Tend] lives)

### 2. Accordion Behavior

- Opening a panel closes any other open panel in the same section
- localStorage persists the last-opened panel ID per section
- On page load, all panels start collapsed (localStorage only restores on explicit open)

### 3. Confirmation Flow

Destructive-ish actions (rest, release, down) require two-tap confirmation:

```
State 1: Default
                                              [Rest]  [Restart]

State 2: First tap on [Rest]
Again to put to rest                          [Rest]  [Restart]
                                                ↑ disabled 2s

State 3: After 2s delay
Again to put to rest                          [Rest]  [Restart]
                                                ↑ enabled

State 4: Second tap (executes)
Putting to rest...                            [Rest]  [Restart]
                                                ↑ disabled + spinner

State 5a: Success (SSE confirms)
                                              [Wake]  [Restart]

State 5b: Failure (API error)
                                              [Rest]  [Restart]
┌─────────────────────────────────────────────────────────────────┐
│ ✕  Failed: container in use by another process                  │
└─────────────────────────────────────────────────────────────────┘
```

**Confirmation hint:** Always left-aligned, verb-agnostic ("Again to...")

**Error area:**
- Appears at panel bottom
- Tap anywhere on error to dismiss
- Clay/red background with contrasting text

### 4. Actions by Entity State

| Entity | State | Available Actions |
|--------|-------|-------------------|
| Offering | running | Rest, Restart |
| Offering | stopped | Wake |
| Offering | degraded | Rest, Restart |
| Seed bank | online | Release |
| Seed bank | offline | (none) |
| Candidate | empty | Prepare |
| Candidate | has_data | (none — show warning) |
| Companion | running | Down |
| Companion | stopped | Up |

**Non-destructive actions** (Wake, Up) execute immediately — no confirmation needed.

**Destructive actions** (Rest, Release, Down, Prepare) require two-tap confirmation.

### 5. Candidates in Seed Banks Section

Candidate devices (empty USB drives) appear in the Seed Banks section with visual distinction:

```
SEED BANKS ─────────────────────────────────────────────────────

┌─────────────────────────────────────────────────────────────────┐
│  ▶ seed-coral-meadow    120 / 931 GB  btrfs · open   ● online  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  ▶ ✦ SANDISK_32GB       32 GB · ready to plant          NEW    │  ← hopeful (honey glow)
└─────────────────────────────────────────────────────────────────┘
```

**Hopeful styling:**
- Solid border with subtle honey glow
- ✦ sparkle icon before name (welcoming, not warning)
- "NEW" status label (positive framing vs. clinical "CANDIDATE")

### 6. Preparation Progress (SSE-driven)

When preparing a candidate, show real-time progress:

```
▼ ⚠ SANDISK_32GB        32 GB · empty            CANDIDATE

  Empty removable device ready for preparation.

  Mount:    /media/stone/SANDISK_32GB

  Formatting (btrfs)...                             [Cancel]
  ████████████░░░░░░░░  45%
```

**Progress phases** (from SSE events):
1. `analyzing` — Checking device eligibility
2. `formatting` — Creating filesystem
3. `mounting` — Mounting to garden path
4. `creating` — Writing manifest and structure

On completion, the candidate transforms into a normal seed bank (loses dashed border).

### 7. Header Tend Button

Move the Tend button to right-align with the glance column:

**Before:**
```
stone-coral-prairie
http://stone-coral-prairie:7185  [Tend]
```

**After:**
```
stone-coral-prairie
http://stone-coral-prairie:7185               offerings 4 🟢 1 ⚫
                                                           [Tend]
```

### 8. Visual State Signals

Entities use one of three visual states based on their condition:

**Hopeful (opportunities):**

| Condition | Example |
|-----------|---------|
| Candidate device | Empty USB drive ready to prepare |
| Available update | New version waiting to apply |
| New companion | Recently discovered companion |

**Visual treatment:** Solid border with honey glow, ✦ sparkle icon, warm/inviting

**Attention (problems):**

| Condition | Example |
|-----------|---------|
| Degraded health | Offering failing health checks |
| Offline unexpectedly | Companion that crashed |
| Error state | Failed operation |

**Visual treatment:** Dashed border, clay accent, ⚠ warning icon

---

## Accessibility

### Keyboard Navigation

- **Tab** moves focus through interactive elements
- **Enter/Space** on collapsed row → expand
- **Enter/Space** on expanded row header → collapse
- **Tab** into expanded panel → focuses first action button
- **Enter** on action button → triggers action flow
- **Escape** in panel → collapse panel

### Screen Reader Considerations

- Chevron has `aria-expanded` attribute
- Panel content has `aria-hidden` when collapsed
- Action buttons have descriptive `aria-label` including current state
- Error area has `role="alert"` for immediate announcement
- Confirmation hint uses `aria-live="polite"`

### Adaptive Hint Text

```javascript
const hintPrefix = matchMedia('(pointer: coarse)').matches
    ? 'Tap again'
    : 'Again';
// Results in: "Again to put to rest" or "Tap again to put to rest"
```

---

## Visual Design

### Panel Styling

```css
.item-panel {
    padding: 1rem 1.25rem;
    border-top: 1px solid var(--vellum-border);
    background: rgba(0, 0, 0, 0.02);
}

@media (prefers-color-scheme: dark) {
    .item-panel {
        background: rgba(255, 255, 255, 0.02);
    }
}
```

### Chevron

```css
.item-chevron {
    font-size: 0.6rem;
    color: var(--stone-400);
    transition: transform 0.2s ease;
    margin-right: 0.75rem;
    flex-shrink: 0;
}

.item-chevron.open {
    transform: rotate(90deg);
}
```

### Color Palette (Three-Tier States)

The portrait uses a warm vellum aesthetic with three semantic accent colors:

| State | Color | Hex | HSL | Use Case |
|-------|-------|-----|-----|----------|
| **Stable** | Sage | `#84a59d` | 165°, 16%, 58% | Healthy, running, online |
| **Hopeful** | Honey | `#c4b060` | 50°, 45%, 57% | Candidates, opportunities, new arrivals |
| **Attention** | Clay | `#d4a373` | 30°, 52%, 64% | Degraded, warnings, errors |

**Hopeful rationale:** Honey (#c4b060) evokes warmth and potential — like afternoon
light or seeds ready to sprout. It's distinct from sage (115° hue difference) and
clay (20° hue difference) while harmonizing with the warm vellum background.

```css
:root {
    /* Existing */
    --accent-sage: #84a59d;      /* Stable */
    --accent-clay: #d4a373;      /* Attention */

    /* New */
    --accent-hopeful: #c4b060;   /* Hopeful */
    --accent-hopeful-glow: rgba(196, 176, 96, 0.2);
}

@media (prefers-color-scheme: dark) {
    :root {
        --accent-hopeful: #d4c170;  /* Brighter for dark backgrounds */
        --accent-hopeful-glow: rgba(212, 193, 112, 0.15);
    }
}
```

### Needs-Attention (Attention State)

```css
.item.needs-attention {
    border-style: dashed;
    border-color: var(--accent-clay);
}

.attention-icon {
    color: var(--accent-clay);
    margin-right: 0.5rem;
}
```

### Hopeful State

```css
.item.hopeful {
    border-color: var(--accent-hopeful);
    box-shadow: 0 0 8px var(--accent-hopeful-glow);
}

.hopeful-icon {
    color: var(--accent-hopeful);
    margin-right: 0.5rem;
}

.item.hopeful .status {
    color: var(--accent-hopeful);
}
```

### Error Area

```css
.panel-error {
    margin-top: 1rem;
    padding: 0.75rem 1rem;
    background: var(--accent-clay);
    color: white;
    border-radius: 3px;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    cursor: pointer;
}

.panel-error::before {
    content: '✕';
    opacity: 0.7;
}
```

### Touch Targets

```css
.action-btn {
    min-height: 44px;
    min-width: 44px;
    padding: 0.5rem 1rem;
}

.item-header {
    min-height: 44px;  /* Entire row is touch target */
}
```

---

## Implementation

### API Dependencies

All required endpoints exist:

| Action | Endpoint | Status |
|--------|----------|--------|
| List candidates | `GET /api/v1/stone/storage/candidates` | Ready |
| Prepare seed bank | `POST /api/v1/stone/storage/prepare` | Ready (async + SSE) |
| Release seed bank | `POST /api/v1/stone/storage/:id/release` | Ready |
| Rest offering | `POST /api/v1/stone/services/:service/rest` | Ready |
| Wake offering | `POST /api/v1/stone/services/:service/wake` | Ready |
| Restart offering | `POST /api/v1/stone/services/:service/restart` | Ready |
| Companion up | `POST /api/v1/stone/companions/:id/up` | Ready |
| Companion down | `POST /api/v1/stone/companions/:id/down` | Ready |

### Portrait Data Changes

`GET /api/v1/stone/portrait` response adds:

```json
{
  "seed_banks": [...],
  "candidates": [
    {
      "device": "/dev/sdb1",
      "label": "SANDISK_32GB",
      "capacity_gb": 32,
      "state": "empty",
      "mount_path": "/media/stone/SANDISK_32GB"
    }
  ]
}
```

### SSE Events Used

| Event | Purpose |
|-------|---------|
| `service.started` | Confirm wake completed |
| `service.stopped` | Confirm rest completed |
| `storage.prepared` | Candidate → seed bank transition |
| `storage.prepare.progress` | Progress bar updates |
| `storage.released` | Confirm release completed |

### File Changes Required

1. **`src/moss/src/api/v1/portrait.rs`**
   - Add `candidates` field to `PortraitResponse`
   - Call `list_candidates()` in `get_portrait_data()`

2. **`src/moss/assets/portrait.html`**
   - Add panel rendering infrastructure
   - Add accordion state management
   - Add confirmation flow logic
   - Add error area component
   - Update header Tend button position
   - Add candidate styling

---

## Checklist

### Backend

- [ ] Add `candidates` to portrait response struct
- [ ] Include candidates in portrait data handler
- [ ] Ensure SSE events include sufficient detail for UI updates

### Frontend

- [ ] Implement `renderPanel(type, entity, isExpanded)` function
- [ ] Add chevron to all list items
- [ ] Implement accordion behavior with localStorage
- [ ] Implement two-tap confirmation flow
- [ ] Implement error area with tap-to-dismiss
- [ ] Add needs-attention styling for candidates
- [ ] Add preparation progress bar (SSE-driven)
- [ ] Move header Tend button to right-aligned position
- [ ] Add keyboard navigation
- [ ] Add ARIA attributes for accessibility
- [ ] Test touch targets on mobile

### Testing

- [ ] Verify accordion closes other panels
- [ ] Verify confirmation flow timing (2s delay)
- [ ] Verify error dismissal
- [ ] Verify SSE updates reflect in UI
- [ ] Verify keyboard-only navigation works
- [ ] Verify screen reader announces state changes
- [ ] Test on touch device (wall-mounted scenario)

---

## References

- [PORTRAIT-0001: Stone Landing Page](PORTRAIT-0001-stone-landing-page.md)
- [Seed Banks Guide](../guides/seed-banks.md)
- [STORAGE-0001: Seed Bank Onboarding](../specs/STORAGE-0001-seed-bank-onboarding.md)
- [API Reference: Services](../reference/api.md#services)
