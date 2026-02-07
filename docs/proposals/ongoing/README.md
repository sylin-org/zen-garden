# Ongoing Proposals

This directory contains proposals that are **substantially implemented (75-95%)** with remaining work in progress.

## Status Criteria

A proposal moves to `ongoing/` when:
- ✅ Core features are implemented (75%+)
- ✅ Architecture and design principles followed
- ✅ Tests passing for implemented features
- 🔶 Optional polish/cleanup remains
- 🔶 Non-critical enhancements deferred

A proposal moves to `implemented/` when:
- ✅ 100% of proposed features complete
- ✅ All acceptance criteria met
- ✅ Documentation complete
- ✅ No remaining work

---

## Current Ongoing Proposals

### [rake-taxonomy-design.md](rake-taxonomy-design.md)
**Status**: ✅ Partially Implemented (60-70%)
**Implementation**: garden-rake CLI (src/rake/)
**Date**: 2026-01-17

**Completed** (60-70%):
- ✅ Zen verbs implemented (offer, rest, wake, observe, watch, tend, place, invite, lift, make)
- ✅ Positional "at" syntax working
- ✅ Auto-discovery working
- ✅ Tending state (context management) implemented
- ✅ API versioning (/api/v1/)

**Remaining Work** (30-40%):
- ❌ Dual syntax NOT implemented (no normative "services create" style)
- ❌ Missing zen verbs: explore, nourish, release, touch, garden
- 🔶 API versionless redirect needs clarification
- 🔶 Self-teaching suggestions system needs implementation
- 🔶 Quiet mode needs verification

**Next Steps**:
1. Implement normative dual syntax (services create/stop/start/etc.)
2. Add missing zen verbs (explore, nourish, release, touch, garden)
3. Or close proposal as "zen-only implementation" and document divergence

**Why in ongoing/**: Zen command vocabulary is implemented and working in production. Dual syntax feature was not implemented, but system is functional without it.

---

## Lifecycle: Moving Between Directories

### proposals/ → ongoing/
When 75-95% complete, move from proposals/ to ongoing/

**Checklist**:
- [x] Update proposal status to "✅ Substantially Implemented (X%)"
- [x] Create implementation status report in docs/
- [x] Move to ongoing/ directory
- [x] Update ongoing/README.md with summary
- [x] Link to implementation status report

### ongoing/ → implemented/
When 100% complete, move from ongoing/ to implemented/

**Checklist**:
- [ ] Update proposal status to "✅ Implemented"
- [ ] Verify all acceptance criteria met
- [ ] Complete implementation report
- [ ] Move to implemented/ directory
- [ ] Update implemented/README.md
- [ ] Archive or remove from ongoing/

### ongoing/ → proposals/
If work stalls and proposal needs re-planning (rare)

**Checklist**:
- [ ] Update status to explain what stalled
- [ ] Move back to proposals/
- [ ] Create new proposal for remaining work

---

## Statistics

| Metric | Value |
|--------|-------|
| Total Ongoing | 1 |
| Average Completion | 70% |
| Total Files Created | 20+ (CLI) |
| Total Lines Added | ~5,000+ |
| Total Tests | 103 |

---

## Related Documentation

- [Implemented Proposals](../../archive/proposals/README.md) - Fully implemented proposals
- [Proposal Validation Summary](../../archive/planning/proposal-validation-summary.md) - Validation summary
- [../../RUST-REFACTORING-STATUS.md](../../RUST-REFACTORING-STATUS.md) - Rust refactoring status

---

**Last Updated**: 2026-01-24
