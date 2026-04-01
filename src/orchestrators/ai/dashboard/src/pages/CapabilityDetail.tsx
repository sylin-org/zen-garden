import { useState, useEffect, useCallback } from "react";
import { useParams, useSearchParams, Link } from "react-router-dom";
import type { DashboardStatus, ModelStatus, SkillInfo } from "../types";
import { CAPABILITY_LABELS, formatBytes } from "../types";
import { stoneColor } from "../utils/stoneColors";
import { isCloudOffering } from "../utils/cloudCatalog";
import { ModelTryIt } from "../components/ModelTryIt";
import { SkillTryIt } from "../components/SkillTryIt";

// ── Sort State ─────────────────────────────────────────────────

type SortDir = "asc" | "desc" | null;

interface SortState {
  column: string;
  direction: SortDir;
}

const DEFAULT_SORT: SortState = { column: "params", direction: "desc" };

function parseParamSize(raw: string | undefined | null): number {
  if (!raw) return 0;
  const lower = raw.toLowerCase().trim();
  if (lower === "cloud") return 0;
  const match = lower.match(/^([\d.]+)\s*([bkmgt])?/);
  if (!match) return 0;
  const value = parseFloat(match[1]);
  switch (match[2]) {
    case "t": return value * 1e12;
    case "g": return value * 1e9;
    case "b": return value * 1e9;
    case "m": return value * 1e6;
    case "k": return value * 1e3;
    default:  return value;
  }
}

function useSortState(capabilityName: string): [SortState, (column: string) => void] {
  const storageKey = `zen-ai-sort-${capabilityName}`;

  const [sort, setSort] = useState<SortState>(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) {
        const parsed = JSON.parse(saved) as SortState;
        if (parsed.column !== undefined && parsed.direction !== undefined) {
          return parsed;
        }
      }
    } catch { /* ignore corrupt data */ }
    return DEFAULT_SORT;
  });

  useEffect(() => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(sort));
    } catch { /* quota exceeded — ignore */ }
  }, [sort, storageKey]);

  const toggleSort = useCallback((column: string) => {
    setSort((prev) => {
      if (prev.column === column) {
        if (prev.direction === "asc") return { column, direction: "desc" };
        if (prev.direction === "desc") return { column: "", direction: null };
        return { column, direction: "asc" };
      }
      return { column, direction: "asc" };
    });
  }, []);

  return [sort, toggleSort];
}

function SortHeader({
  label,
  column,
  sort,
  onSort,
  className = "",
}: {
  label: string;
  column: string;
  sort: SortState;
  onSort: (col: string) => void;
  className?: string;
}) {
  const active = sort.column === column;
  const arrow = active
    ? sort.direction === "asc"
      ? " \u25B2"
      : " \u25BC"
    : "";

  return (
    <th
      className={`py-1.5 font-medium cursor-pointer select-none hover:text-gray-300 transition-colors ${className}`}
      onClick={() => onSort(column)}
    >
      {label}{arrow}
    </th>
  );
}

// ── Props ──────────────────────────────────────────────────────

interface CapabilityDetailProps {
  status: DashboardStatus;
  skills?: SkillInfo[];
}

// ── Main Component ──────────────────────────────────────────────

export function CapabilityDetail({ status, skills = [] }: CapabilityDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const expandedModel = searchParams.get("model");
  const [searchQuery, setSearchQuery] = useState("");

  const cap = status.capabilities.find((c) => c.capability === name);
  const label = CAPABILITY_LABELS[name ?? ""] ?? name ?? "Unknown";

  if (!cap) {
    return (
      <div className="p-6">
        <p className="text-gray-400">Capability &quot;{name}&quot; not found.</p>
        <Link to="/" className="text-blue-400 text-sm hover:underline">
          Back to overview
        </Link>
      </div>
    );
  }

  const pinned = status.config.features.pins[cap.capability];
  const recommended = status.recommendations[cap.capability];

  // Flat model list: all models serving this capability
  const allModels = status.models.filter((m) =>
    m.capabilities.includes(cap.capability),
  );

  const models = searchQuery
    ? allModels.filter((m) =>
        m.model.toLowerCase().includes(searchQuery.toLowerCase()),
      )
    : allModels;

  // Collect unique stone names from all placements
  const stoneNames = [
    ...new Set(
      models.flatMap((m) => m.available_on.map((p) => p.stone)),
    ),
  ];

  const stoneEntries = stoneNames.map((sn) => ({
    name: sn,
    color: stoneColor(sn),
  }));

  const [sort, toggleSort] = useSortState(name ?? "unknown");

  const sortedModels = [...models].sort((a, b) => {
    if (!sort.column || !sort.direction) return 0;
    const dir = sort.direction === "asc" ? 1 : -1;

    switch (sort.column) {
      case "model":
        return dir * a.model.localeCompare(b.model);
      case "provider":
        return dir * modelProvider(a).name.localeCompare(modelProvider(b).name);
      case "params": {
        const aSize = parseParamSize(a.metadata.parameter_size);
        const bSize = parseParamSize(b.metadata.parameter_size);
        const diff = aSize - bSize;
        return diff !== 0 ? dir * diff : a.model.localeCompare(b.model);
      }
      case "context": {
        const aCtx = a.metadata.context_length ?? 0;
        const bCtx = b.metadata.context_length ?? 0;
        const diff = aCtx - bCtx;
        return diff !== 0 ? dir * diff : a.model.localeCompare(b.model);
      }
      default:
        return 0;
    }
  });

  function toggleExpand(modelName: string) {
    if (expandedModel === modelName) {
      setSearchParams({}, { replace: true });
    } else {
      setSearchParams({ model: modelName }, { replace: true });
    }
  }

  async function pinModel(modelName: string) {
    const newPins = {
      ...status.config.features.pins,
      [cap!.capability]: modelName,
    };
    await fetch("/api/settings", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...status.config,
        features: { ...status.config.features, pins: newPins },
      }),
    });
  }

  async function unpinCapability() {
    const newPins = { ...status.config.features.pins };
    delete newPins[cap!.capability];
    await fetch("/api/settings", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ...status.config,
        features: { ...status.config.features, pins: newPins },
      }),
    });
  }

  // Determine provider for a model (first offering from placement, fallback to MFQN)
  function modelProvider(model: ModelStatus): { name: string; cloud: boolean } {
    const offerings = [...new Set(model.available_on.map((p) => p.offering))];
    if (offerings.length > 0) {
      const first = offerings[0];
      return { name: first, cloud: isCloudOffering(first) };
    }
    // Fallback: parse the first MFQN instance string (source|locator|model|...)
    const mfqn = model.instances[0];
    if (mfqn) {
      const source = mfqn.split("|")[0];
      return { name: source, cloud: isCloudOffering(source) };
    }
    return { name: "unknown", cloud: false };
  }

  return (
    <div className="space-y-5 max-w-6xl">
      {/* Breadcrumb + header */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">{label}</span>
        </div>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-medium text-gray-100">{label}</h2>
          <StateBadge state={cap.state} />
          {recommended && !pinned && (
            <span className="text-[10px] text-gray-500 ml-2">
              recommended:{" "}
              <span className="text-emerald-400 font-mono">{recommended}</span>
            </span>
          )}
        </div>
      </div>

      {/* Pinned banner */}
      {pinned && (
        <div className="bg-purple-500/10 border border-purple-500/30 rounded-lg px-4 py-2 flex items-center justify-between">
          <div>
            <span className="text-[10px] text-purple-400 uppercase tracking-wider font-semibold">
              Pinned
            </span>
            <span className="ml-2 text-sm text-gray-200 font-mono">
              {pinned}
            </span>
            <span className="ml-2 text-[11px] text-gray-500">
              overrides recommendation
            </span>
          </div>
          <button
            onClick={unpinCapability}
            className="text-xs px-2 py-1 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30"
          >
            Unpin
          </button>
        </div>
      )}

      {/* Guidance for inactive */}
      {cap.state === "not_installed" && (
        <div className="bg-[#1a1b23] border border-gray-700 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-1">
            No service can serve{" "}
            <span className="text-gray-200">{label}</span>.
          </p>
          <p className="text-[12px] text-gray-500">
            <Link
              to="/infra/services"
              className="text-blue-400 hover:underline"
            >
              Install a compatible offering
            </Link>
            , or{" "}
            <Link
              to="/infra/cloud"
              className="text-purple-400 hover:underline"
            >
              add a cloud provider
            </Link>
            .
          </p>
        </div>
      )}

      {cap.state === "needs_setup" && (
        <div className="bg-[#1a1b23] border border-yellow-500/40 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-1">
            {cap.offerings.map((o, i) => (
              <span key={o}>
                {i > 0 && ", "}
                <Link
                  to={`/infra/services/${o}`}
                  className="text-blue-400 hover:underline"
                >
                  {o}
                </Link>
              </span>
            ))}{" "}
            installed, but no {label} models available.
          </p>
          <p className="text-[12px] text-yellow-400/80">
            <Link
              to={`/infra/services/${cap.offerings[0]}`}
              className="text-yellow-300 hover:underline"
            >
              Pull a model
            </Link>
            , or{" "}
            <Link
              to="/infra/cloud"
              className="text-purple-400 hover:underline"
            >
              add a cloud provider
            </Link>{" "}
            as fallback.
          </p>
        </div>
      )}

      {/* Skills block */}
      {(() => {
        const capSkills = skills.filter((s) => s.capability === name);
        if (capSkills.length === 0) return null;

        return (
          <div className="space-y-2">
            <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
              Skills
            </h3>
            {capSkills.map((skill) => (
              <SkillBlock key={skill.name} skill={skill} />
            ))}
          </div>
        );
      })()}

      {/* Flat model table */}
      {allModels.length > 0 && (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
          {allModels.length > 5 && (
            <div className="px-3 py-2 border-b border-[#2e303a]">
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Filter models..."
                className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-1.5 text-[12px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500/50"
              />
            </div>
          )}
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-[#2e303a] text-gray-500 text-left">
                  <SortHeader label="Model" column="model" sort={sort} onSort={toggleSort} className="px-3" />
                  <SortHeader label="Provider" column="provider" sort={sort} onSort={toggleSort} className="px-3" />
                  <SortHeader label="Params" column="params" sort={sort} onSort={toggleSort} className="px-3" />
                  {stoneEntries.map((se) => (
                    <th
                      key={se.name}
                      className="px-1 py-1.5 font-medium text-center w-6"
                      title={se.name}
                    >
                      <span
                        className={`inline-block w-3 h-3 rounded-sm ${se.color.bg}`}
                      />
                    </th>
                  ))}
                  <th className="px-2 py-1.5 w-10" />
                </tr>
              </thead>
              <tbody className="divide-y divide-[#2e303a]/50">
                {sortedModels.map((model) => {
                  const provider = modelProvider(model);
                  const isRec = model.model === recommended;
                  const isPin = model.model === pinned;
                  const isExp = expandedModel === model.model;

                  return (
                    <ModelRow
                      key={model.model}
                      model={model}
                      capability={name!}
                      provider={provider}
                      stoneEntries={stoneEntries}
                      isRecommended={isRec}
                      isPinned={isPin}
                      isExpanded={isExp}
                      onToggleExpand={() => toggleExpand(model.model)}
                      onPin={() => pinModel(model.model)}
                    />
                  );
                })}
              </tbody>
            </table>
          </div>

          {/* Stone legend */}
          {stoneEntries.length > 0 && (
            <div className="px-4 py-2 border-t border-[#2e303a]/50 flex flex-wrap gap-3">
              {stoneEntries.map((se) => (
                <span
                  key={se.name}
                  className="flex items-center gap-1.5 text-[10px]"
                >
                  <span
                    className={`w-2.5 h-2.5 rounded-sm ${se.color.bg}`}
                  />
                  <span className={se.color.text}>{se.name}</span>
                </span>
              ))}
            </div>
          )}
        </div>
      )}

    </div>
  );
}

// ── Model Row ───────────────────────────────────────────────────

interface StoneEntry {
  name: string;
  color: { bg: string; border: string; text: string; hex: string };
}

interface ModelRowProps {
  model: ModelStatus;
  capability: string;
  provider: { name: string; cloud: boolean };
  stoneEntries: StoneEntry[];
  isRecommended: boolean;
  isPinned: boolean;
  isExpanded: boolean;
  onToggleExpand: () => void;
  onPin: () => void;
}

function ModelRow({
  model,
  capability,
  provider,
  stoneEntries,
  isRecommended,
  isPinned,
  isExpanded,
  onToggleExpand,
  onPin,
}: ModelRowProps) {
  const providerLink = provider.cloud
    ? `/infra/cloud/${provider.name}`
    : `/infra/services/${provider.name}`;

  return (
    <>
      <tr
        className={`text-gray-400 cursor-pointer transition-colors ${
          isPinned
            ? "bg-purple-500/5"
            : isRecommended
              ? "bg-emerald-500/5"
              : "hover:bg-[#22232d]"
        }`}
        onClick={onToggleExpand}
      >
        <td className="px-3 py-1.5 font-mono text-gray-200">
          {isPinned && (
            <span className="text-purple-400 mr-1" title="Pinned">
              P
            </span>
          )}
          {isRecommended && !isPinned && (
            <span className="text-emerald-400 mr-1" title="Recommended">
              *
            </span>
          )}
          {model.model}
        </td>
        <td className="px-3 py-1.5">
          <Link
            to={providerLink}
            onClick={(e) => e.stopPropagation()}
            className={`hover:underline font-mono ${
              provider.cloud ? "text-purple-400" : "text-gray-400"
            }`}
          >
            {provider.name}
            {provider.cloud && (
              <span className="ml-1 text-[10px] text-purple-500">cloud</span>
            )}
          </Link>
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.metadata.parameter_size
            ? `${model.metadata.parameter_size}${model.metadata.quantization_level ? ` ${model.metadata.quantization_level}` : ""}`
            : (provider.cloud ? "cloud" : "-")}
        </td>
        {/* Stone grid (local only) */}
        {stoneEntries.map((se) => {
          if (provider.cloud) {
            return (
              <td key={se.name} className="px-1 py-1.5 text-center">
                <span className="inline-block w-3 h-3" />
              </td>
            );
          }
          const placement = model.available_on.find(
            (p) => p.stone === se.name,
          );
          if (!placement) {
            return (
              <td key={se.name} className="px-1 py-1.5 text-center">
                <span
                  className="inline-block w-3 h-3 rounded-sm bg-gray-800"
                  title={`Not on ${se.name}`}
                />
              </td>
            );
          }
          return (
            <td key={se.name} className="px-1 py-1.5 text-center">
              <span
                className={`inline-block w-3 h-3 rounded-sm ${
                  placement.loaded
                    ? se.color.bg
                    : `${se.color.bg} opacity-30`
                }`}
                title={`${se.name}: ${placement.loaded ? "loaded" : "available"}`}
              />
            </td>
          );
        })}
        <td className="px-2 py-1.5 text-right">
          {!isPinned && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onPin();
              }}
              className="text-[10px] px-1.5 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-purple-500/20 hover:text-purple-300"
              title={`Pin ${model.model}`}
            >
              pin
            </button>
          )}
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td
            colSpan={4 + stoneEntries.length + 1}
            className="bg-[#16171f] px-6 py-3"
          >
            <div className="grid grid-cols-2 gap-4 text-[11px]">
              <div className="space-y-1">
                <DetailRow label="Family" value={model.metadata.family} />
                <DetailRow
                  label="Parameters"
                  value={model.metadata.parameter_size}
                />
                <DetailRow
                  label="Quantization"
                  value={model.metadata.quantization_level}
                />
                <DetailRow
                  label="Context"
                  value={
                    model.metadata.context_length
                      ? `${model.metadata.context_length.toLocaleString()} tokens`
                      : null
                  }
                />
                <DetailRow
                  label="VRAM"
                  value={
                    model.metadata.vram_bytes ? formatBytes(model.metadata.vram_bytes) : null
                  }
                />
                <DetailRow
                  label="Disk"
                  value={
                    model.metadata.size_disk > 0
                      ? formatBytes(model.metadata.size_disk)
                      : provider.cloud
                        ? "cloud"
                        : null
                  }
                />
                <DetailRow
                  label="Capabilities"
                  value={model.capabilities.join(", ")}
                />
              </div>
              <div className="space-y-1">
                <div className="text-gray-500 uppercase tracking-wider text-[10px] font-semibold">
                  Placement
                </div>
                {model.available_on.length > 0 ? (
                  model.available_on.map((p) => (
                    <div
                      key={`${p.stone}-${p.endpoint}`}
                      className="flex items-center gap-2"
                    >
                      <span
                        className={`w-2 h-2 rounded-sm ${stoneColor(p.stone).bg} ${
                          p.loaded ? "" : "opacity-30"
                        }`}
                      />
                      <span className="text-gray-400">{p.stone}</span>
                      <span className="text-gray-600 text-[10px]">
                        {p.loaded ? "loaded" : "available"}
                      </span>
                    </div>
                  ))
                ) : (
                  <span className="text-gray-600">Cloud-hosted</span>
                )}
                <div className="mt-2">
                  <Link
                    to={
                      provider.cloud
                        ? `/infra/cloud/${provider.name}`
                        : `/infra/services/${provider.name}`
                    }
                    className="text-[10px] text-blue-400 hover:underline"
                  >
                    View in {provider.name} &rarr;
                  </Link>
                </div>
              </div>
            </div>
            <ModelTryIt model={model.model} capability={capability} />
          </td>
        </tr>
      )}
    </>
  );
}

// ── Helpers ─────────────────────────────────────────────────────

function DetailRow({
  label,
  value,
}: {
  label: string;
  value: string | null | undefined;
}) {
  return (
    <div className="flex">
      <span className="text-gray-500 w-24 shrink-0">{label}</span>
      <span className="text-gray-300 font-mono">{value ?? "-"}</span>
    </div>
  );
}

// ── Skill Block ──────────────────────────────────────────────────

const SKILL_STATUS_STYLES: Record<string, { bg: string; label: string }> = {
  ready: { bg: "bg-emerald-400/10 text-emerald-400 border-emerald-400/30", label: "Ready" },
  degraded: { bg: "bg-yellow-400/10 text-yellow-400 border-yellow-400/30", label: "Degraded" },
  initializing: { bg: "bg-blue-400/10 text-blue-400 border-blue-400/30", label: "Initializing" },
  provisioning: { bg: "bg-blue-400/10 text-blue-400 border-blue-400/30", label: "Provisioning" },
  failed: { bg: "bg-red-400/10 text-red-400 border-red-400/30", label: "Failed" },
};

function SkillBlock({ skill }: { skill: SkillInfo }) {
  const [expanded, setExpanded] = useState(false);
  const statusStyle = SKILL_STATUS_STYLES[skill.status] ?? SKILL_STATUS_STYLES.failed;

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
      <button
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-[#1e1f28] transition-colors text-left"
        onClick={() => setExpanded((e) => !e)}
      >
        <div className="flex items-center gap-3">
          <span className="text-amber-400 text-sm">⚡</span>
          <div>
            <span className="text-sm font-medium text-gray-100">
              {skill.display_name}
            </span>
            <span className="ml-2 text-[11px] text-gray-500">
              {skill.description}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-3">
          {/* Stone availability squares */}
          {skill.stones && skill.stones.length > 0 && (
            <div className="flex items-center gap-1" title={skill.stones.map(s =>
              `${s.stone}: ${s.reason}${s.reason === "insufficient_vram" ? ` (${s.vram_mb}MB < ${skill.vram_mb}MB)` : ""}`
            ).join("\n")}>
              {skill.stones.map((s) => (
                <span
                  key={s.stone}
                  className={`w-2.5 h-2.5 rounded-sm ${
                    s.available ? "bg-emerald-400" : s.reason === "insufficient_vram" ? "bg-yellow-500" : "bg-gray-600"
                  }`}
                  title={`${s.stone}: ${s.reason}`}
                />
              ))}
            </div>
          )}
          <span
            className={`text-[10px] font-semibold px-2 py-0.5 rounded border ${statusStyle.bg}`}
          >
            {statusStyle.label}
          </span>
          <span className="text-gray-600 text-xs">{expanded ? "▲" : "▼"}</span>
        </div>
      </button>

      {expanded && (
        <div className="px-4 pb-4 border-t border-[#2e303a]">
          {skill.status === "ready" || skill.status === "degraded" ? (
            <div className="pt-3">
              <SkillTryIt skillName={skill.name} />
            </div>
          ) : (
            <div className="pt-3 text-xs text-gray-500">
              {skill.status === "initializing" && "Downloading required models..."}
              {skill.status === "provisioning" && "Deploying models to instances..."}
              {skill.status === "failed" && (
                <span className="text-red-400">
                  Provisioning failed. Check orchestrator logs.
                </span>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function StateBadge({ state }: { state: string }) {
  const styles: Record<string, string> = {
    active: "bg-emerald-400/10 text-emerald-400 border-emerald-400/30",
    needs_setup: "bg-yellow-400/10 text-yellow-400 border-yellow-400/30",
    not_installed: "bg-gray-600/10 text-gray-500 border-gray-600/30",
    degraded: "bg-red-400/10 text-red-400 border-red-400/30",
  };

  return (
    <span
      className={`text-[10px] font-semibold px-2 py-0.5 rounded border ${
        styles[state] ?? styles.not_installed
      }`}
    >
      {state.replace("_", " ")}
    </span>
  );
}
