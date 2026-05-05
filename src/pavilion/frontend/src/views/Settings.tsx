import { useCallback, useEffect, useMemo, useState, type JSX } from "react"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"

interface QuietHours {
  enabled: boolean
  start: string
  end: string
}

interface Settings {
  quiet_hours: QuietHours
  suppressed_kinds: string[]
  autostart_enabled: boolean
}

interface QuietHoursPatch {
  enabled?: boolean
  start?: string
  end?: string
}

interface SettingsPatch {
  quiet_hours?: QuietHoursPatch
  suppressed_kinds?: string[]
  autostart_enabled?: boolean
}

interface SettingsViewProps {
  onClose: () => void
}

const SUPPRESSION_LABELS: Record<string, string> = {
  stone_joined: "Stone joined the garden",
  stone_left: "Stone offline",
  storage_activity: "Storage sync activity",
}

function describeSuppression(kind: string): string {
  return SUPPRESSION_LABELS[kind] ?? kind
}

function isHHMM(s: string): boolean {
  return /^[0-2]\d:[0-5]\d$/.test(s)
}

export function SettingsView({ onClose }: SettingsViewProps): JSX.Element {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState<boolean>(false)

  const refresh = useCallback(async () => {
    try {
      const next = await invoke<Settings>("get_settings")
      setSettings(next)
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    let unlisten: UnlistenFn | undefined
    let cancelled = false

    void (async () => {
      await refresh()
      unlisten = await listen<Settings>("settings-changed", (event) => {
        if (cancelled) return
        setSettings(event.payload)
      })
    })()

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [refresh])

  const apply = useCallback(async (patch: SettingsPatch) => {
    setSaving(true)
    try {
      const next = await invoke<Settings>("set_settings", { patch })
      setSettings(next)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setSaving(false)
    }
  }, [])

  const startValid = useMemo(
    () => (settings ? isHHMM(settings.quiet_hours.start) : true),
    [settings]
  )
  const endValid = useMemo(
    () => (settings ? isHHMM(settings.quiet_hours.end) : true),
    [settings]
  )

  if (!settings) {
    return (
      <main className="content">
        <header className="topbar">
          <button className="garden-pill" onClick={onClose} type="button">
            ← Home
          </button>
          <div className="topbar-spacer" />
        </header>
        <section className="hero">
          <h1>Settings</h1>
          <p className="subtle">{error ?? "Loading…"}</p>
        </section>
      </main>
    )
  }

  return (
    <main className="content">
      <header className="topbar">
        <button className="garden-pill" onClick={onClose} type="button">
          ← Home
        </button>
        <div className="topbar-spacer" />
        <div className="topbar-clock">
          {saving ? "saving…" : "saved"}
        </div>
      </header>

      <section className="hero">
        <h1>Settings</h1>
        <p className="subtle">
          calm by default · quiet hours and per-source suppression
        </p>
      </section>

      {error && (
        <section className="placeholder-note">
          <div className="placeholder-title">Error</div>
          <div className="placeholder-body">{error}</div>
        </section>
      )}

      <section className="settings-group">
        <div className="settings-group-title">Notifications</div>

        <label className="settings-row">
          <input
            type="checkbox"
            checked={settings.quiet_hours.enabled}
            onChange={(e) =>
              apply({ quiet_hours: { enabled: e.target.checked } })
            }
            disabled={saving}
          />
          <span className="settings-row-label">Quiet hours</span>
          <span className="settings-row-help">
            Suppress toasts during the configured window. Activity is
            still logged.
          </span>
        </label>

        <div className="settings-row settings-row-time">
          <span className="settings-row-label">Window</span>
          <input
            type="time"
            value={settings.quiet_hours.start}
            onChange={(e) =>
              apply({ quiet_hours: { start: e.target.value } })
            }
            disabled={saving || !settings.quiet_hours.enabled}
            className={startValid ? "" : "settings-input-invalid"}
            aria-invalid={!startValid}
          />
          <span className="settings-row-sep">to</span>
          <input
            type="time"
            value={settings.quiet_hours.end}
            onChange={(e) =>
              apply({ quiet_hours: { end: e.target.value } })
            }
            disabled={saving || !settings.quiet_hours.enabled}
            className={endValid ? "" : "settings-input-invalid"}
            aria-invalid={!endValid}
          />
          <span className="settings-row-help">
            Wraps over midnight when end is earlier than start.
          </span>
        </div>
      </section>

      <section className="settings-group">
        <div className="settings-group-title">Hidden notification kinds</div>
        {settings.suppressed_kinds.length === 0 ? (
          <div className="settings-empty">
            None hidden. Future "Hide this kind" actions will surface
            here so you can re-enable them.
          </div>
        ) : (
          settings.suppressed_kinds.map((kind) => (
            <div className="settings-row settings-row-pill" key={kind}>
              <span className="settings-row-label">
                {describeSuppression(kind)}
              </span>
              <button
                type="button"
                className="settings-row-action"
                disabled={saving}
                onClick={() =>
                  apply({
                    suppressed_kinds: settings.suppressed_kinds.filter(
                      (k) => k !== kind
                    ),
                  })
                }
              >
                Show again
              </button>
            </div>
          ))
        )}
      </section>

      <section className="settings-group">
        <div className="settings-group-title">Startup</div>
        <label className="settings-row">
          <input
            type="checkbox"
            checked={settings.autostart_enabled}
            onChange={(e) =>
              apply({ autostart_enabled: e.target.checked })
            }
            disabled={saving}
          />
          <span className="settings-row-label">
            Start Pavilion when I sign in
          </span>
          <span className="settings-row-help">
            Adds Pavilion to your user-level autostart entries. The
            OS state is reconciled on every change, including this
            one.
          </span>
        </label>
      </section>
    </main>
  )
}
