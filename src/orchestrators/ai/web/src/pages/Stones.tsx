/**
 * Stones page — per-stone cards with GPU info, VRAM gauges,
 * per-offering breakdown, and loaded models.
 */

import { useSnapshot } from '../hooks/useOrchestrator'
import { HealthBadge, CapBadge, OfferingBadge, VramGauge, Panel, Empty } from '../components/ui'
import { stringColor } from '../lib/meta'
import type { ServiceInstance } from '../types/api'

export function Stones() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  // Group instances by stone name
  const stoneMap = new Map<string, ServiceInstance[]>()
  for (const inst of snapshot.instances) {
    const name = inst.stone.name
    const group = stoneMap.get(name) ?? []
    group.push(inst)
    stoneMap.set(name, group)
  }

  const stones = [...stoneMap.entries()].sort((a, b) => a[0].localeCompare(b[0]))

  return (
    <Panel title={`Stones (${stones.length})`}>
      {stones.length === 0 ? (
        <Empty message="No stones discovered yet" />
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {stones.map(([name, instances]) => (
            <StoneCard key={name} name={name} instances={instances} />
          ))}
        </div>
      )}
    </Panel>
  )
}

interface StoneCardProps {
  name: string
  instances: ServiceInstance[]
}

function StoneCard({ name, instances }: StoneCardProps) {
  // Aggregate VRAM across all offerings on this stone
  const totalVram = Math.max(...instances.map(i => i.vram.total_bytes), 0)
  const usedVram = instances.reduce(
    (sum, i) => sum + i.models_loaded.reduce((s, m) => s + m.vram_bytes, 0),
    0
  )
  const gpu = instances.find(i => i.gpu.name)?.gpu
  const allCaps = new Set(instances.flatMap(i => i.capabilities))

  return (
    <div
      className="bg-[#1a1a1a] border border-white/5 rounded-lg overflow-hidden"
      style={{ borderLeftColor: stringColor(name), borderLeftWidth: 3 }}
    >
      {/* Header */}
      <div className="px-4 py-3 border-b border-white/5">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-semibold text-neutral-200">{name}</h3>
            {gpu?.name && (
              <p className="text-[11px] text-neutral-500 mt-0.5">{gpu.name}</p>
            )}
          </div>
          <div className="flex gap-1">
            {instances.map(i => (
              <HealthBadge key={i.endpoint} health={i.health} />
            ))}
          </div>
        </div>

        {/* VRAM gauge */}
        {totalVram > 0 && (
          <VramGauge total={totalVram} used={usedVram} className="mt-2" />
        )}
      </div>

      {/* Per-offering breakdown */}
      <div className="divide-y divide-white/5">
        {instances.map(inst => (
          <div key={inst.endpoint} className="px-4 py-2.5">
            <div className="flex items-center justify-between mb-1.5">
              <OfferingBadge kind={inst.kind} />
              <span className="text-[10px] text-neutral-600 font-mono truncate max-w-[200px]">
                {inst.endpoint}
              </span>
            </div>
            <div className="flex items-center gap-4 text-[11px] text-neutral-400">
              <span>{inst.models_available.length} models</span>
              <span>{inst.models_loaded.length} loaded</span>
              <span>queue: {inst.queue_depth}</span>
              {inst.priority !== 0 && (
                <span className={inst.priority < 0 ? 'text-neutral-600' : 'text-gold'}>
                  priority: {inst.priority}
                </span>
              )}
            </div>
            {inst.health.status === 'unhealthy' && 'reason' in inst.health && (
              <p className="text-[10px] text-blocked mt-1">{inst.health.reason}</p>
            )}
          </div>
        ))}
      </div>

      {/* Capabilities footer */}
      <div className="px-4 py-2 border-t border-white/5 flex flex-wrap gap-1">
        {[...allCaps].map(cap => (
          <CapBadge key={cap} cap={cap} small />
        ))}
      </div>
    </div>
  )
}
