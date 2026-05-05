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

interface BankSummary {
  name: string
  replica_count: number
  primary_stone?: string | null
  roles?: string[]
}

interface StoragePayload {
  count: number
  banks: BankSummary[]
}

type Severity = "info" | "notice" | "warn" | "urgent"

interface StoneJoinedEvent { kind: "stone_joined"; stone_id: string; stone_name: string; endpoint: string }
interface StoneLeftEvent { kind: "stone_left"; stone_id: string; stone_name: string }
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

function App() {
  const [version, setVersion] = useState<string>("…")
  const [now, setNow] = useState<string>(new Date().toLocaleTimeString())
  const [stones, setStones] = useState<AwareStone[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [services, setServices] = useState<ServicesPayload | null>(null)
  const [servicesError, setServicesError] = useState<string | null>(null)
  const [pond, setPond] = useState<PondPayload | null>(null)
  const [pondError, setPondError] = useState<string | null>(null)
  const [storage, setStorage] = useState<StoragePayload | null>(null)
  const [storageError, setStorageError] = useState<string | null>(null)
  const [activity, setActivity] = useState<ActivityEntry[]>([])

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
    try {
      const result = await invoke<StoragePayload | null>("get_storage")
      setStorage(result)
      setStorageError(null)
    } catch (e) {
      setStorageError(String(e))
      setStorage(null)
    }
  }, [])

  const refreshActivity = useCallback(async () => {
    try {
      const result = await invoke<ActivityEntry[]>("get_activity")
      setActivity(result)
    } catch (e) {
      console.error("get_activity failed:", e)
    }
  }, [])

  // Initial load + push subscriptions.
  useEffect(() => {
    let unlistenTopology: UnlistenFn | undefined
    let unlistenTending: UnlistenFn | undefined
    let unlistenActivity: UnlistenFn | undefined
    let cancelled = false

    const setup = async () => {
      try {
        const [initialStones, initialTended, initialActivity] = await Promise.all([
          invoke<AwareStone[]>("get_topology"),
          invoke<TendedStone | null>("get_tended"),
          invoke<ActivityEntry[]>("get_activity"),
        ])
        if (cancelled) return
        setStones(initialStones)
        setTended(initialTended)
        setActivity(initialActivity)
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
      unlistenActivity = await listen<null>("activity-changed", () => {
        refreshActivity()
      })
    }
    setup()

    getVersion().then(setVersion).catch(() => setVersion("?"))
    const clockId = setInterval(() => setNow(new Date().toLocaleTimeString()), 1000)

    return () => {
      cancelled = true
      unlistenTopology?.()
      unlistenTending?.()
      unlistenActivity?.()
      clearInterval(clockId)
    }
  }, [refreshTendedData, refreshActivity])

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

  const storageTileValue =
    !tended ? "—" :
    storageError ? "!" :
    storage === null ? "…" :
    String(storage.count)
  const storageTileFoot =
    !tended ? "no stone tended" :
    storageError ? "fetch failed" :
    storage === null ? "fetching…" :
    storage.count === 0 ? "no banks reachable" :
    storage.count === 1 ? "1 bank in this garden" :
    `${storage.count} banks in this garden`

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
            <div className="tile-value">{storageTileValue}</div>
            <div className="tile-foot">{storageTileFoot}</div>
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

        {storage && storage.banks.length > 0 && (
          <section className="stones-list">
            <div className="stones-list-title">
              Banks across the garden
            </div>
            {storage.banks.map(bank => {
              const replicas = bank.replica_count === 1
                ? "1 replica"
                : `${bank.replica_count} replicas`
              const trailing = bank.primary_stone
                ? `primary · ${bank.primary_stone}`
                : (bank.roles && bank.roles.length > 0)
                  ? bank.roles.join(", ")
                  : "—"
              return (
                <div className="stone-row" key={bank.name} style={{ cursor: "default" }}>
                  <span className="stone-name">{bank.name}</span>
                  <span className="stone-endpoint">{replicas}</span>
                  <span className="stone-age">{trailing}</span>
                </div>
              )
            })}
          </section>
        )}

        {activity.length > 0 && (
          <section className="stones-list">
            <div className="stones-list-title">Recent activity</div>
            {activity.slice(0, 12).map(entry => {
              const { primary, secondary } = describeActivity(entry.event)
              return (
                <div className="stone-row" key={entry.id} style={{ cursor: "default" }}>
                  <span className="stone-name">
                    <span className={`severity-pip severity-${entry.severity}`} />
                    {primary}
                  </span>
                  <span className="stone-endpoint">{secondary}</span>
                  <span className="stone-age">{formatAgeFromIso(entry.at)}</span>
                </div>
              )
            })}
          </section>
        )}

        <section className="placeholder-note">
          <div className="placeholder-title">Awareness · API integration</div>
          <div className="placeholder-body">
            Topology is push-driven from <code>STONE_CHIRP</code> + provoked
            <code> DISCOVERY_RESPONSE</code>. Services, pond, and storage are
            pull-on-demand against the tended stone (refresh on every
            <code> tending-changed</code>). Tending file shared with Rake at
            <code> ~/.zen-garden/.tending</code>. Toasts fire on stone joined /
            offline and on storage activity bursts; the same events feed the
            Activity row above. Cloud Filter and companions arrive in the
            next milestone.
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

function describeActivity(event: GardenEventPayload): { primary: string; secondary: string } {
  switch (event.kind) {
    case "stone_joined":
      return { primary: `${event.stone_name} joined`, secondary: event.endpoint }
    case "stone_left":
      return { primary: `${event.stone_name} offline`, secondary: "lost contact" }
    case "storage_activity": {
      const total = event.creates + event.modifies + event.deletes
      return {
        primary: `${event.bank_name} synced ${total} files`,
        secondary: `${event.creates} new · ${event.modifies} changed · ${event.deletes} removed`,
      }
    }
  }
}

function formatAgeFromIso(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime()
  if (ms < 0) return "just now"
  const secs = Math.floor(ms / 1000)
  if (secs < 60) return `${secs}s`
  const mins = Math.floor(secs / 60)
  if (mins < 60) return `${mins}m`
  const hrs = Math.floor(mins / 60)
  if (hrs < 24) return `${hrs}h`
  return `${Math.floor(hrs / 24)}d`
}

export default App
