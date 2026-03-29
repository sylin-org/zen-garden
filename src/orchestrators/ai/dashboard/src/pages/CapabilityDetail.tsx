import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import type {
  DashboardStatus,
  ModelStatus,
  InstanceStatus,
} from "../types";
import { CAPABILITY_LABELS, formatBytes } from "../types";

interface CapabilityDetailProps {
  status: DashboardStatus;
}

// ── Stone Colors ────────────────────────────────────────────────
// Deterministic color from stone name. Same stone = same color everywhere.
const STONE_PALETTE = [
  { bg: "bg-blue-500", border: "border-blue-500", text: "text-blue-400", hex: "#3b82f6" },
  { bg: "bg-emerald-500", border: "border-emerald-500", text: "text-emerald-400", hex: "#10b981" },
  { bg: "bg-amber-500", border: "border-amber-500", text: "text-amber-400", hex: "#f59e0b" },
  { bg: "bg-rose-500", border: "border-rose-500", text: "text-rose-400", hex: "#f43f5e" },
  { bg: "bg-violet-500", border: "border-violet-500", text: "text-violet-400", hex: "#8b5cf6" },
  { bg: "bg-cyan-500", border: "border-cyan-500", text: "text-cyan-400", hex: "#06b6d4" },
  { bg: "bg-orange-500", border: "border-orange-500", text: "text-orange-400", hex: "#f97316" },
  { bg: "bg-pink-500", border: "border-pink-500", text: "text-pink-400", hex: "#ec4899" },
];

function stoneColor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash * 31 + name.charCodeAt(i)) | 0;
  }
  return STONE_PALETTE[Math.abs(hash) % STONE_PALETTE.length];
}

const CLOUD_OFFERINGS = [
  "openai", "anthropic", "google", "stability-ai",
  "elevenlabs", "cohere", "deepgram",
];

function isCloudOffering(offering: string): boolean {
  return CLOUD_OFFERINGS.some(
    (c) => offering.toLowerCase().includes(c) || offering.startsWith("cloud:"),
  );
}

// ── Main Component ──────────────────────────────────────────────

export function CapabilityDetail({ status }: CapabilityDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [expanded, setExpanded] = useState<string | null>(null);
  const [pinning, setPinning] = useState(false);

  const cap = status.capabilities.find((c) => c.capability === name);
  const label = CAPABILITY_LABELS[name ?? ""] ?? name ?? "Unknown";

  if (!cap) {
    return (
      <div className="p-6">
        <p className="text-gray-400">Capability &quot;{name}&quot; not found.</p>
        <Link to="/" className="text-blue-400 text-sm hover:underline">Back to overview</Link>
      </div>
    );
  }

  const pinned = status.config.features.pins[cap.capability];

  // Group instances by offering kind
  const instancesByOffering: Record<string, InstanceStatus[]> = {};
  for (const inst of status.instances) {
    if (!inst.capabilities.includes(cap.capability)) continue;
    const group = instancesByOffering[inst.kind] ?? [];
    group.push(inst);
    instancesByOffering[inst.kind] = group;
  }

  // Group models by offering (via their placement)
  const modelsByOffering: Record<string, ModelStatus[]> = {};
  for (const model of status.models) {
    if (!model.capabilities.includes(cap.capability)) continue;
    const offerings = [...new Set(model.available_on.map((p) => p.offering))];
    for (const off of offerings) {
      const group = modelsByOffering[off] ?? [];
      group.push(model);
      modelsByOffering[off] = group;
    }
    // Models with no placement (orphaned from global registry)
    if (offerings.length === 0) {
      const group = modelsByOffering["_unknown"] ?? [];
      group.push(model);
      modelsByOffering["_unknown"] = group;
    }
  }

  // All offering kinds that serve this capability
  const offeringKinds = [
    ...new Set([
      ...Object.keys(instancesByOffering),
      ...Object.keys(modelsByOffering),
    ]),
  ].filter((k) => k !== "_unknown");

  const recommended = status.recommendations[cap.capability];

  async function pinModel(modelName: string) {
    setPinning(true);
    try {
      const newPins = { ...status.config.features.pins, [cap!.capability]: modelName };
      await fetch("/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ...status.config,
          features: { ...status.config.features, pins: newPins },
        }),
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
      await fetch("/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ...status.config,
          features: { ...status.config.features, pins: newPins },
        }),
      });
    } finally {
      setPinning(false);
    }
  }

  return (
    <div className="space-y-6 max-w-6xl">
      {/* Header */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">Overview</Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">{label}</span>
        </div>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-medium text-gray-100">{label}</h2>
          <StateBadge state={cap.state} />
          {recommended && !pinned && (
            <span className="text-[10px] text-gray-500 ml-2">
              recommended: <span className="text-emerald-400 font-mono">{recommended}</span>
            </span>
          )}
        </div>
      </div>

      {/* Pinned Banner */}
      {pinned && (
        <div className="bg-purple-500/10 border border-purple-500/30 rounded-lg px-4 py-2 flex items-center justify-between">
          <div>
            <span className="text-[10px] text-purple-400 uppercase tracking-wider font-semibold">Pinned</span>
            <span className="ml-2 text-sm text-gray-200 font-mono">{pinned}</span>
            <span className="ml-2 text-[11px] text-gray-500">overrides recommendation</span>
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

      {/* Guidance for inactive capabilities */}
      {cap.state === "not_installed" && (
        <div className="bg-[#1a1b23] border border-gray-700 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-1">
            No service can serve <span className="text-gray-200">{label}</span>.
          </p>
          <p className="text-[12px] text-gray-500">
            Install a compatible offering, or <Link to="/cloud" className="text-purple-400 hover:underline">add a cloud provider</Link>.
          </p>
        </div>
      )}

      {cap.state === "needs_setup" && (
        <div className="bg-[#1a1b23] border border-yellow-500/40 rounded-lg p-4">
          <p className="text-sm text-gray-400 mb-1">
            {cap.offerings.join(", ")} installed, but no {label} models available.
          </p>
          <p className="text-[12px] text-yellow-400/80">
            Pull a model, or <Link to="/cloud" className="text-purple-400 hover:underline">add a cloud provider</Link> as fallback.
          </p>
        </div>
      )}

      {/* Per-offering sections */}
      {offeringKinds.map((kind) => {
        const instances = instancesByOffering[kind] ?? [];
        const models = modelsByOffering[kind] ?? [];
        const cloud = isCloudOffering(kind);

        if (cloud) {
          return (
            <CloudOfferingCard
              key={kind}
              kind={kind}
              instances={instances}
              models={models}
              pinned={pinned}
              recommended={recommended}
              pinning={pinning}
              onPin={pinModel}
            />
          );
        }

        // Collect unique stones for this offering
        const stoneNames = [
          ...new Set(instances.map((i) => i.stone_name)),
        ];

        return (
          <LocalOfferingCard
            key={kind}
            kind={kind}
            instances={instances}
            models={models}
            stoneNames={stoneNames}
            pinned={pinned}
            recommended={recommended}
            pinning={pinning}
            expanded={expanded}
            onToggleExpand={(name) => setExpanded(expanded === name ? null : name)}
            onPin={pinModel}
          />
        );
      })}
    </div>
  );
}

// ── Local Offering Card ─────────────────────────────────────────

interface LocalOfferingCardProps {
  kind: string;
  instances: InstanceStatus[];
  models: ModelStatus[];
  stoneNames: string[];
  pinned: string | undefined;
  recommended: string | undefined;
  pinning: boolean;
  expanded: string | null;
  onToggleExpand: (name: string) => void;
  onPin: (name: string) => void;
}

function LocalOfferingCard({
  kind,
  instances,
  models,
  stoneNames,
  pinned,
  recommended,
  pinning,
  expanded,
  onToggleExpand,
  onPin,
}: LocalOfferingCardProps) {
  const stoneColors = stoneNames.map((name) => ({
    name,
    color: stoneColor(name),
    instance: instances.find((i) => i.stone_name === name),
  }));

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
      {/* Offering header */}
      <div className="px-4 py-2 border-b border-[#2e303a] flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium text-gray-200 capitalize">{kind}</span>
          {instances.map((inst) => (
            <span key={inst.endpoint} className="flex items-center gap-1.5 text-[11px] text-gray-500">
              <span className={`w-1.5 h-1.5 rounded-full ${inst.health === "healthy" ? "bg-emerald-400" : "bg-red-400"}`} />
              {inst.stone_name}
              {inst.gpu && <span className="text-gray-600">({inst.gpu})</span>}
            </span>
          ))}
        </div>
        <span className="text-[11px] text-gray-500">
          {models.length} model{models.length !== 1 ? "s" : ""}
        </span>
      </div>

      {/* Model grid with stone columns */}
      {models.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-[#2e303a] text-gray-500 text-left">
                <th className="px-3 py-1.5 font-medium">Model</th>
                <th className="px-3 py-1.5 font-medium">Params</th>
                <th className="px-3 py-1.5 font-medium">Quant</th>
                <th className="px-3 py-1.5 font-medium">VRAM</th>
                {stoneColors.map((sc) => (
                  <th key={sc.name} className="px-1 py-1.5 font-medium text-center w-6" title={sc.name}>
                    <span className={`inline-block w-3 h-3 rounded-sm ${sc.color.bg}`} />
                  </th>
                ))}
                <th className="px-2 py-1.5 w-10"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#2e303a]/50">
              {models.map((model) => {
                const isRec = model.name === recommended;
                const isPin = model.name === pinned;
                const isExp = expanded === model.name;

                return (
                  <ModelRow
                    key={model.name}
                    model={model}
                    stoneColors={stoneColors}
                    isRecommended={isRec}
                    isPinned={isPin}
                    isExpanded={isExp}
                    pinning={pinning}
                    onToggleExpand={() => onToggleExpand(model.name)}
                    onPin={() => onPin(model.name)}
                  />
                );
              })}
            </tbody>
          </table>

          {/* Stone legend */}
          {stoneColors.length > 0 && (
            <div className="px-4 py-2 border-t border-[#2e303a]/50 flex flex-wrap gap-3">
              {stoneColors.map((sc) => (
                <span key={sc.name} className="flex items-center gap-1.5 text-[10px]">
                  <span className={`w-2.5 h-2.5 rounded-sm ${sc.color.bg}`} />
                  <span className={sc.color.text}>{sc.name}</span>
                  {sc.instance && (
                    <span className="text-gray-600">
                      {sc.instance.vram_total_mb > 0 && `${Math.round(sc.instance.vram_total_mb / 1024)}GB`}
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

// ── Model Row ───────────────────────────────────────────────────

interface StoneColorEntry {
  name: string;
  color: typeof STONE_PALETTE[number];
  instance: InstanceStatus | undefined;
}

interface ModelRowProps {
  model: ModelStatus;
  stoneColors: StoneColorEntry[];
  isRecommended: boolean;
  isPinned: boolean;
  isExpanded: boolean;
  pinning: boolean;
  onToggleExpand: () => void;
  onPin: () => void;
}

function ModelRow({
  model,
  stoneColors,
  isRecommended,
  isPinned,
  isExpanded,
  pinning,
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
        <td className="px-3 py-1.5 font-mono text-gray-200">
          {isPinned && <span className="text-purple-400 mr-1" title="Pinned">P</span>}
          {isRecommended && !isPinned && <span className="text-emerald-400 mr-1" title="Recommended">*</span>}
          {model.name}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.parameter_size ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.quantization_level ?? "-"}
        </td>
        <td className="px-3 py-1.5 font-mono text-gray-500">
          {model.vram_bytes ? formatBytes(model.vram_bytes) : "-"}
        </td>
        {/* Stone presence squares */}
        {stoneColors.map((sc) => {
          const placement = model.available_on.find(
            (p) => p.stone === sc.name,
          );
          if (!placement) {
            return (
              <td key={sc.name} className="px-1 py-1.5 text-center">
                <span className="inline-block w-3 h-3 rounded-sm bg-gray-800" title={`Not on ${sc.name}`} />
              </td>
            );
          }
          return (
            <td key={sc.name} className="px-1 py-1.5 text-center">
              <span
                className={`inline-block w-3 h-3 rounded-sm ${
                  placement.loaded
                    ? sc.color.bg
                    : `${sc.color.bg} opacity-30`
                }`}
                title={`${sc.name}: ${placement.loaded ? "loaded" : "available"}`}
              />
            </td>
          );
        })}
        <td className="px-2 py-1.5 text-right">
          {!isPinned && (
            <button
              onClick={(e) => { e.stopPropagation(); onPin(); }}
              disabled={pinning}
              className="text-[10px] px-1.5 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-purple-500/20 hover:text-purple-300 disabled:opacity-50"
              title={`Pin ${model.name}`}
            >
              pin
            </button>
          )}
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td colSpan={5 + stoneColors.length + 1} className="bg-[#16171f] px-6 py-3">
            <div className="grid grid-cols-2 gap-4 text-[11px]">
              <div className="space-y-1">
                <DetailRow label="Family" value={model.family} />
                <DetailRow label="Parameters" value={model.parameter_size} />
                <DetailRow label="Quantization" value={model.quantization_level} />
                <DetailRow label="Context" value={model.context_length ? `${model.context_length.toLocaleString()} tokens` : null} />
                <DetailRow label="VRAM" value={model.vram_bytes ? formatBytes(model.vram_bytes) : null} />
                <DetailRow label="Disk" value={model.size_disk > 0 ? formatBytes(model.size_disk) : null} />
                <DetailRow label="Capabilities" value={model.capabilities.join(", ")} />
              </div>
              <div className="space-y-1">
                <div className="text-gray-500 uppercase tracking-wider text-[10px] font-semibold">Placement</div>
                {model.available_on.map((p) => (
                  <div key={`${p.stone}-${p.endpoint}`} className="flex items-center gap-2">
                    <span className={`w-2 h-2 rounded-sm ${stoneColor(p.stone).bg} ${p.loaded ? "" : "opacity-30"}`} />
                    <span className="text-gray-400">{p.stone}</span>
                    <span className="text-gray-600 text-[10px]">{p.loaded ? "loaded" : "available"}</span>
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

// ── Cloud Offering Card ─────────────────────────────────────────

interface CloudOfferingCardProps {
  kind: string;
  instances: InstanceStatus[];
  models: ModelStatus[];
  pinned: string | undefined;
  recommended: string | undefined;
  pinning: boolean;
  onPin: (name: string) => void;
}

function CloudOfferingCard({
  kind,
  instances,
  models,
  pinned,
  recommended: _recommended,
  pinning,
  onPin,
}: CloudOfferingCardProps) {
  const inst = instances[0];

  return (
    <div className="bg-[#1a1b23] border border-purple-500/20 rounded-lg overflow-hidden">
      <div className="px-4 py-2 border-b border-[#2e303a] flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium text-purple-300 capitalize">{kind}</span>
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-400">cloud</span>
          {inst && (
            <span className="flex items-center gap-1.5 text-[11px] text-gray-500">
              <span className={`w-1.5 h-1.5 rounded-full ${inst.health === "healthy" ? "bg-emerald-400" : "bg-red-400"}`} />
              {inst.health === "healthy" ? "connected" : "unreachable"}
              <span className="text-gray-600">priority {inst.priority}</span>
            </span>
          )}
        </div>
        <span className="text-[11px] text-gray-500">
          {models.length} model{models.length !== 1 ? "s" : ""}
        </span>
      </div>

      {models.length > 0 && (
        <div className="divide-y divide-[#2e303a]/30">
          {models.map((model) => {
            const isPin = model.name === pinned;
            return (
              <div
                key={model.name}
                className={`px-4 py-1.5 flex items-center justify-between text-[12px] ${
                  isPin ? "bg-purple-500/5" : "hover:bg-[#22232d]"
                }`}
              >
                <span className="font-mono text-gray-300">
                  {isPin && <span className="text-purple-400 mr-1">P</span>}
                  {model.name}
                </span>
                {!isPin && (
                  <button
                    onClick={() => onPin(model.name)}
                    disabled={pinning}
                    className="text-[10px] px-1.5 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-purple-500/20 hover:text-purple-300 disabled:opacity-50"
                  >
                    pin
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Helpers ─────────────────────────────────────────────────────

function DetailRow({ label, value }: { label: string; value: string | null | undefined }) {
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
    <span className={`text-[10px] font-semibold px-2 py-0.5 rounded border ${styles[state] ?? styles.not_installed}`}>
      {state.replace("_", " ")}
    </span>
  );
}
