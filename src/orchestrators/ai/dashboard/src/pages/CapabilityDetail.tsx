import { useParams, Link } from "react-router-dom";
import type { DashboardStatus, ModelStatus, InstanceStatus } from "../types";
import { CAPABILITY_LABELS, formatBytes } from "../types";

interface CapabilityDetailProps {
  status: DashboardStatus;
}

export function CapabilityDetail({ status }: CapabilityDetailProps) {
  const { name } = useParams<{ name: string }>();
  const cap = status.capabilities.find((c) => c.capability === name);
  const label = CAPABILITY_LABELS[name ?? ""] ?? name ?? "Unknown";

  if (!cap) {
    return (
      <div className="p-6">
        <p className="text-gray-400">
          Capability &quot;{name}&quot; not found.
        </p>
        <Link to="/" className="text-blue-400 text-sm hover:underline">
          Back to overview
        </Link>
      </div>
    );
  }

  // Models that serve this capability
  const models: ModelStatus[] = status.models.filter((m) =>
    m.capabilities.includes(cap.capability),
  );

  // Instances that serve this capability
  const instances: InstanceStatus[] = status.instances.filter((i) =>
    i.capabilities.includes(cap.capability),
  );

  // Group instances by offering kind
  const offeringGroups: Record<string, InstanceStatus[]> = {};
  for (const inst of instances) {
    const group = offeringGroups[inst.kind] ?? [];
    group.push(inst);
    offeringGroups[inst.kind] = group;
  }

  const recommended = status.recommendations[cap.capability];

  return (
    <div className="space-y-6">
      {/* Header */}
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
        </div>
        {recommended && (
          <p className="text-[12px] text-gray-500 mt-1">
            Recommended:{" "}
            <span className="text-gray-300 font-mono">{recommended}</span>
          </p>
        )}
      </div>

      {/* Not Installed guidance */}
      {cap.state === "not_installed" && (
        <div className="bg-[#1a1b23] border border-gray-700 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-2">
            No service is installed that can serve{" "}
            <span className="text-gray-200">{label}</span>.
          </p>
          <p className="text-[12px] text-gray-500">
            Install an offering that supports this capability to enable it. The
            orchestrator will automatically discover and profile new instances.
          </p>
        </div>
      )}

      {/* Needs Setup guidance */}
      {cap.state === "needs_setup" && (
        <div className="bg-[#1a1b23] border border-yellow-500/40 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-2">
            {cap.offerings.join(", ")} {cap.offerings.length === 1 ? "is" : "are"}{" "}
            installed, but no models for{" "}
            <span className="text-gray-200">{label}</span> are available.
          </p>
          <p className="text-[12px] text-yellow-400/80">
            Pull a model with this capability to start serving requests.
          </p>
        </div>
      )}

      {/* Model Ranking Table */}
      {models.length > 0 && (
        <section>
          <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold mb-3">
            Models ({models.length})
          </h3>
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-[#2e303a] text-gray-500 text-left">
                  <th className="px-3 py-2 font-medium w-8"></th>
                  <th className="px-3 py-2 font-medium">Model</th>
                  <th className="px-3 py-2 font-medium">Offering</th>
                  <th className="px-3 py-2 font-medium">Parameters</th>
                  <th className="px-3 py-2 font-medium">Quant</th>
                  <th className="px-3 py-2 font-medium">Size</th>
                  <th className="px-3 py-2 font-medium">Loaded</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#2e303a]">
                {models.map((model) => {
                  const isRecommended = model.name === recommended;
                  const placements = model.available_on;
                  const offerings = [
                    ...new Set(placements.map((p) => p.offering)),
                  ];
                  const loaded = placements.some((p) => p.loaded);

                  return (
                    <tr
                      key={model.name}
                      className="text-gray-400 hover:bg-[#22232d]"
                    >
                      <td className="px-3 py-1.5 text-center">
                        {isRecommended && (
                          <span className="text-yellow-400" title="Recommended">
                            *
                          </span>
                        )}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-gray-200">
                        {model.name}
                      </td>
                      <td className="px-3 py-1.5">
                        {offerings.map((o) => (
                          <span
                            key={o}
                            className={`inline-block mr-1 ${
                              o.toLowerCase().includes("cloud") ||
                              o.toLowerCase().includes("anthropic") ||
                              o.toLowerCase().includes("openai")
                                ? "text-purple-400"
                                : "text-gray-400"
                            }`}
                          >
                            {o}
                          </span>
                        ))}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-gray-500">
                        {model.parameter_size ?? "-"}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-gray-500">
                        {model.quantization_level ?? "-"}
                      </td>
                      <td className="px-3 py-1.5 font-mono text-gray-500">
                        {model.size_disk > 0
                          ? formatBytes(model.size_disk)
                          : "-"}
                      </td>
                      <td className="px-3 py-1.5">
                        {loaded ? (
                          <span className="text-emerald-400">loaded</span>
                        ) : (
                          <span className="text-gray-600">available</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Offering Operations */}
      {Object.keys(offeringGroups).length > 0 && (
        <section>
          <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold mb-3">
            Offerings
          </h3>
          <div className="space-y-3">
            {Object.entries(offeringGroups).map(([kind, insts]) => (
              <div
                key={kind}
                className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-3"
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm text-gray-200 font-medium capitalize">
                    {kind}
                  </span>
                  <span className="text-[11px] text-gray-500">
                    {insts.length} instance{insts.length !== 1 ? "s" : ""}
                  </span>
                </div>
                <div className="space-y-2">
                  {insts.map((inst) => (
                    <div
                      key={inst.endpoint}
                      className="flex items-center gap-3 text-[12px]"
                    >
                      <span
                        className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                          inst.health === "healthy"
                            ? "bg-emerald-400"
                            : "bg-red-400"
                        }`}
                      />
                      <span className="text-gray-400 font-mono">
                        {inst.stone_name}
                      </span>
                      <span className="text-gray-600 font-mono">
                        {inst.endpoint}
                      </span>
                      {inst.gpu && (
                        <span className="text-gray-500">{inst.gpu}</span>
                      )}
                      <span className="ml-auto text-gray-500">
                        {inst.models_loaded.length} loaded /{" "}
                        {inst.models_available.length} available
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

// ── Helpers ─────────────────────────────────────────────────────

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
