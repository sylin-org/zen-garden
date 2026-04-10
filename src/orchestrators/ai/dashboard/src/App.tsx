import { lazy, Suspense } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import Shell from "./components/Shell";

// Lazy-loaded surfaces
const CreateSurface = lazy(() => import("./features/create/CreateSurface"));
const CreateIndex = lazy(() => import("./features/create/CreateIndex"));
const Workspace = lazy(() => import("./features/create/Workspace"));
const ManageSurface = lazy(() => import("./features/manage/ManageSurface"));
const ConfigureSurface = lazy(() => import("./features/configure/ConfigureSurface"));

// Manage placeholders (Phase 3)
const SkillListPlaceholder = lazy(() =>
  import("./features/manage/Placeholder").then((m) => ({ default: m.SkillListPlaceholder })),
);
const JobListPlaceholder = lazy(() =>
  import("./features/manage/Placeholder").then((m) => ({ default: m.JobListPlaceholder })),
);
const MediaBrowserPlaceholder = lazy(() =>
  import("./features/manage/Placeholder").then((m) => ({ default: m.MediaBrowserPlaceholder })),
);

// Configure placeholders (Phase 4)
const PreferencesPlaceholder = lazy(() =>
  import("./features/configure/Placeholder").then((m) => ({ default: m.PreferencesPlaceholder })),
);
const GardenViewPlaceholder = lazy(() =>
  import("./features/configure/Placeholder").then((m) => ({ default: m.GardenViewPlaceholder })),
);
const ProviderListPlaceholder = lazy(() =>
  import("./features/configure/Placeholder").then((m) => ({ default: m.ProviderListPlaceholder })),
);
const EventLogPlaceholder = lazy(() =>
  import("./features/configure/Placeholder").then((m) => ({ default: m.EventLogPlaceholder })),
);

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
            <Route path="skills" element={<SkillListPlaceholder />} />
            <Route path="jobs" element={<JobListPlaceholder />} />
            <Route path="media" element={<MediaBrowserPlaceholder />} />
          </Route>

          {/* Configure surface */}
          <Route path="configure" element={<ConfigureSurface />}>
            <Route index element={<Navigate to="preferences" replace />} />
            <Route path="preferences" element={<PreferencesPlaceholder />} />
            <Route path="garden" element={<GardenViewPlaceholder />} />
            <Route path="providers" element={<ProviderListPlaceholder />} />
            <Route path="events" element={<EventLogPlaceholder />} />
          </Route>
        </Route>
      </Routes>
    </Suspense>
  );
}
