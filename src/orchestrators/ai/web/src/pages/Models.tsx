/**
 * Models page — sortable table with all known models, per-instance
 * availability, and management actions (pull, delete).
 */

import { useState } from 'react'
import { useSnapshot } from '../hooks/useOrchestrator'
import { Panel, Empty } from '../components/ui'
import { formatContext } from '../lib/meta'

type SortField = 'name' | 'size_disk' | 'context_length' | 'requests'
type SortDir = 'asc' | 'desc'

export function Models() {
  const snapshot = useSnapshot()
  const [sortField, setSortField] = useState<SortField>('name')
  const [sortDir, setSortDir] = useState<SortDir>('asc')
  const [pullModel, setPullModel] = useState('')
  const [actionStatus, setActionStatus] = useState('')

  if (!snapshot) return null

  const { models, instances, metrics } = snapshot

  // Enrich models with instance availability
  const enriched = models.map(m => {
    const availableOn = instances.filter(i =>
      i.models_available.includes(m.name)
    )
    const loadedOn = instances.filter(i =>
      i.models_loaded.some(l => l.name === m.name)
    )
    const requests = metrics.per_model[m.name] ?? 0
    return { ...m, availableOn, loadedOn, requests }
  })

  // Sort
  const sorted = [...enriched].sort((a, b) => {
    let cmp = 0
    switch (sortField) {
      case 'name': cmp = a.name.localeCompare(b.name); break
      case 'size_disk': cmp = a.size_disk - b.size_disk; break
      case 'context_length': cmp = (a.context_length ?? 0) - (b.context_length ?? 0); break
      case 'requests': cmp = a.requests - b.requests; break
    }
    return sortDir === 'asc' ? cmp : -cmp
  })

  function toggleSort(field: SortField) {
    if (sortField === field) {
      setSortDir(d => d === 'asc' ? 'desc' : 'asc')
    } else {
      setSortField(field)
      setSortDir('desc')
    }
  }

  async function handlePull() {
    if (!pullModel.trim()) return
    setActionStatus(`Pulling ${pullModel}...`)
    try {
      const resp = await fetch('/api/management/pull', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name: pullModel }),
      })
      if (resp.ok) {
        setActionStatus(`Pull started for ${pullModel}`)
        setPullModel('')
      } else {
        setActionStatus(`Pull failed: ${resp.statusText}`)
      }
    } catch (e: unknown) {
      setActionStatus(`Pull error: ${e instanceof Error ? e.message : 'unknown'}`)
    }
  }

  async function handleDelete(model: string) {
    if (!confirm(`Delete ${model} from all instances?`)) return
    setActionStatus(`Deleting ${model}...`)
    try {
      const resp = await fetch('/api/management/delete', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name: model }),
      })
      setActionStatus(resp.ok ? `Deleted ${model}` : `Delete failed: ${resp.statusText}`)
    } catch (e: unknown) {
      setActionStatus(`Delete error: ${e instanceof Error ? e.message : 'unknown'}`)
    }
  }

  const SortHeader = ({ field, label }: { field: SortField; label: string }) => (
    <th
      className="cursor-pointer select-none hover:text-neutral-200 transition-colors"
      onClick={() => toggleSort(field)}
    >
      {label} {sortField === field ? (sortDir === 'asc' ? '↑' : '↓') : ''}
    </th>
  )

  return (
    <div className="space-y-4">
      {/* Management bar */}
      <Panel title="Model Management">
        <div className="flex items-center gap-3">
          <input
            type="text"
            value={pullModel}
            onChange={e => setPullModel(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handlePull()}
            placeholder="Model name (e.g. llama3.2:3b)"
            className="flex-1 bg-[#1a1a1a] border border-white/10 rounded-md px-3 py-1.5 text-sm text-neutral-200 placeholder-neutral-600 focus:outline-none focus:border-sage"
          />
          <button
            onClick={handlePull}
            className="px-4 py-1.5 bg-sage/20 text-sage text-sm font-medium rounded-md hover:bg-sage/30 transition-colors"
          >
            Pull
          </button>
          <button
            onClick={async () => {
              if (confirm('Reset all model request counters?')) {
                await fetch('/api/metrics/model-counters/reset', { method: 'POST' })
                setActionStatus('Model counters reset')
              }
            }}
            className="px-4 py-1.5 bg-white/5 text-neutral-400 text-sm rounded-md hover:bg-white/10 transition-colors"
          >
            Reset Counters
          </button>
        </div>
        {actionStatus && (
          <p className="text-[11px] text-neutral-400 mt-2">{actionStatus}</p>
        )}
      </Panel>

      {/* Model table */}
      <Panel title={`Models (${models.length})`}>
        {sorted.length === 0 ? (
          <Empty message="No models known yet" />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="text-left text-neutral-500 border-b border-white/5">
                  <SortHeader field="name" label="Model" />
                  <th>Spec</th>
                  <SortHeader field="context_length" label="Context" />
                  <th>Capabilities</th>
                  <th>Available On</th>
                  <SortHeader field="requests" label="Requests" />
                  <th></th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/5">
                {sorted.map(m => (
                  <tr key={m.name} className="hover:bg-white/[0.02] transition-colors">
                    <td className="py-2 pr-3">
                      <span className="font-mono text-neutral-200">{m.name}</span>
                    </td>
                    <td className="pr-3 text-neutral-400">
                      {m.parameter_size ?? '—'} {m.quantization_level ? `/ ${m.quantization_level}` : ''}
                    </td>
                    <td className="pr-3 text-neutral-400">{formatContext(m.context_length)}</td>
                    <td className="pr-3">
                      <div className="flex flex-wrap gap-0.5">
                        {m.capabilities.map(c => (
                          <span key={c} className="text-[9px] px-1 py-0 rounded bg-white/5 text-neutral-500">{c}</span>
                        ))}
                      </div>
                    </td>
                    <td className="pr-3">
                      <div className="flex gap-1">
                        {m.availableOn.map(inst => (
                          <span
                            key={inst.endpoint}
                            className="w-2 h-2 rounded-full"
                            title={`${inst.stone.name} (${inst.kind})`}
                            style={{
                              backgroundColor: m.loadedOn.includes(inst) ? '#22c55e' : '#666',
                              boxShadow: m.loadedOn.includes(inst) ? '0 0 4px #22c55e' : 'none',
                            }}
                          />
                        ))}
                      </div>
                    </td>
                    <td className="pr-3 font-mono text-neutral-400">{m.requests}</td>
                    <td>
                      <button
                        onClick={() => handleDelete(m.name)}
                        className="text-neutral-600 hover:text-blocked transition-colors"
                        title="Delete model"
                      >
                        ✕
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </div>
  )
}
