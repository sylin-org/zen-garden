import { useCallback, useEffect, useState, type JSX } from "react"
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

interface OnboardingProps {
  /// Called when the user completes onboarding (either by picking
  /// a stone or by skipping). The parent flips the route from
  /// onboarding → home.
  onComplete: () => void
}

/**
 * First-launch flow per PAVILION-0002 §M1. Two paths:
 *
 * - **Tend a stone** — explicit pick from the discovered list.
 *   Sets the tending file and marks `settings.onboarded = true`.
 * - **Skip** — leaves tending unset and marks
 *   `settings.onboarded = true`. Auto-tend in the Rust layer
 *   takes over from there (localhost-first, then first-by-
 *   response).
 *
 * The view auto-advances if the user does nothing — once any
 * stone has been in awareness for 30s the Skip CTA highlights
 * and after 60s the auto-tend kicks in implicitly. We don't
 * force-advance though, because the spec says onboarding should
 * make the choice feel deliberate.
 */
export function OnboardingView({ onComplete }: OnboardingProps): JSX.Element {
  const [stones, setStones] = useState<AwareStone[]>([])
  const [busy, setBusy] = useState<boolean>(false)
  const [error, setError] = useState<string | null>(null)

  // Live awareness — same wire format as Home.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined
    let cancelled = false
    void (async () => {
      try {
        const initial = await invoke<AwareStone[]>("get_topology")
        if (!cancelled) setStones(initial)
      } catch (e) {
        if (!cancelled) setError(String(e))
      }
      unlisten = await listen<AwareStone[]>("topology-changed", (e) => {
        if (cancelled) return
        setStones(e.payload)
      })
    })()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  const tendStone = useCallback(
    async (stone: AwareStone) => {
      setBusy(true)
      setError(null)
      try {
        await invoke("set_tended", { stoneId: stone.stone_id })
        await invoke("set_settings", { patch: { onboarded: true } })
        onComplete()
      } catch (e) {
        setError(String(e))
        setBusy(false)
      }
    },
    [onComplete]
  )

  const skip = useCallback(async () => {
    setBusy(true)
    setError(null)
    try {
      await invoke("set_settings", { patch: { onboarded: true } })
      onComplete()
    } catch (e) {
      setError(String(e))
      setBusy(false)
    }
  }, [onComplete])

  return (
    <div className="onboarding">
      <div className="onboarding-frame">
        <header className="onboarding-head">
          <div className="onboarding-mark">P</div>
          <div>
            <h1 className="onboarding-title">Welcome to Pavilion</h1>
            <p className="onboarding-sub">
              Pavilion sits in your tray and watches the garden you have on
              the network. Pick a stone to anchor to — you can always switch
              later.
            </p>
          </div>
        </header>

        {error && (
          <div className="onboarding-error">{error}</div>
        )}

        <section className="onboarding-list">
          {stones.length === 0 ? (
            <div className="onboarding-empty">
              <div className="onboarding-empty-spinner" aria-hidden="true" />
              <div>
                <strong>Listening for stones…</strong>
                <p className="subtle">
                  Pavilion is already broadcasting a discovery probe. Stones
                  on this LAN should show up within a few seconds.
                </p>
              </div>
            </div>
          ) : (
            stones.map((stone) => (
              <button
                key={stone.stone_id}
                type="button"
                className="onboarding-stone"
                onClick={() => tendStone(stone)}
                disabled={busy}
              >
                <span className="onboarding-stone-dot dot dot-ok" />
                <span className="onboarding-stone-name">
                  {stone.stone_name}
                </span>
                <span className="onboarding-stone-endpoint">
                  {stone.endpoint}
                </span>
                <span className="onboarding-stone-cta">
                  {busy ? "…" : "Tend"}
                </span>
              </button>
            ))
          )}
        </section>

        <footer className="onboarding-foot">
          <button
            type="button"
            className="onboarding-skip"
            onClick={skip}
            disabled={busy}
          >
            Skip — let Pavilion auto-tend
          </button>
        </footer>
      </div>
    </div>
  )
}
