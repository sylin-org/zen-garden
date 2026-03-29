import type { DashboardStatus, StoneStatus } from "../types";

interface StonesProps {
  status: DashboardStatus;
}

function StoneCard({ stone }: { stone: StoneStatus }) {
  const total = stone.vram_total_mb || 1;
  const used = stone.vram_used_mb;
  const pct = Math.min((used / total) * 100, 100);
  const free = total - used;

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="text-sm font-medium text-gray-100">{stone.name}</h3>
          <span className="text-[11px] text-gray-500">
            {stone.gpu ?? "CPU only"}
          </span>
        </div>
        <span
          className={`text-[10px] font-semibold px-2 py-0.5 rounded border ${
            stone.health === "healthy"
              ? "bg-emerald-400/10 text-emerald-400 border-emerald-400/30"
              : "bg-red-400/10 text-red-400 border-red-400/30"
          }`}
        >
          {stone.health}
        </span>
      </div>

      {/* VRAM Bar */}
      {stone.vram_total_mb > 0 && (
        <div className="mb-3">
          <div className="flex justify-between text-[11px] text-gray-500 mb-1">
            <span>VRAM</span>
            <span>
              {(used / 1024).toFixed(1)} / {(total / 1024).toFixed(1)} GB
            </span>
          </div>
          <div className="w-full h-2.5 bg-[#2e303a] rounded-full overflow-hidden">
            <div
              className={`h-full rounded-full transition-all ${
                pct > 90
                  ? "bg-red-500/70"
                  : pct > 70
                    ? "bg-yellow-500/70"
                    : "bg-emerald-500/70"
              }`}
              style={{ width: `${pct}%` }}
            />
          </div>
          <div className="text-right text-[10px] text-gray-600 mt-0.5">
            {(free / 1024).toFixed(1)} GB free
          </div>
        </div>
      )}

      {/* Offerings */}
      {stone.offerings.length > 0 && (
        <div>
          <span className="text-[10px] uppercase tracking-wider text-gray-500 font-semibold">
            Offerings
          </span>
          <div className="mt-1 space-y-1">
            {stone.offerings.map((off) => (
              <div
                key={off.kind}
                className="flex items-center gap-2 text-[12px]"
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                    off.healthy ? "bg-emerald-400" : "bg-red-400"
                  }`}
                />
                <span className="text-gray-300 capitalize">{off.kind}</span>
                <span className="text-gray-500 font-mono">
                  {off.model_count} model{off.model_count !== 1 ? "s" : ""}
                </span>
                {off.loaded_count > 0 && (
                  <span className="text-gray-500 font-mono">
                    ({off.loaded_count} loaded)
                  </span>
                )}
                <span
                  className={`ml-auto text-[11px] ${
                    off.healthy ? "text-emerald-400" : "text-red-400"
                  }`}
                >
                  {off.healthy ? "healthy" : "unhealthy"}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export function Stones({ status }: StonesProps) {
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-medium text-gray-100">Stones</h2>
        <p className="text-[12px] text-gray-500">
          {status.stones.length} stone{status.stones.length !== 1 ? "s" : ""}{" "}
          discovered
        </p>
      </div>

      {status.stones.length === 0 ? (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-6 text-center">
          <p className="text-sm text-gray-500">
            No stones discovered yet. The orchestrator is scanning the network.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
          {status.stones.map((stone) => (
            <StoneCard key={stone.id} stone={stone} />
          ))}
        </div>
      )}
    </div>
  );
}
