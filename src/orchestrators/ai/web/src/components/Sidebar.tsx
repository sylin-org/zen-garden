import { NavLink } from 'react-router-dom'
import { useSnapshot, useConnected } from '../hooks/useOrchestrator'

const NAV_ITEMS = [
  { to: '/', label: 'Overview', icon: '◎' },
  { to: '/stones', label: 'Stones', icon: '⬡' },
  { to: '/models', label: 'Models', icon: '☰' },
  { to: '/fitness', label: 'Fitness', icon: '⚡' },
  { to: '/recommendations', label: 'Recommendations', icon: '★' },
  { to: '/placement', label: 'Placement', icon: '⊞' },
  { to: '/metrics', label: 'Metrics', icon: '▦' },
  { to: '/settings', label: 'Settings', icon: '⚙' },
  { to: '/activity', label: 'Activity', icon: '↻' },
]

export function Sidebar() {
  const snapshot = useSnapshot()
  const connected = useConnected()
  const o = snapshot?.orchestrator

  return (
    <aside className="w-56 shrink-0 border-r border-white/5 bg-[#111] flex flex-col h-screen sticky top-0">
      <div className="p-4 border-b border-white/5">
        <h1 className="text-sm font-semibold text-sage">AI Orchestrator</h1>
        <p className="text-[10px] text-neutral-500 mt-0.5">
          {o ? `v${o.version} · ${o.instances_discovered} instances` : 'Connecting...'}
        </p>
        <div className="flex items-center gap-1.5 mt-2">
          <span className={`w-1.5 h-1.5 rounded-full ${connected ? 'bg-fast animate-pulse' : 'bg-blocked'}`} />
          <span className="text-[10px] text-neutral-500">
            {connected ? 'Connected' : 'Disconnected'}
          </span>
        </div>
      </div>

      <nav className="flex-1 p-2 space-y-0.5 overflow-y-auto">
        {NAV_ITEMS.map(item => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === '/'}
            className={({ isActive }) =>
              `flex items-center gap-2.5 px-3 py-1.5 rounded-md text-[13px] transition-colors ${
                isActive
                  ? 'bg-white/8 text-white font-medium'
                  : 'text-neutral-400 hover:bg-white/5 hover:text-neutral-200'
              }`
            }
          >
            <span className="w-4 text-center opacity-60">{item.icon}</span>
            {item.label}
          </NavLink>
        ))}
      </nav>

      <div className="p-3 border-t border-white/5 text-[10px] text-neutral-600">
        {o && `Uptime: ${formatUptime(o.uptime_secs)}`}
      </div>
    </aside>
  )
}

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}
