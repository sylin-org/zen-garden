import { Routes, Route, useLocation } from "react-router-dom";
import { Sidebar } from "./components/Sidebar";
import { OverviewView } from "./views/Overview";
import { GardenView } from "./views/Garden";
import { StoneDetailView } from "./views/StoneDetail";
import { OfferingsView } from "./views/Offerings";
import { SeedBanksView } from "./views/SeedBanks";
import { ActivityView } from "./views/Activity";
import { PondView } from "./views/Pond";
import "./App.css";

export function App() {
  const { pathname } = useLocation();
  const isFullView = pathname === "/";

  return (
    <div className="shell">
      <Sidebar />
      <main className={isFullView ? "main main-full" : "main"}>
        <Routes>
          <Route path="/" element={<OverviewView />} />
          <Route path="/garden" element={<GardenView />} />
          <Route path="/stones/:stoneId" element={<StoneDetailView />} />
          <Route path="/offerings" element={<OfferingsView />} />
          <Route path="/seeds" element={<SeedBanksView />} />
          <Route path="/activity" element={<ActivityView />} />
          <Route path="/pond" element={<PondView />} />
        </Routes>
      </main>
    </div>
  );
}
