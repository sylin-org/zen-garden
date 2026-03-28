/**
 * Placement page — demand distribution + model-to-stone assignments.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { Panel, Empty } from '../components/ui'

export function Placement() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  const { placement, demand_shares: demand } = snapshot
  const hasDemand = Object.keys(demand).length > 0
  const hasAssignments = Object.keys(placement.assignments).length > 0

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
      {/* Demand distribution */}
      <Panel title="Demand Distribution (5 min)">
        {!hasDemand ? (
          <Empty message="No demand data yet — waiting for inference requests" />
        ) : (
          <div className="space-y-2">
            {Object.entries(demand)
              .sort((a, b) => b[1] - a[1])
              .map(([model, share]) => (
                <div key={model} className="flex items-center gap-3">
                  <span className="text-[11px] font-mono text-neutral-300 w-40 truncate" title={model}>
                    {model}
                  </span>
                  <div className="flex-1 h-2 bg-white/5 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-sage rounded-full transition-all duration-500"
                      style={{ width: `${Math.round(share * 100)}%` }}
                    />
                  </div>
                  <span className="text-[11px] text-neutral-500 w-10 text-right">
                    {(share * 100).toFixed(0)}%
                  </span>
                </div>
              ))}
          </div>
        )}
      </Panel>

      {/* Assignments */}
      <Panel title="Model → Stone Assignments">
        {!hasAssignments ? (
          <Empty message="No placement plan computed yet" />
        ) : (
          <div className="space-y-3">
            {Object.entries(placement.assignments).map(([model, endpoints]) => (
              <div key={model} className="bg-[#1a1a1a] rounded-md p-2.5 border border-white/5">
                <div className="text-[12px] font-mono text-neutral-200 mb-1">{model}</div>
                <div className="flex flex-wrap gap-1">
                  {endpoints.map(ep => (
                    <span key={ep} className="text-[10px] px-2 py-0.5 bg-sage/10 text-sage rounded-full">
                      {ep}
                    </span>
                  ))}
                </div>
              </div>
            ))}
            <div className="flex items-center gap-2 text-[10px] text-neutral-600">
              <span className={`w-2 h-2 rounded-full ${placement.stable ? 'bg-fast' : 'bg-degraded animate-pulse'}`} />
              {placement.stable ? 'Plan is stable' : 'Plan is converging...'}
              {placement.computed_at && (
                <span className="ml-auto">Last: {new Date(placement.computed_at).toLocaleTimeString()}</span>
              )}
            </div>
          </div>
        )}
      </Panel>
    </div>
  )
}
