import { useCallback, useEffect, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

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

interface ServicesViewProps {
  onClose: () => void
}

type Pending = "wake" | "rest" | "restart" | "backup"

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

interface CaptureSnapshotResult {
  snapshot_id: string
  source_fqn: string
  size_total_bytes: number
}

const STATUS_DOT_CLASS: Record<string, string> = {
  running: "dot dot-ok",
  stopped: "dot dot-amber",
  degraded: "dot dot-down",
  failed: "dot dot-down",
}

function statusDotClass(status: string): string {
  return STATUS_DOT_CLASS[status.toLowerCase()] ?? "dot"
}

export function ServicesView({ onClose }: ServicesViewProps): JSX.Element {
  const [services, setServices] = useState<ServicesPayload | null>(null)
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [banks, setBanks] = useState<GardenBankSummary[]>([])
  const [error, setError] = useState<string | null>(null)
  const [backupTarget, setBackupTarget] = useState<ServiceLite | null>(null)
  // Per-service pending action so individual buttons can show
  // "working…" without locking the whole table.
  const [pending, setPending] = useState<Record<string, Pending | undefined>>({})

  const refresh = useCallback(async () => {
    try {
      const [s, t, storage] = await Promise.all([
        invoke<ServicesPayload | null>("get_services"),
        invoke<TendedStone | null>("get_tended"),
        invoke<StoragePayload | null>("get_storage"),
      ])
      setServices(s)
      setTended(t)
      setBanks(storage?.banks ?? [])
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    let unlistenTending: UnlistenFn | undefined
    let cancelled = false

    void (async () => {
      await refresh()
      unlistenTending = await listen("tending-changed", () => {
        if (cancelled) return
        void refresh()
      })
    })()

    return () => {
      cancelled = true
      unlistenTending?.()
    }
  }, [refresh])

  const runAction = useCallback(
    async (name: string, action: Pending) => {
      setPending((p) => ({ ...p, [name]: action }))
      try {
        const cmd =
          action === "wake"
            ? "wake_service"
            : action === "rest"
              ? "rest_service"
              : "restart_service"
        await invoke(cmd, { name })
        // Give moss a moment to update its state, then re-fetch
        // so the row reflects post-action status.
        setTimeout(() => {
          void refresh()
        }, 600)
      } catch (e) {
        setError(`${action} ${name}: ${String(e)}`)
      } finally {
        setPending((p) => {
          const next = { ...p }
          delete next[name]
          return next
        })
      }
    },
    [refresh]
  )

  return (
    <main className="content">
      <header className="topbar">
        <button className="garden-pill" onClick={onClose} type="button">
          ← Home
        </button>
        <div className="topbar-spacer" />
      </header>

      <section className="hero">
        <h1>Services</h1>
        <p className="subtle">
          {tended
            ? `running on ${tended.stone_name}`
            : "no stone tended"}
        </p>
      </section>

      {error && (
        <section className="placeholder-note">
          <div className="placeholder-title">Error</div>
          <div className="placeholder-body">{error}</div>
        </section>
      )}

      {backupTarget && tended && (
        <BackupPickerModal
          service={backupTarget}
          stoneName={tended.stone_name}
          banks={banks}
          onCancel={() => setBackupTarget(null)}
          onCapture={async (target) => {
            const svc = backupTarget
            setBackupTarget(null)
            setPending((p) => ({ ...p, [svc.name]: "backup" }))
            try {
              const result = await invoke<CaptureSnapshotResult>(
                "capture_snapshot",
                {
                  stone: tended.stone_name,
                  fqn: svc.name,
                  target,
                },
              )
              // Light-weight inline confirmation — we reuse the
              // error slot's negative-space treatment but with a
              // success message style.
              setError(
                `Snapshot ${result.snapshot_id.slice(0, 8)}… captured (${result.source_fqn})`,
              )
            } catch (e) {
              setError(`Backup failed: ${String(e)}`)
            } finally {
              setPending((p) => {
                const next = { ...p }
                delete next[svc.name]
                return next
              })
            }
          }}
        />
      )}

      {!tended ? (
        <section className="settings-empty">
          Tend a stone from the Home view to see its services.
        </section>
      ) : !services ? (
        <section className="settings-empty">Loading…</section>
      ) : services.count === 0 ? (
        <section className="settings-empty">
          No offerings on {tended.stone_name}. Plant one with{" "}
          <code>garden-rake plant {"<offering>"}</code>.
        </section>
      ) : (
        <section className="services-grid">
          {services.services.map((svc) => {
            const dot = statusDotClass(svc.status)
            const isRunning = svc.status.toLowerCase() === "running"
            const action = pending[svc.name]
            return (
              <article className="service-card" key={svc.name}>
                <header className="service-card-head">
                  <span className={dot} />
                  <span className="service-card-name">{svc.name}</span>
                  <span className="service-card-status">{svc.status}</span>
                </header>
                <div className="service-card-meta">
                  <span className="service-card-offering">
                    {svc.offering || "—"}
                  </span>
                </div>
                <footer className="service-card-actions">
                  <button
                    type="button"
                    disabled={action !== undefined || isRunning}
                    onClick={() => runAction(svc.name, "wake")}
                  >
                    {action === "wake" ? "starting…" : "Wake"}
                  </button>
                  <button
                    type="button"
                    disabled={action !== undefined || !isRunning}
                    onClick={() => runAction(svc.name, "rest")}
                  >
                    {action === "rest" ? "stopping…" : "Rest"}
                  </button>
                  <button
                    type="button"
                    disabled={action !== undefined}
                    onClick={() => runAction(svc.name, "restart")}
                  >
                    {action === "restart" ? "restarting…" : "Restart"}
                  </button>
                  <button
                    type="button"
                    disabled={action !== undefined}
                    onClick={() => setBackupTarget(svc)}
                    title="Capture a snapshot of this offering"
                  >
                    {action === "backup" ? "backing up…" : "Backup…"}
                  </button>
                </footer>
              </article>
            )
          })}
        </section>
      )}
    </main>
  )
}

interface BackupPickerModalProps {
  service: ServiceLite
  stoneName: string
  banks: GardenBankSummary[]
  onCancel: () => void
  onCapture: (target: string) => void
}

/// Keyboard-equivalent of the canvas drag-to-bank gesture.
/// Lists "Local disk" + every available bank as a target;
/// keyboard users can ↑/↓ + Enter to pick. The drag-canvas
/// remains the eye-and-hand path for the same operation.
function BackupPickerModal({
  service,
  stoneName,
  banks,
  onCancel,
  onCapture,
}: BackupPickerModalProps): JSX.Element {
  const targets = [
    { value: "local", label: "Local disk", note: "<data_dir>/snapshots/" },
    ...banks.map((b) => ({
      value: `bank:${b.name}`,
      label: b.name,
      note: `${b.replica_count} replica${b.replica_count === 1 ? "" : "s"}`,
    })),
  ]
  const [focused, setFocused] = useState(0)

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault()
        onCancel()
        return
      }
      if (e.key === "ArrowDown") {
        e.preventDefault()
        setFocused((f) => Math.min(f + 1, targets.length - 1))
        return
      }
      if (e.key === "ArrowUp") {
        e.preventDefault()
        setFocused((f) => Math.max(f - 1, 0))
        return
      }
      if (e.key === "Enter") {
        e.preventDefault()
        onCapture(targets[focused].value)
        return
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [focused, targets, onCancel, onCapture])

  return (
    <div className="modal-scrim" role="dialog" aria-modal="true">
      <div className="modal-card backup-picker">
        <header className="modal-header">
          <h2>Back up {service.offering || service.name}</h2>
          <button
            type="button"
            className="modal-close"
            onClick={onCancel}
            aria-label="Close"
          >
            ×
          </button>
        </header>
        <p className="modal-sub">
          Capture a snapshot from <code>{stoneName}</code> and place it…
        </p>
        <ul className="backup-target-list">
          {targets.map((t, idx) => (
            <li
              key={t.value}
              className={`backup-target${idx === focused ? " focused" : ""}`}
              onMouseEnter={() => setFocused(idx)}
              onClick={() => onCapture(t.value)}
            >
              <span className="backup-target-label">{t.label}</span>
              <span className="backup-target-note">{t.note}</span>
            </li>
          ))}
        </ul>
        <footer className="modal-footer">
          <span className="modal-hint">↑↓ navigate · Enter pick · Esc cancel</span>
        </footer>
      </div>
    </div>
  )
}
