import { lazy, Suspense } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import Shell from "./components/Shell";

// Lazy-loaded surfaces
const CreateSurface = lazy(() => import("./features/create/CreateSurface"));
const CreateIndex = lazy(() => import("./features/create/CreateIndex"));
const Workspace = lazy(() => import("./features/create/Workspace"));
const ManageSurface = lazy(() => import("./features/manage/ManageSurface"));
const ConfigureSurface = lazy(() => import("./features/configure/ConfigureSurface"));

// Manage pages
const SkillList = lazy(() => import("./features/manage/SkillList"));
const JobList = lazy(() => import("./features/manage/JobList"));
const MediaBrowser = lazy(() => import("./features/manage/MediaBrowser"));

// Configure pages
const PreferenceEditor = lazy(() => import("./features/configure/PreferenceEditor"));
const GardenView = lazy(() => import("./features/configure/GardenView"));
const ProviderList = lazy(() => import("./features/configure/ProviderList"));
const EventLog = lazy(() => import("./features/configure/EventLog"));

function Loading() {
  return (
    <div className="flex items-center justify-center h-full text-text-dim text-sm">
      Loading...
    </div>
  );
}

export default function App() {
  return (
    <Suspense fallback={<Loading />}>
      <Routes>
        <Route element={<Shell />}>
          <Route index element={<Navigate to="/create" replace />} />

          {/* Create surface */}
          <Route path="create" element={<CreateSurface />}>
            <Route index element={<CreateIndex />} />
            <Route path=":modality/:leaf" element={<Workspace />} />
            <Route path=":modality/:leaf/:skill" element={<Workspace />} />
          </Route>

          {/* Manage surface */}
          <Route path="manage" element={<ManageSurface />}>
            <Route index element={<Navigate to="skills" replace />} />
            <Route path="skills" element={<SkillList />} />
            <Route path="jobs" element={<JobList />} />
            <Route path="media" element={<MediaBrowser />} />
          </Route>

          {/* Configure surface */}
          <Route path="configure" element={<ConfigureSurface />}>
            <Route index element={<Navigate to="preferences" replace />} />
            <Route path="preferences" element={<PreferenceEditor />} />
            <Route path="garden" element={<GardenView />} />
            <Route path="providers" element={<ProviderList />} />
            <Route path="events" element={<EventLog />} />
          </Route>
        </Route>
      </Routes>
    </Suspense>
  );
}
