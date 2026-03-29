import { Link } from "react-router-dom";
import type { DashboardStatus, InstanceStatus } from "../types";
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

function groupLocalServices(status: DashboardStatus): ServiceGroup[] {
  const groups = new Map<string, InstanceStatus[]>();
  for (const inst of status.instances) {
    if (isCloudOffering(inst.kind)) continue;
    const list = groups.get(inst.kind) ?? [];
    list.push(inst);
    groups.set(inst.kind, list);
  }

  return [...groups.entries()].map(([kind, instances]) => {
    const stones = [...new Set(instances.map((i) => i.stone_name))];
    const allModels = new Set<string>();
    const loadedModels = new Set<string>();
    for (const inst of instances) {
      for (const m of inst.models_available) allModels.add(m);
      for (const m of inst.models_loaded) loadedModels.add(m);
    }
    const healthy = instances.every((i) => i.health === "healthy");
    const gpu = instances.find((i) => i.gpu)?.gpu ?? null;

    return {
      kind,
      instances,
      modelCount: allModels.size,
      loadedCount: loadedModels.size,
      stones,
      healthy,
      gpu,
    };
  });
}

function ServiceCard({ group }: { group: ServiceGroup }) {
  return (
    <Link
      to={`/infra/services/${group.kind}`}
      className="block bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4 hover:bg-[#22232d] transition-colors"
    >
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <span
            className={`w-2 h-2 rounded-full ${
              group.healthy ? "bg-emerald-400" : "bg-red-400"
            }`}
          />
          <span className="text-sm font-medium text-gray-100 capitalize">
            {group.kind}
          </span>
        </div>
        <span className="text-[10px] text-gray-500 font-mono">
          {group.instances.length} instance{group.instances.length !== 1 ? "s" : ""}
        </span>
      </div>

      <div className="space-y-1 text-[12px]">
        <div className="flex items-center gap-2 text-gray-400">
          <span>
            Stones: {group.stones.join(", ")}
          </span>
        </div>
        <div className="flex items-center gap-3 text-gray-500">
          <span className="font-mono">
            {group.modelCount} model{group.modelCount !== 1 ? "s" : ""}
          </span>
          {group.loadedCount > 0 && (
            <span className="font-mono text-emerald-400/70">
              {group.loadedCount} loaded
            </span>
          )}
        </div>
        {group.gpu && (
          <div className="text-[11px] text-gray-500">{group.gpu}</div>
        )}
      </div>
    </Link>
  );
}

export function ServicesList({ status }: ServicesListProps) {
  const services = groupLocalServices(status);

  return (
    <div className="space-y-6 max-w-5xl">
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">Services</span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">Local Services</h2>
        <p className="text-[12px] text-gray-500">
          {services.length} service{services.length !== 1 ? "s" : ""} installed
        </p>
      </div>

      {services.length === 0 ? (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-6 text-center">
          <p className="text-sm text-gray-500">
            No local AI services detected. Install an offering via Zen Garden.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {services.map((group) => (
            <ServiceCard key={group.kind} group={group} />
          ))}
        </div>
      )}
    </div>
  );
}
