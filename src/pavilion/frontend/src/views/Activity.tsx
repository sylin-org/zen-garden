import { useCallback, useEffect, useMemo, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

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

interface ActivityViewProps {
  onClose: () => void
}

type KindFilter = "all" | "stone_joined" | "stone_left" | "storage_activity"
type PromotedFilter = "all" | "promoted" | "quiet"

const KIND_LABELS: Record<KindFilter, string> = {
  all: "All",
  stone_joined: "Stone joined",
  stone_left: "Stone offline",
  storage_activity: "Storage activity",
}

export function ActivityView({ onClose }: ActivityViewProps): JSX.Element {
  const [activity, setActivity] = useState<ActivityEntry[]>([])
  const [kindFilter, setKindFilter] = useState<KindFilter>("all")
  const [promotedFilter, setPromotedFilter] = useState<PromotedFilter>("all")

  const refresh = useCallback(async () => {
    try {
      const entries = await invoke<ActivityEntry[]>("get_activity")
      setActivity(entries)
    } catch (e) {
      console.error("get_activity failed:", e)
    }
  }, [])

  useEffect(() => {
    let unlisten: UnlistenFn | undefined
    let cancelled = false
    void (async () => {
      await refresh()
      unlisten = await listen<null>("activity-changed", () => {
        if (cancelled) return
        void refresh()
      })
    })()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [refresh])

  const filtered = useMemo(() => {
    return activity.filter((entry) => {
      if (kindFilter !== "all" && entry.event.kind !== kindFilter) return false
      if (promotedFilter === "promoted" && !entry.promoted) return false
      if (promotedFilter === "quiet" && entry.promoted) return false
      return true
    })
  }, [activity, kindFilter, promotedFilter])

  // Bucket by relative day so the user sees structure rather than
  // an undifferentiated firehose. "Today" / "Yesterday" / older
  // entries collapse to dated headings.
  const groups = useMemo(() => groupByDay(filtered), [filtered])

  return (
    <main className="content">
      <header className="topbar">
        <button className="garden-pill" onClick={onClose} type="button">
          ← Home
        </button>
        <div className="topbar-spacer" />
        <div className="topbar-clock">
          {activity.length} entr{activity.length === 1 ? "y" : "ies"}
        </div>
      </header>

      <section className="hero">
        <h1>Activity</h1>
        <p className="subtle">
          Every accepted event lands here, whether it fired a toast or
          stayed quiet. The ring buffer holds the most recent 200.
        </p>
      </section>

      <section className="activity-filters">
        <FilterChip
          label="Kind"
          options={(["all", "stone_joined", "stone_left", "storage_activity"] as KindFilter[]).map(
            (k) => ({ value: k, label: KIND_LABELS[k] })
          )}
          value={kindFilter}
          onChange={(v) => setKindFilter(v as KindFilter)}
        />
        <FilterChip
          label="Toast"
          options={[
            { value: "all", label: "All" },
            { value: "promoted", label: "Promoted" },
            { value: "quiet", label: "Quiet" },
          ]}
          value={promotedFilter}
          onChange={(v) => setPromotedFilter(v as PromotedFilter)}
        />
      </section>

      {filtered.length === 0 ? (
        <section className="settings-empty">
          {activity.length === 0
            ? "Nothing has happened yet. Discover or tend a stone to start filling the feed."
            : "No entries match these filters."}
        </section>
      ) : (
        <section className="activity-feed">
          {groups.map((g) => (
            <div className="activity-group" key={g.label}>
              <div className="activity-group-heading">{g.label}</div>
              {g.entries.map((entry) => (
                <ActivityRow entry={entry} key={entry.id} />
              ))}
            </div>
          ))}
        </section>
      )}
    </main>
  )
}

function ActivityRow({ entry }: { entry: ActivityEntry }): JSX.Element {
  const { primary, secondary } = describeActivity(entry.event)
  const time = formatTimeOnly(entry.at)
  return (
    <article className="activity-entry">
      <span className={`severity-pip severity-${entry.severity}`} />
      <div className="activity-entry-body">
        <div className="activity-entry-primary">
          {primary}
          {entry.promoted && (
            <span
              className="activity-entry-promoted"
              title="A toast fired for this event"
            >
              toasted
            </span>
          )}
        </div>
        <div className="activity-entry-secondary">{secondary}</div>
      </div>
      <span className="activity-entry-time">{time}</span>
    </article>
  )
}

interface FilterOption<T extends string> {
  value: T
  label: string
}

function FilterChip<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string
  options: FilterOption<T>[]
  value: T
  onChange: (v: T) => void
}): JSX.Element {
  return (
    <div className="activity-filter">
      <span className="activity-filter-label">{label}</span>
      <div className="activity-filter-chips">
        {options.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={`activity-filter-chip ${
              value === opt.value ? "activity-filter-chip-on" : ""
            }`}
            onClick={() => onChange(opt.value)}
          >
            {opt.label}
          </button>
        ))}
      </div>
    </div>
  )
}

interface DayGroup {
  label: string
  entries: ActivityEntry[]
}

function groupByDay(entries: ActivityEntry[]): DayGroup[] {
  if (entries.length === 0) return []
  const today = startOfDay(new Date())
  const yesterday = new Date(today.getTime() - 86_400_000)
  const groups: DayGroup[] = []
  let current: DayGroup | null = null
  for (const entry of entries) {
    const at = new Date(entry.at)
    const day = startOfDay(at)
    const label =
      day.getTime() === today.getTime()
        ? "Today"
        : day.getTime() === yesterday.getTime()
          ? "Yesterday"
          : day.toLocaleDateString(undefined, {
              weekday: "long",
              month: "short",
              day: "numeric",
            })
    if (!current || current.label !== label) {
      current = { label, entries: [] }
      groups.push(current)
    }
    current.entries.push(entry)
  }
  return groups
}

function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate())
}

function formatTimeOnly(iso: string): string {
  const d = new Date(iso)
  return d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  })
}

function describeActivity(event: GardenEventPayload): {
  primary: string
  secondary: string
} {
  switch (event.kind) {
    case "stone_joined":
      return { primary: `${event.stone_name} joined`, secondary: event.endpoint }
    case "stone_left":
      return { primary: `${event.stone_name} offline`, secondary: "lost contact" }
    case "storage_activity": {
      const total = event.creates + event.modifies + event.deletes
      return {
        primary: `${event.bank_name} synced ${total} files on ${event.stone_name}`,
        secondary: `${event.creates} new · ${event.modifies} changed · ${event.deletes} removed`,
      }
    }
  }
}
