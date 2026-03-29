import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import type {
  DashboardStatus,
  ModelStatus,
  InstanceStatus,
} from "../types";
import { formatBytes } from "../types";
import { stoneColor } from "../utils/stoneColors";
import { isCloudOffering, CAP_COLORS } from "../utils/cloudCatalog";

interface ServiceDetailProps {
  status: DashboardStatus;
}

export function ServiceDetail({ status }: ServiceDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [expanded, setExpanded] = useState<string | null>(null);

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
  const models = status.models.filter((m) =>
    m.available_on.some((p) => p.offering === name),
  );

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

      {/* Action buttons placeholder */}
      <div className="flex gap-2">
        <ActionButton label="Pull Model" />
        <ActionButton label="Sync Models" />
        <ActionButton label="Run Benchmark" />
        <ActionButton label="Refresh" />
      </div>

      {/* Full model table */}
      {models.length > 0 && (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
          <div className="px-4 py-2 border-b border-[#2e303a]">
            <span className="text-sm font-medium text-gray-200">
              All Models
            </span>
            <span className="ml-2 text-[11px] text-gray-500">
              {models.length} total
            </span>
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
                </tr>
              </thead>
              <tbody className="divide-y divide-[#2e303a]/50">
                {models.map((model) => {
                  const isExp = expanded === model.name;
                  const loaded = model.available_on.some((p) => p.loaded);

                  return (
                    <ServiceModelRow
                      key={model.name}
                      model={model}
                      stoneEntries={stoneEntries}
                      isExpanded={isExp}
                      loaded={loaded}
                      onToggleExpand={() =>
                        setExpanded(isExp ? null : model.name)
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
  onToggleExpand: () => void;
}

function ServiceModelRow({
  model,
  stoneEntries,
  isExpanded,
  loaded,
  onToggleExpand,
}: ServiceModelRowProps) {
  return (
    <>
      <tr
        className="text-gray-400 cursor-pointer hover:bg-[#22232d] transition-colors"
        onClick={onToggleExpand}
      >
        <td className="px-3 py-1.5 font-mono text-gray-200">{model.name}</td>
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
          {model.parameter_size ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.quantization_level ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.size_disk > 0 ? formatBytes(model.size_disk) : "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.vram_bytes ? formatBytes(model.vram_bytes) : "-"}
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
      </tr>
      {isExpanded && (
        <tr>
          <td
            colSpan={7 + stoneEntries.length + 1}
            className="bg-[#16171f] px-6 py-3"
          >
            <div className="grid grid-cols-2 gap-4 text-[11px]">
              <div className="space-y-1">
                <DetailRow label="Family" value={model.family} />
                <DetailRow label="Parameters" value={model.parameter_size} />
                <DetailRow
                  label="Quantization"
                  value={model.quantization_level}
                />
                <DetailRow
                  label="Context"
                  value={
                    model.context_length
                      ? `${model.context_length.toLocaleString()} tokens`
                      : null
                  }
                />
                <DetailRow
                  label="VRAM"
                  value={
                    model.vram_bytes ? formatBytes(model.vram_bytes) : null
                  }
                />
                <DetailRow
                  label="Disk"
                  value={
                    model.size_disk > 0 ? formatBytes(model.size_disk) : null
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
                  </div>
                ))}
              </div>
            </div>
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

function ActionButton({ label }: { label: string }) {
  return (
    <button
      className="text-[11px] px-3 py-1.5 rounded border border-[#2e303a] text-gray-400 hover:text-gray-200 hover:border-gray-500 transition-colors"
      title="Coming soon"
    >
      {label}
    </button>
  );
}
