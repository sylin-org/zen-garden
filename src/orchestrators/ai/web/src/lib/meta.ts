/**
 * Capability and offering metadata — single source of truth for
 * icons, labels, colors, and sort order.
 */

import type { Capability, OfferingKind, Verdict, InstanceHealth } from '../types/api'

// ── Capability metadata ─────────────────────────────────────────

interface CapMeta {
  rank: number
  label: string
  icon: string
  color: string
}

export const CAP_META: Record<Capability, CapMeta> = {
  generate:   { rank: 0,  label: 'Generation',  icon: '⚙',  color: '#84a59d' },
  chat:       { rank: 1,  label: 'Chat',         icon: '💬', color: '#84a59d' },
  embed:      { rank: 2,  label: 'Embedding',    icon: '🔍', color: '#7ea8be' },
  vision:     { rank: 3,  label: 'Vision',        icon: '👁', color: '#c4b060' },
  tools:      { rank: 4,  label: 'Tool Use',      icon: '🔧', color: '#d4a373' },
  think:      { rank: 5,  label: 'Thinking',      icon: '🧠', color: '#b07aa1' },
  imagine:    { rank: 6,  label: 'Imagine',       icon: '🎨', color: '#e07a5f' },
  edit:       { rank: 7,  label: 'Edit',          icon: '✏',  color: '#c08497' },
  render:     { rank: 8,  label: 'Render',        icon: '🎬', color: '#81b29a' },
  transcribe: { rank: 9,  label: 'Transcribe',    icon: '🎤', color: '#d4a373' },
  speak:      { rank: 10, label: 'Speak',          icon: '🔊', color: '#84a59d' },
  rerank:     { rank: 11, label: 'Rerank',         icon: '↕',  color: '#7ea8be' },
  translate:  { rank: 12, label: 'Translate',      icon: '🌐', color: '#c4b060' },
}

// ── Offering metadata ───────────────────────────────────────────

interface OfferingMeta {
  label: string
  color: string
  icon: string
  cloud?: boolean
}

export const OFFERING_META: Partial<Record<OfferingKind, OfferingMeta>> = {
  ollama:            { label: 'Ollama',          color: '#84a59d', icon: '🦙' },
  comfyui:           { label: 'ComfyUI',         color: '#d4a373', icon: '🎨' },
  whispercpp:        { label: 'whisper.cpp',     color: '#c4b060', icon: '🎤' },
  speaches:          { label: 'Speaches',        color: '#b07aa1', icon: '🔊' },
  'openedai-speech': { label: 'OpenedAI Speech', color: '#7ea8be', icon: '🗣' },
  infinity:          { label: 'Infinity',        color: '#81b29a', icon: '∞' },
  libretranslate:    { label: 'LibreTranslate',  color: '#e07a5f', icon: '🌐' },
  openai:            { label: 'OpenAI',          color: '#a8a29e', icon: '☁', cloud: true },
  anthropic:         { label: 'Anthropic',       color: '#a8a29e', icon: '☁', cloud: true },
  google:            { label: 'Google',          color: '#a8a29e', icon: '☁', cloud: true },
  cohere:            { label: 'Cohere',          color: '#a8a29e', icon: '☁', cloud: true },
  deepgram:          { label: 'Deepgram',        color: '#a8a29e', icon: '☁', cloud: true },
}

// ── Verdict colors ──────────────────────────────────────────────

export const VERDICT_COLOR: Record<Verdict, string> = {
  fast: '#22c55e',
  degraded: '#eab308',
  vetoed: '#f97316',
  blocked: '#ef4444',
}

export const VERDICT_LABEL: Record<Verdict, string> = {
  fast: 'Fast',
  degraded: 'Degraded',
  vetoed: 'Vetoed',
  blocked: 'Blocked',
}

// ── Health helpers ──────────────────────────────────────────────

export function healthLabel(h: InstanceHealth): string {
  return h.status
}

export function healthColor(h: InstanceHealth): string {
  switch (h.status) {
    case 'healthy': return '#22c55e'
    case 'unhealthy': return '#ef4444'
    case 'profiling': return '#eab308'
  }
}

// ── Format helpers ──────────────────────────────────────────────

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(0)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${bytes} B`
}

export function formatNum(n: number): string {
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`
  return String(n)
}

export function formatDuration(secs: number): string {
  if (secs < 60) return `${secs}s`
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

export function formatContext(tokens: number | null): string {
  if (!tokens) return '—'
  if (tokens >= 1000) return `${(tokens / 1000).toFixed(0)}K`
  return String(tokens)
}

/** Deterministic color from a string (for stone cards). */
export function stringColor(s: string): string {
  let hash = 0
  for (let i = 0; i < s.length; i++) {
    hash = s.charCodeAt(i) + ((hash << 5) - hash)
  }
  const hue = Math.abs(hash) % 360
  return `hsl(${hue}, 35%, 55%)`
}

/** Check if an offering kind is a cloud provider. */
export function isCloud(kind: OfferingKind): boolean {
  return OFFERING_META[kind]?.cloud === true
}
