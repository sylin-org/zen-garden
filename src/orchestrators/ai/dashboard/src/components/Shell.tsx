import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useCatalog } from "../contexts/CatalogContext";
import { useJobManager } from "../contexts/JobManagerContext";

/** Default route when clicking a modality leaf in the sidebar. */
const MODALITY_DEFAULTS: Record<string, string> = {
  text: "/create/text/chat",
  image: "/create/image/generate",
  audio: "/create/audio/generate",
};

const MANAGE_ITEMS = [
  { path: "/manage/skills", label: "Skills" },
  { path: "/manage/jobs", label: "Jobs" },
  { path: "/manage/media", label: "Media" },
];

const CONFIGURE_ITEMS = [
  { path: "/configure/preferences", label: "Preferences" },
  { path: "/configure/garden", label: "Garden" },
  { path: "/configure/providers", label: "Providers" },
  { path: "/configure/events", label: "Events" },
];

export default function Shell() {
  const location = useLocation();
  const { catalog } = useCatalog();
  const { connected } = useJobManager();

  const isActive = (prefix: string) => location.pathname.startsWith(prefix);
  const isExact = (path: string) => location.pathname === path;

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      {/* ── Single unified sidebar ── */}
      <aside className="w-[220px] shrink-0 flex flex-col bg-sidebar border-r border-border select-none">
        {/* Logo */}
        <div className="flex items-center gap-2 px-[18px] py-4 border-b border-border">
          <span className="text-[13px] font-bold tracking-tight">
            <span className="text-accent">Zen</span> Garden
          </span>
          <span className="ml-auto text-[10px] text-text-dimmer">AI</span>
        </div>

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto py-2">
          {/* ── CREATE ── */}
          <SidebarGroup
            label="CREATE"
            to="/create"
            active={isExact("/create")}
          />
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
          <SidebarGroup
            label="MANAGE"
            to="/manage"
            active={isExact("/manage")}
            className="mt-4"
          />
          {MANAGE_ITEMS.map((item) => (
            <SidebarLeaf
              key={item.path}
              to={item.path}
              label={item.label}
              active={isActive(item.path)}
            />
          ))}

          {/* ── CONFIGURE ── */}
          <SidebarGroup
            label="CONFIGURE"
            to="/configure"
            active={isExact("/configure")}
            className="mt-4"
          />
          {CONFIGURE_ITEMS.map((item) => (
            <SidebarLeaf
              key={item.path}
              to={item.path}
              label={item.label}
              active={isActive(item.path)}
            />
          ))}
        </nav>

        {/* Status footer */}
        <div className="flex items-center gap-1.5 px-[18px] py-2.5 border-t border-border text-[10px] text-text-dimmer">
          <div
            className={`w-[5px] h-[5px] rounded-full shrink-0 ${connected ? "bg-green" : "bg-red"}`}
          />
          <span>{connected ? "Connected" : "Disconnected"}</span>
          {catalog && (
            <span className="ml-auto">
              {catalog.providers.filter((p) => p.enabled).length} providers
            </span>
          )}
        </div>
      </aside>

      {/* ── Main content area ── */}
      <main className="flex-1 flex flex-col overflow-hidden">
        <Outlet />
      </main>
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
        "block px-[18px] pt-3 pb-1 text-[10px] uppercase tracking-widest font-semibold transition-colors",
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
  icon?: string;
  label: string;
  active: boolean;
}) {
  return (
    <NavLink
      to={to}
      className={[
        "flex items-center gap-2 px-[18px] py-[7px] text-[12px] cursor-pointer",
        "border-l-2 transition-all",
        active
          ? "bg-accent-bg text-accent border-accent font-medium"
          : "text-text-dim border-transparent hover:bg-surface hover:text-text",
      ].join(" ")}
    >
      {icon && <span className="w-[18px] text-center text-[14px] shrink-0">{icon}</span>}
      <span className="truncate">{label}</span>
    </NavLink>
  );
}
