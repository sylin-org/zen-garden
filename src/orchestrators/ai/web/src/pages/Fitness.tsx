/**
 * Fitness page — benchmark controls, progress, GPU fitness matrix.
 * Three-panel state machine: Setup → Running → Results.
 */

import { useState } from 'react'
import { useSnapshot } from '../hooks/useOrchestrator'
import { VerdictBadge, Panel, Empty } from '../components/ui'
import type { BenchmarkData, GpuMatrixEntry } from '../types/api'

export function Fitness() {
  const snapshot = useSnapshot()
  if (!snapshot) return null

  const { benchmark } = snapshot
  const isIdle = benchmark.status === 'idle' || benchmark.status === 'completed' || benchmark.status === 'cancelled' || benchmark.status === 'failed'
  const isRunning = benchmark.status === 'running'
  const hasResults = benchmark.gpu_matrix.entries.length > 0

  return (
    <div className="space-y-4">
      {isIdle && <SetupPanel />}
      {isRunning && <RunningPanel benchmark={benchmark} />}
      {hasResults && <ResultsPanel benchmark={benchmark} />}
      {!hasResults && isIdle && <Empty message="No benchmark data yet. Run a benchmark to profile instance fitness." />}
    </div>
  )
}

function SetupPanel() {
  const [status, setStatus] = useState('')

  async function startBenchmark() {
    setStatus('Starting benchmark...')
    try {
      const resp = await fetch('/api/benchmark/start', { method: 'POST' })
      const data = await resp.json()
      setStatus(resp.ok ? `Benchmark started: ${data.id}` : `Error: ${JSON.stringify(data)}`)
    } catch (e: unknown) {
      setStatus(`Error: ${e instanceof Error ? e.message : 'unknown'}`)
    }
  }

  return (
    <Panel title="Benchmark Setup">
      <div className="space-y-3">
        <p className="text-[12px] text-neutral-400">
          Run a fitness benchmark across all healthy instances. Tests each model on each
          stone for throughput, cold start time, and capability-specific verdicts.
        </p>
        <div className="flex items-center gap-3">
          <button
            onClick={startBenchmark}
            className="px-4 py-2 bg-sage/20 text-sage text-sm font-medium rounded-md hover:bg-sage/30 transition-colors"
          >
            Run Benchmark
          </button>
          {status && <span className="text-[11px] text-neutral-400">{status}</span>}
        </div>
      </div>
    </Panel>
  )
}

function RunningPanel({ benchmark }: { benchmark: BenchmarkData }) {
  async function cancelBenchmark() {
    await fetch('/api/benchmark/cancel', { method: 'POST' })
  }

  const stoneCount = benchmark.stones.length
  const completed = (benchmark.stones as Array<{ status: string }>).filter(
    s => s.status === 'done' || s.status === 'skipped' || s.status === 'error'
  ).length
  const pct = stoneCount > 0 ? Math.round((completed / stoneCount) * 100) : 0

  return (
    <Panel title="Benchmark Running">
      <div className="space-y-3">
        <div className="flex items-center gap-3">
          <div className="flex-1">
            <div className="h-2 bg-white/5 rounded-full overflow-hidden">
              <div
                className="h-full bg-sage rounded-full transition-all duration-500"
                style={{ width: `${pct}%` }}
              />
            </div>
          </div>
          <span className="text-sm text-neutral-300 font-mono">{pct}%</span>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-[12px] text-neutral-400">
            {completed} of {stoneCount} stones profiled
          </span>
          <button
            onClick={cancelBenchmark}
            className="px-3 py-1 bg-blocked/20 text-blocked text-[12px] rounded-md hover:bg-blocked/30 transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </Panel>
  )
}

function ResultsPanel({ benchmark }: { benchmark: BenchmarkData }) {
  const entries = benchmark.gpu_matrix.entries
  if (entries.length === 0) return null

  // Group by model, then by stone
  const models = [...new Set(entries.map(e => e.model))].sort()
  const stones = [...new Set(entries.map(e => e.stone_name))].sort()

  function getEntry(model: string, stone: string): GpuMatrixEntry | undefined {
    return entries.find(e => e.model === model && e.stone_name === stone)
  }

  return (
    <Panel
      title="Fitness Matrix"
      action={
        <a
          href="/api/benchmark/export"
          className="text-[11px] text-sage hover:underline"
          target="_blank"
        >
          Export JSON
        </a>
      }
    >
      <div className="overflow-x-auto">
        <table className="w-full text-[11px]">
          <thead>
            <tr className="text-left text-neutral-500 border-b border-white/5">
              <th className="py-2 pr-4 font-semibold">Model</th>
              {stones.map(s => (
                <th key={s} className="py-2 px-3 text-center font-semibold">{s}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-white/5">
            {models.map(model => (
              <tr key={model} className="hover:bg-white/[0.02]">
                <td className="py-2 pr-4 font-mono text-neutral-300 whitespace-nowrap">{model}</td>
                {stones.map(stone => {
                  const e = getEntry(model, stone)
                  if (!e) return <td key={stone} className="px-3 text-center text-neutral-700">—</td>
                  return (
                    <td key={stone} className="px-3 text-center">
                      <VerdictBadge verdict={e.verdict} />
                      <div className="text-[9px] text-neutral-500 mt-0.5">
                        {e.median_tps > 0 ? `${e.median_tps.toFixed(1)} tok/s` : ''}
                        {e.cold_start_ms > 0 ? ` · ${(e.cold_start_ms / 1000).toFixed(1)}s cold` : ''}
                      </div>
                    </td>
                  )
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {benchmark.completed_at && (
        <p className="text-[10px] text-neutral-600 mt-3">
          Completed: {new Date(benchmark.completed_at).toLocaleString()}
        </p>
      )}
    </Panel>
  )
}
