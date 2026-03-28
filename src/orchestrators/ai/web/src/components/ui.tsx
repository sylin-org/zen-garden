/**
 * Shared UI primitives — Grafana-inspired, consistent visual language.
 */

import type { InstanceHealth, Verdict, Capability, OfferingKind } from '../types/api'
import { healthColor, healthLabel, VERDICT_COLOR, VERDICT_LABEL, CAP_META, OFFERING_META, formatBytes, formatNum } from '../lib/meta'

// ── Stat Card ───────────────────────────────────────────────────

interface StatCardProps {
  value: string | number
  label: string
  color?: string
}

export function StatCard({ value, label, color }: StatCardProps) {
  return (
    <div className="bg-[#1a1a1a] border border-white/5 rounded-lg p-4 text-center">
      <div className="text-2xl font-bold" style={{ color: color ?? '#84a59d' }}>
        {typeof value === 'number' ? formatNum(value) : value}
      </div>
      <div className="text-[11px] text-neutral-500 mt-1">{label}</div>
    </div>
  )
}

// ── Health Badge ────────────────────────────────────────────────

interface HealthBadgeProps {
  health: InstanceHealth
}

export function HealthBadge({ health }: HealthBadgeProps) {
  const color = healthColor(health)
  const label = healthLabel(health)
  return (
    <span
      className="inline-flex items-center gap-1.5 text-[11px] font-medium px-2 py-0.5 rounded-full"
      style={{ backgroundColor: `${color}20`, color }}
    >
      <span
        className={`w-1.5 h-1.5 rounded-full ${label === 'healthy' ? 'animate-pulse' : ''}`}
        style={{ backgroundColor: color }}
      />
      {label}
    </span>
  )
}

// ── Verdict Badge ───────────────────────────────────────────────

interface VerdictBadgeProps {
  verdict: Verdict
}

export function VerdictBadge({ verdict }: VerdictBadgeProps) {
  const color = VERDICT_COLOR[verdict]
  return (
    <span
      className="inline-flex items-center gap-1 text-[11px] font-medium px-2 py-0.5 rounded-full"
      style={{ backgroundColor: `${color}20`, color }}
    >
      <span className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: color }} />
      {VERDICT_LABEL[verdict]}
    </span>
  )
}

// ── Capability Badge ────────────────────────────────────────────

interface CapBadgeProps {
  cap: Capability
  small?: boolean
}

export function CapBadge({ cap, small }: CapBadgeProps) {
  const meta = CAP_META[cap]
  if (!meta) return <span className="text-[10px] text-neutral-500">{cap}</span>
  return (
    <span
      className={`inline-flex items-center gap-1 font-medium rounded-full ${
        small ? 'text-[10px] px-1.5 py-0' : 'text-[11px] px-2 py-0.5'
      }`}
      style={{ backgroundColor: `${meta.color}20`, color: meta.color }}
    >
      {meta.icon} {meta.label}
    </span>
  )
}

// ── Offering Badge ──────────────────────────────────────────────

interface OfferingBadgeProps {
  kind: OfferingKind
}

export function OfferingBadge({ kind }: OfferingBadgeProps) {
  const meta = OFFERING_META[kind]
  const label = meta?.label ?? kind
  const color = meta?.color ?? '#666'
  return (
    <span
      className="inline-flex items-center gap-1 text-[11px] font-medium px-2 py-0.5 rounded-full"
      style={{ backgroundColor: `${color}20`, color }}
    >
      {meta?.icon} {label}
      {meta?.cloud && <span className="text-[9px] opacity-60">cloud</span>}
    </span>
  )
}

// ── VRAM Gauge ──────────────────────────────────────────────────

interface VramGaugeProps {
  total: number
  used?: number
  free?: number | null
  className?: string
}

export function VramGauge({ total, used, free, className }: VramGaugeProps) {
  if (total === 0) return <span className="text-[10px] text-neutral-600">No VRAM</span>

  const usedBytes = used ?? (free != null ? total - free : 0)
  const pct = Math.round((usedBytes / total) * 100)
  const barColor = pct > 80 ? '#ef4444' : pct > 60 ? '#d4a373' : '#84a59d'

  return (
    <div className={className}>
      <div className="flex justify-between text-[10px] text-neutral-500 mb-0.5">
        <span>{formatBytes(usedBytes)} used</span>
        <span>{formatBytes(total)}</span>
      </div>
      <div className="h-1.5 bg-white/5 rounded-full overflow-hidden">
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{ width: `${pct}%`, backgroundColor: barColor }}
        />
      </div>
    </div>
  )
}

// ── Panel ───────────────────────────────────────────────────────

interface PanelProps {
  title: string
  children: React.ReactNode
  action?: React.ReactNode
  className?: string
}

export function Panel({ title, children, action, className }: PanelProps) {
  return (
    <div className={`bg-[#141414] border border-white/5 rounded-lg ${className ?? ''}`}>
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-white/5">
        <h3 className="text-[13px] font-semibold text-neutral-300">{title}</h3>
        {action}
      </div>
      <div className="p-4">{children}</div>
    </div>
  )
}

// ── Empty State ─────────────────────────────────────────────────

interface EmptyProps {
  message: string
}

export function Empty({ message }: EmptyProps) {
  return (
    <div className="text-center py-8 text-neutral-600 text-sm italic">{message}</div>
  )
}
