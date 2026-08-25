import React, { useState, useEffect, Suspense } from 'react'

// Dynamically import ALL .jsx files from diagrams folder
// Vite's import.meta.glob gives us automatic discovery!
const diagramModules = import.meta.glob('./diagrams/*.jsx')

// Color palette for categories
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
  red: {
    border: 'border-red-500/30',
    bg: 'bg-red-500/5',
    text: 'text-red-400',
    hover: 'hover:border-red-500/50 hover:bg-red-500/10',
    active: 'border-red-500 bg-red-500/20',
  },
  rose: {
    border: 'border-rose-500/30',
    bg: 'bg-rose-500/5',
    text: 'text-rose-400',
    hover: 'hover:border-rose-500/50 hover:bg-rose-500/10',
    active: 'border-rose-500 bg-rose-500/20',
  },
  zinc: {
    border: 'border-zinc-600/30',
    bg: 'bg-zinc-600/5',
    text: 'text-zinc-400',
    hover: 'hover:border-zinc-500/50 hover:bg-zinc-600/10',
    active: 'border-zinc-500 bg-zinc-600/20',
  },
}

// Helper: convert filename to readable name
function filenameToTitle(filename) {
  return filename
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

function Loading() {
  return (
    <div className="w-full h-full flex items-center justify-center bg-zinc-900">
      <div className="text-zinc-500 animate-pulse">Loading diagram...</div>
    </div>
  )
}

function Menu({ diagrams, categories, categoryOrder, selected, onSelect, onClose, isVisible }) {
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

        {categoryOrder.map((categoryName) => {
          const items = categories[categoryName]
          if (!items || items.length === 0) return null
          
          const categoryColor = items[0]?.color || 'zinc'
          const colors = colorClasses[categoryColor] || colorClasses.zinc
          
          return (
            <div key={categoryName} className="mb-6">
              <h2 className={`text-xs tracking-wider mb-2 ${colors.text}`}>
                {categoryName.toUpperCase()}
              </h2>
              <div className="space-y-1">
                {items.map((item) => {
                  const itemColors = colorClasses[item.color] || colorClasses.zinc
                  return (
                    <button
                      key={item.id}
                      onClick={() => {
                        onSelect(item.id)
                        onClose()
                      }}
                      className={`
                        w-full text-left px-3 py-2 rounded border transition-all
                        ${selected === item.id 
                          ? itemColors.active 
                          : `${itemColors.border} ${itemColors.bg} ${itemColors.hover}`}
                      `}
                    >
                      <div className={`text-sm ${selected === item.id ? itemColors.text : 'text-zinc-300'}`}>
                        {item.name}
                      </div>
                      {item.description && (
                        <div className="text-xs text-zinc-500">{item.description}</div>
                      )}
                    </button>
                  )
                })}
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

        <div className="mt-4 pt-4 border-t border-zinc-800">
          <div className="text-zinc-700 text-xs">
            {Object.keys(diagrams).length} diagrams loaded
          </div>
        </div>
      </div>
    </div>
  )
}

function App() {
  const [diagrams, setDiagrams] = useState({})
  const [categories, setCategories] = useState({})
  const [categoryOrder, setCategoryOrder] = useState([])
  const [allItems, setAllItems] = useState([])
  const [selected, setSelected] = useState(null)
  const [menuVisible, setMenuVisible] = useState(true)
  const [loading, setLoading] = useState(true)
  const [CurrentDiagram, setCurrentDiagram] = useState(null)

  // Load all diagram modules on mount
  useEffect(() => {
    async function loadDiagrams() {
      const loaded = {}
      const cats = {}
      const catOrders = {} // Track lowest categoryOrder for each category
      const items = []

      for (const path in diagramModules) {
        try {
          const module = await diagramModules[path]()
          
          // Extract filename without path and extension
          const filename = path.replace('./diagrams/', '').replace('.jsx', '')
          
          // Get metadata from module export, or create defaults from filename
          const meta = module.metadata || {}

          const item = {
            id: filename,
            name: meta.name || filenameToTitle(filename),
            description: meta.description || '',
            category: meta.category || 'Other',
            color: meta.color || 'zinc',
            order: meta.order ?? 999,
            component: module.default
          }

          loaded[filename] = item
          items.push(item)

          // Group by category
          if (!cats[item.category]) {
            cats[item.category] = []
          }
          cats[item.category].push(item)

          // Track category order (lowest categoryOrder wins for each category)
          const catOrder = meta.categoryOrder ?? 999
          if (catOrders[item.category] === undefined || catOrder < catOrders[item.category]) {
            catOrders[item.category] = catOrder
          }
        } catch (err) {
          console.error(`Failed to load ${path}:`, err)
        }
      }

      // Sort items within each category by order, then name
      for (const cat in cats) {
        cats[cat].sort((a, b) => {
          if (a.order !== b.order) return a.order - b.order
          return a.name.localeCompare(b.name)
        })
      }

      // Build category order dynamically (Other always last)
      const dynamicCategoryOrder = Object.keys(cats)
        .filter(c => c !== 'Other')
        .sort((a, b) => (catOrders[a] ?? 999) - (catOrders[b] ?? 999))
      
      if (cats['Other']) {
        dynamicCategoryOrder.push('Other')
      }

      // Build sorted items list for keyboard navigation
      const sortedItems = []
      for (const catName of dynamicCategoryOrder) {
        if (cats[catName]) {
          sortedItems.push(...cats[catName])
        }
      }

      setDiagrams(loaded)
      setCategories(cats)
      setCategoryOrder(dynamicCategoryOrder)
      setAllItems(sortedItems)
      setLoading(false)

      // Select first diagram
      if (sortedItems.length > 0) {
        setSelected(sortedItems[0].id)
        setCurrentDiagram(() => sortedItems[0].component)
      }
    }

    loadDiagrams()
  }, [])

  // Update current diagram when selection changes
  useEffect(() => {
    if (selected && diagrams[selected]) {
      setCurrentDiagram(() => diagrams[selected].component)
    }
  }, [selected, diagrams])

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e) => {
      // Ignore if user is typing in an input
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return

      if (e.key === 'm' || e.key === 'M') {
        setMenuVisible(v => !v)
      }
      if (e.key === 'Escape') {
        setMenuVisible(false)
      }
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
        e.preventDefault()
        const currentIndex = allItems.findIndex(i => i.id === selected)
        const nextIndex = (currentIndex + 1) % allItems.length
        setSelected(allItems[nextIndex].id)
      }
      if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
        e.preventDefault()
        const currentIndex = allItems.findIndex(i => i.id === selected)
        const prevIndex = (currentIndex - 1 + allItems.length) % allItems.length
        setSelected(allItems[prevIndex].id)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selected, allItems])

  if (loading) {
    return (
      <div className="h-screen bg-zinc-900 flex items-center justify-center">
        <div className="text-zinc-500">Discovering diagrams...</div>
      </div>
    )
  }

  return (
    <div className="h-screen bg-zinc-900 overflow-hidden">
      <Menu 
        diagrams={diagrams}
        categories={categories}
        categoryOrder={categoryOrder}
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
      {!menuVisible && selected && diagrams[selected] && (
        <div className="fixed top-4 right-4 z-40 px-3 py-2 bg-zinc-800/80 border border-zinc-700 rounded-lg">
          <span className="text-zinc-400 text-sm">{diagrams[selected].name}</span>
        </div>
      )}

      {/* Diagram container */}
      <div className={`h-full transition-all duration-300 ${menuVisible ? 'ml-80' : 'ml-0'}`}>
        <Suspense fallback={<Loading />}>
          {CurrentDiagram && <CurrentDiagram />}
        </Suspense>
      </div>
    </div>
  )
}

export default App
