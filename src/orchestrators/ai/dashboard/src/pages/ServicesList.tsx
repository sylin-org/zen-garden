import { Link } from "react-router-dom";
import type { DashboardStatus, InstanceStatus } from "../types";
import { SERVICE_CATALOG } from "../utils/serviceCatalog";
import { CAP_COLORS } from "../utils/cloudCatalog";
import { isCloudOffering } from "../utils/cloudCatalog";

interface ServicesListProps {
  status: DashboardStatus;
}

interface ServiceGroup {
  kind: string;
  instances: InstanceStatus[];
  modelCount: number;
  loadedCount: number;
  stones: string[];
  healthy: boolean;
  gpu: string | null;
}

function groupLocalServices(status: DashboardStatus): Map<string, ServiceGroup> {
  const groups = new Map<string, ServiceGroup>();
  for (const inst of status.instances) {
    if (isCloudOffering(inst.kind)) continue;

    const existing = groups.get(inst.kind);
    if (existing) {
      existing.instances.push(inst);
      if (!existing.stones.includes(inst.stone_name)) {
        existing.stones.push(inst.stone_name);
      }
      if (inst.gpu && !existing.gpu) existing.gpu = inst.gpu;
      if (inst.health !== "healthy") existing.healthy = false;
    } else {
      groups.set(inst.kind, {
        kind: inst.kind,
        instances: [inst],
        modelCount: inst.models_available.length,
        loadedCount: inst.models_loaded.length,
        stones: [inst.stone_name],
        healthy: inst.health === "healthy",
        gpu: inst.gpu,
      });
    }
  }

  // Aggregate model counts across instances
  for (const group of groups.values()) {
    const allModels = new Set<string>();
    const loaded = new Set<string>();
    for (const inst of group.instances) {
      for (const m of inst.models_available) allModels.add(m);
      for (const m of inst.models_loaded) loaded.add(m);
    }
    group.modelCount = allModels.size;
    group.loadedCount = loaded.size;
  }

  return groups;
}

export function ServicesList({ status }: ServicesListProps) {
  const installed = groupLocalServices(status);
  const installedKinds = new Set(installed.keys());

  // Summary
  const totalInstances = [...installed.values()].reduce(
    (sum, g) => sum + g.instances.length,
    0,
  );
  const totalStones = new Set(
    [...installed.values()].flatMap((g) => g.stones),
  ).size;

  return (
    <div className="space-y-6 max-w-5xl">
      {/* Header */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">Services</span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">
          Local AI Services
        </h2>
        <p className="text-[12px] text-gray-500">
          {installed.size > 0
            ? `${installed.size} service${installed.size !== 1 ? "s" : ""} running on ${totalStones} stone${totalStones !== 1 ? "s" : ""} (${totalInstances} instance${totalInstances !== 1 ? "s" : ""})`
            : "No local AI services detected"}
        </p>
      </div>

      {/* All services — installed and available */}
      <div className="space-y-3">
        {SERVICE_CATALOG.map((catalog) => {
          const group = installed.get(catalog.id);
          const isInstalled = !!group;

          return (
            <div
              key={catalog.id}
              className={`rounded-lg border p-4 ${
                isInstalled
                  ? "border-emerald-500/30 bg-[#1a1b23]"
                  : "border-[#2e303a] bg-[#1a1b23]/50"
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  {/* Service name + status */}
                  <div className="flex items-center gap-2 mb-1">
                    {isInstalled && (
                      <span
                        className={`w-2 h-2 rounded-full ${
                          group.healthy ? "bg-emerald-400" : "bg-red-400"
                        }`}
                      />
                    )}
                    <h3 className="text-sm font-semibold text-gray-100">
                      {catalog.name}
                    </h3>
                    {isInstalled ? (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-300 font-mono">
                        {group.instances.length} instance
                        {group.instances.length !== 1 ? "s" : ""}
                      </span>
                    ) : (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-400">
                        not installed
                      </span>
                    )}
                    {catalog.gpuRequired && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-500/10 text-yellow-400/70">
                        GPU required
                      </span>
                    )}
                  </div>

                  {/* Description */}
                  <p className="text-[11px] text-gray-500 mb-2">
                    {catalog.description}
                  </p>

                  {/* Capabilities */}
                  <div className="flex flex-wrap gap-1 mb-2">
                    {catalog.capabilities.map((cap) => (
                      <span
                        key={cap}
                        className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                          CAP_COLORS[cap] ?? "bg-gray-700 text-gray-400"
                        }`}
                      >
                        {cap}
                      </span>
                    ))}
                  </div>

                  {/* Instance details when installed */}
                  {isInstalled && (
                    <div className="flex items-center gap-4 text-[11px] text-gray-500">
                      <span>
                        Stones: {group.stones.join(", ")}
                      </span>
                      <span className="font-mono">
                        {group.modelCount} model
                        {group.modelCount !== 1 ? "s" : ""}
                      </span>
                      {group.loadedCount > 0 && (
                        <span className="font-mono text-emerald-400/70">
                          {group.loadedCount} loaded
                        </span>
                      )}
                      {group.gpu && (
                        <span className="text-gray-600">{group.gpu}</span>
                      )}
                    </div>
                  )}
                </div>

                {/* Actions */}
                <div className="ml-4 flex flex-col gap-2">
                  {isInstalled ? (
                    <Link
                      to={`/infra/services/${catalog.id}`}
                      className="text-xs px-3 py-1.5 rounded bg-emerald-500/20 text-emerald-300 hover:bg-emerald-500/30 text-center"
                    >
                      Manage
                    </Link>
                  ) : (
                    <a
                      href={catalog.docsUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-xs px-3 py-1.5 rounded bg-[#2e303a] text-gray-400 hover:bg-[#3e404a] text-center"
                    >
                      Learn more
                    </a>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Docker hint for uninstalled services */}
      {installedKinds.size < SERVICE_CATALOG.length && (
        <div className="text-[11px] text-gray-600 bg-[#1a1b23]/30 border border-[#2e303a]/50 rounded px-4 py-3">
          To install a service, use{" "}
          <code className="text-gray-400">rake plant &lt;offering&gt;</code> or
          deploy via the Zen Garden dashboard on any stone.
        </div>
      )}
    </div>
  );
}
