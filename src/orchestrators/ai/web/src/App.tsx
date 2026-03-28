import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { OrchestratorProvider } from './hooks/useOrchestrator'
import { Sidebar } from './components/Sidebar'
import { Overview } from './pages/Overview'
import { Stones } from './pages/Stones'
import { Models } from './pages/Models'
import { Fitness } from './pages/Fitness'
import { Recommendations } from './pages/Recommendations'
import { Placement } from './pages/Placement'
import { Metrics } from './pages/Metrics'
import { Settings } from './pages/Settings'
import { Activity } from './pages/Activity'

export default function App() {
  return (
    <BrowserRouter>
      <OrchestratorProvider>
        <div className="flex min-h-screen bg-[#0d0d0d] text-neutral-300">
          <Sidebar />
          <main className="flex-1 p-6 overflow-auto">
            <Routes>
              <Route path="/" element={<Overview />} />
              <Route path="/stones" element={<Stones />} />
              <Route path="/models" element={<Models />} />
              <Route path="/fitness" element={<Fitness />} />
              <Route path="/recommendations" element={<Recommendations />} />
              <Route path="/placement" element={<Placement />} />
              <Route path="/metrics" element={<Metrics />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/activity" element={<Activity />} />
            </Routes>
          </main>
        </div>
      </OrchestratorProvider>
    </BrowserRouter>
  )
}
