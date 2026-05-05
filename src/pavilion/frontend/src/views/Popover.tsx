import { useCallback, useEffect, useMemo, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow"

// ── Wire types (mirror the Tauri command shapes) ────────────────────

interface AwareStone {
  stone_id: string
  stone_name: string
  endpoint: string
  health: string
  services_count: number
  last_seen: string
  age_secs: number
  seen_first_secs: number
}

interface TendedStone {
  stone_name: string
  endpoint: string
}

type Severity = "info" | "notice" | "warn" | "urgent"

interface StoneJoinedEvent {
  kind: "stone_joined"
  stone_id: string
  stone_name: string
  endpoint: string
}
interface StoneLeftEvent {
  kind: "stone_left"
  stone_id: string
  stone_name: string
}
interface StorageActivityEvent {
  kind: "storage_activity"
  stone_name: string
  bank_name: string
  creates: number
  modifies: number
  deletes: number
}
type GardenEventPayload = StoneJoinedEvent | StoneLeftEvent | StorageActivityEvent

interface ActivityEntry {
  id: string
  at: string
  event: GardenEventPayload
  severity: Severity
  promoted: boolean
}

interface Suggestion {
  id: string
  kind: string
  title: string
  body: string
  cta_label: string
  cta_target: string
}

const POPOVER_RECENT_LIMIT = 4

/// Tray popover surface — the small acrylic-backdropped flyout that
/// appears when the user left-clicks the tray icon. Designed for
/// at-a-glance triage: tended-stone status, the active facilitator
/// suggestion (if any), the last few activity events, and a single
/// CTA into the main window. Click-outside dismissal is handled by
/// the Rust window-blur handler.
export function PopoverView(): JSX.Element {
  const [stones, setStones] = useState<AwareStone[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [activity, setActivity] = useState<ActivityEntry[]>([])
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [s, t, a, sug] = await Promise.all([
        invoke<AwareStone[]>("get_topology"),
        invoke<TendedStone | null>("get_tended"),
        invoke<ActivityEntry[]>("get_activity"),
        invoke<Suggestion | null>("get_suggestion"),
      ])
      setStones(s)
      setTended(t)
      setActivity(a)
      setSuggestion(sug)
    } catch (e) {
      console.error("popover refresh failed:", e)
    }
  }, [])

  useEffect(() => {
    let unlistenTopology: UnlistenFn | undefined
    let unlistenTending: UnlistenFn | undefined
    let unlistenActivity: UnlistenFn | undefined
    let unlistenSuggestion: UnlistenFn | undefined
    let cancelled = false

    void (async () => {
      await refresh()
      unlistenTopology = await listen("topology-changed", () => {
        if (!cancelled) void refresh()
      })
      unlistenTending = await listen("tending-changed", () => {
        if (!cancelled) void refresh()
      })
      unlistenActivity = await listen("activity-changed", () => {
        if (!cancelled) void refresh()
      })
      unlistenSuggestion = await listen("suggestion-changed", () => {
        if (!cancelled) void refresh()
      })
    })()

    return () => {
      cancelled = true
      unlistenTopology?.()
      unlistenTending?.()
      unlistenActivity?.()
      unlistenSuggestion?.()
    }
  }, [refresh])

  const tendedReachable = useMemo(() => {
    if (!tended) return false
    return stones.some(
      (s) =>
        s.stone_name === tended.stone_name || s.endpoint === tended.endpoint,
    )
  }, [stones, tended])

  const recent = useMemo(
    () => activity.slice(0, POPOVER_RECENT_LIMIT),
    [activity],
  )

  const openMain = useCallback(async () => {
    // Show the main window, then hide the popover so the user
    // doesn't see two surfaces overlapping for the brief moment
    // before the main window steals focus.
    try {
      await invoke("show_main_window")
    } catch (e) {
      console.error("show_main_window failed:", e)
    }
    try {
      await getCurrentWebviewWindow().hide()
    } catch {
      // hide failure is non-fatal — window will lose focus to main
      // and our blur handler will hide it momentarily.
    }
  }, [])

  // Dismissing the suggestion uses the same Tauri command the main
  // window calls, so engine state stays in sync.
  const dismissSuggestion = useCallback(async (id: string) => {
    try {
      await invoke("dismiss_suggestion", { id })
    } catch (e) {
      console.error("dismiss_suggestion failed:", e)
    }
  }, [])

  return (
    <div className="popover-shell">
      <header className="popover-header">
        <div className="brand-mark popover-brand-mark">P</div>
        <div className="popover-title">Pavilion</div>
      </header>

      <section className="popover-status">
        <span className={statusDotClass(tendedReachable, tended, stones)} />
        <span className="popover-status-text">
          {statusText(tendedReachable, tended, stones)}
        </span>
      </section>

      {suggestion && (
        <section className="popover-suggestion">
          <div className="popover-suggestion-title">{suggestion.title}</div>
          <div className="popover-suggestion-body">{suggestion.body}</div>
          <div className="popover-suggestion-actions">
            <button
              type="button"
              className="popover-cta-primary"
              onClick={() => void openMain()}
            >
              {suggestion.cta_label}
            </button>
            <button
              type="button"
              className="popover-cta-ghost"
              onClick={() => void dismissSuggestion(suggestion.id)}
            >
              Dismiss
            </button>
          </div>
        </section>
      )}

      <section className="popover-recent">
        <div className="popover-section-title">Recent</div>
        {recent.length === 0 ? (
          <div className="popover-empty">No activity yet.</div>
        ) : (
          <ul className="popover-recent-list">
            {recent.map((entry) => (
              <li key={entry.id} className={`popover-recent-row sev-${entry.severity}`}>
                <span className="popover-recent-time">{formatTimeOnly(entry.at)}</span>
                <span className="popover-recent-text">
                  {describeActivity(entry.event)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <footer className="popover-footer">
        <button
          type="button"
          className="popover-cta-primary popover-cta-block"
          onClick={() => void openMain()}
        >
          Open Pavilion
        </button>
      </footer>
    </div>
  )
}

function statusDotClass(
  reachable: boolean,
  tended: TendedStone | null,
  stones: AwareStone[],
): string {
  if (reachable) return "dot dot-ok"
  if (tended) return "dot dot-down"
  if (stones.length > 0) return "dot dot-amber"
  return "dot"
}

function statusText(
  reachable: boolean,
  tended: TendedStone | null,
  stones: AwareStone[],
): string {
  if (reachable) return `connected to ${tended!.stone_name}`
  if (tended) return `${tended.stone_name} silent`
  if (stones.length > 0) return `${stones.length} stone${stones.length === 1 ? "" : "s"} in earshot`
  return "no garden in earshot"
}

function formatTimeOnly(iso: string): string {
  const d = new Date(iso)
  return d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  })
}

function describeActivity(event: GardenEventPayload): string {
  switch (event.kind) {
    case "stone_joined":
      return `${event.stone_name} joined`
    case "stone_left":
      return `${event.stone_name} offline`
    case "storage_activity": {
      const total = event.creates + event.modifies + event.deletes
      return `${event.bank_name} synced ${total} file${total === 1 ? "" : "s"}`
    }
  }
}
