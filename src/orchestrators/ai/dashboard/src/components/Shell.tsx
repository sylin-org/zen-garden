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
      {/* ── Sidebar (160px, always expanded with labels) ── */}
      <aside className="w-[160px] shrink-0 flex flex-col bg-sidebar border-r border-border select-none">
        {/* Logo */}
        <NavLink to="/create" className="flex items-center gap-2 px-4 py-3.5 border-b border-border">
          <span className="text-accent font-bold text-sm">✦</span>
          <span className="text-[12px] font-bold tracking-tight">
            <span className="text-accent">Zen</span> Garden
          </span>
        </NavLink>

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto py-1">
          {/* ── CREATE ── */}
          <SidebarGroup label="CREATE" to="/create" active={isExact("/create")} />
          {catalog?.modalities.map((mod) => (
            <SidebarLeaf
              key={mod.id}
              to={MODALITY_DEFAULTS[mod.id] ?? `/create/${mod.id}`}
              icon={mod.icon}
              label={mod.label}
              active={isActive(`/create/${mod.id}`)}
            />
          ))}

          {/* ── MANAGE ── */}
          <SidebarGroup label="MANAGE" to="/manage" active={isExact("/manage")} className="mt-3" />
          {MANAGE_ITEMS.filter((i) => !i.group).map((item) => (
            <SidebarLeaf
              key={item.path}
              to={item.path}
              icon={item.icon}
              label={item.label}
              active={isActive(item.path)}
            />
          ))}

          {/* ── CONFIGURE ── */}
          <SidebarGroup label="CONFIGURE" to="/configure" active={isExact("/configure")} className="mt-3" />
          {CONFIGURE_ITEMS.filter((i) => !i.group).map((item) => (
            <SidebarLeaf
              key={item.path}
              to={item.path}
              icon={item.icon}
              label={item.label}
              active={isActive(item.path)}
            />
          ))}
        </nav>

        {/* Connection status */}
        <div className="flex items-center gap-1.5 px-4 py-2.5 border-t border-border text-[10px] text-text-dimmer">
          <div className={`w-1.5 h-1.5 rounded-full shrink-0 ${connected ? "bg-green" : "bg-red"}`} />
          <span>{connected ? "Connected" : "Disconnected"}</span>
          {catalog && (
            <span className="ml-auto">
              {catalog.providers.filter((p) => p.enabled).length}
            </span>
          )}
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

function SidebarGroup({
  label,
  to,
  active,
  className,
}: {
  label: string;
  to: string;
  active: boolean;
  className?: string;
}) {
  return (
    <NavLink
      to={to}
      className={[
        "block px-4 pt-2.5 pb-1 text-[9px] uppercase tracking-widest font-semibold transition-colors",
        active ? "text-accent" : "text-text-dimmer hover:text-text-dim",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {label}
    </NavLink>
  );
}

function SidebarLeaf({
  to,
  icon,
  label,
  active,
}: {
  to: string;
  icon: string;
  label: string;
  active: boolean;
}) {
  return (
    <NavLink
      to={to}
      className={[
        "flex items-center gap-2 px-4 py-[6px] text-[12px] cursor-pointer",
        "border-l-2 transition-all",
        active
          ? "bg-accent-bg text-accent border-accent font-medium"
          : "text-text-dim border-transparent hover:bg-surface hover:text-text",
      ].join(" ")}
    >
      <span className="w-[16px] text-center text-[13px] shrink-0">{icon}</span>
      <span className="truncate">{label}</span>
    </NavLink>
  );
}
