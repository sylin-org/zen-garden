import { NavLink } from "react-router-dom";
import type { DashboardStatus, CapabilityState, SkillInfo } from "../types";
import { ALL_CAPABILITIES, CAPABILITY_LABELS } from "../types";
import { isCloudOffering } from "../utils/cloudCatalog";

interface SidebarProps {
  status: DashboardStatus | null;
  skills?: SkillInfo[];
}

const STATE_DOT: Record<CapabilityState, string> = {
  active: "bg-emerald-400",
  needs_setup: "bg-yellow-400",
  not_installed: "bg-gray-600",
  degraded: "bg-red-400",
};

function capabilityState(
  status: DashboardStatus | null,
  cap: string,
): CapabilityState {
  if (!status) return "not_installed";
  const found = status.capabilities.find((c) => c.capability === cap);
  return found?.state ?? "not_installed";
}

function countLocalServices(status: DashboardStatus | null): number {
  if (!status) return 0;
  const kinds = new Set<string>();
  for (const inst of status.instances) {
    if (!isCloudOffering(inst.kind)) {
      kinds.add(inst.kind);
    }
  }
  return kinds.size;
}

function countCloudProviders(status: DashboardStatus | null): number {
  if (!status) return 0;
  const kinds = new Set<string>();
  for (const inst of status.instances) {
    if (isCloudOffering(inst.kind)) {
      kinds.add(inst.kind);
    }
  }
  return kinds.size;
}

function skillCountForCapability(skills: SkillInfo[] | undefined, cap: string): number {
  if (!skills) return 0;
  return skills.filter((s) => s.capability === cap).length;
}

export function Sidebar({ status, skills }: SidebarProps) {
  const localCount = countLocalServices(status);
  const cloudCount = countCloudProviders(status);

  return (
    <aside className="w-48 shrink-0 border-r border-[#2e303a] bg-[#0f1117] flex flex-col h-screen sticky top-0">
      <div className="px-4 py-4 border-b border-[#2e303a]">
        <h1 className="text-sm font-semibold text-gray-100 tracking-wide">
          Zen Garden AI
        </h1>
        {status && (
          <span className="text-[10px] text-gray-500 font-mono">
            v{status.version}
          </span>
        )}
      </div>

      <nav className="flex-1 overflow-y-auto py-2">
        <NavLink
          to="/"
          end
          className={({ isActive }) =>
            `block px-4 py-1.5 text-[13px] ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
            }`
          }
        >
          Overview
        </NavLink>

        <div className="px-4 pt-4 pb-1">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
            AI
          </span>
        </div>

        {ALL_CAPABILITIES.map((cap) => {
          const state = capabilityState(status, cap);
          const skillCount = skillCountForCapability(skills, cap);
          return (
            <NavLink
              key={cap}
              to={`/capability/${cap}`}
              className={({ isActive }) =>
                `flex items-center justify-between px-4 py-1 text-[13px] ${
                  isActive
                    ? "text-gray-100 bg-[#1a1b23]"
                    : state === "not_installed" && skillCount === 0
                      ? "text-gray-500 hover:text-gray-300 hover:bg-[#1a1b23]/50"
                      : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
                }`
              }
            >
              <span className="flex items-center gap-2">
                <span
                  className={`w-1.5 h-1.5 rounded-full shrink-0 ${STATE_DOT[state]}`}
                />
                {CAPABILITY_LABELS[cap] ?? cap}
              </span>
              {skillCount > 0 && (
                <span className="text-[10px] text-amber-400 font-mono">
                  +{skillCount}
                </span>
              )}
            </NavLink>
          );
        })}

        <div className="px-4 pt-4 pb-1">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
            Infra
          </span>
        </div>

        <NavLink
          to="/infra/services"
          className={({ isActive }) =>
            `flex items-center justify-between px-4 py-1.5 text-[13px] ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
            }`
          }
        >
          <span>Services</span>
          {localCount > 0 && (
            <span className="text-[10px] text-gray-500 font-mono">{localCount}</span>
          )}
        </NavLink>

        <NavLink
          to="/infra/cloud"
          className={({ isActive }) =>
            `flex items-center justify-between px-4 py-1.5 text-[13px] ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
            }`
          }
        >
          <span className="flex items-center gap-2">
            <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-purple-500" />
            Cloud
          </span>
          {cloudCount > 0 && (
            <span className="text-[10px] text-purple-400 font-mono">{cloudCount}</span>
          )}
        </NavLink>

        <NavLink
          to="/infra/stones"
          className={({ isActive }) =>
            `block px-4 py-1.5 text-[13px] ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
            }`
          }
        >
          Stones
        </NavLink>

        <NavLink
          to="/infra/secrets"
          className={({ isActive }) =>
            `block px-4 py-1.5 text-[13px] ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-400 hover:text-gray-200 hover:bg-[#1a1b23]/50"
            }`
          }
        >
          Secrets
        </NavLink>
      </nav>

      <div className="border-t border-[#2e303a] p-2">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `block px-3 py-1.5 text-[13px] rounded ${
              isActive
                ? "text-gray-100 bg-[#1a1b23]"
                : "text-gray-500 hover:text-gray-300"
            }`
          }
        >
          Settings
        </NavLink>
      </div>
    </aside>
  );
}
