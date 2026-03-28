/**
 * Activity page — real-time SSE event log.
 */

import { useEvents } from '../hooks/useOrchestrator'
import { Panel, Empty } from '../components/ui'

export function Activity() {
  const events = useEvents()

  return (
    <Panel title={`Activity Log (${events.length} events)`}>
      {events.length === 0 ? (
        <Empty message="No events received yet — waiting for SSE stream" />
      ) : (
        <div className="max-h-[600px] overflow-y-auto font-mono text-[11px] space-y-0">
          {events.map((ev, i) => (
            <div
              key={i}
              className="flex gap-3 py-1 px-2 border-b border-white/[0.03] hover:bg-white/[0.02]"
            >
              <span className="text-neutral-600 shrink-0 w-16">{ev.time}</span>
              <span className="text-sage font-semibold shrink-0 w-40 truncate">{ev.type}</span>
              <span className="text-neutral-500 truncate">{ev.data}</span>
            </div>
          ))}
        </div>
      )}
    </Panel>
  )
}
