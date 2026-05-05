import { useCallback, useEffect, useMemo, useState } from "react"
import { getVersion } from "@tauri-apps/api/app"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

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

interface ServiceLite {
  name: string
  offering: string
  status: string
}

interface ServicesPayload {
  count: number
  services: ServiceLite[]
}

interface PondPayload {
  initialised: boolean
  status: string
  name: string | null
  member_count: number | null
  cornerstone: string | null
}

function App() {
  const [version, setVersion] = useState<string>("…")
  const [now, setNow] = useState<string>(new Date().toLocaleTimeString())
  const [stones, setStones] = useState<AwareStone[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [services, setServices] = useState<ServicesPayload | null>(null)
  const [servicesError, setServicesError] = useState<string | null>(null)
  const [pond, setPond] = useState<PondPayload | null>(null)
  const [pondError, setPondError] = useState<string | null>(null)

  // Fetch tended-stone data (services + pond). Called on mount and on
  // every tending-changed event. Errors are surfaced into per-tile
  // error state, not thrown.
  const refreshTendedData = useCallback(async () => {
    try {
      const result = await invoke<ServicesPayload | null>("get_services")
      setServices(result)
      setServicesError(null)
    } catch (e) {
      setServicesError(String(e))
      setServices(null)
    }
    try {
      const result = await invoke<PondPayload | null>("get_pond_status")
      setPond(result)
      setPondError(null)
    } catch (e) {
      setPondError(String(e))
      setPond(null)
    }
  }, [])

  // Initial load + push subscriptions.
  useEffect(() => {
    let unlistenTopology: UnlistenFn | undefined
    let unlistenTending: UnlistenFn | undefined
    let cancelled = false

    const setup = async () => {
      try {
        const [initialStones, initialTended] = await Promise.all([
          invoke<AwareStone[]>("get_topology"),
          invoke<TendedStone | null>("get_tended"),
        ])
        if (cancelled) return
        setStones(initialStones)
        setTended(initialTended)
        if (initialTended) {
          refreshTendedData()
        }
      } catch (e) {
        console.error("initial load failed:", e)
      }

      unlistenTopology = await listen<AwareStone[]>("topology-changed", (event) => {
        setStones(event.payload)
      })
      unlistenTending = await listen<TendedStone>("tending-changed", (event) => {
        setTended(event.payload)
        // Tending changed → re-fetch all tended-stone data.
        refreshTendedData()
      })
    }
    setup()

    getVersion().then(setVersion).catch(() => setVersion("?"))
    const clockId = setInterval(() => setNow(new Date().toLocaleTimeString()), 1000)

    return () => {
      cancelled = true
      unlistenTopology?.()
      unlistenTending?.()
      clearInterval(clockId)
    }
  }, [refreshTendedData])

  const setStoneAsTended = useCallback(async (stone: AwareStone) => {
    try {
      await invoke("set_tended", { stoneId: stone.stone_id })
    } catch (e) {
      console.error("set_tended failed:", e)
    }
  }, [])

  // Derived state ────────────────────────────────────────────────

  const tendedReachable = useMemo(() => {
    if (!tended) return false
    return stones.some(s => s.stone_name === tended.stone_name || s.endpoint === tended.endpoint)
  }, [stones, tended])

  const gardenPill = tended
    ? `tending ${tended.stone_name}`
    : stones.length > 0
      ? `${stones.length} stone${stones.length === 1 ? "" : "s"} aware · auto-tending…`
      : "no garden yet"

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

  // ── Tile values ──────────────────────────────────────────────

  const stoneTile = String(stones.length)
  const stoneFoot = stones.length === 0 ? "listening for chirps…" : "chirping in earshot · TTL 90s"

  const servicesTileValue =
    !tended ? "—" :
    servicesError ? "!" :
    services === null ? "…" :
    String(services.count)
  const servicesTileFoot =
    !tended ? "no stone tended" :
    servicesError ? "fetch failed" :
    services === null ? "fetching…" :
    services.count === 0 ? "no offerings on this stone" :
    `running on ${tended.stone_name}`

  const pondTileValue =
    !tended ? "—" :
    pondError ? "!" :
    pond === null ? "…" :
    !pond.initialised ? "—" :
    pond.member_count !== null ? String(pond.member_count) :
    "•"
  const pondTileFoot =
    !tended ? "no stone tended" :
    pondError ? "fetch failed" :
    pond === null ? "fetching…" :
    !pond.initialised ? "no pond on this stone" :
    pond.name ? pond.name :
    pond.status

  return (
    <div className="pavilion-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">P</div>
          <div className="brand-name">Pavilion</div>
        </div>
        <nav className="nav">
          <a className="nav-item active">Home</a>
          <a className="nav-item disabled">Garden</a>
          <a className="nav-item disabled">Storage</a>
          <a className="nav-item disabled">Services</a>
          <a className="nav-item disabled">Companions</a>
          <a className="nav-item disabled">Pond</a>
          <a className="nav-item disabled">Activity</a>
          <div className="nav-spacer" />
          <a className="nav-item disabled">Settings</a>
        </nav>
      </aside>

      <main className="content">
        <header className="topbar">
          <div className="garden-pill">{gardenPill}</div>
          <div className="topbar-spacer" />
          <div className="topbar-clock">{now}</div>
        </header>

        <section className="hero">
          <h1>Pavilion is running.</h1>
          <p className="subtle">
            v{version} · awareness via UDP chirps · tending via ~/.zen-garden/.tending
          </p>
        </section>

        <section className="tiles">
          <article className="tile">
            <div className="tile-label">Stones</div>
            <div className="tile-value">{stoneTile}</div>
            <div className="tile-foot">{stoneFoot}</div>
          </article>
          <article className="tile">
            <div className="tile-label">Storage</div>
            <div className="tile-value">—</div>
            <div className="tile-foot">no banks reachable</div>
          </article>
          <article className="tile">
            <div className="tile-label">Services</div>
            <div className="tile-value">{servicesTileValue}</div>
            <div className="tile-foot">{servicesTileFoot}</div>
          </article>
          <article className="tile">
            <div className="tile-label">Pond</div>
            <div className="tile-value">{pondTileValue}</div>
            <div className="tile-foot">{pondTileFoot}</div>
          </article>
        </section>

        {stones.length > 0 && (
          <section className="stones-list">
            <div className="stones-list-title">Aware stones · click to tend</div>
            {stones.map(s => {
              const isTended = tended?.stone_name === s.stone_name
              return (
                <button
                  key={s.stone_id}
                  className={`stone-row ${isTended ? "stone-row-tended" : ""}`}
                  onClick={() => setStoneAsTended(s)}
                  disabled={isTended}
                  title={isTended ? "Currently tended" : `Tend ${s.stone_name}`}
                >
                  <span className="stone-name">{s.stone_name}</span>
                  <span className="stone-endpoint">{s.endpoint}</span>
                  <span className="stone-age">{s.age_secs}s</span>
                </button>
              )
            })}
          </section>
        )}

        {services && services.services.length > 0 && (
          <section className="stones-list">
            <div className="stones-list-title">
              Services on {tended!.stone_name}
            </div>
            {services.services.map(svc => (
              <div className="stone-row" key={svc.name} style={{ cursor: "default" }}>
                <span className="stone-name">{svc.name}</span>
                <span className="stone-endpoint">{svc.offering}</span>
                <span className="stone-age">{svc.status}</span>
              </div>
            ))}
          </section>
        )}

        <section className="placeholder-note">
          <div className="placeholder-title">Awareness · API integration · DISC-0001 cleanup</div>
          <div className="placeholder-body">
            Topology is push-driven from <code>STONE_CHIRP</code> + provoked
            <code> DISCOVERY_RESPONSE</code>. Services and pond are pull-on-demand
            against the tended stone (refresh on every <code>tending-changed</code>).
            Tending file shared with Rake at <code>~/.zen-garden/.tending</code>.
            Cloud Filter, storage, and companions arrive in the next milestone.
          </div>
        </section>
      </main>

      <footer className="statusbar">
        <span className={statusDotClass} />
        {statusText}
        <span className="sep">·</span>
        <span>0 syncing</span>
        <span className="sep">·</span>
        <span>quiet hours off</span>
      </footer>
    </div>
  )
}

export default App
