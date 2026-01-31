import React, { useState, useEffect, Suspense, lazy } from 'react'

// Lazy load all diagrams
const diagrams = {
  // Core Concepts
  'mdns-discovery': lazy(() => import('./diagrams/mdns-discovery.jsx')),
  'node-vs-stone': lazy(() => import('./diagrams/node-vs-stone.jsx')),
  'tempo-breathing': lazy(() => import('./diagrams/tempo-breathing.jsx')),
  'price-breakdown': lazy(() => import('./diagrams/price-breakdown.jsx')),
  'seed-bank-migration': lazy(() => import('./diagrams/seed-bank-migration.jsx')),
  
  // How Things Work
  'discovery-cascade': lazy(() => import('./diagrams/discovery-cascade.jsx')),
  'ceremony-workflow': lazy(() => import('./diagrams/ceremony-workflow.jsx')),
  'connection-string': lazy(() => import('./diagrams/connection-string.jsx')),
  'stone-health': lazy(() => import('./diagrams/stone-health.jsx')),
  'cost-comparison': lazy(() => import('./diagrams/cost-comparison.jsx')),
  'capability-aware-app': lazy(() => import('./diagrams/capability-aware-app.jsx')),
  
  // Architecture
  'symmetric-vs-asymmetric': lazy(() => import('./diagrams/symmetric-vs-asymmetric.jsx')),
  'service-origins': lazy(() => import('./diagrams/service-origins.jsx')),
  'aws-bridge': lazy(() => import('./diagrams/aws-bridge.jsx')),
  'tending': lazy(() => import('./diagrams/tending.jsx')),
  'graceful-degradation': lazy(() => import('./diagrams/graceful-degradation.jsx')),
  
  // Problem → Insight
  'configuration-explosion': lazy(() => import('./diagrams/configuration-explosion.jsx')),
  'abstraction-tax': lazy(() => import('./diagrams/abstraction-tax.jsx')),
  'feedback-through-glass': lazy(() => import('./diagrams/feedback-through-glass.jsx')),
  'scale-theater': lazy(() => import('./diagrams/scale-theater.jsx')),
  'knowledge-wall': lazy(() => import('./diagrams/knowledge-wall.jsx')),
}

const categories = [
  {
    name: 'Core Concepts',
    color: 'amber',
    items: [
      { id: 'mdns-discovery', name: 'mDNS Discovery', desc: 'Stones finding each other' },
      { id: 'node-vs-stone', name: 'Node vs Stone', desc: 'The vocabulary philosophy' },
      { id: 'tempo-breathing', name: 'Tempo Breathing', desc: 'Firefly idle vs busy' },
      { id: 'price-breakdown', name: 'Price Breakdown', desc: 'The $192.50 reveal' },
      { id: 'seed-bank-migration', name: 'Seed-Bank Migration', desc: 'File journey with app' },
    ]
  },
  {
    name: 'How Things Work',
    color: 'blue',
    items: [
      { id: 'discovery-cascade', name: 'Discovery Cascade', desc: '--at → env → cache → UDP' },
      { id: 'ceremony-workflow', name: 'Ceremony Workflow', desc: 'Harvest, update, rollback' },
      { id: 'connection-string', name: 'Connection String', desc: 'Abstract → concrete' },
      { id: 'stone-health', name: 'Stone Health', desc: 'Thriving / withering / wilting' },
      { id: 'cost-comparison', name: 'Cost Comparison', desc: '5-year cloud vs garden' },
      { id: 'capability-aware-app', name: 'Capability-Aware App', desc: 'Features light up' },
    ]
  },
  {
    name: 'Architecture',
    color: 'purple',
    items: [
      { id: 'symmetric-vs-asymmetric', name: 'Symmetric vs Asymmetric', desc: 'Cloud uniformity vs diversity' },
      { id: 'service-origins', name: 'Service Origins', desc: 'Planted / Adopted / Borrowed' },
      { id: 'aws-bridge', name: 'AWS Bridge', desc: 'Same code, anywhere' },
      { id: 'tending', name: 'Tending', desc: 'Context like cd' },
      { id: 'graceful-degradation', name: 'Graceful Degradation', desc: 'Stone dies, garden heals' },
    ]
  },
  {
    name: 'Problem → Insight',
    color: 'green',
    items: [
      { id: 'configuration-explosion', name: 'Configuration Explosion', desc: '246 lines → 1 command' },
      { id: 'abstraction-tax', name: 'Abstraction Tax', desc: '8 layers → 2 layers' },
      { id: 'feedback-through-glass', name: 'Feedback Through Glass', desc: 'Dashboards vs ambient' },
      { id: 'scale-theater', name: 'Scale Theater', desc: 'Billion-user arch, 12 users' },
      { id: 'knowledge-wall', name: 'Knowledge Wall', desc: 'Buttons vs systems' },
    ]
  },
]

const colorClasses = {
  amber: {
    border: 'border-amber-500/30',
    bg: 'bg-amber-500/5',
    text: 'text-amber-400',
    hover: 'hover:border-amber-500/50 hover:bg-amber-500/10',
    active: 'border-amber-500 bg-amber-500/20',
  },
  blue: {
    border: 'border-blue-500/30',
    bg: 'bg-blue-500/5',
    text: 'text-blue-400',
    hover: 'hover:border-blue-500/50 hover:bg-blue-500/10',
    active: 'border-blue-500 bg-blue-500/20',
  },
  purple: {
    border: 'border-purple-500/30',
    bg: 'bg-purple-500/5',
    text: 'text-purple-400',
    hover: 'hover:border-purple-500/50 hover:bg-purple-500/10',
    active: 'border-purple-500 bg-purple-500/20',
  },
  green: {
    border: 'border-green-500/30',
    bg: 'bg-green-500/5',
    text: 'text-green-400',
    hover: 'hover:border-green-500/50 hover:bg-green-500/10',
    active: 'border-green-500 bg-green-500/20',
  },
}

function Loading() {
  return (
    <div className="w-full h-full flex items-center justify-center bg-zinc-900">
      <div className="text-zinc-500">Loading diagram...</div>
    </div>
  )
}

function Menu({ selected, onSelect, onClose, isVisible }) {
  return (
    <div className={`
      fixed inset-y-0 left-0 w-80 bg-zinc-900 border-r border-zinc-800 
      transform transition-transform duration-300 z-50 overflow-y-auto
      ${isVisible ? 'translate-x-0' : '-translate-x-full'}
    `}>
      <div className="p-4">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-xl font-light text-zinc-200">Zen Garden</h1>
            <p className="text-zinc-500 text-sm">Animated Diagrams</p>
          </div>
          <button 
            onClick={onClose}
            className="text-zinc-500 hover:text-zinc-300 p-2"
          >
            ✕
          </button>
        </div>

        {categories.map((category) => {
          const colors = colorClasses[category.color]
          return (
            <div key={category.name} className="mb-6">
              <h2 className={`text-xs tracking-wider mb-2 ${colors.text}`}>
                {category.name.toUpperCase()}
              </h2>
              <div className="space-y-1">
                {category.items.map((item) => (
                  <button
                    key={item.id}
                    onClick={() => {
                      onSelect(item.id)
                      onClose()
                    }}
                    className={`
                      w-full text-left px-3 py-2 rounded border transition-all
                      ${selected === item.id 
                        ? colors.active 
                        : `${colors.border} ${colors.bg} ${colors.hover}`}
                    `}
                  >
                    <div className={`text-sm ${selected === item.id ? colors.text : 'text-zinc-300'}`}>
                      {item.name}
                    </div>
                    <div className="text-xs text-zinc-500">{item.desc}</div>
                  </button>
                ))}
              </div>
            </div>
          )
        })}

        <div className="mt-8 pt-4 border-t border-zinc-800">
          <div className="text-zinc-600 text-xs space-y-1">
            <p>Press <kbd className="px-1 py-0.5 bg-zinc-800 rounded">M</kbd> to toggle menu</p>
            <p>Press <kbd className="px-1 py-0.5 bg-zinc-800 rounded">F11</kbd> for fullscreen</p>
            <p>Press <kbd className="px-1 py-0.5 bg-zinc-800 rounded">←</kbd> <kbd className="px-1 py-0.5 bg-zinc-800 rounded">→</kbd> to navigate</p>
          </div>
        </div>
      </div>
    </div>
  )
}

function App() {
  const [selected, setSelected] = useState('mdns-discovery')
  const [menuVisible, setMenuVisible] = useState(true)

  // Keyboard navigation
  useEffect(() => {
    const allItems = categories.flatMap(c => c.items)
    
    const handleKeyDown = (e) => {
      if (e.key === 'm' || e.key === 'M') {
        setMenuVisible(v => !v)
      }
      if (e.key === 'Escape') {
        setMenuVisible(false)
      }
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
        const currentIndex = allItems.findIndex(i => i.id === selected)
        const nextIndex = (currentIndex + 1) % allItems.length
        setSelected(allItems[nextIndex].id)
      }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
        const currentIndex = allItems.findIndex(i => i.id === selected)
        const prevIndex = (currentIndex - 1 + allItems.length) % allItems.length
        setSelected(allItems[prevIndex].id)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selected])

  const DiagramComponent = diagrams[selected]

  return (
    <div className="h-screen bg-zinc-900 overflow-hidden">
      <Menu 
        selected={selected} 
        onSelect={setSelected} 
        onClose={() => setMenuVisible(false)}
        isVisible={menuVisible}
      />

      {/* Menu toggle button */}
      {!menuVisible && (
        <button
          onClick={() => setMenuVisible(true)}
          className="fixed top-4 left-4 z-40 px-3 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-zinc-400 text-sm hover:border-zinc-600 hover:text-zinc-300 transition-all"
        >
          ☰ Menu
        </button>
      )}

      {/* Current diagram name */}
      {!menuVisible && (
        <div className="fixed top-4 right-4 z-40 px-3 py-2 bg-zinc-800/80 border border-zinc-700 rounded-lg">
          <span className="text-zinc-400 text-sm">{selected}</span>
        </div>
      )}

      {/* Diagram container */}
      <div className={`h-full transition-all duration-300 ${menuVisible ? 'ml-80' : 'ml-0'}`}>
        <Suspense fallback={<Loading />}>
          {DiagramComponent && <DiagramComponent />}
        </Suspense>
      </div>
    </div>
  )
}

export default App
