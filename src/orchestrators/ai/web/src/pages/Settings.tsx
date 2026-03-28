/**
 * Settings page — configuration toggles with immediate persistence.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { Panel } from '../components/ui'

export function Settings() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  const { config } = snapshot

  async function updateSetting(patch: Record<string, unknown>) {
    await fetch('/api/settings', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ features: patch }),
    })
  }

  return (
    <Panel title="Orchestrator Settings">
      <div className="space-y-4">
        {/* Auto-pull mode */}
        <div className="flex items-center justify-between py-2 border-b border-white/5">
          <div>
            <div className="text-[13px] text-neutral-200 font-medium">Auto-pull Mode</div>
            <div className="text-[11px] text-neutral-500 mt-0.5">
              {config.auto_pull_mode === 'off' && 'No automatic model management. Unknown model → 404.'}
              {config.auto_pull_mode === 'sync' && 'Replicate models across stones in the same tier.'}
              {config.auto_pull_mode === 'on_demand' && 'Sync + pull unknown models on demand.'}
            </div>
          </div>
          <div className="flex bg-[#1a1a1a] rounded-md border border-white/10 overflow-hidden">
            {(['off', 'sync', 'on_demand'] as const).map(mode => (
              <button
                key={mode}
                onClick={() => updateSetting({ auto_pull_mode: mode })}
                className={`px-3 py-1.5 text-[12px] transition-colors ${
                  config.auto_pull_mode === mode
                    ? 'bg-sage/20 text-sage font-medium'
                    : 'text-neutral-500 hover:text-neutral-300'
                }`}
              >
                {mode === 'on_demand' ? 'On Demand' : mode.charAt(0).toUpperCase() + mode.slice(1)}
              </button>
            ))}
          </div>
        </div>

        {/* Delete idle models */}
        <div className="flex items-center justify-between py-2 border-b border-white/5">
          <div>
            <div className="text-[13px] text-neutral-200 font-medium">Delete Idle Models</div>
            <div className="text-[11px] text-neutral-500 mt-0.5">
              Automatically remove models with zero requests in the measurement window.
            </div>
          </div>
          <Toggle
            checked={config.delete_on_idle}
            onChange={v => updateSetting({ delete_on_idle: v })}
          />
        </div>

        {/* Metrics collection */}
        <div className="flex items-center justify-between py-2">
          <div>
            <div className="text-[13px] text-neutral-200 font-medium">Metrics Collection</div>
            <div className="text-[11px] text-neutral-500 mt-0.5">
              Track request counts, token throughput, and per-stone performance.
            </div>
          </div>
          <Toggle
            checked={config.metrics_enabled}
            onChange={v => updateSetting({ metrics_enabled: v })}
          />
        </div>
      </div>
    </Panel>
  )
}

interface ToggleProps {
  checked: boolean
  onChange: (value: boolean) => void
}

function Toggle({ checked, onChange }: ToggleProps) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`relative w-10 h-5 rounded-full transition-colors ${
        checked ? 'bg-sage' : 'bg-neutral-700'
      }`}
    >
      <span
        className={`absolute top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
          checked ? 'translate-x-5' : 'translate-x-0.5'
        }`}
      />
    </button>
  )
}
