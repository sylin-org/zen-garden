import { useEffect, useState } from "react"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

/// Per-job progress state surfaced to the Canvas's forming-chips
/// (and any other consumer of an active operation). `percent` is
/// computed once both `step` and `total` are known; while a job is
/// pre-flight (steps reported before the volume count is known on
/// capture), `percent` stays `null` and the chip falls back to a
/// pulse animation rather than misleading 0%.
export interface JobProgressState {
  /// 0.0..=1.0 fraction of completion. `null` when total isn't
  /// known yet (early steps in a capture, or before any progress
  /// event has arrived).
  percent: number | null
  /// Most recent progress message — drives the chip label / tooltip.
  message: string | null
  /// `"Pending" | "Running" | "Completed" | "Failed"` — derived
  /// from snapshot frames + terminal events.
  status: "pending" | "running" | "completed" | "failed" | "unknown"
  /// Final result payload — populated only on `Completed`.
  /// Shape depends on the operation; consumers typecast.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  result: Record<string, any> | null
  /// Final error string — populated only on `Failed`.
  error: string | null
  /// Operation name from the snapshot frame, e.g. `"capture_snapshot"`.
  operation: string | null
}

interface JobSnapshotEvent {
  id: string
  operation?: string
  status: string
  current_step?: number | null
  total_steps?: number | null
  last_message?: string | null
  result?: Record<string, unknown> | null
  error?: string | null
}

interface JobProgressEvent {
  job_id: string
  message: string
  step?: number | null
  total_steps?: number | null
}

interface JobCompletedEvent {
  job_id: string
  result: Record<string, unknown>
}

interface JobFailedEvent {
  job_id: string
  error: string
}

const INITIAL: JobProgressState = {
  percent: null,
  message: null,
  status: "unknown",
  result: null,
  error: null,
  operation: null,
}

/// Subscribe to per-job Tauri events for `jobId` and return the
/// latest progress state. Pass `null` when no job is active — the
/// hook returns the initial state and never subscribes.
///
/// Wire to the Pavilion-side `commands::capture_snapshot` /
/// `plant_snapshot` flow which emits:
///
/// - `job:started` — already used by the canvas to register the
///   forming-chip. Not consumed by this hook.
/// - `job:snapshot` — full Job state. Hook reflects status,
///   current_step, total_steps, last_message, result, error.
/// - `job:progress` — per-step delta. Updates step/total/message.
/// - `job:completed` — terminal success with result.
/// - `job:failed` — terminal failure with error.
///
/// Each event is filtered by `job_id` so multiple in-flight jobs
/// can each have their own hook instance without cross-talk.
export function useJobProgress(jobId: string | null): JobProgressState {
  const [state, setState] = useState<JobProgressState>(INITIAL)

  useEffect(() => {
    if (!jobId) {
      setState(INITIAL)
      return
    }

    let cancelled = false
    const unlisteners: UnlistenFn[] = []

    void (async () => {
      // ── Snapshot ─────────────────────────────────────────────
      unlisteners.push(
        await listen<JobSnapshotEvent>("job:snapshot", (e) => {
          if (cancelled) return
          if (e.payload.id !== jobId) return
          setState((prev) => ({
            ...prev,
            status: normalizeStatus(e.payload.status),
            percent: computePercent(
              e.payload.current_step,
              e.payload.total_steps,
            ),
            message: e.payload.last_message ?? prev.message,
            result: e.payload.result ?? prev.result,
            error: e.payload.error ?? prev.error,
            operation: e.payload.operation ?? prev.operation,
          }))
        }),
      )

      // ── Progress ─────────────────────────────────────────────
      unlisteners.push(
        await listen<JobProgressEvent>("job:progress", (e) => {
          if (cancelled) return
          if (e.payload.job_id !== jobId) return
          setState((prev) => ({
            ...prev,
            // Progress events imply Running.
            status: prev.status === "completed" || prev.status === "failed"
              ? prev.status
              : "running",
            percent: computePercent(
              e.payload.step,
              e.payload.total_steps,
            ) ?? prev.percent,
            message: e.payload.message ?? prev.message,
          }))
        }),
      )

      // ── Completed ───────────────────────────────────────────
      unlisteners.push(
        await listen<JobCompletedEvent>("job:completed", (e) => {
          if (cancelled) return
          if (e.payload.job_id !== jobId) return
          setState((prev) => ({
            ...prev,
            status: "completed",
            // Snap to 100% on completion regardless of whether the
            // last progress event arrived.
            percent: 1.0,
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            result: e.payload.result as Record<string, any>,
          }))
        }),
      )

      // ── Failed ──────────────────────────────────────────────
      unlisteners.push(
        await listen<JobFailedEvent>("job:failed", (e) => {
          if (cancelled) return
          if (e.payload.job_id !== jobId) return
          setState((prev) => ({
            ...prev,
            status: "failed",
            error: e.payload.error,
          }))
        }),
      )
    })()

    return () => {
      cancelled = true
      for (const unlisten of unlisteners) unlisten()
    }
  }, [jobId])

  return state
}

function normalizeStatus(s: string): JobProgressState["status"] {
  switch (s.toLowerCase()) {
    case "pending":
      return "pending"
    case "running":
      return "running"
    case "completed":
      return "completed"
    case "failed":
      return "failed"
    default:
      return "unknown"
  }
}

function computePercent(
  step: number | null | undefined,
  total: number | null | undefined,
): number | null {
  if (typeof step !== "number" || typeof total !== "number") return null
  if (total <= 0) return null
  // Clamp to [0, 1] — defensively, for any wire-format glitch
  // where step exceeds total.
  return Math.max(0, Math.min(1, step / total))
}
