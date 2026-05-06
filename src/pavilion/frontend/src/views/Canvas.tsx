import { useCallback, useEffect, useMemo, useRef, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { GardenSphere, type TrackData } from "../lib/garden-sphere"

/// Pavilion's view of a stone — what `get_topology` returns from
/// the Awareness aggregate. Lantern's Stone shape is richer
/// (resources, services, capabilities); we map AwareStone into a
/// shape GardenSphere can consume, and the sphere falls back to
/// sensible defaults for missing fields.
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

interface CanvasProps {
  onClose: () => void
}

/// Canvas — the unified spatial substrate for ORCH-0039's
/// drag-canvas UX. Renders stones today; banks, offerings, and
/// seeds layer on in subsequent commits.
///
/// Data flow: get_topology Tauri command → AwareStone[] →
/// adapted to GardenSphere's stone shape. The sphere holds its
/// own diff-based update API (addStone / removeStone /
/// updateStone) so we don't tear down + rebuild on every event.
export function CanvasView({ onClose }: CanvasProps): JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const sphereRef = useRef<GardenSphere | null>(null)
  const knownIdsRef = useRef<Set<string>>(new Set())

  const [hovered, setHovered] = useState<string | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [tracked, setTracked] = useState<TrackData | null>(null)
  const [stones, setStones] = useState<AwareStone[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [error, setError] = useState<string | null>(null)

  /// Map AwareStone → the shape GardenSphere consumes. Missing
  /// fields fall through to GardenSphere's own defaults; the
  /// resource arcs render empty when resources is absent and
  /// the sphere edge-builder treats missing offerings as none.
  const toSphereShape = useCallback(
    (s: AwareStone) => ({
      stone_id: s.stone_id,
      stone_name: s.stone_name,
      health: s.health,
      // GardenSphere checks `stone.color` first, then falls back.
      // We don't have a color field in AwareStone yet; the
      // sphere's FALLBACK_COLOR (sage) is a sane default.
      // resources, offerings, tags omitted — sphere copes.
    }),
    [],
  )

  // ── Mount the sphere once, dispose on unmount ────────────────
  useEffect(() => {
    if (!containerRef.current) return
    const sphere = new GardenSphere(containerRef.current, {
      onHover: setHovered,
      onTransition: ({ selectedId, departingId: _departingId }) => {
        setSelectedId(selectedId)
      },
      onTrack: (data) => setTracked(data),
    })
    sphereRef.current = sphere
    return () => {
      sphere.destroy()
      sphereRef.current = null
      knownIdsRef.current.clear()
    }
  }, [])

  // ── Diff-based sync from stones state to sphere ──────────────
  useEffect(() => {
    const sphere = sphereRef.current
    if (!sphere) return
    const next = new Set(stones.map((s) => s.stone_id))
    const known = knownIdsRef.current

    if (known.size === 0 && stones.length > 0) {
      // First population — bulk setData is the cheap path.
      sphere.setData(stones.map(toSphereShape))
      stones.forEach((s) => known.add(s.stone_id))
      return
    }

    // Adds + updates.
    stones.forEach((s) => {
      if (!known.has(s.stone_id)) {
        sphere.addStone(toSphereShape(s))
        known.add(s.stone_id)
      } else {
        sphere.updateStone(s.stone_id, toSphereShape(s))
      }
    })
    // Removes.
    Array.from(known).forEach((id) => {
      if (!next.has(id)) {
        sphere.removeStone(id)
        known.delete(id)
      }
    })
  }, [stones, toSphereShape])

  // ── Initial load + topology subscription ─────────────────────
  const refresh = useCallback(async () => {
    try {
      const [s, t] = await Promise.all([
        invoke<AwareStone[]>("get_topology"),
        invoke<TendedStone | null>("get_tended"),
      ])
      setStones(s)
      setTended(t)
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    let unlistenTopology: UnlistenFn | undefined
    let unlistenTending: UnlistenFn | undefined
    let cancelled = false
    void (async () => {
      await refresh()
      unlistenTopology = await listen<AwareStone[]>(
        "topology-changed",
        (e) => {
          if (cancelled) return
          setStones(e.payload)
        },
      )
      unlistenTending = await listen<TendedStone>("tending-changed", (e) => {
        if (cancelled) return
        setTended(e.payload)
      })
    })()
    return () => {
      cancelled = true
      unlistenTopology?.()
      unlistenTending?.()
    }
  }, [refresh])

  const selectedStone = useMemo(
    () => stones.find((s) => s.stone_id === selectedId) ?? null,
    [stones, selectedId],
  )

  return (
    <main className="content canvas-content">
      <header className="topbar canvas-topbar">
        <button className="garden-pill" onClick={onClose} type="button">
          ← Home
        </button>
        <div className="topbar-spacer" />
        <span className="canvas-stone-count">
          {stones.length === 0
            ? "no stones in earshot"
            : `${stones.length} stone${stones.length === 1 ? "" : "s"}`}
        </span>
      </header>

      {error && (
        <section className="placeholder-note">
          <div className="placeholder-title">Error</div>
          <div className="placeholder-body">{error}</div>
        </section>
      )}

      <div className="canvas-stage">
        <div ref={containerRef} className="canvas-mount" />

        {selectedStone && tracked?.selected && (
          <CanvasStoneCard
            stone={selectedStone}
            tendedName={tended?.stone_name}
            position={tracked.selected.pos}
            onDismiss={() => {
              sphereRef.current?.resetView()
              setSelectedId(null)
            }}
          />
        )}

        {hovered && hovered !== selectedId && tracked?.hovered && (
          <CanvasHoverChip
            stone={stones.find((s) => s.stone_id === hovered)}
            position={tracked.hovered.pos}
          />
        )}
      </div>

      <footer className="canvas-hint">
        Right-drag to rotate · scroll to zoom · click a stone to focus
      </footer>
    </main>
  )
}

/// Floating detail card for the selected stone. Pinned to its
/// projected screen position so the user can keep both the
/// 3D context and the details visible.
interface CanvasStoneCardProps {
  stone: AwareStone
  tendedName: string | undefined
  position: { x: number; y: number }
  onDismiss: () => void
}

function CanvasStoneCard({
  stone,
  tendedName,
  position,
  onDismiss,
}: CanvasStoneCardProps): JSX.Element {
  const isTended = tendedName === stone.stone_name
  return (
    <div
      className="canvas-card"
      style={{ left: `${position.x}px`, top: `${position.y}px` }}
    >
      <header className="canvas-card-header">
        <span className={`dot ${healthDotClass(stone.health)}`} />
        <span className="canvas-card-title">{displayName(stone.stone_name)}</span>
        <button
          type="button"
          className="canvas-card-close"
          onClick={onDismiss}
          aria-label="Close"
        >
          ×
        </button>
      </header>
      <dl className="canvas-card-body">
        <div className="canvas-card-row">
          <dt>Endpoint</dt>
          <dd className="kv-value-mono">{stone.endpoint}</dd>
        </div>
        <div className="canvas-card-row">
          <dt>Health</dt>
          <dd>{stone.health}</dd>
        </div>
        <div className="canvas-card-row">
          <dt>Services</dt>
          <dd>{stone.services_count}</dd>
        </div>
        <div className="canvas-card-row">
          <dt>Last seen</dt>
          <dd>{formatAgeSecs(stone.age_secs)}</dd>
        </div>
      </dl>
      {isTended && <div className="canvas-card-tended-pill">tended</div>}
    </div>
  )
}

interface CanvasHoverChipProps {
  stone: AwareStone | undefined
  position: { x: number; y: number }
}

function CanvasHoverChip({
  stone,
  position,
}: CanvasHoverChipProps): JSX.Element | null {
  if (!stone) return null
  return (
    <div
      className="canvas-hover-chip"
      style={{ left: `${position.x}px`, top: `${position.y}px` }}
    >
      {displayName(stone.stone_name)}
    </div>
  )
}

function displayName(stoneName: string): string {
  return stoneName.startsWith("stone-") ? stoneName.slice(6) : stoneName
}

function healthDotClass(health: string): string {
  const h = health.toLowerCase()
  if (h === "healthy" || h === "thriving") return "dot-ok"
  if (h === "degraded" || h === "withering") return "dot-amber"
  if (h === "unhealthy" || h === "down" || h === "offline") return "dot-down"
  return ""
}

function formatAgeSecs(secs: number): string {
  if (secs < 60) return `${secs}s ago`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h ago`
  return `${Math.floor(secs / 86_400)}d ago`
}
