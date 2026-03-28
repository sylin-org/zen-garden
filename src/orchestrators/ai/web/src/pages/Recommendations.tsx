/**
 * Recommendations page — per-capability panels showing the recommended
 * model, score rationale, and pin/unpin controls.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { Panel, Empty } from '../components/ui'
import { CAP_META } from '../lib/meta'
import type { Capability } from '../types/api'

const ROLES: Array<{ key: string; label: string }> = [
  { key: 'quick', label: 'Quick' },
  { key: 'chat', label: 'Chat' },
  { key: 'synthesis', label: 'Synthesis' },
  { key: 'vision', label: 'Vision' },
  { key: 'ocr', label: 'OCR' },
  { key: 'tools', label: 'Tool Use' },
  { key: 'thinking', label: 'Reasoning' },
  { key: 'embedding', label: 'Embedding' },
  { key: 'imagine', label: 'Imagine' },
  { key: 'transcribe', label: 'Transcribe' },
  { key: 'speak', label: 'Speak' },
  { key: 'rerank', label: 'Rerank' },
  { key: 'translate', label: 'Translate' },
]

export function Recommendations() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  const { recommended_models: recs, config } = snapshot
  const pins = config.pins

  // Only show roles that have a recommendation
  const activeRoles = ROLES.filter(r => recs[r.key])

  async function handlePin(capability: string, model: string) {
    await fetch(`/v1/recommendations/${capability}/pin`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ model }),
    })
  }

  async function handleUnpin(capability: string) {
    await fetch(`/v1/recommendations/${capability}/pin`, { method: 'DELETE' })
  }

  return (
    <Panel title="Model Recommendations">
      {activeRoles.length === 0 ? (
        <Empty message="No recommendations computed yet — run a benchmark or wait for instances to be discovered" />
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
          {activeRoles.map(role => {
            const model = recs[role.key]
            const isPinned = pins[role.key] != null
            const cap = role.key as Capability
            const meta = CAP_META[cap]

            return (
              <div
                key={role.key}
                className="bg-[#1a1a1a] border border-white/5 rounded-lg p-3 hover:border-white/10 transition-colors"
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-base">{meta?.icon}</span>
                    <span className="text-[13px] font-semibold text-neutral-200">{role.label}</span>
                  </div>
                  {isPinned ? (
                    <button
                      onClick={() => handleUnpin(role.key)}
                      className="text-[10px] text-gold hover:text-gold/80 transition-colors"
                      title="Unpin (return to automatic selection)"
                    >
                      📌 pinned
                    </button>
                  ) : (
                    <button
                      onClick={() => model && handlePin(role.key, model)}
                      className="text-[10px] text-neutral-600 hover:text-neutral-400 transition-colors"
                      title="Pin this model"
                    >
                      pin
                    </button>
                  )}
                </div>

                <div className="font-mono text-sm text-sage truncate" title={model}>
                  {model ?? '—'}
                </div>

                {isPinned && (
                  <p className="text-[10px] text-gold/60 mt-1">
                    Manually pinned — overrides automatic scoring
                  </p>
                )}
              </div>
            )
          })}
        </div>
      )}
    </Panel>
  )
}
