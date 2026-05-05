import { useCallback, useEffect, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

interface TendAction {
  kind: "tend"
  stone_id: string
  stone_name: string
}

interface OpenViewAction {
  kind: "open_view"
  view: string
}

type SuggestionAction = TendAction | OpenViewAction

interface Suggestion {
  id: string
  kind: string
  title: string
  body: string
  action_label: string
  action: SuggestionAction
}

interface FacilitatorBannerProps {
  onNavigate: (view: string) => void
}

/**
 * Inline suggestion banner. Renders at most one active suggestion;
 * the FacilitatorEngine on the Rust side picks which to surface.
 *
 * Three controls per the interaction-design spec §5: primary
 * action, "Not now" (session-local dismissal), and "Hide this
 * kind" (persistent through Settings.suppressed_kinds).
 */
export function FacilitatorBanner({
  onNavigate,
}: FacilitatorBannerProps): JSX.Element | null {
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null)
  const [busy, setBusy] = useState<boolean>(false)

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<Suggestion | null>("get_suggestion")
      setSuggestion(s)
    } catch (e) {
      console.error("get_suggestion failed:", e)
    }
  }, [])

  useEffect(() => {
    let unlisten: UnlistenFn | undefined
    let cancelled = false
    void (async () => {
      await refresh()
      unlisten = await listen<Suggestion | null>("suggestion-changed", (e) => {
        if (cancelled) return
        setSuggestion(e.payload)
      })
    })()
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [refresh])

  const runAction = useCallback(async () => {
    if (!suggestion) return
    setBusy(true)
    try {
      switch (suggestion.action.kind) {
        case "tend":
          await invoke("set_tended", {
            stoneId: suggestion.action.stone_id,
          })
          break
        case "open_view":
          onNavigate(suggestion.action.view)
          break
      }
    } catch (e) {
      console.error("facilitator action failed:", e)
    } finally {
      setBusy(false)
    }
  }, [onNavigate, suggestion])

  const dismissForNow = useCallback(async () => {
    if (!suggestion) return
    setBusy(true)
    try {
      await invoke("dismiss_suggestion", { id: suggestion.id })
    } catch (e) {
      console.error("dismiss_suggestion failed:", e)
    } finally {
      setBusy(false)
    }
  }, [suggestion])

  const hideKind = useCallback(async () => {
    if (!suggestion) return
    setBusy(true)
    try {
      await invoke("hide_suggestion_kind", { kind: suggestion.kind })
    } catch (e) {
      console.error("hide_suggestion_kind failed:", e)
    } finally {
      setBusy(false)
    }
  }, [suggestion])

  if (!suggestion) return null

  return (
    <section className="facilitator">
      <div className="facilitator-glyph">💡</div>
      <div className="facilitator-body">
        <div className="facilitator-title">{suggestion.title}</div>
        <div className="facilitator-text">{suggestion.body}</div>
      </div>
      <div className="facilitator-actions">
        <button
          type="button"
          className="facilitator-primary"
          onClick={runAction}
          disabled={busy}
        >
          {suggestion.action_label}
        </button>
        <button
          type="button"
          className="facilitator-secondary"
          onClick={dismissForNow}
          disabled={busy}
        >
          Not now
        </button>
        <button
          type="button"
          className="facilitator-tertiary"
          onClick={hideKind}
          disabled={busy}
          title="Don't suggest this kind again"
        >
          Hide this kind
        </button>
      </div>
    </section>
  )
}
