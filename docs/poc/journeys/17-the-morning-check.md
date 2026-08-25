# The Morning Check

*Coffee in hand, you run `nourish`.*

---

## The Story

It's become a ritual. Every morning, while the coffee brews, you open a terminal and run:

```bash
garden-rake nourish
```

```
Checking garden for updates...

📦 Garden-wide Update Status

Summary: 2 available, 0 blocked

───────────────────────────────────────────────

  stone-amber-ridge
    AVAILABLE:
      • redis 7.2.5 → 7.2.6

  stone-coral-reef
    AVAILABLE:
      • nginx 1.25.3 → 1.25.4

───────────────────────────────────────────────

Apply updates:
  [A] All updates
  [O] Offerings only
  [F] Firmware only
  [S] Select specific
  [Q] Cancel

Choice:
```

Two updates available. Both minor patches. Nothing urgent.

You press Q. Not today—you're about to leave for work. But now you know: the garden is slightly behind, and you can update this weekend.

---

Some mornings are different:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 3 available, 1 blocked

───────────────────────────────────────────────

  stone-amber-ridge
    AVAILABLE:
      • mongodb 7.0.8 → 8.0.0 (major version)
      • redis 7.2.5 → 7.2.6

  stone-coral-reef
    AVAILABLE:
      • postgres 16.2 → 16.3
    BLOCKED:
      ⚠ elasticsearch 7.17 → 8.12: Requires AVX (CPU: Pentium Silver J5005)

───────────────────────────────────────────────
```

Three things to notice:

1. **MongoDB 8.0.0** — A major version jump. Major versions often have breaking changes. You'll read the release notes before updating.

2. **Redis and Postgres patches** — Minor updates. Usually safe to apply.

3. **Elasticsearch blocked** — The new version requires AVX instructions, but stone-coral-reef has an older CPU that doesn't support them. The garden won't let you install something that can't run.

You decide to update the safe ones:

```
Choice: S

Select updates to apply:
  [1] mongodb 7.0.8 → 8.0.0 (stone-amber-ridge)
  [2] redis 7.2.5 → 7.2.6 (stone-amber-ridge)
  [3] postgres 16.2 → 16.3 (stone-coral-reef)
  [A] All above
  [Q] Cancel

Choice: 2,3
```

You select Redis and Postgres—the safe patches. MongoDB can wait until you've read the migration guide.

```
Applying selected updates...

  [1/2] Nourishing redis on stone-amber-ridge
        Collecting harvest... done
        Applying update... done
        Verifying health... passed
        ✓ redis 7.2.5 → 7.2.6

  [2/2] Nourishing postgres on stone-coral-reef
        Collecting harvest... done
        Applying update... done
        Verifying health... passed
        ✓ postgres 16.2 → 16.3

2 offerings updated successfully.
MongoDB skipped (not selected).
Elasticsearch skipped (blocked: missing CPU feature).
```

Coffee's ready. Garden's a little healthier. Time for work.

---

A few weeks later, you see something new:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 1 offering, 2 firmware

───────────────────────────────────────────────

  stone-amber-ridge
    AVAILABLE:
      • redis 7.2.6 → 7.2.7

  stone-coral-reef
    FIRMWARE:
      • System Firmware 1.17.0 → 1.38.0 ⟳ reboot required
        Confidence: Tested (LVFS)
        Fixes: CVE-2024-1234, CVE-2024-5678

  stone-bronze-canyon
    FIRMWARE:
      • System Firmware 1.7.1 → 1.38.0 ⟳ reboot required
        Confidence: Suggested (LVFS)

───────────────────────────────────────────────
```

Firmware updates. The garden detected that the BIOS on two Stones can be updated.

Notice the confidence levels:
- **Tested** on stone-coral-reef: This firmware has been verified working on this exact hardware model
- **Suggested** on stone-bronze-canyon: This firmware should work, but hasn't been explicitly tested on this model

The security fixes (CVE numbers) are important. BIOS vulnerabilities can be serious. But firmware updates require a reboot—you'll do this after hours.

```
Choice: O
```

You select "Offerings only" for now. Redis updates immediately. Firmware can wait for the weekend.

---

Saturday morning. Time to update firmware.

```bash
garden-rake nourish --firmware-only
```

```
📦 Firmware Updates

  stone-coral-reef
    • System Firmware 1.17.0 → 1.38.0

  stone-bronze-canyon
    • System Firmware 1.7.1 → 1.38.0

Both updates require reboot. Proceed? [y/N] y

Updating firmware...

  [1/2] stone-coral-reef
        Downloading firmware capsule... done
        Staging for installation... done
        ✓ Firmware staged. Will install on reboot.

  [2/2] stone-bronze-canyon
        Downloading firmware capsule... done
        Staging for installation... done
        ✓ Firmware staged. Will install on reboot.

Firmware staged on 2 stones.
Run 'garden-rake stir <stone>' to reboot and apply.
```

The firmware is downloaded and staged, but not installed yet. You need to reboot:

```bash
garden-rake stir stone-coral-reef
garden-rake stir stone-bronze-canyon
```

```
Rebooting stone-coral-reef...
Rebooting stone-bronze-canyon...
```

Both Stones go down. A minute later, they come back up. You check:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 0 available, 0 blocked

All offerings and firmware are up to date.
```

Everything current. No updates pending. The morning check is clean.

---

## What Just Happened

### The Detection Pipeline

When you run `nourish`, the garden checks multiple sources:

```
┌─────────────────────────────────────────────────────────────────┐
│  NOURISHMENT CHECK PIPELINE                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. OFFERING UPDATES                                            │
│     For each running offering:                                  │
│     ├─ Get current image digest (docker inspect)               │
│     ├─ Query registry for latest digest                        │
│     ├─ Compare digests (not tags!)                             │
│     └─ If different: update available                           │
│                                                                 │
│  2. CONSTRAINT CHECKING                                         │
│     For each available update:                                  │
│     ├─ Load offering template requirements                      │
│     ├─ Check against Stone hardware capabilities                │
│     │   ├─ CPU features (AVX, SSE4.2, etc.)                    │
│     │   ├─ Memory requirements                                 │
│     │   └─ Disk space                                          │
│     └─ If constraint fails: mark as blocked                     │
│                                                                 │
│  3. FIRMWARE DETECTION (Linux only)                            │
│     ├─ Query fwupd for available updates                       │
│     ├─ Check LVFS (Linux Vendor Firmware Service)              │
│     ├─ Get confidence level (Tested/Suggested)                 │
│     └─ Note reboot requirements                                 │
│                                                                 │
│  4. AGGREGATE RESULTS                                           │
│     ├─ Group by Stone                                          │
│     ├─ Separate available vs. blocked                          │
│     └─ Present to user                                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Digest Comparison

The garden compares image *digests*, not tags. This is important.

Consider: you're running `mongo:7.0.8`. The registry also has `mongo:7.0.8`. Are they the same?

Maybe not. The maintainer might have rebuilt the image (security patch, base image update). The tag is the same, but the content changed.

```
Your running image:  sha256:abc123...
Registry image:      sha256:def456...  (tag is still 7.0.8)

Tags match, but digests differ → Update available
```

This is how the garden catches "silent" updates that don't change version numbers.

### Constraint Validation

Not every update can run on every Stone. The garden checks:

```yaml
# Elasticsearch 8.x template
name: elasticsearch
requirements:
  cpu_features:
    - avx  # Required for vector operations
  memory_mb: 2048
  disk_gb: 10
```

When stone-coral-reef's CPU doesn't have AVX:

```
⚠ elasticsearch 7.17 → 8.12: Requires AVX (CPU: Pentium Silver J5005)
```

The garden won't let you install software that will crash on startup. The blocked update stays blocked until you either:
- Move Elasticsearch to a Stone with AVX
- Accept that you're staying on 7.17

### Firmware Updates

Firmware updates use fwupd and LVFS (Linux Vendor Firmware Service):

1. **fwupd** is a daemon that manages firmware on Linux
2. **LVFS** is a repository of firmware images for various hardware
3. The garden queries fwupd: "What updates are available for this machine?"

Confidence levels:
- **Tested**: LVFS has reports of successful updates on this exact hardware
- **Suggested**: The firmware should work based on hardware ID, but no success reports yet

Firmware updates are staged, not applied immediately. The actual installation happens during reboot when the UEFI firmware takes over.

### The Ritual

The morning check isn't just about finding updates. It's about **awareness**.

In traditional infrastructure, updates are events. You get an alert, you schedule maintenance, you do the update. Between updates, you don't think about versions.

With the morning check, updates are ambient. You know what's available. You choose when to apply. Nothing surprises you because you saw it coming days ago.

This changes the relationship with updates:
- **Before**: "Oh no, there's a critical update. Emergency maintenance!"
- **After**: "I've known about this for a week. I'll update Saturday."

---

## The Routine

Here's a sustainable rhythm for garden maintenance:

**Daily (1 minute):**
```bash
garden-rake nourish
# Press Q. Just look. Note anything important.
```

**Weekly (15 minutes):**
```bash
garden-rake nourish
# Apply safe patches (minor versions)
# Review major versions, read release notes
# Note any blocked updates
```

**Monthly (1 hour):**
```bash
garden-rake nourish --firmware-only
# Apply firmware updates during low-traffic time
# Reboot Stones that need it
# Review any long-standing blocked updates
```

This isn't a rigid schedule—adjust to your environment. The point is: small, regular attention beats infrequent, panicked maintenance.

---

## Commands From This Journey

```bash
# Check for all updates
garden-rake nourish

# Check without prompt (for scripting)
garden-rake nourish --updates-only

# Update offerings only (skip firmware)
garden-rake nourish --offerings-only

# Update firmware only (skip offerings)
garden-rake nourish --firmware-only

# Update everything
garden-rake nourish --all

# Update specific offering
garden-rake nourish mongodb

# Update specific Stone
garden-rake nourish stone-amber-ridge

# Reboot a Stone (for firmware installation)
garden-rake stir stone-coral-reef

# Skip safety checks (dangerous)
garden-rake nourish mongodb recklessly
```

---

*Zen Garden Documentation — Journeys*
