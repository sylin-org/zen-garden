import { Routes, Route } from "react-router-dom";
import { useStatus } from "./hooks/useStatus";
import { Sidebar } from "./components/Sidebar";
import { Overview } from "./pages/Overview";
import { CapabilityDetail } from "./pages/CapabilityDetail";
import { Stones } from "./pages/Stones";
import { Cloud } from "./pages/Cloud";
import { Settings } from "./pages/Settings";

function App() {
  const { status, loading, error } = useStatus();

  return (
    <div className="flex min-h-screen bg-[#0f1117] text-gray-400 font-[system-ui]">
      <Sidebar status={status} />

      <main className="flex-1 min-w-0 p-6 overflow-y-auto">
        {loading && !status && (
          <div className="flex items-center justify-center h-64">
            <span className="text-sm text-gray-500">Loading...</span>
          </div>
        )}

        {error && !status && (
          <div className="flex flex-col items-center justify-center h-64 gap-2">
            <span className="text-sm text-red-400">{error}</span>
            <span className="text-[11px] text-gray-500">
              Is the orchestrator running on port 7190?
            </span>
          </div>
        )}

        {error && status && (
          <div className="mb-4 px-3 py-2 bg-yellow-400/5 border border-yellow-500/30 rounded text-[12px] text-yellow-400">
            {error}
          </div>
        )}

        {status && (
          <Routes>
            <Route path="/" element={<Overview status={status} />} />
            <Route
              path="/capability/:name"
              element={<CapabilityDetail status={status} />}
            />
            <Route path="/stones" element={<Stones status={status} />} />
            <Route path="/cloud" element={<Cloud status={status} />} />
            <Route path="/settings" element={<Settings status={status} />} />
          </Routes>
        )}
      </main>
    </div>
  );
}

export default App;
