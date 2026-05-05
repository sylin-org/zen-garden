import { useCallback, useEffect, useMemo, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { HomeView } from "./views/Home"
import { ServicesView } from "./views/Services"
import { SettingsView } from "./views/Settings"

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

interface Settings {
  quiet_hours: { enabled: boolean; start: string; end: string }
  suppressed_kinds: string[]
  autostart_enabled: boolean
}

type View = "home" | "services" | "settings"

function App(): JSX.Element {
  const [view, setView] = useState<View>("home")
  const [stones, setStones] = useState<AwareStone[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)

  // Shell-level state — drives the status bar and the sidebar
  // active row. Per-view state stays inside the view components.
  useEffect(() => {
    let unlistenTopology: UnlistenFn | undefined
    let unlistenTending: UnlistenFn | undefined
    let unlistenSettings: UnlistenFn | undefined
    let cancelled = false

    const setup = async () => {
      try {
        const [s, t, st] = await Promise.all([
          invoke<AwareStone[]>("get_topology"),
          invoke<TendedStone | null>("get_tended"),
          invoke<Settings>("get_settings"),
        ])
        if (cancelled) return
        setStones(s)
        setTended(t)
        setSettings(st)
      } catch (e) {
        console.error("shell initial load failed:", e)
      }

      unlistenTopology = await listen<AwareStone[]>("topology-changed", (e) => {
        setStones(e.payload)
      })
      unlistenTending = await listen<TendedStone>("tending-changed", (e) => {
        setTended(e.payload)
      })
      unlistenSettings = await listen<Settings>("settings-changed", (e) => {
        setSettings(e.payload)
      })
    }
    setup()

    return () => {
      cancelled = true
      unlistenTopology?.()
      unlistenTending?.()
      unlistenSettings?.()
    }
  }, [])

  const tendedReachable = useMemo(() => {
    if (!tended) return false
    return stones.some(s => s.stone_name === tended.stone_name || s.endpoint === tended.endpoint)
  }, [stones, tended])

  const statusDotClass =
    tendedReachable ? "dot dot-ok" :
    tended ? "dot dot-down" :
    stones.length > 0 ? "dot dot-amber" :
    "dot"

  const statusText =
    tendedReachable ? `connected to ${tended!.stone_name}` :
    tended ? `${tended.stone_name} silent` :
    stones.length > 0 ? "selecting tended stone…" :
    "no garden in earshot"

  const quietHoursLabel = settings?.quiet_hours.enabled
    ? `quiet hours ${settings.quiet_hours.start}–${settings.quiet_hours.end}`
    : "quiet hours off"

  const goHome = useCallback(() => setView("home"), [])

  return (
    <div className="pavilion-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">P</div>
          <div className="brand-name">Pavilion</div>
        </div>
        <nav className="nav">
          <a
            className={`nav-item ${view === "home" ? "active" : ""}`}
            onClick={() => setView("home")}
          >
            Home
          </a>
          <a className="nav-item disabled">Garden</a>
          <a className="nav-item disabled">Storage</a>
          <a
            className={`nav-item ${view === "services" ? "active" : ""}`}
            onClick={() => setView("services")}
          >
            Services
          </a>
          <a className="nav-item disabled">Companions</a>
          <a className="nav-item disabled">Pond</a>
          <a className="nav-item disabled">Activity</a>
          <div className="nav-spacer" />
          <a
            className={`nav-item ${view === "settings" ? "active" : ""}`}
            onClick={() => setView("settings")}
          >
            Settings
          </a>
        </nav>
      </aside>

      {view === "home" && <HomeView />}
      {view === "services" && <ServicesView onClose={goHome} />}
      {view === "settings" && <SettingsView onClose={goHome} />}

      <footer className="statusbar">
        <span className={statusDotClass} />
        {statusText}
        <span className="sep">·</span>
        <span>{quietHoursLabel}</span>
      </footer>
    </div>
  )
}

export default App
