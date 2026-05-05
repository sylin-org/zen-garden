import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react"
import { invoke } from "@tauri-apps/api/core"

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

interface ServiceLite {
  name: string
  offering: string
  status: string
}

interface ServicesPayload {
  count: number
  services: ServiceLite[]
}

interface TendedStone {
  stone_name: string
  endpoint: string
}

export type PaletteView = "home" | "services" | "pond" | "activity" | "settings"

type Action =
  | { id: string; label: string; hint: string; kind: "navigate"; view: PaletteView }
  | { id: string; label: string; hint: string; kind: "tend"; stone_id: string }
  | {
      id: string
      label: string
      hint: string
      kind: "service"
      op: "wake" | "rest" | "restart"
      name: string
    }

interface CommandPaletteProps {
  onClose: () => void
  onNavigate: (view: PaletteView) => void
}

/// Tiny fuzzy matcher: returns a score when every char of `query`
/// appears in `text` in order, otherwise null. Lower scores rank
/// higher (matches that are tighter clusters score lower).
function fuzzyScore(query: string, text: string): number | null {
  if (query.length === 0) return 0
  const q = query.toLowerCase()
  const t = text.toLowerCase()
  let qi = 0
  let lastMatch = -1
  let score = 0
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      if (lastMatch >= 0) {
        score += ti - lastMatch
      }
      lastMatch = ti
      qi++
    }
  }
  if (qi < q.length) return null
  return score
}

export function CommandPalette({ onClose, onNavigate }: CommandPaletteProps): JSX.Element {
  const [query, setQuery] = useState<string>("")
  const [stones, setStones] = useState<AwareStone[]>([])
  const [services, setServices] = useState<ServicesPayload | null>(null)
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [selectedIdx, setSelectedIdx] = useState<number>(0)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const listRef = useRef<HTMLDivElement | null>(null)

  // Pull a fresh snapshot when the palette opens — stale data from
  // a long-idle window is worse than a 50ms re-fetch.
  useEffect(() => {
    inputRef.current?.focus()
    void (async () => {
      try {
        const [s, t] = await Promise.all([
          invoke<AwareStone[]>("get_topology"),
          invoke<TendedStone | null>("get_tended"),
        ])
        setStones(s)
        setTended(t)
      } catch {
        // ignore; results will just be empty for that source
      }
      try {
        const sv = await invoke<ServicesPayload | null>("get_services")
        setServices(sv)
      } catch {
        // ignore
      }
    })()
  }, [])

  const allActions: Action[] = useMemo(() => {
    const acts: Action[] = []

    // Destinations — always available.
    acts.push(
      { id: "nav:home", label: "Open Home", hint: "destination", kind: "navigate", view: "home" },
      {
        id: "nav:services",
        label: "Open Services",
        hint: "destination",
        kind: "navigate",
        view: "services",
      },
      { id: "nav:pond", label: "Open Pond", hint: "destination", kind: "navigate", view: "pond" },
      {
        id: "nav:activity",
        label: "Open Activity",
        hint: "destination",
        kind: "navigate",
        view: "activity",
      },
      {
        id: "nav:settings",
        label: "Open Settings",
        hint: "destination",
        kind: "navigate",
        view: "settings",
      }
    )

    // Tend a stone.
    for (const stone of stones) {
      const isTended = tended?.stone_name === stone.stone_name
      if (isTended) continue
      acts.push({
        id: `tend:${stone.stone_id}`,
        label: `Tend ${stone.stone_name}`,
        hint: stone.endpoint,
        kind: "tend",
        stone_id: stone.stone_id,
      })
    }

    // Services on the tended stone.
    if (services && tended) {
      for (const svc of services.services) {
        const running = svc.status.toLowerCase() === "running"
        if (!running) {
          acts.push({
            id: `wake:${svc.name}`,
            label: `Wake ${svc.name}`,
            hint: `${svc.offering} on ${tended.stone_name}`,
            kind: "service",
            op: "wake",
            name: svc.name,
          })
        }
        if (running) {
          acts.push({
            id: `rest:${svc.name}`,
            label: `Rest ${svc.name}`,
            hint: `${svc.offering} on ${tended.stone_name}`,
            kind: "service",
            op: "rest",
            name: svc.name,
          })
        }
        acts.push({
          id: `restart:${svc.name}`,
          label: `Restart ${svc.name}`,
          hint: `${svc.offering} on ${tended.stone_name}`,
          kind: "service",
          op: "restart",
          name: svc.name,
        })
      }
    }

    return acts
  }, [stones, services, tended])

  const ranked: Action[] = useMemo(() => {
    if (query.trim().length === 0) {
      return allActions
    }
    const q = query.trim()
    const scored: { action: Action; score: number }[] = []
    for (const a of allActions) {
      const haystack = `${a.label} ${a.hint}`
      const s = fuzzyScore(q, haystack)
      if (s !== null) scored.push({ action: a, score: s })
    }
    scored.sort((a, b) => a.score - b.score)
    return scored.map((s) => s.action)
  }, [allActions, query])

  // Reset selection whenever the result set changes.
  useEffect(() => {
    setSelectedIdx(0)
  }, [ranked.length])

  // Keep the highlighted row in view when arrow-keying through a
  // longer list.
  useEffect(() => {
    const list = listRef.current
    if (!list) return
    const el = list.querySelector<HTMLElement>(
      `[data-palette-idx="${selectedIdx}"]`
    )
    el?.scrollIntoView({ block: "nearest" })
  }, [selectedIdx])

  const execute = useCallback(
    async (action: Action) => {
      try {
        switch (action.kind) {
          case "navigate":
            onNavigate(action.view)
            onClose()
            return
          case "tend":
            await invoke("set_tended", { stoneId: action.stone_id })
            onClose()
            return
          case "service": {
            const cmd =
              action.op === "wake"
                ? "wake_service"
                : action.op === "rest"
                  ? "rest_service"
                  : "restart_service"
            await invoke(cmd, { name: action.name })
            onClose()
            return
          }
        }
      } catch (e) {
        console.error("palette action failed:", e)
      }
    },
    [onClose, onNavigate]
  )

  const onKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === "Escape") {
        e.preventDefault()
        onClose()
        return
      }
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setSelectedIdx((i) => Math.min(i + 1, Math.max(ranked.length - 1, 0)))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setSelectedIdx((i) => Math.max(i - 1, 0))
        return
      }
      if (e.key === "Enter") {
        e.preventDefault()
        const a = ranked[selectedIdx]
        if (a) void execute(a)
      }
    },
    [execute, onClose, ranked, selectedIdx]
  )

  return (
    <div
      className="palette-backdrop"
      onClick={onClose}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className="palette"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <input
          ref={inputRef}
          className="palette-input"
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Type a destination, stone, or service…"
          aria-label="Command palette input"
          spellCheck={false}
          autoComplete="off"
        />
        <div className="palette-results" ref={listRef}>
          {ranked.length === 0 ? (
            <div className="palette-empty">No matches</div>
          ) : (
            ranked.map((a, i) => (
              <button
                key={a.id}
                type="button"
                data-palette-idx={i}
                className={`palette-row ${i === selectedIdx ? "palette-row-selected" : ""}`}
                onMouseEnter={() => setSelectedIdx(i)}
                onClick={() => void execute(a)}
              >
                <span className="palette-row-label">{a.label}</span>
                <span className="palette-row-hint">{a.hint}</span>
              </button>
            ))
          )}
        </div>
        <div className="palette-footer">
          <span>↑↓ navigate</span>
          <span className="sep">·</span>
          <span>↵ run</span>
          <span className="sep">·</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  )
}
