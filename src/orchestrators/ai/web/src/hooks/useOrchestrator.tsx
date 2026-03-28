/**
 * Orchestrator state provider — single SSE connection, fan-out to React tree.
 *
 * The backend pushes a `status.snapshot` event every 3s containing the full
 * orchestrator state. Individual mutation events (registry.updated, etc.) are
 * also delivered for real-time activity logging.
 *
 * Components consume state via `useSnapshot()` and events via `useEvents()`.
 * No polling — all state flows through the single EventSource.
 */

import { createContext, useContext, useEffect, useReducer, useCallback, useRef, type ReactNode } from 'react'
import type { Snapshot } from '../types/api'

// ── Context types ───────────────────────────────────────────────

interface OrchestratorState {
  snapshot: Snapshot | null
  connected: boolean
  events: ActivityEntry[]
}

export interface ActivityEntry {
  time: string
  type: string
  data: string
}

type Action =
  | { type: 'SNAPSHOT'; payload: Snapshot }
  | { type: 'EVENT'; payload: ActivityEntry }
  | { type: 'CONNECTED' }
  | { type: 'DISCONNECTED' }

const MAX_EVENTS = 200

function reducer(state: OrchestratorState, action: Action): OrchestratorState {
  switch (action.type) {
    case 'SNAPSHOT':
      return { ...state, snapshot: action.payload, connected: true }
    case 'EVENT': {
      const events = [action.payload, ...state.events].slice(0, MAX_EVENTS)
      return { ...state, events }
    }
    case 'CONNECTED':
      return { ...state, connected: true }
    case 'DISCONNECTED':
      return { ...state, connected: false }
    default:
      return state
  }
}

const initialState: OrchestratorState = {
  snapshot: null,
  connected: false,
  events: [],
}

// ── Context ─────────────────────────────────────────────────────

const OrchestratorContext = createContext<OrchestratorState>(initialState)

const KNOWN_EVENTS = [
  'status.snapshot',
  'registry.updated',
  'job.created', 'job.completed', 'job.failed',
  'settings.updated',
  'tending.changed',
  'placement.updated',
  'benchmark.started', 'benchmark.progress', 'benchmark.completed',
  'recommendations.updated',
]

// ── Provider ────────────────────────────────────────────────────

interface OrchestratorProviderProps {
  children: ReactNode
}

export function OrchestratorProvider({ children }: OrchestratorProviderProps) {
  const [state, dispatch] = useReducer(reducer, initialState)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const connect = useCallback(() => {
    const es = new EventSource('/api/events')

    const handleEvent = (ev: MessageEvent) => {
      const now = new Date().toLocaleTimeString()

      if (ev.type === 'status.snapshot') {
        try {
          const snapshot = JSON.parse(ev.data) as Snapshot
          dispatch({ type: 'SNAPSHOT', payload: snapshot })
        } catch {
          // Malformed snapshot — skip
        }
        return
      }

      // All other events go to the activity log
      dispatch({
        type: 'EVENT',
        payload: { time: now, type: ev.type, data: ev.data },
      })
    }

    KNOWN_EVENTS.forEach(t => es.addEventListener(t, handleEvent as EventListener))

    // Fallback for unnamed events
    es.onmessage = (ev) => {
      const now = new Date().toLocaleTimeString()
      dispatch({
        type: 'EVENT',
        payload: { time: now, type: 'message', data: ev.data },
      })
    }

    es.onopen = () => {
      dispatch({ type: 'CONNECTED' })
    }

    es.onerror = () => {
      es.close()
      dispatch({ type: 'DISCONNECTED' })
      reconnectTimer.current = setTimeout(connect, 5000)
    }

    return es
  }, [])

  useEffect(() => {
    // Initial fetch for immediate data (SSE snapshot takes up to 3s)
    fetch('/api/status')
      .then(r => r.json())
      .then(data => {
        if (data && typeof data === 'object' && data.orchestrator) {
          dispatch({ type: 'SNAPSHOT', payload: data as Snapshot })
        }
      })
      .catch(() => {/* will connect via SSE */})

    const es = connect()

    return () => {
      es.close()
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
    }
  }, [connect])

  return (
    <OrchestratorContext.Provider value={state}>
      {children}
    </OrchestratorContext.Provider>
  )
}

// ── Hooks ───────────────────────────────────────────────────────

/** Access the latest orchestrator snapshot. */
export function useSnapshot(): Snapshot | null {
  return useContext(OrchestratorContext).snapshot
}

/** Access the SSE connection status. */
export function useConnected(): boolean {
  return useContext(OrchestratorContext).connected
}

/** Access the activity event log. */
export function useEvents(): ActivityEntry[] {
  return useContext(OrchestratorContext).events
}
