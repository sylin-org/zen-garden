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

type Pending = "wake" | "rest" | "restart"

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
  const [error, setError] = useState<string | null>(null)
  // Per-service pending action so individual buttons can show
  // "working…" without locking the whole table.
  const [pending, setPending] = useState<Record<string, Pending | undefined>>({})

  const refresh = useCallback(async () => {
    try {
      const [s, t] = await Promise.all([
        invoke<ServicesPayload | null>("get_services"),
        invoke<TendedStone | null>("get_tended"),
      ])
      setServices(s)
      setTended(t)
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
                </footer>
              </article>
            )
          })}
        </section>
      )}
    </main>
  )
}
