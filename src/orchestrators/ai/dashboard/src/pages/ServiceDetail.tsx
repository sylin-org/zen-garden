import { useState, useCallback } from "react";
import { useParams, Link } from "react-router-dom";
import type {
  DashboardStatus,
  ModelStatus,
  InstanceStatus,
} from "../types";
import { formatBytes } from "../types";
import { stoneColor } from "../utils/stoneColors";
import { isCloudOffering, CAP_COLORS } from "../utils/cloudCatalog";
import { ModelTryIt } from "../components/ModelTryIt";

interface ServiceDetailProps {
  status: DashboardStatus;
}

export function ServiceDetail({ status }: ServiceDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  // Find instances for this service kind
  const instances = status.instances.filter(
    (i) => i.kind === name && !isCloudOffering(i.kind),
  );

  if (instances.length === 0) {
    return (
      <div className="p-6">
        <p className="text-gray-400">
          Service &quot;{name}&quot; not found.
        </p>
        <Link
          to="/infra/services"
          className="text-blue-400 text-sm hover:underline"
        >
          Back to services
        </Link>
      </div>
    );
  }

  // All models served by this offering (check placement offering field)
  const allModels = status.models.filter((m) =>
    m.available_on.some((p) => p.offering === name),
  );

  const models = searchQuery
    ? allModels.filter((m) =>
        m.model.toLowerCase().includes(searchQuery.toLowerCase()),
      )
    : allModels;

  // Unique stones
  const stoneNames = [...new Set(instances.map((i) => i.stone_name))];
  const stoneEntries = stoneNames.map((sn) => ({
    name: sn,
    color: stoneColor(sn),
    instance: instances.find((i) => i.stone_name === sn),
  }));

  return (
    <div className="space-y-5 max-w-6xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <Link
            to="/infra/services"
            className="text-gray-500 hover:text-gray-300 text-sm"
          >
            Services
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium capitalize">
            {name}
          </span>
        </div>
        <h2 className="text-lg font-medium text-gray-100 capitalize">
          {name}
        </h2>
      </div>

      {/* Instance cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        {instances.map((inst) => (
          <InstanceCard key={inst.endpoint} inst={inst} />
        ))}
      </div>

      {/* Action bar */}
      {name && (
        <ServiceActions
          offering={name}
          instances={instances}
          models={models}
        />
      )}

      {/* Full model table */}
      {allModels.length > 0 && (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
          <div className="px-4 py-2 border-b border-[#2e303a] flex items-center justify-between gap-3">
            <div>
              <span className="text-sm font-medium text-gray-200">
                All Models
              </span>
              <span className="ml-2 text-[11px] text-gray-500">
                {models.length}{searchQuery ? ` / ${allModels.length}` : ""} total
              </span>
            </div>
            {allModels.length > 5 && (
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Filter models..."
                className="bg-[#0f1117] border border-[#2e303a] rounded px-3 py-1 text-[12px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500/50 w-56"
              />
            )}
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-[#2e303a] text-gray-500 text-left">
                  <th className="px-3 py-1.5 font-medium">Model</th>
                  <th className="px-3 py-1.5 font-medium">Capabilities</th>
                  <th className="px-3 py-1.5 font-medium">Params</th>
                  <th className="px-3 py-1.5 font-medium">Quant</th>
                  <th className="px-3 py-1.5 font-medium">Size</th>
                  <th className="px-3 py-1.5 font-medium">VRAM</th>
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
                  <th className="px-2 py-1.5 font-medium">Status</th>
                  <th className="px-2 py-1.5 font-medium">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#2e303a]/50">
                {models.map((model) => {
                  const isExp = expanded === model.model;
                  const loaded = model.available_on.some((p) => p.loaded);

                  return (
                    <ServiceModelRow
                      key={model.model}
                      model={model}
                      stoneEntries={stoneEntries}
                      isExpanded={isExp}
                      loaded={loaded}
                      offering={name ?? ""}
                      onToggleExpand={() =>
                        setExpanded(isExp ? null : model.model)
                      }
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
                  {se.instance && se.instance.vram_total_mb > 0 && (
                    <span className="text-gray-600">
                      {Math.round(se.instance.vram_total_mb / 1024)}GB
                    </span>
                  )}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Service Actions Bar ────────────────────────────────────────

interface ServiceActionsProps {
  offering: string;
  instances: InstanceStatus[];
  models: ModelStatus[];
}

type ActionState = "idle" | "loading" | "success" | "error";

function ServiceActions({ offering, instances, models }: ServiceActionsProps) {
  const [refreshState, setRefreshState] = useState<ActionState>("idle");
  const [refreshResult, setRefreshResult] = useState<string | null>(null);

  const [pullModel, setPullModel] = useState("");
  const [pullState, setPullState] = useState<ActionState>("idle");
  const [pullResult, setPullResult] = useState<string | null>(null);
  const [showPullInput, setShowPullInput] = useState(false);

  const targets = instances.map((i) => i.endpoint);

  const handleRefresh = useCallback(async () => {
    setRefreshState("loading");
    setRefreshResult(null);
    try {
      const res = await fetch(`/api/services/${offering}/refresh`, {
        method: "POST",
      });
      const data = await res.json();
      if (res.ok) {
        setRefreshState("success");
        setRefreshResult(`${data.models} models found`);
      } else {
        setRefreshState("error");
        setRefreshResult(data.message ?? "refresh failed");
      }
    } catch {
      setRefreshState("error");
      setRefreshResult("network error");
    }
    setTimeout(() => {
      setRefreshState("idle");
      setRefreshResult(null);
    }, 3000);
  }, [offering]);

  const handlePull = useCallback(async () => {
    if (!pullModel.trim()) return;
    setPullState("loading");
    setPullResult(null);
    try {
      const res = await fetch(`/api/services/${offering}/pull`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ model: pullModel.trim(), targets }),
      });
      const data = await res.json();
      if (res.ok) {
        setPullState("success");
        setPullResult(`Job ${data.job_id} queued`);
        setPullModel("");
        setShowPullInput(false);
      } else {
        setPullState("error");
        setPullResult(data.message ?? "pull failed");
      }
    } catch {
      setPullState("error");
      setPullResult("network error");
    }
    setTimeout(() => {
      setPullState("idle");
      setPullResult(null);
    }, 5000);
  }, [offering, pullModel, targets]);

  // Suppress unused-variable warning: models is available for future sync/benchmark
  void models;

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 flex-wrap">
        <ActionButton
          label="Refresh"
          state={refreshState}
          onClick={handleRefresh}
        />
        <ActionButton
          label="Pull Model"
          state={showPullInput ? "idle" : pullState}
          onClick={() => setShowPullInput((s) => !s)}
        />
        <ActionButton
          label="Sync Models"
          state="idle"
          onClick={async () => {
            await fetch(`/api/services/${offering}/sync`, { method: "POST" });
          }}
          title="Coming soon"
        />
        <ActionButton
          label="Run Benchmark"
          state="idle"
          onClick={async () => {
            await fetch(`/api/services/${offering}/benchmark`, {
              method: "POST",
            });
          }}
          title="Coming soon"
        />
      </div>

      {/* Pull model input row */}
      {showPullInput && (
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={pullModel}
            onChange={(e) => setPullModel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handlePull();
            }}
            placeholder="Model name (e.g. nomic-embed-text)"
            className="text-[12px] px-2 py-1.5 rounded border border-[#2e303a] bg-[#12131a] text-gray-200 placeholder-gray-600 focus:outline-none focus:border-blue-500 w-72"
            autoFocus
          />
          <button
            onClick={handlePull}
            disabled={pullState === "loading" || !pullModel.trim()}
            className="text-[11px] px-3 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-40 transition-colors"
          >
            {pullState === "loading" ? "Pulling..." : "Pull"}
          </button>
          <button
            onClick={() => {
              setShowPullInput(false);
              setPullModel("");
            }}
            className="text-[11px] px-2 py-1.5 text-gray-500 hover:text-gray-300"
          >
            Cancel
          </button>
        </div>
      )}

      {/* Result messages */}
      {refreshResult && (
        <StatusMessage state={refreshState} message={refreshResult} />
      )}
      {pullResult && (
        <StatusMessage state={pullState} message={pullResult} />
      )}
    </div>
  );
}

// ── Instance Card ───────────────────────────────────────────────

function InstanceCard({ inst }: { inst: InstanceStatus }) {
  const total = inst.vram_total_mb || 1;
  const budget = inst.vram_budget_mb;
  const pct = Math.min((budget / total) * 100, 100);

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full ${
              inst.health === "healthy" ? "bg-emerald-400" : "bg-red-400"
            }`}
          />
          <span className="text-sm text-gray-200">{inst.stone_name}</span>
        </div>
        <span className="text-[10px] text-gray-500 font-mono">
          {inst.health}
        </span>
      </div>
      {inst.gpu && (
        <div className="text-[11px] text-gray-500 mb-2">{inst.gpu}</div>
      )}
      {inst.vram_total_mb > 0 && (
        <div className="mb-1">
          <div className="flex justify-between text-[10px] text-gray-500 mb-0.5">
            <span>VRAM Budget</span>
            <span className="font-mono">
              {(budget / 1024).toFixed(1)} / {(total / 1024).toFixed(1)} GB
            </span>
          </div>
          <div className="w-full h-1.5 bg-[#2e303a] rounded-full overflow-hidden">
            <div
              className="h-full bg-emerald-500/70 rounded-full"
              style={{ width: `${pct}%` }}
            />
          </div>
        </div>
      )}
      <div className="text-[11px] text-gray-500 font-mono">
        {inst.models_loaded.length} loaded / {inst.models_available.length}{" "}
        available
      </div>
    </div>
  );
}

// ── Service Model Row ───────────────────────────────────────────

interface ServiceStoneEntry {
  name: string;
  color: { bg: string; border: string; text: string; hex: string };
  instance: InstanceStatus | undefined;
}

interface ServiceModelRowProps {
  model: ModelStatus;
  stoneEntries: ServiceStoneEntry[];
  isExpanded: boolean;
  loaded: boolean;
  offering: string;
  onToggleExpand: () => void;
}

function ServiceModelRow({
  model,
  stoneEntries,
  isExpanded,
  loaded,
  offering,
  onToggleExpand,
}: ServiceModelRowProps) {
  const [actionState, setActionState] = useState<ActionState>("idle");

  const handleLoadUnload = useCallback(
    async (action: "load" | "unload", endpoint: string) => {
      setActionState("loading");
      try {
        const res = await fetch(`/api/services/${offering}/${action}`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ model: model.model, endpoint }),
        });
        if (res.ok) {
          setActionState("success");
        } else {
          setActionState("error");
        }
      } catch {
        setActionState("error");
      }
      setTimeout(() => setActionState("idle"), 3000);
    },
    [offering, model.model],
  );

  const handleDelete = useCallback(async () => {
    if (!confirm(`Delete ${model.model} from all instances?`)) return;
    setActionState("loading");
    try {
      const res = await fetch(
        `/api/services/${offering}/models/${encodeURIComponent(model.model)}`,
        { method: "DELETE" },
      );
      if (res.ok) {
        setActionState("success");
      } else {
        setActionState("error");
      }
    } catch {
      setActionState("error");
    }
    setTimeout(() => setActionState("idle"), 3000);
  }, [offering, model.model]);

  return (
    <>
      <tr
        className="text-gray-400 cursor-pointer hover:bg-[#22232d] transition-colors"
        onClick={onToggleExpand}
      >
        <td className="px-3 py-1.5 font-mono text-gray-200">{model.model}</td>
        <td className="px-3 py-1.5">
          <div className="flex flex-wrap gap-1">
            {model.capabilities.map((cap) => (
              <Link
                key={cap}
                to={`/capability/${cap}`}
                onClick={(e) => e.stopPropagation()}
                className={`text-[10px] px-1 py-0.5 rounded font-mono hover:opacity-80 ${
                  CAP_COLORS[cap] ?? "bg-gray-700 text-gray-400"
                }`}
              >
                {cap}
              </Link>
            ))}
          </div>
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.metadata.parameter_size ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.metadata.quantization_level ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.metadata.size_disk > 0 ? formatBytes(model.metadata.size_disk) : "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.metadata.vram_bytes ? formatBytes(model.metadata.vram_bytes) : "-"}
        </td>
        {stoneEntries.map((se) => {
          const placement = model.available_on.find(
            (p) => p.stone === se.name,
          );
          if (!placement) {
            return (
              <td key={se.name} className="px-1 py-1.5 text-center">
                <span className="inline-block w-3 h-3 rounded-sm bg-gray-800" />
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
              />
            </td>
          );
        })}
        <td className="px-2 py-1.5 text-right">
          <span
            className={`text-[10px] font-mono ${
              loaded ? "text-emerald-400" : "text-gray-600"
            }`}
          >
            {loaded ? "loaded" : "idle"}
          </span>
        </td>
        <td className="px-2 py-1.5" onClick={(e) => e.stopPropagation()}>
          {actionState === "loading" ? (
            <span className="text-[10px] text-yellow-400">...</span>
          ) : (
            <div className="flex gap-1">
              {loaded ? (
                <ModelActionBtn
                  label="Unload"
                  onClick={() => {
                    const ep = model.available_on.find((p) => p.loaded)?.endpoint;
                    if (ep) handleLoadUnload("unload", ep);
                  }}
                />
              ) : (
                <ModelActionBtn
                  label="Load"
                  onClick={() => {
                    const ep = model.available_on[0]?.endpoint;
                    if (ep) handleLoadUnload("load", ep);
                  }}
                />
              )}
              <ModelActionBtn label="Del" onClick={handleDelete} danger />
            </div>
          )}
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td
            colSpan={7 + stoneEntries.length + 2}
            className="bg-[#16171f] px-6 py-3"
          >
            <div className="grid grid-cols-2 gap-4 text-[11px]">
              <div className="space-y-1">
                <DetailRow label="Family" value={model.metadata.family} />
                <DetailRow label="Parameters" value={model.metadata.parameter_size} />
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
                    model.metadata.size_disk > 0 ? formatBytes(model.metadata.size_disk) : null
                  }
                />
              </div>
              <div className="space-y-1">
                <div className="text-gray-500 uppercase tracking-wider text-[10px] font-semibold">
                  Placement
                </div>
                {model.available_on.map((p) => (
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
                    <button
                      onClick={() => {
                        const action = p.loaded ? "unload" : "load";
                        fetch(`/api/services/${offering}/${action}`, {
                          method: "POST",
                          headers: { "Content-Type": "application/json" },
                          body: JSON.stringify({
                            model: model.model,
                            endpoint: p.endpoint,
                          }),
                        });
                      }}
                      className="text-[9px] text-blue-400 hover:text-blue-300 ml-1"
                    >
                      [{p.loaded ? "unload" : "load"}]
                    </button>
                  </div>
                ))}
              </div>
            </div>
            {model.capabilities.length > 0 && (
              <ServiceModelTryIt model={model.model} capabilities={model.capabilities} />
            )}
          </td>
        </tr>
      )}
    </>
  );
}

// ── Service Model TryIt (with capability selector) ─────────────

function ServiceModelTryIt({
  model,
  capabilities,
}: {
  model: string;
  capabilities: string[];
}) {
  const [selectedCap, setSelectedCap] = useState(capabilities[0]);

  return (
    <div className="mt-3 border-t border-[#2e303a]/50 pt-3">
      {capabilities.length > 1 && (
        <div className="flex items-center gap-2 mb-2">
          <span className="text-[10px] text-gray-500 uppercase tracking-wider">
            Capability
          </span>
          <select
            value={selectedCap}
            onChange={(e) => setSelectedCap(e.target.value)}
            className="bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[11px] text-gray-300 focus:outline-none focus:border-blue-500/50"
          >
            {capabilities.map((cap) => (
              <option key={cap} value={cap}>
                {cap}
              </option>
            ))}
          </select>
        </div>
      )}
      <ModelTryIt model={model} capability={selectedCap} />
    </div>
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

interface ActionButtonProps {
  label: string;
  state: ActionState;
  onClick: () => void;
  title?: string;
}

function ActionButton({ label, state, onClick, title }: ActionButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={state === "loading"}
      title={title}
      className={`text-[11px] px-3 py-1.5 rounded border transition-colors ${
        state === "loading"
          ? "border-yellow-600 text-yellow-400 cursor-wait"
          : state === "success"
            ? "border-emerald-600 text-emerald-400"
            : state === "error"
              ? "border-red-600 text-red-400"
              : "border-[#2e303a] text-gray-400 hover:text-gray-200 hover:border-gray-500"
      }`}
    >
      {state === "loading" ? `${label}...` : label}
    </button>
  );
}

function ModelActionBtn({
  label,
  onClick,
  danger,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`text-[9px] px-1.5 py-0.5 rounded border transition-colors ${
        danger
          ? "border-red-800 text-red-500 hover:text-red-400 hover:border-red-600"
          : "border-[#2e303a] text-gray-500 hover:text-gray-300 hover:border-gray-500"
      }`}
    >
      {label}
    </button>
  );
}

function StatusMessage({
  state,
  message,
}: {
  state: ActionState;
  message: string;
}) {
  const color =
    state === "success"
      ? "text-emerald-400"
      : state === "error"
        ? "text-red-400"
        : "text-gray-400";
  return <div className={`text-[11px] ${color}`}>{message}</div>;
}
