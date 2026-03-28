/**
 * Metrics page — aggregate counters, per-stone breakdown, top models.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { StatCard, Panel, Empty } from '../components/ui'
import { formatNum } from '../lib/meta'

export function Metrics() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  const { metrics: m } = snapshot
  const stoneEntries = Object.entries(m.per_stone).sort((a, b) => b[1].requests - a[1].requests)
  const topModels = Object.entries(m.per_model)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10)

  async function resetAll() {
    if (!confirm('Reset all metrics counters? This cannot be undone.')) return
    await fetch('/api/metrics/reset', { method: 'POST' })
  }

  return (
    <div className="space-y-4">
      {/* Aggregate counters */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-3">
        <StatCard value={m.requests_total} label="Requests" />
        <StatCard value={m.tokens_in_total} label="Tokens In" />
        <StatCard value={m.tokens_out_total} label="Tokens Out" />
        <StatCard value={m.errors_total} label="Errors" color={m.errors_total > 0 ? '#ef4444' : undefined} />
        <StatCard
          value={m.started_at ? new Date(m.started_at).toLocaleDateString() : '—'}
          label="Since"
        />
      </div>

      {/* Per-stone breakdown */}
      <Panel
        title="Per-Stone Breakdown"
        action={
          <button
            onClick={resetAll}
            className="text-[11px] text-neutral-500 hover:text-blocked transition-colors"
          >
            Reset All
          </button>
        }
      >
        {stoneEntries.length === 0 ? (
          <Empty message="No per-stone metrics yet" />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="text-left text-neutral-500 border-b border-white/5">
                  <th className="py-2 pr-4">Stone</th>
                  <th className="pr-4 text-right">Requests</th>
                  <th className="pr-4 text-right">Tokens Out</th>
                  <th className="pr-4 text-right">Errors</th>
                  <th className="text-right">Avg (ms)</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/5">
                {stoneEntries.map(([stone, data]) => {
                  const avgMs = data.requests > 0
                    ? Math.round(data.total_duration_ns / data.requests / 1_000_000)
                    : 0
                  return (
                    <tr key={stone} className="hover:bg-white/[0.02]">
                      <td className="py-2 pr-4 text-neutral-200">{stone}</td>
                      <td className="pr-4 text-right font-mono text-neutral-300">{formatNum(data.requests)}</td>
                      <td className="pr-4 text-right font-mono text-neutral-400">{formatNum(data.tokens_out)}</td>
                      <td className="pr-4 text-right font-mono" style={{ color: data.errors > 0 ? '#ef4444' : '#666' }}>
                        {data.errors}
                      </td>
                      <td className="text-right font-mono text-neutral-400">{avgMs > 0 ? `${avgMs}` : '—'}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      {/* Top models */}
      <Panel title="Top Models by Request Count">
        {topModels.length === 0 ? (
          <Empty message="No model request data yet" />
        ) : (
          <div className="space-y-1.5">
            {topModels.map(([model, count], i) => (
              <div key={model} className="flex items-center gap-3">
                <span className="text-[10px] text-neutral-600 w-5 text-right">#{i + 1}</span>
                <span className="text-[12px] font-mono text-neutral-300 flex-1 truncate">{model}</span>
                <span className="text-[12px] font-mono text-neutral-500">{formatNum(count)}</span>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  )
}
