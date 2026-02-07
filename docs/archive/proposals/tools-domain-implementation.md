# Tools Domain Implementation Report

**Status**: Implemented  
**Date**: 2026-02-06  
**Scope**: Greenfield (no backward compatibility shims, no legacy wrappers)

---

## Executive Summary

The Tools domain proposal was implemented as a clean cut:

1. One normalized tool projection for offerings and seed banks.
2. One normative automation API surface:
   - `GET /api/v1/garden/tools`
   - `GET /api/v1/garden/tools/stream`
3. One inter-Moss propagation channel for tools:
   - `TOOLS_BEACON` (`tools_beacon`)
4. Event-driven wishful readiness in `garden-rake find`, including capability-aware wishes.
5. Capability state persistence and garden-wide propagation.

---

## Architectural Result (SoC, DDD, SOLID)

Bounded context introduced:

- `Tools` as its own domain in Moss:
  - projection
  - readiness policy
  - cache/history/replay
  - capability state mutation orchestration

Concern split after implementation:

- `Presence`: presentation and Companion stream
- `Tools`: normative automation state/stream
- `Storage`: seed bank lifecycle and routing data
- `Topology`: stone liveness and endpoint awareness

Dependency direction:

- Shared contracts in `garden_common::tools`
- Moss API and infra depend on domain tools modules
- P2P announcement handling consumes `TOOLS_BEACON` contract

---

## Implemented Contracts

Shared contracts added in `garden-common`:

- `ToolType`, `ToolState`
- `ToolProjection`, `ToolDelta`, `ToolsBeacon`
- `CapabilityWish`, `CapabilitySelector`
- `tool_fqid` parse/build helpers
- event constants (`tools.snapshot`, `tool.upsert`, `tool.remove`, `tools.heartbeat`, etc.)

Announcement type added:

- `TOOLS_BEACON` in `src/common/src/infra/communications/announcement_types.rs`

---

## Moss Domain and API Changes

### New domain modules

- `src/moss/src/domain/tools/cache.rs`
- `src/moss/src/domain/tools/projector.rs`
- `src/moss/src/domain/tools/readiness.rs`
- `src/moss/src/domain/tools/events.rs`
- `src/moss/src/domain/tools/capability_orchestrator.rs`

### New API handlers

- `src/moss/src/api/v1/tools.rs`
  - `GET /api/v1/garden/tools`
  - `GET /api/v1/garden/tools/stream`

### AppState integration

- Added `tools_cache` and `tools_tx`.
- Added projection refresh and beacon publish helpers.
- `persist_offerings()` now reconciles tools projection.

### Router integration

- Routes registered in `src/moss/src/bootstrap/router.rs`.

---

## Stream Semantics Delivered

Current stream emits:

- `tools.snapshot` (first event)
- `tool.upsert`
- `tool.remove`
- `tools.heartbeat` (every 15s)

Replay/resume behavior:

- `since=<cursor>` support
- `Last-Event-ID` support (cursor or event id lookup)
- at-least-once delivery with dedupe by `event_id`
- history-backed replay from tools cache

---

## Capability-Aware Wishful Delivered

`garden-rake find` now supports capability-aware wishful flow:

- Parses capability wishes with `parse_capability_wish(...)`
- Ensures capability using offering capability API
- Waits for readiness through `/api/v1/garden/tools/stream` (no job polling path)

Examples:

- `garden-rake find mongodb wishfully`
- `garden-rake find "ollama[model1,model2]" wishfully`
- `garden-rake find ollama:modelv1 wishfully`

Capability persistence:

- add/remove operations mutate offering `sub_capabilities`
- persisted via offerings persistence path
- projected into tools stream and propagated with `TOOLS_BEACON`

---

## Inter-Moss Propagation Delivered

`TOOLS_BEACON` ingestion/emission implemented in coordinator and tools infra:

- peers ingest deltas and update projection cache
- local updates broadcast normalized deltas
- stone goodbye removes projected tools for offline stone
- new stone detection triggers snapshot beacon broadcast

Storage relationship:

- `STORAGE_BEACON` remains for storage routing concerns
- tool propagation now uses `TOOLS_BEACON`

---

## Validation Executed

Commands run:

```bash
cargo test -p garden-common tools:: -- --nocapture
cargo check -p garden-moss
cargo check -p garden-rake
cargo test -p garden-moss domain::tools::cache::tests:: -- --nocapture
```

Result: all commands completed successfully.

---

## Minor Delta vs Spec Draft

The shared event constants include capability sync lifecycle names:

- `tool.capability.sync_started`
- `tool.capability.sync_completed`
- `tool.capability.sync_failed`

Current emitted SSE frames are `tools.snapshot`, `tool.upsert`, `tool.remove`, `tools.heartbeat`.
Capability transitions are represented through `tool.upsert` payload fields (`capabilities`, `capability_revision`, `capability_delta`).

---

## User Guide

Adapter and operator usage guide:

- `docs/guides/tools-domain-user-guide.md`
