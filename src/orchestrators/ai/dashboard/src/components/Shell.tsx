import { useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useCatalog } from "../contexts/CatalogContext";
import { useJobManager } from "../contexts/JobManagerContext";
import OverviewPanel from "./OverviewPanel";

/** Default route when clicking a modality leaf in the sidebar. */
const MODALITY_DEFAULTS: Record<string, string> = {
  text: "/create/text/chat",
  image: "/create/image/generate",
  audio: "/create/audio/generate",
};

interface NavItem {
  path: string;
  icon: string;
  label: string;
  group?: boolean;
}

const MANAGE_ITEMS: NavItem[] = [
  { path: "/manage", icon: "☰", label: "Manage", group: true },
  { path: "/manage/skills", icon: "✦", label: "Skills" },
  { path: "/manage/jobs", icon: "⏱", label: "Jobs" },
  { path: "/manage/media", icon: "◻", label: "Media" },
];

const CONFIGURE_ITEMS: NavItem[] = [
  { path: "/configure", icon: "⚙", label: "Configure", group: true },
  { path: "/configure/preferences", icon: "☰", label: "Preferences" },
  { path: "/configure/garden", icon: "❋", label: "Garden" },
  { path: "/configure/providers", icon: "⬡", label: "Providers" },
  { path: "/configure/events", icon: "↯", label: "Events" },
];

export default function Shell() {
  const location = useLocation();
  const { catalog } = useCatalog();
  const { connected } = useJobManager();
  const [overviewOpen, setOverviewOpen] = useState(() => window.innerWidth > 1400);

  const isActive = (prefix: string) => location.pathname === prefix || location.pathname.startsWith(prefix + "/");
  const isExact = (path: string) => location.pathname === path;

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      {/* ── Icon Sidebar (52px) ── */}
      <aside className="w-[52px] shrink-0 flex flex-col items-center bg-sidebar border-r border-border select-none">
        {/* Logo */}
        <NavLink to="/create" className="py-3 text-accent font-bold text-lg" title="Zen Garden AI">
          ✦
        </NavLink>

        <div className="w-6 border-t border-border mb-2" />

        {/* CREATE group */}
        <SidebarIcon to="/create" icon="+" label="Create" active={isExact("/create")} group />
        {catalog?.modalities.map((mod) => (
          <SidebarIcon
            key={mod.id}
            to={MODALITY_DEFAULTS[mod.id] ?? `/create/${mod.id}`}
            icon={mod.icon}
            label={mod.label}
            active={isActive(`/create/${mod.id}`)}
          />
        ))}

        <div className="w-6 border-t border-border my-2" />

        {/* MANAGE group */}
        {MANAGE_ITEMS.map((item) => (
          <SidebarIcon
            key={item.path}
            to={item.path}
            icon={item.icon}
            label={item.label}
            active={item.group ? isExact(item.path) : isActive(item.path)}
            group={item.group}
          />
        ))}

        <div className="w-6 border-t border-border my-2" />

        {/* CONFIGURE group */}
        {CONFIGURE_ITEMS.map((item) => (
          <SidebarIcon
            key={item.path}
            to={item.path}
            icon={item.icon}
            label={item.label}
            active={item.group ? isExact(item.path) : isActive(item.path)}
            group={item.group}
          />
        ))}

        {/* Spacer */}
        <div className="flex-1" />

        {/* Connection status */}
        <div className="pb-3" title={connected ? "Connected" : "Disconnected"}>
          <div className={`w-2 h-2 rounded-full ${connected ? "bg-green" : "bg-red"}`} />
        </div>
      </aside>

      {/* ── Main content area + Overview panel ── */}
      <div className="flex-1 flex overflow-hidden relative">
        <main className="flex-1 flex flex-col overflow-hidden">
          <Outlet />
        </main>
        <OverviewPanel
          open={overviewOpen}
          onToggle={() => setOverviewOpen((p) => !p)}
        />
      </div>
    </div>
  );
}

function SidebarIcon({
  to,
  icon,
  label,
  active,
  group,
}: {
  to: string;
  icon: string;
  label: string;
  active: boolean;
  group?: boolean;
}) {
  return (
    <NavLink
      to={to}
      title={label}
      className={[
        "relative flex items-center justify-center w-10 h-10 rounded-lg text-[14px] transition-all",
        group ? "text-[10px] font-bold uppercase tracking-wider mt-0" : "",
        active
          ? "bg-accent-bg text-accent"
          : "text-text-dim hover:bg-surface hover:text-text",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {/* Active indicator */}
      {active && (
        <div className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 bg-accent rounded-r" />
      )}
      <span>{icon}</span>
    </NavLink>
  );
}
