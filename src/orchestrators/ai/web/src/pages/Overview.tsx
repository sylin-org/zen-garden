/**
 * Overview page — capability-centric primary view.
 * Shows active capabilities, key metrics, and offering distribution.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { StatCard, OfferingBadge, Panel, Empty } from '../components/ui'
import { CAP_META, formatDuration } from '../lib/meta'
import type { Capability, ServiceInstance } from '../types/api'

export function Overview() {
  const snapshot = useSnapshot()
  if (!snapshot) return <Loading />

  const { orchestrator: o, instances, recommended_models: recs, metrics } = snapshot

  // Aggregate capabilities from healthy instances
  const capMap = new Map<Capability, { instances: ServiceInstance[]; offerings: Set<string> }>()
  for (const inst of instances) {
    if (inst.health.status !== 'healthy') continue
    for (const cap of inst.capabilities) {
      const entry = capMap.get(cap) ?? { instances: [], offerings: new Set() }
      entry.instances.push(inst)
      entry.offerings.add(inst.kind)
      capMap.set(cap, entry)
    }
  }

  const sortedCaps = [...capMap.entries()].sort(
    (a, b) => (CAP_META[a[0]]?.rank ?? 99) - (CAP_META[b[0]]?.rank ?? 99)
  )

  return (
    <div className="space-y-6">
      {/* Key metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <StatCard value={o.offerings_registered} label="Offerings Registered" />
        <StatCard value={o.instances_discovered} label="Instances Discovered" />
        <StatCard value={o.models_known} label="Models Known" />
        <StatCard value={formatDuration(o.uptime_secs)} label="Uptime" />
      </div>

      {/* Request metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <StatCard value={metrics.requests_total} label="Total Requests" color="#7ea8be" />
        <StatCard value={metrics.tokens_out_total} label="Tokens Generated" color="#81b29a" />
        <StatCard value={metrics.errors_total} label="Errors" color={metrics.errors_total > 0 ? '#ef4444' : '#666'} />
        <StatCard
          value={metrics.requests_total > 0
            ? `${((1 - metrics.errors_total / metrics.requests_total) * 100).toFixed(1)}%`
            : '—'}
          label="Success Rate"
          color="#84a59d"
        />
      </div>

      {/* Capabilities grid */}
      <Panel title="Active Capabilities">
        {sortedCaps.length === 0 ? (
          <Empty message="No capabilities available — waiting for instances to be discovered" />
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {sortedCaps.map(([cap, { instances: capInsts, offerings }]) => {
              const meta = CAP_META[cap]
              const rec = recs[cap]
              return (
                <div
                  key={cap}
                  className="bg-[#1a1a1a] border border-white/5 rounded-lg p-3 hover:border-white/10 transition-colors"
                >
                  <div className="flex items-center gap-2 mb-2">
                    <span className="text-lg">{meta?.icon}</span>
                    <span className="text-sm font-semibold text-neutral-200">{meta?.label ?? cap}</span>
                    <span className="ml-auto text-[11px] text-neutral-500">
                      {capInsts.length} instance{capInsts.length !== 1 ? 's' : ''}
                    </span>
                  </div>
                  <div className="flex flex-wrap gap-1 mb-2">
                    {[...offerings].map(kind => (
                      <OfferingBadge key={kind} kind={kind as any} />
                    ))}
                  </div>
                  {rec && (
                    <div className="text-[11px] text-neutral-400 mt-1 font-mono truncate">
                      recommended: {rec}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </Panel>

      {/* Offering distribution */}
      <Panel title="Offering Distribution">
        {Object.keys(snapshot.offering_counts).length === 0 ? (
          <Empty message="No offerings discovered yet" />
        ) : (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            {Object.entries(snapshot.offering_counts).map(([kind, count]) => (
              <div key={kind} className="flex items-center gap-2 bg-[#1a1a1a] rounded-lg p-3 border border-white/5">
                <OfferingBadge kind={kind as any} />
                <span className="ml-auto text-sm font-semibold text-neutral-300">{count}</span>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  )
}

function Loading() {
  return (
    <div className="flex items-center justify-center h-64 text-neutral-600">
      <div className="text-center">
        <div className="animate-spin w-6 h-6 border-2 border-sage border-t-transparent rounded-full mx-auto mb-3" />
        <p className="text-sm">Connecting to orchestrator...</p>
      </div>
    </div>
  )
}
