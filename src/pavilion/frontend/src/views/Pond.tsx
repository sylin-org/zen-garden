import { useCallback, useEffect, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { CeremonyModal, type CeremonyKind } from "./CeremonyModal"

interface PondPayload {
  initialised: boolean
  status: string
  name: string | null
  member_count: number | null
  cornerstone: string | null
}

interface TendedStone {
  stone_name: string
  endpoint: string
}

interface PondViewProps {
  onClose: () => void
}

const STATUS_DOT_CLASS: Record<string, string> = {
  active: "dot dot-ok",
  healthy: "dot dot-ok",
  locked: "dot dot-amber",
  inactive: "dot dot-amber",
  uninitialised: "dot",
  unknown: "dot",
}

function statusDotClass(status: string): string {
  return STATUS_DOT_CLASS[status.toLowerCase()] ?? "dot"
}

export function PondView({ onClose }: PondViewProps): JSX.Element {
  const [pond, setPond] = useState<PondPayload | null>(null)
  const [tended, setTended] = useState<TendedStone | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [ceremony, setCeremony] = useState<CeremonyKind | null>(null)

  const refresh = useCallback(async () => {
    try {
      const [p, t] = await Promise.all([
        invoke<PondPayload | null>("get_pond_status"),
        invoke<TendedStone | null>("get_tended"),
      ])
      setPond(p)
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

  return (
    <main className="content">
      <header className="topbar">
        <button className="garden-pill" onClick={onClose} type="button">
          ← Home
        </button>
        <div className="topbar-spacer" />
      </header>

      <section className="hero">
        <h1>Pond</h1>
        <p className="subtle">
          {tended
            ? `security ceremonies bound to ${tended.stone_name}`
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
          Tend a stone from the Home view to see its pond.
        </section>
      ) : !pond ? (
        <section className="settings-empty">Loading…</section>
      ) : !pond.initialised ? (
        <>
          <section className="placeholder-note">
            <div className="placeholder-title">No pond on this stone</div>
            <div className="placeholder-body">
              Place a keystone to initialise the pond — Pavilion will
              walk you through the ceremony.
            </div>
          </section>

          <section className="pond-actions">
            <button
              type="button"
              className="pond-action-primary"
              onClick={() => setCeremony("init")}
            >
              Place keystone
            </button>
            <button
              type="button"
              className="pond-action-secondary"
              onClick={() => setCeremony("join")}
            >
              Join an existing pond
            </button>
          </section>
        </>
      ) : (
        <>
          <section className="settings-group">
            <div className="settings-group-title">Status</div>
            <div className="pond-status-row">
              <span className={statusDotClass(pond.status)} />
              <span className="pond-status-label">{pond.status}</span>
            </div>
          </section>

          <section className="settings-group">
            <div className="settings-group-title">Identity</div>
            <KeyValue
              k="Pond name"
              v={pond.name ?? "—"}
              mono={pond.name !== null}
            />
            <KeyValue
              k="Cornerstone"
              v={pond.cornerstone ?? "—"}
              mono={pond.cornerstone !== null}
            />
            <KeyValue
              k="Members"
              v={
                pond.member_count !== null
                  ? `${pond.member_count}`
                  : "unknown"
              }
            />
          </section>

          <section className="pond-actions">
            <button
              type="button"
              className="pond-action-primary"
              onClick={() => setCeremony("invite")}
            >
              Open enrollment
            </button>
            <button
              type="button"
              className="pond-action-secondary"
              onClick={() => setCeremony("unlock")}
              disabled={pond.status.toLowerCase() === "active"}
              title={
                pond.status.toLowerCase() === "active"
                  ? "Already unlocked"
                  : "Unlock the pond after a stone restart"
              }
            >
              Unlock pond
            </button>
          </section>
        </>
      )}

      {ceremony && (
        <CeremonyModal
          kind={ceremony}
          onClose={() => {
            setCeremony(null)
            void refresh()
          }}
        />
      )}
    </main>
  )
}

function KeyValue({
  k,
  v,
  mono = false,
}: {
  k: string
  v: string
  mono?: boolean
}): JSX.Element {
  return (
    <div className="settings-row settings-row-pill">
      <span className="settings-row-label">{k}</span>
      <span className={mono ? "kv-value-mono" : "kv-value"}>{v}</span>
    </div>
  )
}
