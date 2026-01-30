# Portrait Enhancement Proposal: Storage & Nurturing Visibility

**Status**: Draft
**Author**: Claude
**Date**: 2026-01-30

## Overview

Enhance the stone portrait (landing page) to provide meaningful storage and backup information for both specialists (sysadmins, DevOps) and hobbyists (home lab users).

## Current State

The portrait currently shows:
- **Foundation**: CPU, Memory, Disk (single disk total only)
- **Offerings**: Name, container, port, status
- **Seed Banks**: Name, used/capacity, filesystem, visibility, online

**What's missing:**
- No backup status per offering
- No storage breakdown (data vs backups vs system)
- No backup history or trends
- No quick actions for backup/restore
- No retention policy visibility

---

## Proposed Enhancements

### 1. Per-Offering Nurturing Status

Add backup information to each offering card:

```
┌─────────────────────────────────────────────────────────────────┐
│  mongodb                                docker.io/mongo:7   :27017
│  ├─ Status: running · healthy
│  ├─ Data: 2.3 GB
│  ├─ Last backup: 2h ago (slot B) ✓
│  ├─ Last replicated: 2h ago → seed-bank-zen-garden ✓
│  └─ [Backup Now] [Restore ▾]
└─────────────────────────────────────────────────────────────────┘
```

**New fields in `PortraitOffering`:**
```rust
pub struct PortraitOffering {
    // existing...
    pub data_size_bytes: Option<u64>,
    pub nurturing: Option<OfferingNurturing>,
}

pub struct OfferingNurturing {
    pub last_local_backup: Option<DateTime<Utc>>,
    pub last_local_slot: Option<String>,      // "A" or "B"
    pub last_replication: Option<DateTime<Utc>>,
    pub last_replication_target: Option<String>,
    pub next_scheduled: Option<DateTime<Utc>>,
    pub local_backup_size_bytes: u64,
    pub remote_backup_count: u32,
}
```

**User value:**
- Specialists: Know backup health at a glance, identify stale backups
- Hobbyists: Confidence that data is protected, easy access to restore

---

### 2. Storage Breakdown Section

Add detailed storage section between Foundation and Offerings:

```
══════════ STORAGE ══════════

┌────────────────────────────────────────┐
│  System Disk (/dev/sda)                │
│  ━━━━━━━━━━━━━━━━░░░░  234 / 500 GB    │
│                                        │
│  Breakdown:                            │
│    Offering Data    182 GB  ████████   │
│    Local Backups     38 GB  ██         │
│    Docker Images     12 GB  █          │
│    System             2 GB  ░          │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│  seed-bank-zen-garden (USB)    online  │
│  ━━░░░░░░░░░░░░░░░░░░  62 / 932 GB     │
│                                        │
│  Backups:                              │
│    mongodb       5 copies   12 GB      │
│    redis         5 copies    2 GB      │
│    vault         3 copies    1 GB      │
│  Total: 13 snapshots, 15 GB            │
│  Last sync: 2h ago                     │
└────────────────────────────────────────┘
```

**New response structure:**
```rust
pub struct PortraitStorage {
    pub system_disk: DiskBreakdown,
    pub additional_disks: Vec<DiskBreakdown>,
}

pub struct DiskBreakdown {
    pub mount_point: String,
    pub device: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub breakdown: StorageBreakdown,
}

pub struct StorageBreakdown {
    pub offering_data_bytes: u64,
    pub local_backups_bytes: u64,
    pub docker_images_bytes: u64,
    pub system_bytes: u64,
}
```

**User value:**
- Specialists: Capacity planning, identify what's consuming space
- Hobbyists: Understand where disk space goes, when to add storage

---

### 3. Enhanced Seed Bank Section

Expand seed bank cards with backup details:

```
┌─────────────────────────────────────────────────────────────────┐
│  seed-bank-zen-garden                          USB · btrfs · open
│  ━━░░░░░░░░░░░░░░░░░░  62 / 932 GB (7%)                   online
│
│  Backed up offerings:
│    mongodb    5 copies (12 GB)  last: 2h ago   ▸
│    redis      5 copies (2 GB)   last: 2h ago   ▸
│    vault      3 copies (1 GB)   last: 6h ago   ▸
│
│  Total: 13 snapshots · 15 GB · Retention: 5/offering
│  Origin: stone-coral-prairie · Since: 2025-12-15
└─────────────────────────────────────────────────────────────────┘
```

**Enhanced `PortraitSeedBank`:**
```rust
pub struct PortraitSeedBank {
    // existing...
    pub snapshot_count: u32,
    pub backed_up_offerings: Vec<SeedBankOffering>,
    pub last_sync: Option<DateTime<Utc>>,
    pub origin_stone: String,
    pub created_at: DateTime<Utc>,
}

pub struct SeedBankOffering {
    pub name: String,
    pub snapshot_count: u32,
    pub size_bytes: u64,
    pub last_backup: Option<DateTime<Utc>>,
}
```

**User value:**
- Specialists: Verify replication is working, audit backup coverage
- Hobbyists: See that external backup is happening, feel secure

---

### 4. Quick Actions (Optional Enhancement)

Add action buttons to offering cards for common operations:

```html
<div class="offering-actions">
    <button onclick="backupNow('mongodb')">Backup Now</button>
    <select onchange="restore('mongodb', this.value)">
        <option>Restore from...</option>
        <option value="slot-a">Slot A (2h ago)</option>
        <option value="slot-b">Slot B (26h ago)</option>
        <option value="remote">seed-bank-zen-garden (2h ago)</option>
    </select>
</div>
```

**Considerations:**
- Requires confirmation dialog before destructive operations
- Should show progress indicator during operation
- Need error handling and user feedback

**User value:**
- Specialists: Quick access without CLI
- Hobbyists: Approachable interface for recovery

---

### 5. Nurturing Health Summary

Add a summary bar in the glance section (hero area):

```
┌──────────────────────────────────────────────────────────────────┐
│  stone-coral-prairie                                             │
│  STONE                                                           │
│                                                                  │
│  Uptime: 14d    Moss: 2d                                        │
│  Offerings: 5🟢 0🔴                                              │
│  Backups: 5 protected · 0 stale · 1 seed bank online            │
└──────────────────────────────────────────────────────────────────┘
```

**Backup health states:**
- **Protected**: Local backup < 24h AND replicated < 24h
- **Stale**: Local backup > 24h OR never replicated
- **Unprotected**: No backups at all

**User value:**
- At-a-glance backup health without scrolling
- Immediate visibility of problems

---

## Data Requirements

To implement these enhancements, we need additional API data:

### 1. Offering Data Size
Need to calculate volume sizes per offering:
```rust
async fn get_offering_data_size(offering_id: &str) -> u64 {
    // Sum of all volume sizes for this offering's container
}
```

### 2. Nurturing Status per Offering
Query nurturing store for each offering:
```rust
async fn get_offering_nurturing_status(offering_id: &str) -> Option<OfferingNurturing> {
    // Get slot info, last backup times, replication status
}
```

### 3. Storage Breakdown
Requires directory size calculations:
```rust
async fn calculate_storage_breakdown() -> StorageBreakdown {
    // /var/lib/zen-garden/harvests/ → local backups
    // Docker volume sizes → offering data
    // /var/lib/docker → docker overhead
}
```

### 4. Seed Bank Details
Query each seed bank for backup details:
```rust
async fn get_seed_bank_backup_details(seed_bank: &SeedBankInfo) -> Vec<SeedBankOffering> {
    // Read seed bank index, aggregate by offering
}
```

---

## UI/UX Considerations

### For Specialists
- Show precise timestamps, not just "2h ago"
- Include offering_id for correlation with logs
- Expose raw bytes alongside human-readable sizes
- Link to API endpoints for scripting

### For Hobbyists
- Use friendly time formats ("2 hours ago", "yesterday")
- Traffic-light colors for backup health (green/yellow/red)
- Hide technical details behind expandable sections
- Confirm before any destructive action

### Accessibility
- ARIA labels for status indicators
- Keyboard navigation for actions
- Screen reader friendly status announcements
- Sufficient color contrast

### Performance
- Cache expensive calculations (storage breakdown)
- Lazy load seed bank details on expand
- Rate limit refresh to avoid excessive API calls
- Show stale data with "updating..." indicator

---

## Implementation Priority

| Enhancement | Priority | Effort | Value |
|------------|----------|--------|-------|
| Per-offering backup status | P1 | Medium | High |
| Nurturing health summary | P1 | Low | High |
| Enhanced seed bank cards | P2 | Medium | Medium |
| Storage breakdown | P2 | High | Medium |
| Quick actions | P3 | High | Medium |

### Suggested Order
1. Add `nurturing` field to `PortraitOffering` (most value, moderate effort)
2. Add backup health summary to glance section (quick win)
3. Enhance seed bank cards with backup counts (builds on #1)
4. Storage breakdown section (requires new calculations)
5. Quick action buttons (needs confirmation dialogs, error handling)

---

## Wireframe Summary

```
┌─────────────────────────────────────────────────────────────────┐
│  HERO: Stone name, uptime, backup health summary                │
├─────────────────────────────────────────────────────────────────┤
│  FOUNDATION: CPU | Memory | Disk                                │
├─────────────────────────────────────────────────────────────────┤
│  STORAGE: Disk breakdown, seed bank details                     │  ← NEW
├─────────────────────────────────────────────────────────────────┤
│  OFFERINGS: Each with backup status, data size, actions         │  ← ENHANCED
├─────────────────────────────────────────────────────────────────┤
│  SEED BANKS: Backup counts, last sync, retention status         │  ← ENHANCED
├─────────────────────────────────────────────────────────────────┤
│  COMPANIONS: Companions (unchanged)                               │
├─────────────────────────────────────────────────────────────────┤
│  HORIZON: Visible stones (unchanged)                            │
└─────────────────────────────────────────────────────────────────┘
```

---

## Open Questions

1. Should quick actions require authentication?
2. How to handle backup/restore in progress (show progress bar?)
3. Should storage breakdown be real-time or cached (performance)?
4. How to present multiple seed banks (list vs tabs)?
5. Mobile layout considerations for storage visualization?

---

*This proposal addresses gaps identified in the nurturing user guide and provides actionable enhancements for the stone portrait.*
