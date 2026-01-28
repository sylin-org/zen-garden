# Proposal Alignment Checklist

**Purpose**: Track alignment between proposal specs and existing documentation  
**Created**: 2026-01-28  
**Status**: In Progress

---

## Summary of Decisions

The following decisions were made to align the proposals with existing docs:

| # | Topic | Decision |
|---|-------|----------|
| 1 | Connection String Format | `zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>]` |
| 2 | Protocol vs Offering | `s3`/`storage` = protocols (wire format), `minio`/`mongodb` = offerings (software) |
| 3 | Environment Variable Prefix | Standardize on `ZG_` (replacing `GARDEN_` and `ZEN_GARDEN_`) |
| 4 | Storage API Port | 7185 (same as all Moss APIs) |
| 5 | Config File Name | `moss.toml` (standardized) |
| 6 | mDNS TXT Records | Extended with: `capability`, `instance`, `admission`, `protocols`, `protocol_default` |
| 7 | Admission Policy Syntax | `offer mongodb:staging privately`, `set-admission communal/dedicated` |
| 8 | Storage Adoption Command | `seed-bank add <path> --name <name>` (replacing `tend <path> as seed-bank`) |
| 9 | Resolution API | `GET /api/v1/resolve?offering=&instance=&protocol=` |
| 10 | Lantern Scope | Document for future; defer implementation |

---

## Updated Proposal Documents

### ✅ zen-garden-service-resolution-spec.md

Fully updated with:
- [x] Connection string grammar with optional protocol prefix
- [x] Protocol vs Offering section explaining distinction
- [x] Extended manifest format with `protocols` array
- [x] All examples use `protocols` instead of `provides`
- [x] mDNS TXT Records section with new fields
- [x] Admission Policy with CLI commands (`privately`, `publicly`, `set-admission`, `rename-to`)
- [x] Resolution API endpoint specification
- [x] Environment variables using `ZG_` prefix
- [x] Updated Appendix A (Grammar) and Appendix B (Environment Variables)

### ✅ zen-garden-storage-capability-spec.md

Fully updated with:
- [x] Executive Summary with Protocol vs Offering distinction
- [x] Port 7185 throughout (was 7180)
- [x] mDNS announcement uses `protocols=s3,storage` (was `capability=s3`)
- [x] MinIO manifest uses `protocols` array (was `provides`)
- [x] Capability Ladder diagram updated with correct commands
- [x] All examples use `seed-bank add` command (was `tend ... as seed-bank`)
- [x] Connection strings use `zen-garden:s3//` format
- [x] Summary section updated with protocol vs capability terminology

---

## Existing Documents Requiring Updates

The following existing documents may need updates to align with the proposals:

### Priority 1: Core Specs (Must Update)

| Document | Status | Changes Needed |
|----------|--------|----------------|
| `docs/specs/api-v1.md` | ✅ Done | Added `/api/v1/resolve`, storage endpoints, seed bank endpoints |
| `docs/specs/offerings.md` | ✅ Done | Added `protocols` array to manifest format, updated service discovery |
| `docs/specs/discovery.md` | ✅ Done | Updated mDNS TXT record fields, new connection string format |

### Priority 2: Decision Records (Review)

| Document | Status | Changes Needed |
|----------|--------|----------------|
| `docs/decisions/MDNS-0001-single-service-type.md` | ✅ Done | Updated TXT fields for protocols, instance, admission |
| `docs/decisions/RAKE-0010-caching.md` | ✅ Done | Updated to use `ZG_STONE` environment variable |

### Priority 3: Guides and References (Update Later)

| Document | Status | Changes Needed |
|----------|--------|----------------|
| `docs/ARCHITECTURE-REFERENCE.md` | ✅ Done | Added resolution API, updated environment variables to `ZG_` |
| `docs/reference/config.md` | ✅ Done | Updated to `moss.toml`, added seed bank config section |
| `docs/reference/architecture-overview.md` | ✅ Done | Updated environment variables to `ZG_` prefix |
| `docs/reference/driver-specification.md` | ✅ Done | Updated environment variables to `ZG_` prefix |
| `docs/reference/ports.md` | ✅ Done | Updated config file path to `moss.toml` |
| `docs/specs/moss-daemon-lifecycle.md` | ✅ Done | Updated config file path to `moss.toml` |
| `docs/specs/HEY-TELL-SYNTAX.md` | ✅ Done | Updated environment variables to `ZG_` prefix |
| `docs/specs/CRICKET-SPEC.md` | ✅ Done | Updated `ZG_STONE` environment variable |
| `docs/ops/release-notes.md` | ✅ Done | Updated config file path to `moss.toml` |
| `docs/ops/build-distribution.md` | ✅ Done | Updated config file name to `moss.toml` |
| `docs/README.md` | ✅ Done | Updated config reference |
| `docs/proposals/ongoing/cli-taxonomy.md` | ✅ Done | Updated environment variables to `ZG_` prefix |

---

## Code Changes Required

After documentation is aligned, the following code changes are needed:

### Common (garden_common)

- [ ] Add `protocols` field to offering manifest struct
- [ ] Update environment variable constants to `ZG_` prefix
- [ ] Add `AdmissionPolicy` enum (`Communal`, `Dedicated`)
- [ ] Add `ResolveRequest` and `ResolveResponse` types

### Moss

- [ ] Implement `/api/v1/resolve` endpoint
- [ ] Implement `/api/v1/storage` endpoints (S3 gateway)
- [ ] Update mDNS announcements with new TXT fields
- [ ] Implement `seed-bank` subcommand in API
- [ ] Add admission policy handling

### Rake

- [ ] Implement `seed-bank add` command
- [ ] Implement `set-admission` command
- [ ] Implement `offering rename-to` command
- [ ] Update `offer` command with `privately`/`publicly` suffix
- [ ] Update environment variable handling

---

## Manifest Schema Changes

The offering manifest schema needs these additions:

```yaml
# Before
provides:
  - mongodb

# After
protocols:
  - port: 27017
    protocol: mongodb
    default: true
  - port: 9000
    protocol: storage
    sidecar: backup-agent
```

---

## Environment Variable Migration

| Old | New |
|-----|-----|
| `GARDEN_DATA_DIR` | `ZG_DATA_DIR` |
| `GARDEN_CONFIG_DIR` | `ZG_CONFIG_DIR` |
| `GARDEN_STONE_NAME` | `ZG_STONE_NAME` |
| `ZEN_GARDEN_CONTAINER` | `ZG_CONTAINER` |
| ... | ... |

**Note**: Support both old and new during transition period with deprecation warnings.

---

## Next Steps

1. ⬜ Review `docs/specs/api-v1.md` and update with resolve endpoint
2. ⬜ Review `docs/specs/offerings.md` and add protocols array format
3. ⬜ Review `docs/specs/discovery.md` and update mDNS TXT fields
4. ⬜ Update ARCHITECTURE-REFERENCE.md with new patterns
5. ⬜ Begin implementation in code

---

**End of Checklist**
