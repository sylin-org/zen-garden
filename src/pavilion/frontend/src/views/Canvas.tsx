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

/// Wire shape returned by `get_storage` — garden-wide bank summary
/// from the tended Moss. The canvas reads `name` + `replica_count`
/// for the bank node label; `roles` is surfaced on the detail card.
interface GardenBankSummary {
  name: string
  replica_count: number
  primary_stone: string | null
  roles: string[]
}

interface StoragePayload {
  count: number
  banks: GardenBankSummary[]
}

/// One offering running on the tended stone — enough to render
/// a draggable service chip and to dispatch the capture call.
interface ServiceLite {
  name: string
  offering: string
  status: string
}

interface ServicesPayload {
  count: number
  services: ServiceLite[]
}

interface CaptureSnapshotResult {
  snapshot_id: string
  event_id: string
  source_fqn: string
  source_stone: string
  size_total_bytes: number
  volumes: number
  external_mounts: number
}

/// Drag payload format. The canvas's drop zone reads dataTransfer
/// to know what's being dragged and apply the right pairing.
interface DragOfferingPayload {
  kind: "offering"
  source_stone: string
  fqn: string
  display_name: string
}

interface DragSeedPayload {
  kind: "seed"
  snapshot_id: string
  source_fqn: string
  source_stone: string
  bank_name: string
}

type DragPayload = DragOfferingPayload | DragSeedPayload

interface BankSeedEntry {
  snapshot_id: string
  source_fqn: string
  source_stone: string
  source_event_id: string
  created_at: string
  size_total_bytes: number
}

interface BankSeedsResult {
  bank: string
  count: number
  seeds: BankSeedEntry[]
}

interface PlantSnapshotResult {
  snapshot_id: string
  event_id: string
  source_fqn: string
  target_fqn: string
  digest_drift: string
}

const DRAG_MIME = "application/zen-garden+json"

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

  const knownBankIdsRef = useRef<Set<string>>(new Set())
  const [hovered, setHovered] = useState<string | null>(null)
  const [hoveredKind, setHoveredKind] = useState<"stone" | "bank" | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedKind, setSelectedKind] = useState<"stone" | "bank" | null>(null)
  const [tracked, setTracked] = useState<TrackData | null>(null)
  const [stones, setStones] = useState<AwareStone[]>([])
  const [banks, setBanks] = useState<GardenBankSummary[]>([])
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [error, setError] = useState<string | null>(null)

  /// Services on the *currently selected stone* (only fetched when
  /// the user picks a stone we know how to talk to — typically the
  /// tended one). Populates the offering chips on the stone card
  /// that the drag layer renders.
  const [selectedServices, setSelectedServices] = useState<ServiceLite[]>([])

  /// Seeds living in the *currently selected bank*. Fetched lazily
  /// when a bank becomes the selected node. Bank-scoped seed
  /// catalogs let the bank card render draggable seed chips that
  /// drop onto stones to plant.
  const [selectedSeeds, setSelectedSeeds] = useState<BankSeedEntry[]>([])

  /// Drag state — which bank node the cursor is currently over so
  /// the sphere can highlight it as a valid drop target. Set on
  /// dragover, cleared on dragleave/drop.
  const [dragOver, setDragOver] = useState<string | null>(null)
  /// Active forming-snapshot indicators per bank (real values,
  /// updated by the capture command's progress + completion).
  const [forming, setForming] = useState<
    Record<string, { fqn: string; bankName: string }>
  >({})
  /// Most recent backup result for the optimistic toast.
  const [lastCapture, setLastCapture] = useState<CaptureSnapshotResult | null>(
    null,
  )

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
      onHover: (id, kind) => {
        setHovered(id)
        setHoveredKind(kind ?? null)
      },
      onTransition: ({ selectedId, departingId: _departingId, kind }) => {
        setSelectedId(selectedId)
        setSelectedKind(kind ?? null)
      },
      onTrack: (data) => setTracked(data),
    })
    sphereRef.current = sphere
    return () => {
      sphere.destroy()
      sphereRef.current = null
      knownIdsRef.current.clear()
      knownBankIdsRef.current.clear()
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

  // ── Bank sync (mirrors the stone diff path) ──────────────────
  useEffect(() => {
    const sphere = sphereRef.current
    if (!sphere) return
    const next = new Set(banks.map((b) => b.name))
    const known = knownBankIdsRef.current

    if (known.size === 0 && banks.length > 0) {
      sphere.setBanks(banks)
      banks.forEach((b) => known.add(b.name))
      return
    }
    banks.forEach((b) => {
      if (!known.has(b.name)) {
        sphere.addBank(b)
        known.add(b.name)
      } else {
        sphere.updateBank(b.name, b)
      }
    })
    Array.from(known).forEach((id) => {
      if (!next.has(id)) {
        sphere.removeBank(id)
        known.delete(id)
      }
    })
  }, [banks])

  // ── Initial load + topology subscription ─────────────────────
  const refresh = useCallback(async () => {
    try {
      const [s, t, storage] = await Promise.all([
        invoke<AwareStone[]>("get_topology"),
        invoke<TendedStone | null>("get_tended"),
        invoke<StoragePayload | null>("get_storage"),
      ])
      setStones(s)
      setTended(t)
      setBanks(storage?.banks ?? [])
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

  // ── Fetch the selected stone's services when it's the tended stone ──
  // The Pavilion API surface today only exposes services for the
  // tended stone (`get_services`). Selecting a non-tended stone
  // simply doesn't populate the chip list — the user can tend it
  // first to manage offerings on it.
  useEffect(() => {
    if (selectedKind !== "stone") {
      setSelectedServices([])
      return
    }
    const stone = stones.find((s) => s.stone_id === selectedId)
    if (!stone || stone.stone_name !== tended?.stone_name) {
      setSelectedServices([])
      return
    }
    let cancelled = false
    void (async () => {
      try {
        const payload = await invoke<ServicesPayload | null>("get_services")
        if (!cancelled) setSelectedServices(payload?.services ?? [])
      } catch (e) {
        // Service-fetch failure shouldn't kill the whole canvas;
        // surface as empty chip list and log.
        console.error("get_services failed:", e)
        if (!cancelled) setSelectedServices([])
      }
    })()
    return () => {
      cancelled = true
    }
  }, [selectedId, selectedKind, tended, stones])

  /// Fetch seeds when the user selects a bank — populates the
  /// draggable chips on the bank's detail card. Reset to empty
  /// when the selection moves away.
  useEffect(() => {
    if (selectedKind !== "bank" || !selectedId) {
      setSelectedSeeds([])
      return
    }
    let cancelled = false
    void (async () => {
      try {
        const result = await invoke<BankSeedsResult>("list_seeds_in_bank", {
          bankName: selectedId,
        })
        if (!cancelled) {
          setSelectedSeeds(result.seeds)
          // Update the bank node's seed-count chip to reflect
          // the catalog the user is now looking at.
          sphereRef.current?.setSeedCount(selectedId, result.count)
        }
      } catch (e) {
        console.error("list_seeds_in_bank failed:", e)
        if (!cancelled) setSelectedSeeds([])
      }
    })()
    return () => {
      cancelled = true
    }
  }, [selectedId, selectedKind])

  // ── Drag-drop handlers on the canvas mount ────────────────────
  /// Pick the valid drop target for a given drag payload by
  /// reading the sphere's currently-hovered node + kind. Returns
  /// `null` when the drop wouldn't be valid (wrong kind under
  /// cursor, or nothing under cursor).
  const validDropTargetFor = useCallback(
    (payload: DragPayload): { kind: "stone" | "bank"; id: string } | null => {
      if (!hovered || !hoveredKind) return null
      if (payload.kind === "offering" && hoveredKind === "bank") {
        return { kind: "bank", id: hovered }
      }
      if (payload.kind === "seed" && hoveredKind === "stone") {
        // Find the stone object so we can resolve its name (the
        // sphere's hovered id is stone_id; plant takes
        // stone_name).
        return { kind: "stone", id: hovered }
      }
      return null
    },
    [hovered, hoveredKind],
  )

  const handleDragOver = useCallback(
    (e: React.DragEvent) => {
      // Only consume drag events that carry our payload — leaves
      // native browser drag (e.g. file-into-window) alone.
      if (!e.dataTransfer.types.includes(DRAG_MIME)) return
      e.preventDefault()
      e.dataTransfer.dropEffect = "copy"

      // Pavilion can't read the dataTransfer on dragover (only
      // dragstart and drop expose it on most browsers). Use the
      // sphere's hovered slot as the drop-target indicator —
      // the inset glow on the canvas mount lights up as long as
      // *something* is under the cursor; the drop handler
      // validates the kind pairing.
      setDragOver(hovered)
    },
    [hovered],
  )

  const handleDragLeave = useCallback(() => {
    setDragOver(null)
  }, [])

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      const raw = e.dataTransfer.getData(DRAG_MIME)
      setDragOver(null)
      if (!raw) return
      e.preventDefault()
      let payload: DragPayload
      try {
        payload = JSON.parse(raw) as DragPayload
      } catch {
        return
      }

      const target = validDropTargetFor(payload)
      if (!target) return

      // Dispatch on the (source-kind, target-kind) pair per the
      // ORCH-0039 resolution table.
      if (payload.kind === "offering" && target.kind === "bank") {
        const bankName = target.id
        const formingId = `${payload.source_stone}::${payload.fqn}->${bankName}`
        setForming((prev) => ({
          ...prev,
          [formingId]: { fqn: payload.fqn, bankName },
        }))
        try {
          const result = await invoke<CaptureSnapshotResult>("capture_snapshot", {
            stone: payload.source_stone,
            fqn: payload.fqn,
            target: `bank:${bankName}`,
          })
          setLastCapture(result)
          // Refresh the bank's seed catalog so the chip count
          // reflects the new arrival.
          try {
            const updated = await invoke<BankSeedsResult>("list_seeds_in_bank", {
              bankName,
            })
            sphereRef.current?.setSeedCount(bankName, updated.count)
            if (selectedKind === "bank" && selectedId === bankName) {
              setSelectedSeeds(updated.seeds)
            }
          } catch {
            // Catalog refresh failure is cosmetic; the capture
            // succeeded and the next selection will resync.
          }
        } catch (err) {
          setError(`Backup failed: ${String(err)}`)
        } finally {
          setForming((prev) => {
            const next = { ...prev }
            delete next[formingId]
            return next
          })
        }
        return
      }

      if (payload.kind === "seed" && target.kind === "stone") {
        // Stones in awareness are keyed by stone_id; plant takes
        // stone_name. Look up the stone object to resolve.
        const stone = stones.find((s) => s.stone_id === target.id)
        if (!stone) return
        const formingId = `${payload.snapshot_id}->${stone.stone_name}`
        setForming((prev) => ({
          ...prev,
          [formingId]: {
            fqn: payload.source_fqn,
            bankName: payload.bank_name,
          },
        }))
        try {
          const result = await invoke<PlantSnapshotResult>("plant_snapshot", {
            targetStone: stone.stone_name,
            targetFqn: payload.source_fqn,
            fromSnapshot: payload.snapshot_id,
            fromStone: payload.source_stone,
            fromFqn: payload.source_fqn,
          })
          setLastCapture(null)
          setError(null)
          setLastPlant(result)
        } catch (err) {
          setError(`Plant failed: ${String(err)}`)
        } finally {
          setForming((prev) => {
            const next = { ...prev }
            delete next[formingId]
            return next
          })
        }
        return
      }
    },
    [validDropTargetFor, stones, selectedKind, selectedId],
  )

  const [lastPlant, setLastPlant] = useState<PlantSnapshotResult | null>(null)

  // Push the drag-target highlight into the sphere's hovered slot
  // so the existing hover-glow CSS path applies. Without this the
  // bank wouldn't visibly indicate "valid drop target".
  useEffect(() => {
    const sphere = sphereRef.current
    if (!sphere) return
    // The sphere already updates its hoveredId on pointermove, so
    // dragOver effectively mirrors the same state when drag is
    // active. No additional plumbing needed at the sphere level —
    // the CSS class on the canvas-mount handles the color shift.
  }, [dragOver])

  const selectedStone = useMemo(
    () =>
      selectedKind === "stone"
        ? (stones.find((s) => s.stone_id === selectedId) ?? null)
        : null,
    [stones, selectedId, selectedKind],
  )

  const selectedBank = useMemo(
    () =>
      selectedKind === "bank"
        ? (banks.find((b) => b.name === selectedId) ?? null)
        : null,
    [banks, selectedId, selectedKind],
  )

  const hoveredStone = useMemo(
    () =>
      hoveredKind === "stone"
        ? (stones.find((s) => s.stone_id === hovered) ?? null)
        : null,
    [stones, hovered, hoveredKind],
  )

  const hoveredBank = useMemo(
    () =>
      hoveredKind === "bank"
        ? (banks.find((b) => b.name === hovered) ?? null)
        : null,
    [banks, hovered, hoveredKind],
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
        <div
          ref={containerRef}
          className={`canvas-mount${dragOver ? " canvas-mount-drop-active" : ""}`}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
        />

        {selectedStone && tracked?.selected && (
          <CanvasStoneCard
            stone={selectedStone}
            tendedName={tended?.stone_name}
            position={tracked.selected.pos}
            services={selectedServices}
            onDismiss={() => {
              sphereRef.current?.resetView()
              setSelectedId(null)
              setSelectedKind(null)
            }}
          />
        )}

        {selectedBank && tracked?.selected && (
          <CanvasBankCard
            bank={selectedBank}
            position={tracked.selected.pos}
            seeds={selectedSeeds}
            onDismiss={() => {
              sphereRef.current?.resetView()
              setSelectedId(null)
              setSelectedKind(null)
            }}
          />
        )}

        {hovered && hovered !== selectedId && tracked?.hovered && (
          <CanvasHoverChip
            label={
              hoveredKind === "bank"
                ? hoveredBank?.name
                : hoveredStone
                  ? displayName(hoveredStone.stone_name)
                  : undefined
            }
            position={tracked.hovered.pos}
            kind={hoveredKind ?? "stone"}
          />
        )}
      </div>

      {lastCapture && (
        <CanvasToast
          message={`Snapshot captured: ${lastCapture.source_fqn} (${formatBytesShort(lastCapture.size_total_bytes)})`}
          onDismiss={() => setLastCapture(null)}
        />
      )}

      {lastPlant && (
        <CanvasToast
          message={`Planted ${lastPlant.target_fqn}${
            lastPlant.digest_drift === "drift" ? " (manifest drift)" : ""
          }`}
          onDismiss={() => setLastPlant(null)}
        />
      )}

      {Object.keys(forming).length > 0 && (
        <div className="canvas-forming-rail">
          {Object.entries(forming).map(([id, info]) => (
            <div
              key={id}
              className="seed seed-forming"
              title={`${info.fqn} → ${info.bankName}`}
            >
              <span className="seed-glyph" aria-hidden>
                ◆
              </span>
              <span className="seed-label">
                {info.fqn} → {info.bankName}
              </span>
            </div>
          ))}
        </div>
      )}

      <footer className="canvas-hint">
        Right-drag to rotate · scroll to zoom · drag an offering to a bank to back it up
      </footer>
    </main>
  )
}

interface CanvasToastProps {
  message: string
  onDismiss: () => void
}

function CanvasToast({ message, onDismiss }: CanvasToastProps): JSX.Element {
  // Auto-dismiss after 5 s. The user can also click to dismiss
  // immediately. The CSS class slides it in from the bottom and
  // fades it out as the timer fires.
  useEffect(() => {
    const t = setTimeout(onDismiss, 5000)
    return () => clearTimeout(t)
  }, [onDismiss])
  return (
    <div className="canvas-toast" role="status" onClick={onDismiss}>
      {message}
    </div>
  )
}

/// Floating detail card for the selected stone. Pinned to its
/// projected screen position so the user can keep both the
/// 3D context and the details visible.
interface CanvasStoneCardProps {
  stone: AwareStone
  tendedName: string | undefined
  position: { x: number; y: number }
  services: ServiceLite[]
  onDismiss: () => void
}

function CanvasStoneCard({
  stone,
  tendedName,
  position,
  services,
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

      {services.length > 0 && (
        <section className="canvas-card-offerings">
          <div className="canvas-card-section-title">
            Drag to a bank to back up
          </div>
          <div className="canvas-card-offering-chips">
            {services.map((svc) => (
              <OfferingChip
                key={svc.name}
                stoneName={stone.stone_name}
                service={svc}
              />
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

interface OfferingChipProps {
  stoneName: string
  service: ServiceLite
}

function OfferingChip({ stoneName, service }: OfferingChipProps): JSX.Element {
  const onDragStart = (e: React.DragEvent) => {
    const payload: DragOfferingPayload = {
      kind: "offering",
      source_stone: stoneName,
      fqn: service.name,
      display_name: service.offering,
    }
    e.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload))
    e.dataTransfer.effectAllowed = "copy"
  }
  return (
    <div
      className={`canvas-offering-chip status-${service.status}`}
      draggable
      onDragStart={onDragStart}
      title={`Drag to bank to snapshot ${service.name}`}
    >
      <span
        className={`canvas-offering-chip-dot status-${service.status}`}
        aria-hidden
      />
      <span className="canvas-offering-chip-label">{service.offering}</span>
    </div>
  )
}

interface CanvasBankCardProps {
  bank: GardenBankSummary
  position: { x: number; y: number }
  seeds: BankSeedEntry[]
  onDismiss: () => void
}

function CanvasBankCard({
  bank,
  position,
  seeds,
  onDismiss,
}: CanvasBankCardProps): JSX.Element {
  return (
    <div
      className="canvas-card canvas-card-bank"
      style={{ left: `${position.x}px`, top: `${position.y}px` }}
    >
      <header className="canvas-card-header">
        <span className="canvas-card-bank-glyph" aria-hidden>
          ◆
        </span>
        <span className="canvas-card-title">{bank.name}</span>
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
          <dt>Replicas</dt>
          <dd>{bank.replica_count}</dd>
        </div>
        <div className="canvas-card-row">
          <dt>Primary</dt>
          <dd className={bank.primary_stone ? "kv-value-mono" : ""}>
            {bank.primary_stone ?? "—"}
          </dd>
        </div>
        <div className="canvas-card-row">
          <dt>Roles</dt>
          <dd>{bank.roles.length === 0 ? "—" : bank.roles.join(" · ")}</dd>
        </div>
      </dl>

      {seeds.length > 0 && (
        <section className="canvas-card-seeds">
          <div className="canvas-card-section-title">
            Drag a seed to a stone to plant
          </div>
          <div className="canvas-card-seed-list">
            {seeds.map((seed) => (
              <SeedChip key={seed.snapshot_id} seed={seed} bankName={bank.name} />
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

interface SeedChipProps {
  seed: BankSeedEntry
  bankName: string
}

function SeedChip({ seed, bankName }: SeedChipProps): JSX.Element {
  const onDragStart = (e: React.DragEvent) => {
    const payload: DragSeedPayload = {
      kind: "seed",
      snapshot_id: seed.snapshot_id,
      source_fqn: seed.source_fqn,
      source_stone: seed.source_stone,
      bank_name: bankName,
    }
    e.dataTransfer.setData(DRAG_MIME, JSON.stringify(payload))
    e.dataTransfer.effectAllowed = "copy"
  }
  return (
    <div
      className="seed seed-draggable"
      draggable
      onDragStart={onDragStart}
      title={`Drag to a stone to plant ${seed.source_fqn} (${formatBytesShort(
        seed.size_total_bytes,
      )})`}
    >
      <span className="seed-glyph" aria-hidden>
        ◆
      </span>
      <span className="seed-label">
        {seed.source_fqn}
        <span className="seed-meta">
          {" · "}
          {formatRelative(seed.created_at)}
        </span>
      </span>
    </div>
  )
}

interface CanvasHoverChipProps {
  label: string | undefined
  position: { x: number; y: number }
  kind: "stone" | "bank"
}

function CanvasHoverChip({
  label,
  position,
  kind,
}: CanvasHoverChipProps): JSX.Element | null {
  if (!label) return null
  return (
    <div
      className={`canvas-hover-chip canvas-hover-chip-${kind}`}
      style={{ left: `${position.x}px`, top: `${position.y}px` }}
    >
      {label}
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

function formatBytesShort(bytes: number): string {
  if (!bytes) return "0B"
  const units = ["B", "K", "M", "G", "T", "P"]
  let i = 0
  let n = bytes
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024
    i += 1
  }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)}${units[i]}`
}

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime()
  const now = Date.now()
  const secs = Math.max(0, Math.floor((now - then) / 1000))
  return formatAgeSecs(secs)
}

function formatAgeSecs(secs: number): string {
  if (secs < 60) return `${secs}s ago`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h ago`
  return `${Math.floor(secs / 86_400)}d ago`
}
