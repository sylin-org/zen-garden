import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import type { DashboardStatus, ModelStatus, InstanceStatus } from "../types";
import { CAPABILITY_LABELS, formatBytes } from "../types";

interface CapabilityDetailProps {
  status: DashboardStatus;
}

const CLOUD_OFFERINGS = [
  "openai",
  "anthropic",
  "google",
  "stability-ai",
  "elevenlabs",
  "cohere",
  "deepgram",
];

function isCloudOffering(offering: string): boolean {
  return CLOUD_OFFERINGS.some(
    (c) => offering.toLowerCase().includes(c) || offering.startsWith("cloud:"),
  );
}

export function CapabilityDetail({ status }: CapabilityDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [pinning, setPinning] = useState(false);

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
  const pinned = status.config.features.pins[cap.capability];

  async function pinModel(modelName: string) {
    setPinning(true);
    try {
      const newPins = { ...status.config.features.pins, [cap!.capability]: modelName };
      const newConfig = {
        ...status.config,
        features: { ...status.config.features, pins: newPins },
      };
      await fetch("/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(newConfig),
      });
    } finally {
      setPinning(false);
    }
  }

  async function unpinCapability() {
    setPinning(true);
    try {
      const newPins = { ...status.config.features.pins };
      delete newPins[cap!.capability];
      const newConfig = {
        ...status.config,
        features: { ...status.config.features, pins: newPins },
      };
      await fetch("/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(newConfig),
      });
    } finally {
      setPinning(false);
    }
  }

  return (
    <div className="space-y-6 max-w-5xl">
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
      </div>

      {/* Pinned Model Banner */}
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
              overrides recommendation engine
            </span>
          </div>
          <button
            onClick={unpinCapability}
            disabled={pinning}
            className="text-xs px-2 py-1 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 disabled:opacity-50"
          >
            Unpin
          </button>
        </div>
      )}

      {/* Not Installed guidance */}
      {cap.state === "not_installed" && (
        <div className="bg-[#1a1b23] border border-gray-700 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-2">
            No service is installed that can serve{" "}
            <span className="text-gray-200">{label}</span>.
          </p>
          <p className="text-[12px] text-gray-500">
            Install an offering that supports this capability, or{" "}
            <Link to="/cloud" className="text-purple-400 hover:underline">
              add a cloud provider
            </Link>
            .
          </p>
        </div>
      )}

      {/* Needs Setup guidance */}
      {cap.state === "needs_setup" && (
        <div className="bg-[#1a1b23] border border-yellow-500/40 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-2">
            {cap.offerings.join(", ")}{" "}
            {cap.offerings.length === 1 ? "is" : "are"} installed, but no models
            for <span className="text-gray-200">{label}</span> are available.
          </p>
          <p className="text-[12px] text-yellow-400/80">
            Pull a model with this capability, or{" "}
            <Link to="/cloud" className="text-purple-400 hover:underline">
              add a cloud provider
            </Link>{" "}
            as fallback.
          </p>
        </div>
      )}

      {/* Model Ranking Table */}
      {models.length > 0 && (
        <section>
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold">
              Models ({models.length})
            </h3>
            {recommended && !pinned && (
              <span className="text-[10px] text-gray-500">
                recommended:{" "}
                <span className="text-emerald-400 font-mono">
                  {recommended}
                </span>
              </span>
            )}
          </div>
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-[#2e303a] text-gray-500 text-left">
                  <th className="px-3 py-2 font-medium w-6"></th>
                  <th className="px-3 py-2 font-medium">Model</th>
                  <th className="px-3 py-2 font-medium">Offering</th>
                  <th className="px-3 py-2 font-medium">Parameters</th>
                  <th className="px-3 py-2 font-medium">Quant</th>
                  <th className="px-3 py-2 font-medium">Size</th>
                  <th className="px-3 py-2 font-medium w-16">Status</th>
                  <th className="px-3 py-2 font-medium w-10"></th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#2e303a]">
                {models.map((model) => {
                  const isRecommended = model.name === recommended;
                  const isPinned = model.name === pinned;
                  const isExpanded = expanded === model.name;
                  const placements = model.available_on;
                  const offerings = [
                    ...new Set(placements.map((p) => p.offering)),
                  ];
                  const loaded = placements.some((p) => p.loaded);
                  const cloud = offerings.some(isCloudOffering);

                  return (
                    <ModelRow
                      key={model.name}
                      model={model}
                      offerings={offerings}
                      loaded={loaded}
                      cloud={cloud}
                      isRecommended={isRecommended}
                      isPinned={isPinned}
                      isExpanded={isExpanded}
                      pinning={pinning}
                      capabilityName={cap.capability}
                      onToggleExpand={() =>
                        setExpanded(isExpanded ? null : model.name)
                      }
                      onPin={() => pinModel(model.name)}
                    />
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
                      <span className="text-gray-600 font-mono text-[11px]">
                        {inst.endpoint}
                      </span>
                      {inst.gpu && (
                        <span className="text-gray-500 text-[11px]">
                          {inst.gpu}
                        </span>
                      )}
                      {inst.priority < 0 && (
                        <span className="text-purple-400 text-[10px]">
                          priority {inst.priority}
                        </span>
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

// ── Model Row (expandable) ──────────────────────────────────────

interface ModelRowProps {
  model: ModelStatus;
  offerings: string[];
  loaded: boolean;
  cloud: boolean;
  isRecommended: boolean;
  isPinned: boolean;
  isExpanded: boolean;
  pinning: boolean;
  capabilityName: string;
  onToggleExpand: () => void;
  onPin: () => void;
}

function ModelRow({
  model,
  offerings,
  loaded,
  cloud,
  isRecommended,
  isPinned,
  isExpanded,
  pinning,
  capabilityName: _capabilityName,
  onToggleExpand,
  onPin,
}: ModelRowProps) {
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
        <td className="px-3 py-1.5 text-center">
          {isPinned ? (
            <span className="text-purple-400 text-xs" title="Pinned">
              P
            </span>
          ) : isRecommended ? (
            <span className="text-emerald-400 text-xs" title="Recommended">
              *
            </span>
          ) : (
            <span className="text-gray-700 text-xs">
              {isExpanded ? "v" : ">"}
            </span>
          )}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-200">{model.name}</td>
        <td className="px-3 py-1.5">
          {offerings.map((o) => (
            <span
              key={o}
              className={`inline-block mr-1 text-[11px] ${
                isCloudOffering(o) ? "text-purple-400" : "text-gray-400"
              }`}
            >
              {o}
              {isCloudOffering(o) && " \u2601"}
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
          {model.size_disk > 0 ? formatBytes(model.size_disk) : cloud ? "cloud" : "-"}
        </td>
        <td className="px-3 py-1.5">
          {loaded ? (
            <span className="text-emerald-400">loaded</span>
          ) : cloud ? (
            <span className="text-purple-400">cloud</span>
          ) : (
            <span className="text-gray-600">avail</span>
          )}
        </td>
        <td className="px-3 py-1.5 text-right">
          {!isPinned && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onPin();
              }}
              disabled={pinning}
              className="text-[10px] px-1.5 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-purple-500/20 hover:text-purple-300 disabled:opacity-50"
              title={`Pin ${model.name} for this capability`}
            >
              pin
            </button>
          )}
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td colSpan={8} className="bg-[#16171f] px-6 py-3">
            <div className="grid grid-cols-2 gap-4 text-[11px]">
              <div className="space-y-1.5">
                <div className="text-gray-500 uppercase tracking-wider text-[10px] font-semibold mb-1">
                  Details
                </div>
                <Row label="Family" value={model.family} />
                <Row label="Parameters" value={model.parameter_size} />
                <Row label="Quantization" value={model.quantization_level} />
                <Row
                  label="Context"
                  value={
                    model.context_length
                      ? `${model.context_length.toLocaleString()} tokens`
                      : null
                  }
                />
                <Row
                  label="VRAM"
                  value={
                    model.vram_bytes
                      ? formatBytes(model.vram_bytes)
                      : null
                  }
                />
                <Row
                  label="Disk"
                  value={
                    model.size_disk > 0
                      ? formatBytes(model.size_disk)
                      : null
                  }
                />
                <Row
                  label="Capabilities"
                  value={model.capabilities.join(", ")}
                />
              </div>
              <div className="space-y-1.5">
                <div className="text-gray-500 uppercase tracking-wider text-[10px] font-semibold mb-1">
                  Placement
                </div>
                {model.available_on.length === 0 && (
                  <span className="text-gray-600">no placements</span>
                )}
                {model.available_on.map((p) => (
                  <div
                    key={`${p.stone}-${p.endpoint}`}
                    className="flex items-center gap-2"
                  >
                    <span
                      className={`w-1.5 h-1.5 rounded-full ${
                        p.loaded ? "bg-emerald-400" : "bg-gray-600"
                      }`}
                    />
                    <span className="text-gray-400">{p.stone}</span>
                    <span className="text-gray-600 font-mono text-[10px]">
                      {p.offering}
                    </span>
                    <span className="text-gray-500">
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

function Row({
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
