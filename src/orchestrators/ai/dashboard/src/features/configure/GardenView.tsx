import { useEffect, useState } from "react";
import { get } from "../../api/client";
import type { ResourcesResponse, StoneResources } from "../../api/types";
import { useSSE } from "../../hooks/useSSE";

export default function GardenView() {
  const [resources, setResources] = useState<StoneResources[]>([]);
  const [selected, setSelected] = useState<StoneResources | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchResources = async () => {
    try {
      const data = await get<ResourcesResponse>("/v1/resources");
      setResources(data.stones);
    } catch { /* non-fatal */ }
    setLoading(false);
  };

  useEffect(() => { fetchResources(); }, []);

  useSSE({
    focus: "resources.stone.*",
    onEvent: () => { fetchResources(); },
  });

  if (loading) {
    return <div className="p-4 text-text-dimmer text-sm">Loading...</div>;
  }

  if (resources.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-text-dimmer text-sm italic">
        No stones connected. The orchestrator discovers stones via the garden — check that Moss is running.
      </div>
    );
  }

  return (
    <div className="flex h-full">
      {/* Master: stone cards */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="grid grid-cols-2 gap-3">
          {resources.map((stone) => (
            <div
              key={stone.name}
              onClick={() => setSelected(stone)}
              className={[
                "p-4 rounded-lg border cursor-pointer transition-colors",
                selected?.name === stone.name
                  ? "border-accent bg-accent-bg"
                  : "border-border hover:border-border-focus bg-surface",
              ].join(" ")}
            >
              <div className="text-[13px] font-medium mb-2">{stone.name}</div>
              {stone.gpus.map((gpu) => (
                <div key={gpu.index} className="mb-1">
                  <div className="text-[10px] text-text-dim">{gpu.name}</div>
                  {gpu.total_vram_mb != null && (
                    <div className="mt-0.5 h-1.5 bg-surface-3 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-accent rounded-full"
                        style={{
                          width: `${Math.min(100, (gpu.committed_mb / gpu.total_vram_mb) * 100)}%`,
                        }}
                      />
                    </div>
                  )}
                  <div className="text-[9px] text-text-dimmer mt-0.5">
                    {gpu.committed_mb} / {gpu.total_vram_mb ?? "?"} MB VRAM
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>

      {/* Detail */}
      <div className="w-[350px] shrink-0 border-l border-border overflow-y-auto bg-surface">
        {selected ? (
          <StoneDetail stone={selected} />
        ) : (
          <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
            Select a stone
          </div>
        )}
      </div>
    </div>
  );
}

function StoneDetail({ stone }: { stone: StoneResources }) {
  return (
    <div className="p-5">
      <h3 className="text-sm font-semibold mb-4">{stone.name}</h3>

      {stone.gpus.map((gpu) => (
        <div key={gpu.index} className="mb-4 p-3 bg-surface-2 rounded-lg">
          <div className="text-[11px] font-medium mb-2">{gpu.name}</div>
          <div className="space-y-1 text-[10px]">
            <KV k="Vendor" v={gpu.vendor} />
            <KV k="Compute" v={gpu.compute_stack.join(", ")} />
            <KV k="VRAM Total" v={gpu.total_vram_mb ? `${gpu.total_vram_mb} MB` : "unknown"} />
            <KV k="VRAM Committed" v={`${gpu.committed_mb} MB`} />
            <KV k="Headroom" v={`${gpu.headroom_mb} MB`} />
            <KV k="Mode" v={gpu.mode} />
          </div>
        </div>
      ))}

      {stone.memory.total_mb != null && (
        <div className="mb-4">
          <div className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold mb-1">Memory</div>
          <div className="text-[11px] text-text-dim">
            {stone.memory.committed_mb} / {stone.memory.total_mb} MB
          </div>
        </div>
      )}

      {Object.keys(stone.claims).length > 0 && (
        <div>
          <div className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold mb-1">Claims</div>
          <pre className="text-[10px] text-text-dim bg-surface-2 p-2 rounded overflow-auto">
            {JSON.stringify(stone.claims, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function KV({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between">
      <span className="text-text-dimmer">{k}</span>
      <span className="text-text">{v}</span>
    </div>
  );
}
